use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentType {
    Weapon,
    Engine,
    Power,
    Sensor,
    Armor,
    Comms,
    Logistics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentStats {
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub space: f64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub power_gen: f64,
    #[serde(default)]
    pub power_draw: f64,
    #[serde(default)]
    pub damage: f64,
    #[serde(default)]
    pub range_hexes: u32,
    /// Contribution to whatever carries this component's baseline initiative
    /// (see `usmf-sim`'s round loop) -- e.g. a lightweight recon sensor or a
    /// well-drilled radio operator's kit might add a little, heavy armor might
    /// be modeled as a negative delta. Purely additive; the unit's own
    /// `base_initiative` takes the max across its own_assets/personnel, not a
    /// sum, so one fast element doesn't get diluted by slower ones.
    #[serde(default)]
    pub initiative: f64,
    /// Contribution to a combatant's per-round action-point budget
    /// (`CombatantState::max_action_points`, design_doc.md §8) -- same shape
    /// as `initiative`: additive across a loadout, and `base_action_points`
    /// takes the max across own_assets/personnel rather than summing, so one
    /// well-equipped element sets the pace instead of being diluted.
    #[serde(default)]
    pub action_points: f64,
    /// A weapon Component's cost, in action points, to fire
    /// (`CombatantState::attack_ap_cost`). Per-weapon, like `damage`/
    /// `range_hexes` -- not summed across a loadout (see `usmf-sim::spawn`'s
    /// representative-weapon selection).
    #[serde(default)]
    pub attack_ap_cost: u32,
    /// Contribution to whatever carries this component's `hit_points` pool
    /// (`legacy_linear_v1`'s health pool -- design_doc.md §8) -- summed
    /// across a loadout the same way `weight`/`cost` already are, since a
    /// chassis/armor Component's toughness genuinely does add up.
    #[serde(default)]
    pub hit_points: f64,
    #[serde(default)]
    pub capabilities: HashMap<String, i32>,
    /// Per-ruleset combat data (design_doc.md §2.1) -- e.g. a weapon's
    /// `{"cepheus_vehicle_v1": {"damage_dice": "6D6", "damage_type": "Sap"}}`
    /// alongside a coarse `{"aggregate_strength_v1": {"combat_power": 12}}`.
    /// Kept as a raw JSON blob per ruleset, not a fixed struct, because
    /// different rulesets need different, unpredictable fields -- the same
    /// "heterogeneous, don't force one schema" reasoning already applied to
    /// `stats` as a whole. A Component with no entry for a given ruleset
    /// simply has nothing to contribute to it.
    #[serde(default)]
    pub rulesets: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub id: i64,
    pub name: String,
    pub component_type: ComponentType,
    pub stats: ComponentStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rulesets_block_round_trips_through_json() {
        let stats = ComponentStats {
            damage: 0.0,
            rulesets: HashMap::from([
                (
                    "cepheus_vehicle_v1".to_string(),
                    json!({"damage_dice": "6D6", "damage_type": "Sap", "armor_front": 40.0}),
                ),
                (
                    "aggregate_strength_v1".to_string(),
                    json!({"combat_power": 12.0}),
                ),
            ]),
            ..Default::default()
        };

        let json = serde_json::to_string(&stats).unwrap();
        let back: ComponentStats = serde_json::from_str(&json).unwrap();

        assert_eq!(back.rulesets["cepheus_vehicle_v1"]["damage_dice"], "6D6");
        assert_eq!(back.rulesets["aggregate_strength_v1"]["combat_power"], 12.0);
    }

    #[test]
    fn missing_rulesets_defaults_to_empty_not_an_error() {
        let stats: ComponentStats = serde_json::from_str("{}").unwrap();
        assert!(stats.rulesets.is_empty());
    }
}
