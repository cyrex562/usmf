use serde::{Deserialize, Serialize};

use crate::component::Component;
use crate::loadout::{validate_loadout, LoadoutCapacity};
pub use crate::loadout::{
    LoadoutItem as PersonnelLoadoutItem, LoadoutTotals as PersonnelTotals,
    LoadoutValidation as PersonnelValidation,
};

/// An individually-modeled role/position (e.g. "Rifleman", "Squad Leader",
/// "Combat Medic") — structurally the same idea as an Asset (a capacity slotted
/// with Components), but the capacity here is a soldier's carry limit rather than
/// a vehicle's chassis envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonnelType {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub role_category: Option<String>,
    pub max_carry_weight: f64,
    pub max_carry_space: f64,
    #[serde(default)]
    pub base_cost: f64,
    pub loadout: Vec<PersonnelLoadoutItem>,
}

/// A soldier's carry capacity is fixed by the body, not by chassis lookup, so
/// this never reports a "missing capacity" violation the way `validate_asset` can.
pub fn validate_personnel_loadout(
    personnel_type: &PersonnelType,
    components: &[Component],
) -> PersonnelValidation {
    let capacity = LoadoutCapacity {
        max_weight: personnel_type.max_carry_weight,
        max_space: personnel_type.max_carry_space,
        base_cost: personnel_type.base_cost,
    };
    validate_loadout(&personnel_type.loadout, Some(capacity), None, components)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{ComponentStats, ComponentType};

    fn component(id: i64, weight: f64, space: f64) -> Component {
        Component {
            id,
            name: format!("component-{id}"),
            component_type: ComponentType::Weapon,
            stats: ComponentStats {
                weight,
                space,
                ..Default::default()
            },
        }
    }

    #[test]
    fn rifleman_within_carry_capacity() {
        let components = vec![component(1, 8.0, 1.0), component(2, 20.0, 3.0)];
        let rifleman = PersonnelType {
            id: 1,
            name: "Rifleman".into(),
            role_category: Some("Infantry".into()),
            max_carry_weight: 60.0,
            max_carry_space: 10.0,
            base_cost: 0.0,
            loadout: vec![
                PersonnelLoadoutItem {
                    component_id: 1,
                    quantity: 1,
                },
                PersonnelLoadoutItem {
                    component_id: 2,
                    quantity: 1,
                },
            ],
        };

        let result = validate_personnel_loadout(&rifleman, &components);
        assert!(result.valid, "{:?}", result.violations);
        assert_eq!(result.totals.weight, 28.0);
    }

    #[test]
    fn overloaded_soldier_is_flagged() {
        let components = vec![component(1, 80.0, 1.0)];
        let overloaded = PersonnelType {
            id: 1,
            name: "Overloaded Mule".into(),
            role_category: None,
            max_carry_weight: 60.0,
            max_carry_space: 10.0,
            base_cost: 0.0,
            loadout: vec![PersonnelLoadoutItem {
                component_id: 1,
                quantity: 1,
            }],
        };

        let result = validate_personnel_loadout(&overloaded, &components);
        assert!(!result.valid);
        assert!(result.violations.iter().any(|v| v.contains("Overweight")));
    }
}
