use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A ruleset's rolled-up combat picture: every numeric field contributed
/// under that ruleset id, summed across whatever it was rolled up from (see
/// `merge_combat_profiles`). Deliberately a flat map rather than a fixed
/// struct -- different rulesets need different, unpredictable fields (e.g.
/// `armor_front`/`hull_points` for a granular vehicle ruleset vs. a single
/// `combat_power` for an aggregate one), and `usmf-core` shouldn't have to
/// know a specific ruleset's field names to roll it up (design_doc.md §2.1,
/// §3.7).
pub type CombatProfile = HashMap<String, f64>;

/// Extracts every numeric leaf field from a JSON object into a flat
/// `CombatProfile`. Non-numeric fields (strings, bools, nested
/// objects/arrays -- e.g. a weapon's own `damage_dice`/`damage_type`) are
/// intentionally skipped: they describe one specific component's own
/// attack, not something meaningful to sum across a whole loadout. A
/// resolver that needs them reads them directly off that component's own
/// `stats.rulesets` entry instead of off a rolled-up profile. Not a JSON
/// object at all (or missing) produces an empty profile, not an error.
pub fn numeric_fields(value: &serde_json::Value) -> CombatProfile {
    let mut fields = CombatProfile::new();
    if let Some(obj) = value.as_object() {
        for (key, v) in obj {
            if let Some(n) = v.as_f64() {
                fields.insert(key.clone(), n);
            }
        }
    }
    fields
}

/// Scales every field in each of `profiles`' rulesets by `qty` and merges
/// the result into `accumulator`, summing into any existing entries. Shared
/// by every level that rolls up combat profiles -- per-component within
/// `validate_loadout`, per-Asset/PersonnelType and per-subordinate-unit
/// within `rollup_unit` -- so "sum, scaled by quantity" applies the same way
/// weight/cost/capabilities already do.
pub fn merge_combat_profiles(
    accumulator: &mut HashMap<String, CombatProfile>,
    profiles: &HashMap<String, CombatProfile>,
    qty: f64,
) {
    for (ruleset_id, profile) in profiles {
        let entry = accumulator.entry(ruleset_id.clone()).or_default();
        for (field, value) in profile {
            *entry.entry(field.clone()).or_insert(0.0) += value * qty;
        }
    }
}

/// A named, storable ruleset descriptor -- the domain shape of a row in the
/// `rulesets` table (usmf-db), mirroring `RelationshipTypeSpec`'s "typed
/// result without usmf-core depending on usmf-db" reasoning. Metadata only:
/// this catalogs what rulesets exist and what granularity they apply to for
/// UI/validation purposes. It does not gate which `CombatResolver`s
/// `usmf-sim::ResolverRegistry` actually registers -- that stays code-driven
/// (design_doc.md §3.7's "metadata is data, the resolution math is still
/// Rust" boundary).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesetSpec {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub source: Option<String>,
    pub supports_individual: bool,
    pub supports_aggregate: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numeric_fields_keeps_numbers_and_drops_everything_else() {
        let raw = json!({
            "damage_dice": "6D6",
            "damage_type": "Sap",
            "armor_front": 40.0,
            "crew": 3,
            "nested": { "x": 1 },
        });

        let fields = numeric_fields(&raw);
        assert_eq!(fields.get("armor_front"), Some(&40.0));
        assert_eq!(fields.get("crew"), Some(&3.0));
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn numeric_fields_of_non_object_is_empty() {
        assert!(numeric_fields(&json!("not an object")).is_empty());
    }

    #[test]
    fn merge_combat_profiles_scales_by_quantity_and_sums_across_calls() {
        let mut accumulator: HashMap<String, CombatProfile> = HashMap::new();
        let mut profiles: HashMap<String, CombatProfile> = HashMap::new();
        profiles.insert(
            "aggregate_strength_v1".to_string(),
            CombatProfile::from([("combat_power".to_string(), 3.0)]),
        );

        merge_combat_profiles(&mut accumulator, &profiles, 4.0);
        assert_eq!(accumulator["aggregate_strength_v1"]["combat_power"], 12.0);

        // A second contribution to the same ruleset/field sums rather than
        // overwrites.
        merge_combat_profiles(&mut accumulator, &profiles, 2.0);
        assert_eq!(accumulator["aggregate_strength_v1"]["combat_power"], 18.0);
    }
}
