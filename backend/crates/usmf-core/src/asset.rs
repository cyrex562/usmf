use serde::{Deserialize, Serialize};

use crate::component::Component;
use crate::loadout::{validate_loadout, LoadoutCapacity};
pub use crate::loadout::{
    LoadoutItem as AssetComponent, LoadoutTotals as AssetTotals,
    LoadoutValidation as AssetValidation,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChassisSpec {
    pub name: String,
    pub max_weight: f64,
    pub max_space: f64,
    pub base_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: i64,
    pub name: String,
    pub chassis_type: String,
    pub components: Vec<AssetComponent>,
}

/// Ports V2's `validate_asset_logic`: sums component stats scaled by quantity and
/// checks them against the chassis envelope (weight/space/power).
pub fn validate_asset(
    asset: &Asset,
    chassis: Option<&ChassisSpec>,
    components: &[Component],
) -> AssetValidation {
    let capacity = chassis.map(|c| LoadoutCapacity {
        max_weight: c.max_weight,
        max_space: c.max_space,
        base_cost: c.base_cost,
    });
    let missing_capacity = chassis
        .is_none()
        .then(|| format!("Unknown chassis type: {}", asset.chassis_type));

    validate_loadout(&asset.components, capacity, missing_capacity, components)
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

    #[test]
    fn unknown_chassis_is_flagged_but_still_totals_components() {
        let components = vec![component(1, 300.0, 3.0, 400.0, 0.0)];
        let asset = Asset {
            id: 1,
            name: "Mystery Rig".into(),
            chassis_type: "Nonexistent".into(),
            components: vec![AssetComponent {
                component_id: 1,
                quantity: 1,
            }],
        };

        let result = validate_asset(&asset, None, &components);
        assert!(!result.valid);
        assert!(result
            .violations
            .iter()
            .any(|v| v.contains("Unknown chassis type")));
        assert_eq!(result.totals.weight, 300.0);
    }
}
