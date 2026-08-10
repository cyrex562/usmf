use std::collections::HashMap;

use usmf_core::{
    base_initiative, rollup_unit, AssetTotals, CombatProfile, Granularity, HexCoord,
    PersonnelComposition, PersonnelTotals, Unit,
};

use crate::combat::{CepheusWeapon, AGGREGATE_STRENGTH_V1, CEPHEUS_VEHICLE_V1, LEGACY_LINEAR_V1};
use crate::engine::CombatantState;

/// Per-weapon `cepheus_vehicle_v1` (#17) attack data, keyed by whichever
/// Asset/PersonnelType carries it. `CepheusWeapon` is per-component,
/// non-summable data (see its doc comment) that can't ride the existing
/// `CombatProfile` rollup the way `armor`/`hull_points` do -- callers
/// assemble this once from real Asset/Component data, the same way
/// `asset_totals`/`personnel_totals` are pre-assembled rollups rather than
/// raw rows `expand_placement` looks up itself.
#[derive(Debug, Clone, Default)]
pub struct WeaponProfiles {
    pub assets: HashMap<i64, CepheusWeapon>,
    pub personnel: HashMap<i64, CepheusWeapon>,
}

/// Threshold (in expanded-instance count, `instance_count`) above which a
/// placement defaults to `Aggregate` granularity when the scenario doesn't
/// specify one explicitly (design_doc.md §2.2). A parameter, not a hardcoded
/// cutoff baked into `resolve_granularity` itself, so a caller can tune it.
pub const DEFAULT_AGGREGATE_THRESHOLD: u32 = 20;

/// Every `CombatantState` field that isn't yet derivable from Component
/// stats -- `max_action_points`/`attack_ap_cost`/weapon stats/`hit_points`
/// deriving from a specific weapon/chassis Component is a separate, still-
/// open question (design_doc.md §8), not something this issue's granularity
/// split resolves. Callers supply these until that lands; every instance
/// produced by one `expand_placement` call currently gets the same values.
#[derive(Debug, Clone, Copy)]
pub struct CombatDefaults {
    pub hit_points: f64,
    pub weapon_range_hexes: u32,
    pub weapon_damage: f64,
    pub attack_ap_cost: u32,
    pub max_action_points: u32,
}

/// How many individual instances (own_assets + detailed personnel) `unit`
/// would expand into under `Granularity::Individual`. Used both to pick a
/// default granularity (`resolve_granularity`) and, at `Individual`
/// expansion, to know how many `CombatantState`s to produce. `Simplified`
/// personnel contribute to the *count* here (so the default-threshold
/// decision still accounts for them) even though they can't individually
/// expand at `Individual` granularity (see `expand_placement`) -- there's no
/// `PersonnelType` to derive per-instance stats from.
pub fn instance_count(unit: &Unit) -> u32 {
    let assets: u32 = unit.own_assets.iter().map(|a| a.quantity).sum();
    let personnel: u32 = match &unit.personnel {
        PersonnelComposition::Simplified { count } => *count,
        PersonnelComposition::Detailed { entries } => entries.iter().map(|e| e.quantity).sum(),
    };
    assets + personnel
}

/// Resolves an explicit-or-default granularity for a placement (design_doc.md
/// §2.2): an explicit choice always wins; otherwise `Aggregate` above
/// `threshold` instances, `Individual` at or below it.
pub fn resolve_granularity(
    explicit: Option<Granularity>,
    unit: &Unit,
    threshold: u32,
) -> Granularity {
    explicit.unwrap_or_else(|| {
        if instance_count(unit) > threshold {
            Granularity::Aggregate
        } else {
            Granularity::Individual
        }
    })
}

/// Every combatant expanded from one unit occupies a distinct band of ids so
/// no two placements' combatants can collide regardless of instance count --
/// `Aggregate` placements just use instance index 0 within their own band,
/// rather than the bare unit id, so the id scheme doesn't need a special
/// case per granularity.
const INSTANCE_ID_STRIDE: i64 = 1_000_000;

fn combatant_id(unit_id: i64, instance_index: u32) -> i64 {
    unit_id * INSTANCE_ID_STRIDE + instance_index as i64
}

/// Which granular ruleset (if any) a specific Asset/PersonnelType's rolled-up
/// `CombatProfile` supports, for an `Individual`-granularity instance's
/// `ruleset_id`. Only `cepheus_vehicle_v1` exists to check for so far --
/// revisit with real priority rules if a second granular ruleset needs one.
fn individual_ruleset_id(totals: &usmf_core::LoadoutTotals) -> String {
    if totals.combat_profiles.contains_key(CEPHEUS_VEHICLE_V1) {
        CEPHEUS_VEHICLE_V1.to_string()
    } else {
        LEGACY_LINEAR_V1.to_string()
    }
}

/// Pulls `cepheus_vehicle_v1`'s three plain-number fields out of a rolled-up
/// `CombatProfile` -- these ride the existing rollup (unlike `CepheusWeapon`,
/// see `WeaponProfiles`) because they're summable, ordinary numbers, the
/// same as `armor`/`hull_points`/`structure_points`. `"armor"` is a single
/// value per vehicle, not faceted by firing arc -- the simplest option that
/// doesn't require the engine to model a facing/angle concept it doesn't
/// have yet.
fn cepheus_numeric_fields(profile: &CombatProfile) -> (Option<f64>, Option<f64>, Option<f64>) {
    (
        profile.get("armor").copied(),
        profile.get("hull_points").copied(),
        profile.get("structure_points").copied(),
    )
}

#[allow(clippy::too_many_arguments)]
fn combatant_from_totals(
    id: i64,
    source_unit_id: i64,
    side: &str,
    position: HexCoord,
    base_initiative: f64,
    ruleset_id: String,
    profile: Option<&CombatProfile>,
    weapon: Option<CepheusWeapon>,
    defaults: CombatDefaults,
) -> CombatantState {
    let (armor, hull_points, structure_points) = profile
        .map(cepheus_numeric_fields)
        .unwrap_or((None, None, None));

    CombatantState {
        combatant_id: id,
        source_unit_id,
        side: side.to_string(),
        position,
        base_initiative,
        max_action_points: defaults.max_action_points,
        action_points: defaults.max_action_points,
        weapon_range_hexes: defaults.weapon_range_hexes,
        weapon_damage: defaults.weapon_damage,
        attack_ap_cost: defaults.attack_ap_cost,
        hit_points: defaults.hit_points,
        destroyed: false,
        ruleset_id,
        granularity: Granularity::Individual,
        strength_points: None,
        armor,
        hull_points,
        structure_points,
        weapon,
    }
}

/// Expands one scenario placement into the `CombatantState`(s) it resolves
/// to at `Individual` or `Aggregate` granularity (design_doc.md §2.2).
///
/// - `Aggregate` produces exactly one `CombatantState`, its `strength_points`
///   seeded from `unit`'s own rolled-up `aggregate_strength_v1` combat power
///   (`rollup_unit` over just this unit with no subordinates -- a placed
///   unit is a leaf fighting element, not a whole subtree, per §2.2) --
///   reusing that existing aggregation rather than inventing a new number.
/// - `Individual` produces one `CombatantState` per owned Asset instance and
///   per Detailed personnel entry instance, each carrying its own
///   Asset's/PersonnelType's `CombatProfile`-derived `ruleset_id`.
///   `Simplified` personnel (a bare headcount, no `PersonnelType` to derive
///   per-instance stats from) don't produce individual combatants -- a unit
///   placed `Individual` with only `Simplified` personnel expands to just
///   its own_assets' instances, if any.
#[allow(clippy::too_many_arguments)]
pub fn expand_placement(
    unit: &Unit,
    side: &str,
    position: HexCoord,
    granularity: Granularity,
    asset_totals: &HashMap<i64, AssetTotals>,
    personnel_totals: &HashMap<i64, PersonnelTotals>,
    weapons: &WeaponProfiles,
    defaults: CombatDefaults,
) -> Vec<CombatantState> {
    let initiative = base_initiative(unit, asset_totals, personnel_totals);

    match granularity {
        Granularity::Aggregate => {
            let rollup = rollup_unit(
                unit.id,
                std::slice::from_ref(unit),
                &[],
                None,
                asset_totals,
                personnel_totals,
            );
            let strength_points = rollup
                .combat_profiles
                .get(AGGREGATE_STRENGTH_V1)
                .and_then(|profile| profile.get("combat_power"))
                .copied()
                .unwrap_or(0.0);

            let mut combatant = combatant_from_totals(
                combatant_id(unit.id, 0),
                unit.id,
                side,
                position,
                initiative,
                AGGREGATE_STRENGTH_V1.to_string(),
                None,
                None,
                defaults,
            );
            combatant.granularity = Granularity::Aggregate;
            combatant.strength_points = Some(strength_points);
            vec![combatant]
        }
        Granularity::Individual => {
            let mut instances = Vec::new();
            let mut index = 0u32;

            for owned in &unit.own_assets {
                let Some(totals) = asset_totals.get(&owned.asset_id) else {
                    continue;
                };
                let ruleset_id = individual_ruleset_id(totals);
                let profile = totals.combat_profiles.get(CEPHEUS_VEHICLE_V1);
                let weapon = weapons.assets.get(&owned.asset_id).copied();
                for _ in 0..owned.quantity {
                    instances.push(combatant_from_totals(
                        combatant_id(unit.id, index),
                        unit.id,
                        side,
                        position,
                        initiative,
                        ruleset_id.clone(),
                        profile,
                        weapon,
                        defaults,
                    ));
                    index += 1;
                }
            }

            if let PersonnelComposition::Detailed { entries } = &unit.personnel {
                for entry in entries {
                    let Some(totals) = personnel_totals.get(&entry.personnel_type_id) else {
                        continue;
                    };
                    let ruleset_id = individual_ruleset_id(totals);
                    let profile = totals.combat_profiles.get(CEPHEUS_VEHICLE_V1);
                    let weapon = weapons.personnel.get(&entry.personnel_type_id).copied();
                    for _ in 0..entry.quantity {
                        instances.push(combatant_from_totals(
                            combatant_id(unit.id, index),
                            unit.id,
                            side,
                            position,
                            initiative,
                            ruleset_id.clone(),
                            profile,
                            weapon,
                            defaults,
                        ));
                        index += 1;
                    }
                }
            }

            instances
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::CepheusDamageType;
    use std::collections::HashMap;
    use usmf_core::{
        FormationKind, PersonnelComposition, PersonnelTotals, UnitAsset, UnitPersonnelEntry,
        UnitType,
    };

    fn defaults() -> CombatDefaults {
        CombatDefaults {
            hit_points: 100.0,
            weapon_range_hexes: 4,
            weapon_damage: 20.0,
            attack_ap_cost: 4,
            max_action_points: 10,
        }
    }

    fn unit_with_assets(id: i64, asset_id: i64, quantity: u32) -> Unit {
        Unit {
            id,
            name: format!("unit-{id}"),
            unit_type: UnitType::Line,
            formation_kind: FormationKind::Standing,
            own_assets: vec![UnitAsset { asset_id, quantity }],
            personnel: PersonnelComposition::default(),
            c2_capacity: None,
        }
    }

    #[test]
    fn instance_count_sums_assets_and_personnel() {
        let mut unit = unit_with_assets(1, 10, 4);
        unit.personnel = PersonnelComposition::Detailed {
            entries: vec![UnitPersonnelEntry {
                personnel_type_id: 20,
                quantity: 9,
            }],
        };
        assert_eq!(instance_count(&unit), 13);
    }

    #[test]
    fn resolve_granularity_respects_explicit_override() {
        let unit = unit_with_assets(1, 10, 100);
        assert_eq!(
            resolve_granularity(Some(Granularity::Individual), &unit, 20),
            Granularity::Individual
        );
    }

    #[test]
    fn resolve_granularity_defaults_by_threshold() {
        let small = unit_with_assets(1, 10, 4);
        let large = unit_with_assets(2, 10, 40);
        assert_eq!(
            resolve_granularity(None, &small, 20),
            Granularity::Individual
        );
        assert_eq!(
            resolve_granularity(None, &large, 20),
            Granularity::Aggregate
        );
    }

    #[test]
    fn aggregate_placement_produces_one_combatant_with_rolled_up_strength() {
        let unit = unit_with_assets(1, 10, 4);
        let mut asset_totals = HashMap::new();
        asset_totals.insert(
            10,
            AssetTotals {
                combat_profiles: HashMap::from([(
                    AGGREGATE_STRENGTH_V1.to_string(),
                    HashMap::from([("combat_power".to_string(), 3.0)]),
                )]),
                ..Default::default()
            },
        );

        let combatants = expand_placement(
            &unit,
            "blue",
            HexCoord::new(0, 0),
            Granularity::Aggregate,
            &asset_totals,
            &HashMap::new(),
            &WeaponProfiles::default(),
            defaults(),
        );

        assert_eq!(combatants.len(), 1);
        let combatant = &combatants[0];
        assert_eq!(combatant.granularity, Granularity::Aggregate);
        // 4 assets * 3.0 combat_power each, already scaled by quantity in
        // AssetTotals -> UnitRollup (usmf-core::loadout/unit).
        assert_eq!(combatant.strength_points, Some(12.0));
        assert_eq!(combatant.source_unit_id, unit.id);
        assert_eq!(combatant.ruleset_id, AGGREGATE_STRENGTH_V1);
    }

    #[test]
    fn individual_placement_produces_one_combatant_per_asset_instance() {
        let unit = unit_with_assets(1, 10, 4);
        let mut asset_totals = HashMap::new();
        asset_totals.insert(
            10,
            AssetTotals {
                combat_profiles: HashMap::from([(
                    CEPHEUS_VEHICLE_V1.to_string(),
                    HashMap::from([
                        ("armor".to_string(), 40.0),
                        ("hull_points".to_string(), 20.0),
                        ("structure_points".to_string(), 10.0),
                    ]),
                )]),
                ..Default::default()
            },
        );
        let weapons = WeaponProfiles {
            assets: HashMap::from([(
                10,
                CepheusWeapon {
                    dice_count: 6,
                    dice_sides: 6,
                    damage_type: CepheusDamageType::Sap,
                },
            )]),
            personnel: HashMap::new(),
        };

        let combatants = expand_placement(
            &unit,
            "blue",
            HexCoord::new(0, 0),
            Granularity::Individual,
            &asset_totals,
            &HashMap::new(),
            &weapons,
            defaults(),
        );

        assert_eq!(combatants.len(), 4);
        let ids: std::collections::HashSet<i64> =
            combatants.iter().map(|c| c.combatant_id).collect();
        assert_eq!(ids.len(), 4, "every instance gets a distinct combatant_id");
        for combatant in &combatants {
            assert_eq!(combatant.granularity, Granularity::Individual);
            assert_eq!(combatant.source_unit_id, unit.id);
            assert_eq!(combatant.ruleset_id, CEPHEUS_VEHICLE_V1);
            assert_eq!(combatant.strength_points, None);
            assert_eq!(combatant.armor, Some(40.0));
            assert_eq!(combatant.hull_points, Some(20.0));
            assert_eq!(combatant.structure_points, Some(10.0));
            assert!(combatant.weapon.is_some());
        }
    }

    #[test]
    fn individual_asset_with_no_ruleset_data_falls_back_to_legacy() {
        let unit = unit_with_assets(1, 10, 1);
        let mut asset_totals = HashMap::new();
        asset_totals.insert(10, AssetTotals::default());

        let combatants = expand_placement(
            &unit,
            "blue",
            HexCoord::new(0, 0),
            Granularity::Individual,
            &asset_totals,
            &HashMap::new(),
            &WeaponProfiles::default(),
            defaults(),
        );

        assert_eq!(combatants[0].ruleset_id, LEGACY_LINEAR_V1);
        assert_eq!(combatants[0].armor, None);
        assert!(combatants[0].weapon.is_none());
    }

    #[test]
    fn individual_placement_includes_detailed_personnel_instances() {
        let mut unit = unit_with_assets(1, 10, 1);
        unit.personnel = PersonnelComposition::Detailed {
            entries: vec![UnitPersonnelEntry {
                personnel_type_id: 20,
                quantity: 2,
            }],
        };
        let mut asset_totals = HashMap::new();
        asset_totals.insert(10, AssetTotals::default());
        let mut personnel_totals = HashMap::new();
        personnel_totals.insert(20, PersonnelTotals::default());

        let combatants = expand_placement(
            &unit,
            "blue",
            HexCoord::new(0, 0),
            Granularity::Individual,
            &asset_totals,
            &personnel_totals,
            &WeaponProfiles::default(),
            defaults(),
        );

        // 1 asset instance + 2 personnel instances.
        assert_eq!(combatants.len(), 3);
    }

    #[test]
    fn individual_placement_skips_simplified_personnel() {
        let mut unit = unit_with_assets(1, 10, 1);
        unit.personnel = PersonnelComposition::Simplified { count: 50 };
        let mut asset_totals = HashMap::new();
        asset_totals.insert(10, AssetTotals::default());

        let combatants = expand_placement(
            &unit,
            "blue",
            HexCoord::new(0, 0),
            Granularity::Individual,
            &asset_totals,
            &HashMap::new(),
            &WeaponProfiles::default(),
            defaults(),
        );

        // Only the one asset instance -- no PersonnelType to derive the 50
        // Simplified personnel's individual stats from.
        assert_eq!(combatants.len(), 1);
    }

    #[test]
    fn individual_placement_threads_personnel_weapon_profile() {
        let unit = Unit {
            id: 1,
            name: "unit-1".to_string(),
            unit_type: UnitType::Line,
            formation_kind: FormationKind::Standing,
            own_assets: vec![],
            personnel: PersonnelComposition::Detailed {
                entries: vec![UnitPersonnelEntry {
                    personnel_type_id: 20,
                    quantity: 1,
                }],
            },
            c2_capacity: None,
        };
        let mut personnel_totals = HashMap::new();
        personnel_totals.insert(
            20,
            PersonnelTotals {
                combat_profiles: HashMap::from([(
                    CEPHEUS_VEHICLE_V1.to_string(),
                    HashMap::from([("armor".to_string(), 5.0)]),
                )]),
                ..Default::default()
            },
        );
        let weapons = WeaponProfiles {
            assets: HashMap::new(),
            personnel: HashMap::from([(
                20,
                CepheusWeapon {
                    dice_count: 3,
                    dice_sides: 6,
                    damage_type: CepheusDamageType::Ap(2),
                },
            )]),
        };

        let combatants = expand_placement(
            &unit,
            "blue",
            HexCoord::new(0, 0),
            Granularity::Individual,
            &HashMap::new(),
            &personnel_totals,
            &weapons,
            defaults(),
        );

        assert_eq!(combatants.len(), 1);
        assert_eq!(combatants[0].ruleset_id, CEPHEUS_VEHICLE_V1);
        assert_eq!(combatants[0].armor, Some(5.0));
        assert_eq!(
            combatants[0].weapon,
            Some(CepheusWeapon {
                dice_count: 3,
                dice_sides: 6,
                damage_type: CepheusDamageType::Ap(2),
            })
        );
    }
}
