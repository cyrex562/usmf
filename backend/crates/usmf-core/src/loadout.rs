use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::component::Component;

#[derive(Debug, Clone, Copy)]
pub struct LoadoutCapacity {
    pub max_weight: f64,
    pub max_space: f64,
    pub base_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadoutItem {
    pub component_id: i64,
    pub quantity: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadoutTotals {
    pub weight: f64,
    pub space: f64,
    pub cost: f64,
    pub power_gen: f64,
    pub power_draw: f64,
    pub initiative: f64,
    pub capabilities: HashMap<String, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadoutValidation {
    pub valid: bool,
    pub violations: Vec<String>,
    pub totals: LoadoutTotals,
}

/// Shared by Asset (vehicle chassis + components) and PersonnelType (a soldier's
/// carry capacity + equipment): both are "some capacity slotted with components,"
/// so they validate and total the same way. `capacity: None` skips the weight/space
/// envelope check and reports `missing_capacity_violation` instead, if given.
pub fn validate_loadout(
    items: &[LoadoutItem],
    capacity: Option<LoadoutCapacity>,
    missing_capacity_violation: Option<String>,
    components: &[Component],
) -> LoadoutValidation {
    let mut totals = LoadoutTotals {
        cost: capacity.map(|c| c.base_cost).unwrap_or(0.0),
        ..Default::default()
    };
    let mut violations = Vec::new();

    for item in items {
        let Some(component) = components.iter().find(|c| c.id == item.component_id) else {
            violations.push(format!("Component ID {} not found.", item.component_id));
            continue;
        };
        let qty = item.quantity as f64;
        let stats = &component.stats;

        totals.weight += stats.weight * qty;
        totals.space += stats.space * qty;
        totals.cost += stats.cost * qty;
        totals.power_gen += stats.power_gen * qty;
        totals.power_draw += stats.power_draw * qty;
        totals.initiative += stats.initiative * qty;
        for (tag, level) in &stats.capabilities {
            *totals.capabilities.entry(tag.clone()).or_insert(0) += level * item.quantity as i32;
        }
    }

    match capacity {
        None => {
            if let Some(msg) = missing_capacity_violation {
                violations.push(msg);
            }
        }
        Some(cap) => {
            if totals.weight > cap.max_weight {
                violations.push(format!("Overweight: {}/{}", totals.weight, cap.max_weight));
            }
            if totals.space > cap.max_space {
                violations.push(format!("No space: {}/{}", totals.space, cap.max_space));
            }
        }
    }

    if totals.power_draw > totals.power_gen {
        violations.push(format!(
            "Insufficient power: generating {}, need {}",
            totals.power_gen, totals.power_draw
        ));
    }

    LoadoutValidation {
        valid: violations.is_empty(),
        violations,
        totals,
    }
}
