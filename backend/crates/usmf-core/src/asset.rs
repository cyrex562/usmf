use serde::{Deserialize, Serialize};

use crate::component::Component;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChassisSpec {
    pub name: String,
    pub max_weight: f64,
    pub max_space: f64,
    pub base_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetComponent {
    pub component_id: i64,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: i64,
    pub name: String,
    pub chassis_type: String,
    pub components: Vec<AssetComponent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetTotals {
    pub weight: f64,
    pub space: f64,
    pub cost: f64,
    pub power_gen: f64,
    pub power_draw: f64,
    pub capabilities: std::collections::HashMap<String, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetValidation {
    pub valid: bool,
    pub violations: Vec<String>,
    pub totals: AssetTotals,
}

/// Ports V2's `validate_asset_logic`: sums component stats scaled by quantity and
/// checks them against the chassis envelope (weight/space/power).
pub fn validate_asset(
    asset: &Asset,
    chassis: Option<&ChassisSpec>,
    components: &[Component],
) -> AssetValidation {
    let mut totals = AssetTotals {
        cost: chassis.map(|c| c.base_cost).unwrap_or(0.0),
        ..Default::default()
    };
    let mut violations = Vec::new();

    for entry in &asset.components {
        let Some(component) = components.iter().find(|c| c.id == entry.component_id) else {
            violations.push(format!("Component ID {} not found.", entry.component_id));
            continue;
        };
        let qty = entry.quantity as f64;
        let stats = &component.stats;

        totals.weight += stats.weight * qty;
        totals.space += stats.space * qty;
        totals.cost += stats.cost * qty;
        totals.power_gen += stats.power_gen * qty;
        totals.power_draw += stats.power_draw * qty;
        for (tag, level) in &stats.capabilities {
            *totals.capabilities.entry(tag.clone()).or_insert(0) += level * entry.quantity as i32;
        }
    }

    match chassis {
        None => violations.push(format!("Unknown chassis type: {}", asset.chassis_type)),
        Some(spec) => {
            if totals.weight > spec.max_weight {
                violations.push(format!("Overweight: {}/{}", totals.weight, spec.max_weight));
            }
            if totals.space > spec.max_space {
                violations.push(format!("No space: {}/{}", totals.space, spec.max_space));
            }
        }
    }

    if totals.power_draw > totals.power_gen {
        violations.push(format!(
            "Insufficient power: generating {}, need {}",
            totals.power_gen, totals.power_draw
        ));
    }

    AssetValidation {
        valid: violations.is_empty(),
        violations,
        totals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{ComponentStats, ComponentType};

    fn component(id: i64, weight: f64, space: f64, power_gen: f64, power_draw: f64) -> Component {
        Component {
            id,
            name: format!("component-{id}"),
            component_type: ComponentType::Engine,
            stats: ComponentStats {
                weight,
                space,
                power_gen,
                power_draw,
                ..Default::default()
            },
        }
    }

    #[test]
    fn valid_asset_within_chassis_envelope() {
        let chassis = ChassisSpec {
            name: "Light Wheeled".into(),
            max_weight: 2000.0,
            max_space: 8.0,
            base_cost: 1000.0,
        };
        let components = vec![component(1, 300.0, 3.0, 400.0, 0.0)];
        let asset = Asset {
            id: 1,
            name: "Scout Car".into(),
            chassis_type: chassis.name.clone(),
            components: vec![AssetComponent {
                component_id: 1,
                quantity: 1,
            }],
        };

        let result = validate_asset(&asset, Some(&chassis), &components);
        assert!(result.valid, "{:?}", result.violations);
        assert_eq!(result.totals.weight, 300.0);
    }

    #[test]
    fn overweight_asset_is_flagged() {
        let chassis = ChassisSpec {
            name: "Light Wheeled".into(),
            max_weight: 500.0,
            max_space: 8.0,
            base_cost: 1000.0,
        };
        let components = vec![component(1, 1200.0, 6.0, 0.0, 10.0)];
        let asset = Asset {
            id: 1,
            name: "Overloaded".into(),
            chassis_type: chassis.name.clone(),
            components: vec![AssetComponent {
                component_id: 1,
                quantity: 1,
            }],
        };

        let result = validate_asset(&asset, Some(&chassis), &components);
        assert!(!result.valid);
        assert!(result.violations.iter().any(|v| v.contains("Overweight")));
        assert!(result
            .violations
            .iter()
            .any(|v| v.contains("Insufficient power")));
    }
}
