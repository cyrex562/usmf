use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::asset::AssetTotals;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitType {
    Hq,
    Line,
    Support,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    pub id: i64,
    pub name: String,
    pub unit_type: UnitType,
    pub parent_id: Option<i64>,
    pub asset_id: Option<i64>,
    /// Direct-child capacity for span-of-control warnings. `None` = unlimited.
    pub c2_capacity: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnitRollup {
    pub weight: f64,
    pub cost: f64,
    pub daily_supply_consumption: f64,
    pub capabilities: HashMap<String, i32>,
    pub span_of_control_warnings: Vec<String>,
}

/// Ports V2's `calculate_unit_stats`: recursively sums weight/cost/supply and
/// capability tags up the tree, and flags any HQ whose direct-child count
/// exceeds its `c2_capacity`. Unlike V2, leaf weight/cost/supply come from the
/// asset's real computed totals rather than a hardcoded placeholder.
pub fn rollup_unit(
    root_id: i64,
    units: &[Unit],
    asset_totals: &HashMap<i64, AssetTotals>,
) -> UnitRollup {
    let by_id: HashMap<i64, &Unit> = units.iter().map(|u| (u.id, u)).collect();
    let mut children_of: HashMap<i64, Vec<&Unit>> = HashMap::new();
    for u in units {
        if let Some(parent_id) = u.parent_id {
            children_of.entry(parent_id).or_default().push(u);
        }
    }

    fn recurse(
        id: i64,
        by_id: &HashMap<i64, &Unit>,
        children_of: &HashMap<i64, Vec<&Unit>>,
        asset_totals: &HashMap<i64, AssetTotals>,
    ) -> UnitRollup {
        let mut rollup = UnitRollup::default();
        let Some(unit) = by_id.get(&id) else {
            return rollup;
        };

        if let Some(asset_id) = unit.asset_id {
            if let Some(totals) = asset_totals.get(&asset_id) {
                rollup.weight += totals.weight;
                rollup.cost += totals.cost;
                rollup.daily_supply_consumption += totals.power_draw;
                for (tag, level) in &totals.capabilities {
                    *rollup.capabilities.entry(tag.clone()).or_insert(0) += level;
                }
            }
        }

        let children = children_of.get(&id).cloned().unwrap_or_default();
        if let Some(capacity) = unit.c2_capacity {
            if children.len() as u32 > capacity {
                rollup.span_of_control_warnings.push(format!(
                    "{} has {} direct subordinates but C2 capacity {}",
                    unit.name,
                    children.len(),
                    capacity
                ));
            }
        }

        for child in children {
            let child_rollup = recurse(child.id, by_id, children_of, asset_totals);
            rollup.weight += child_rollup.weight;
            rollup.cost += child_rollup.cost;
            rollup.daily_supply_consumption += child_rollup.daily_supply_consumption;
            for (tag, level) in child_rollup.capabilities {
                *rollup.capabilities.entry(tag).or_insert(0) += level;
            }
            rollup
                .span_of_control_warnings
                .extend(child_rollup.span_of_control_warnings);
        }

        rollup
    }

    recurse(root_id, &by_id, &children_of, asset_totals)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hq(id: i64, parent: Option<i64>, capacity: Option<u32>) -> Unit {
        Unit {
            id,
            name: format!("hq-{id}"),
            unit_type: UnitType::Hq,
            parent_id: parent,
            asset_id: None,
            c2_capacity: capacity,
        }
    }

    fn leaf(id: i64, parent: i64, asset_id: i64) -> Unit {
        Unit {
            id,
            name: format!("leaf-{id}"),
            unit_type: UnitType::Line,
            parent_id: Some(parent),
            asset_id: Some(asset_id),
            c2_capacity: None,
        }
    }

    #[test]
    fn rolls_up_leaf_totals_and_capabilities() {
        let units = vec![hq(1, None, Some(2)), leaf(2, 1, 10), leaf(3, 1, 11)];
        let mut asset_totals = HashMap::new();
        asset_totals.insert(
            10,
            AssetTotals {
                weight: 5000.0,
                cost: 1000.0,
                capabilities: HashMap::from([("indirect_fire".to_string(), 1)]),
                ..Default::default()
            },
        );
        asset_totals.insert(
            11,
            AssetTotals {
                weight: 3000.0,
                cost: 500.0,
                capabilities: HashMap::from([("indirect_fire".to_string(), 1)]),
                ..Default::default()
            },
        );

        let rollup = rollup_unit(1, &units, &asset_totals);
        assert_eq!(rollup.weight, 8000.0);
        assert_eq!(rollup.cost, 1500.0);
        assert_eq!(rollup.capabilities.get("indirect_fire"), Some(&2));
        assert!(rollup.span_of_control_warnings.is_empty());
    }

    #[test]
    fn flags_span_of_control_violation() {
        let units = vec![hq(1, None, Some(1)), leaf(2, 1, 10), leaf(3, 1, 11)];
        let asset_totals = HashMap::new();

        let rollup = rollup_unit(1, &units, &asset_totals);
        assert_eq!(rollup.span_of_control_warnings.len(), 1);
    }
}
