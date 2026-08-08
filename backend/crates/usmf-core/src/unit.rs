use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::asset::AssetTotals;
use crate::personnel::PersonnelTotals;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitType {
    Hq,
    Line,
    Support,
}

/// `Standing` units are the permanent force structure; `TaskForce` units are
/// stood up ad hoc for a mission (often around a borrowed HQ) and dissolved
/// afterward. Structurally both are just a `Unit` that gains subordinates
/// through `UnitRelationship`s — this tag is informational (reporting/UI),
/// not a different code path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationKind {
    Standing,
    TaskForce,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitAsset {
    pub asset_id: i64,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitPersonnelEntry {
    pub personnel_type_id: i64,
    pub quantity: u32,
}

/// A unit's personnel can be tracked as a bare headcount (`Simplified`) when
/// per-soldier detail isn't needed, or as a quantified list of `PersonnelType`s
/// (`Detailed`) when it is — e.g. a rifle squad as "8x Rifleman, 1x Squad Leader,"
/// each with its own loadout rolling up into the unit's weight/cost/capabilities.
///
/// Struct variants, not newtype variants (`Simplified { count }`, not
/// `Simplified(u32)`): serde's internal tagging (`tag = "mode"`, needed so the
/// JSON stays a flat, self-describing object) can only merge the tag into a
/// map-shaped variant body -- a newtype wrapping a bare integer has nowhere to
/// put it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PersonnelComposition {
    Simplified { count: u32 },
    Detailed { entries: Vec<UnitPersonnelEntry> },
}

impl Default for PersonnelComposition {
    fn default() -> Self {
        PersonnelComposition::Simplified { count: 0 }
    }
}

/// A node in the force structure. Deliberately holds no `parent_id`: composition
/// (its own Assets/personnel) and command relationships (who it reports to, who
/// reports to it) are both first-class and separate from this struct -- see
/// `UnitRelationship`. A unit can carry its own Assets/personnel *and* have
/// subordinates at the same time (e.g. a Company HQ's own vehicle/staff plus its
/// subordinate platoons).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    pub id: i64,
    pub name: String,
    pub unit_type: UnitType,
    #[serde(default = "default_formation_kind")]
    pub formation_kind: FormationKind,
    #[serde(default)]
    pub own_assets: Vec<UnitAsset>,
    #[serde(default)]
    pub personnel: PersonnelComposition,
    /// Direct-command capacity for span-of-control warnings. `None` = unlimited.
    #[serde(default)]
    pub c2_capacity: Option<u32>,
}

fn default_formation_kind() -> FormationKind {
    FormationKind::Standing
}

/// The doctrinal effect of a command relationship, expressed as data rather than
/// hardcoded per an enum match -- so new relationship types (beyond the standard
/// Organic/Attached/OPCON/TACON/Direct Support/General Support set) can be defined
/// without a code change (see `relationship_type_specs` in usmf-db). These consts
/// are just convenient presets matching current U.S. doctrine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipRules {
    /// Counted toward the superior's span-of-control and effective command tree.
    pub includes_in_span_of_control: bool,
    /// Logistics/sustainment responsibility moves from the unit's organic parent
    /// to the superior in this relationship.
    pub sustainment_transfers: bool,
    /// Counted toward the superior's aggregated combat power (weight/cost/
    /// capabilities) rollup.
    pub includes_in_combat_power_rollup: bool,
}

impl RelationshipRules {
    pub const ORGANIC: Self = Self {
        includes_in_span_of_control: true,
        sustainment_transfers: true,
        includes_in_combat_power_rollup: true,
    };
    pub const ATTACHED: Self = Self {
        includes_in_span_of_control: true,
        sustainment_transfers: true,
        includes_in_combat_power_rollup: true,
    };
    pub const OPCON: Self = Self {
        includes_in_span_of_control: true,
        sustainment_transfers: false,
        includes_in_combat_power_rollup: true,
    };
    pub const TACON: Self = Self {
        includes_in_span_of_control: true,
        sustainment_transfers: false,
        includes_in_combat_power_rollup: true,
    };
    pub const DIRECT_SUPPORT: Self = Self {
        includes_in_span_of_control: false,
        sustainment_transfers: false,
        includes_in_combat_power_rollup: false,
    };
    pub const GENERAL_SUPPORT: Self = Self {
        includes_in_span_of_control: false,
        sustainment_transfers: false,
        includes_in_combat_power_rollup: false,
    };
}

/// A named, storable `RelationshipRules` -- the domain shape of a row in the
/// `relationship_type_specs` table (usmf-db). Exists so callers get a typed
/// result from listing "what relationship types are configured" without
/// usmf-core depending on usmf-db.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipTypeSpec {
    pub name: String,
    pub rules: RelationshipRules,
}

/// A typed, time-bounded command relationship between two units. The permanent
/// TO&E tree is just every unit's `Organic` relationship to its parent formation;
/// everything else (Attached, OPCON, TACON, Direct Support, General Support, or a
/// user-defined label) layers temporary task organization on top without
/// disturbing that permanent structure. A Task Force is nothing more than a unit
/// that has gained others this way for a mission window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitRelationship {
    pub id: i64,
    pub superior_unit_id: i64,
    pub subordinate_unit_id: i64,
    /// Human-readable label ("Organic", "Attached", "OPCON", "TACON", "Direct
    /// Support", "General Support", or a custom one); `rules` carries its actual
    /// effect so the rollup logic here never needs to match on this string.
    pub relationship_type: String,
    pub rules: RelationshipRules,
    /// Turn number (or design-time sequence value) the relationship starts;
    /// `None` = already in effect / no defined start.
    pub effective_from_turn: Option<i64>,
    /// `None` = still active / open-ended (this is how a permanent Organic
    /// relationship is expressed -- no end).
    pub effective_until_turn: Option<i64>,
    #[serde(default)]
    pub notes: Option<String>,
}

impl UnitRelationship {
    /// `as_of: None` ignores time bounds entirely (useful for a full doctrinal/
    /// "what's configured" view at design time). `as_of: Some(turn)` gives the
    /// temporally accurate view during a running simulation.
    pub fn is_active_at(&self, as_of: Option<i64>) -> bool {
        match as_of {
            None => true,
            Some(t) => {
                self.effective_from_turn.is_none_or(|f| f <= t)
                    && self.effective_until_turn.is_none_or(|u| t < u)
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnitRollup {
    pub weight: f64,
    pub cost: f64,
    pub personnel_headcount: u32,
    pub daily_supply_consumption: f64,
    pub capabilities: HashMap<String, i32>,
    pub span_of_control_warnings: Vec<String>,
}

/// Baseline initiative for `usmf-sim`'s round loop (design_doc.md §3): the max
/// individual-item initiative total among this unit's own directly-held
/// assets/personnel -- "fastest/most alert element sets the pace," not a sum.
/// Deliberately *not* recursive: only units that carry their own composition
/// occupy a hex and take an initiative slot (§2.2), so a HQ's aggregate
/// subordinate tree is irrelevant here. Units with no composition of their own
/// (pure command nodes) return 0.0 and simply never enter the initiative order.
pub fn base_initiative(
    unit: &Unit,
    asset_totals: &HashMap<i64, AssetTotals>,
    personnel_totals: &HashMap<i64, PersonnelTotals>,
) -> f64 {
    let mut contributions: Vec<f64> = unit
        .own_assets
        .iter()
        .filter_map(|owned| asset_totals.get(&owned.asset_id))
        .map(|totals| totals.initiative)
        .collect();

    if let PersonnelComposition::Detailed { entries } = &unit.personnel {
        contributions.extend(
            entries
                .iter()
                .filter_map(|entry| personnel_totals.get(&entry.personnel_type_id))
                .map(|totals| totals.initiative),
        );
    }

    contributions.into_iter().reduce(f64::max).unwrap_or(0.0)
}

/// Recursively rolls up a unit's own composition plus every subordinate reached
/// through a `UnitRelationship` active at `as_of` and flagged
/// `includes_in_span_of_control` (this is the "effective command tree" for that
/// point in time, not just the permanent Organic tree). Combat power and
/// sustainment are folded in independently per relationship, per `RelationshipRules`
/// -- e.g. an OPCON subordinate's personnel/equipment count toward this unit's
/// combat power, but its logistics draw stays with its organic parent.
///
/// Known simplification: only relationships with `includes_in_span_of_control`
/// are traversed, so a hypothetical sustainment-only (no command) relationship
/// wouldn't be picked up here. Not needed by any relationship type in the current
/// doctrinal set; revisit if a future custom relationship type needs it.
pub fn rollup_unit(
    root_id: i64,
    units: &[Unit],
    relationships: &[UnitRelationship],
    as_of: Option<i64>,
    asset_totals: &HashMap<i64, AssetTotals>,
    personnel_totals: &HashMap<i64, PersonnelTotals>,
) -> UnitRollup {
    let by_id: HashMap<i64, &Unit> = units.iter().map(|u| (u.id, u)).collect();

    fn recurse(
        id: i64,
        by_id: &HashMap<i64, &Unit>,
        relationships: &[UnitRelationship],
        as_of: Option<i64>,
        asset_totals: &HashMap<i64, AssetTotals>,
        personnel_totals: &HashMap<i64, PersonnelTotals>,
    ) -> UnitRollup {
        let mut rollup = UnitRollup::default();
        let Some(unit) = by_id.get(&id) else {
            return rollup;
        };

        for owned in &unit.own_assets {
            if let Some(totals) = asset_totals.get(&owned.asset_id) {
                let qty = owned.quantity as f64;
                rollup.weight += totals.weight * qty;
                rollup.cost += totals.cost * qty;
                rollup.daily_supply_consumption += totals.power_draw * qty;
                for (tag, level) in &totals.capabilities {
                    *rollup.capabilities.entry(tag.clone()).or_insert(0) +=
                        level * owned.quantity as i32;
                }
            }
        }

        match &unit.personnel {
            PersonnelComposition::Simplified { count } => rollup.personnel_headcount += count,
            PersonnelComposition::Detailed { entries } => {
                for entry in entries {
                    rollup.personnel_headcount += entry.quantity;
                    if let Some(totals) = personnel_totals.get(&entry.personnel_type_id) {
                        let qty = entry.quantity as f64;
                        rollup.weight += totals.weight * qty;
                        rollup.cost += totals.cost * qty;
                        for (tag, level) in &totals.capabilities {
                            *rollup.capabilities.entry(tag.clone()).or_insert(0) +=
                                level * entry.quantity as i32;
                        }
                    }
                }
            }
        }

        let command_children: Vec<&UnitRelationship> = relationships
            .iter()
            .filter(|r| {
                r.superior_unit_id == id
                    && r.rules.includes_in_span_of_control
                    && r.is_active_at(as_of)
            })
            .collect();

        if let Some(capacity) = unit.c2_capacity {
            if command_children.len() as u32 > capacity {
                rollup.span_of_control_warnings.push(format!(
                    "{} has {} direct subordinates but C2 capacity {}",
                    unit.name,
                    command_children.len(),
                    capacity
                ));
            }
        }

        for rel in command_children {
            let child_rollup = recurse(
                rel.subordinate_unit_id,
                by_id,
                relationships,
                as_of,
                asset_totals,
                personnel_totals,
            );

            if rel.rules.includes_in_combat_power_rollup {
                rollup.weight += child_rollup.weight;
                rollup.cost += child_rollup.cost;
                rollup.personnel_headcount += child_rollup.personnel_headcount;
                for (tag, level) in child_rollup.capabilities {
                    *rollup.capabilities.entry(tag).or_insert(0) += level;
                }
            }
            if rel.rules.sustainment_transfers {
                rollup.daily_supply_consumption += child_rollup.daily_supply_consumption;
            }
            rollup
                .span_of_control_warnings
                .extend(child_rollup.span_of_control_warnings);
        }

        rollup
    }

    recurse(
        root_id,
        &by_id,
        relationships,
        as_of,
        asset_totals,
        personnel_totals,
    )
}

/// Returns the IDs of `root_id` and every unit reachable from it through the
/// same effective-command-tree traversal `rollup_unit` performs (relationships
/// active at `as_of` and flagged `includes_in_span_of_control`). Lets callers
/// (the API's rollup handler) scope per-unit lookups -- asset/personnel totals,
/// etc. -- to only the units that actually feed into a given rollup, instead of
/// every unit in the system.
pub fn effective_subtree_unit_ids(
    root_id: i64,
    units: &[Unit],
    relationships: &[UnitRelationship],
    as_of: Option<i64>,
) -> std::collections::HashSet<i64> {
    let known_ids: std::collections::HashSet<i64> = units.iter().map(|u| u.id).collect();
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![root_id];

    while let Some(id) = stack.pop() {
        if !known_ids.contains(&id) || !visited.insert(id) {
            continue;
        }
        for rel in relationships.iter().filter(|r| {
            r.superior_unit_id == id && r.rules.includes_in_span_of_control && r.is_active_at(as_of)
        }) {
            stack.push(rel.subordinate_unit_id);
        }
    }

    visited
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hq(id: i64, capacity: Option<u32>) -> Unit {
        Unit {
            id,
            name: format!("hq-{id}"),
            unit_type: UnitType::Hq,
            formation_kind: FormationKind::Standing,
            own_assets: vec![],
            personnel: PersonnelComposition::default(),
            c2_capacity: capacity,
        }
    }

    fn leaf(id: i64, asset_id: i64) -> Unit {
        Unit {
            id,
            name: format!("leaf-{id}"),
            unit_type: UnitType::Line,
            formation_kind: FormationKind::Standing,
            own_assets: vec![UnitAsset {
                asset_id,
                quantity: 1,
            }],
            personnel: PersonnelComposition::default(),
            c2_capacity: None,
        }
    }

    fn organic(id: i64, superior: i64, subordinate: i64) -> UnitRelationship {
        UnitRelationship {
            id,
            superior_unit_id: superior,
            subordinate_unit_id: subordinate,
            relationship_type: "Organic".into(),
            rules: RelationshipRules::ORGANIC,
            effective_from_turn: None,
            effective_until_turn: None,
            notes: None,
        }
    }

    #[test]
    fn base_initiative_takes_max_not_sum_across_own_assets() {
        let mut unit = leaf(1, 10);
        unit.own_assets.push(UnitAsset {
            asset_id: 11,
            quantity: 1,
        });
        let mut asset_totals = HashMap::new();
        asset_totals.insert(
            10,
            AssetTotals {
                initiative: 3.0,
                ..Default::default()
            },
        );
        asset_totals.insert(
            11,
            AssetTotals {
                initiative: 7.0,
                ..Default::default()
            },
        );

        assert_eq!(base_initiative(&unit, &asset_totals, &HashMap::new()), 7.0);
    }

    #[test]
    fn base_initiative_considers_detailed_personnel_too() {
        let mut unit = hq(1, None);
        unit.personnel = PersonnelComposition::Detailed {
            entries: vec![UnitPersonnelEntry {
                personnel_type_id: 20,
                quantity: 9,
            }],
        };
        let mut personnel_totals = HashMap::new();
        personnel_totals.insert(
            20,
            PersonnelTotals {
                initiative: 4.0,
                ..Default::default()
            },
        );

        assert_eq!(
            base_initiative(&unit, &HashMap::new(), &personnel_totals),
            4.0
        );
    }

    #[test]
    fn base_initiative_defaults_to_zero_for_pure_command_node() {
        let unit = hq(1, None);
        assert_eq!(
            base_initiative(&unit, &HashMap::new(), &HashMap::new()),
            0.0
        );
    }

    #[test]
    fn personnel_composition_round_trips_through_json() {
        // Regression test: PersonnelComposition previously used newtype variants
        // (`Simplified(u32)`) under `#[serde(tag = "mode")]` internal tagging,
        // which serde_json cannot represent (a bare integer has nowhere to merge
        // the tag) and panicked at serialization time. Struct variants fix it.
        let simplified = PersonnelComposition::Simplified { count: 9 };
        let json = serde_json::to_string(&simplified).unwrap();
        assert_eq!(json, r#"{"mode":"simplified","count":9}"#);
        let back: PersonnelComposition = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            PersonnelComposition::Simplified { count: 9 }
        ));

        let detailed = PersonnelComposition::Detailed {
            entries: vec![UnitPersonnelEntry {
                personnel_type_id: 1,
                quantity: 5,
            }],
        };
        let json = serde_json::to_string(&detailed).unwrap();
        let back: PersonnelComposition = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, PersonnelComposition::Detailed { entries } if entries.len() == 1));
    }

    #[test]
    fn rolls_up_leaf_totals_and_capabilities_through_organic_tree() {
        let units = vec![hq(1, Some(2)), leaf(2, 10), leaf(3, 11)];
        let relationships = vec![organic(1, 1, 2), organic(2, 1, 3)];
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

        let rollup = rollup_unit(
            1,
            &units,
            &relationships,
            None,
            &asset_totals,
            &HashMap::new(),
        );
        assert_eq!(rollup.weight, 8000.0);
        assert_eq!(rollup.cost, 1500.0);
        assert_eq!(rollup.capabilities.get("indirect_fire"), Some(&2));
        assert!(rollup.span_of_control_warnings.is_empty());
    }

    #[test]
    fn flags_span_of_control_violation() {
        let units = vec![hq(1, Some(1)), leaf(2, 10), leaf(3, 11)];
        let relationships = vec![organic(1, 1, 2), organic(2, 1, 3)];

        let rollup = rollup_unit(
            1,
            &units,
            &relationships,
            None,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(rollup.span_of_control_warnings.len(), 1);
    }

    #[test]
    fn opcon_subordinate_counts_for_combat_power_but_not_sustainment() {
        let units = vec![hq(1, Some(5)), leaf(2, 10)];
        let relationships = vec![UnitRelationship {
            id: 1,
            superior_unit_id: 1,
            subordinate_unit_id: 2,
            relationship_type: "OPCON".into(),
            rules: RelationshipRules::OPCON,
            effective_from_turn: None,
            effective_until_turn: None,
            notes: None,
        }];
        let mut asset_totals = HashMap::new();
        asset_totals.insert(
            10,
            AssetTotals {
                weight: 2000.0,
                cost: 400.0,
                power_draw: 50.0,
                ..Default::default()
            },
        );

        let rollup = rollup_unit(
            1,
            &units,
            &relationships,
            None,
            &asset_totals,
            &HashMap::new(),
        );
        assert_eq!(rollup.weight, 2000.0);
        assert_eq!(rollup.daily_supply_consumption, 0.0); // sustainment stays with organic parent
    }

    #[test]
    fn direct_support_unit_is_excluded_from_span_of_control_and_rollup() {
        let units = vec![hq(1, Some(5)), leaf(2, 10)];
        let relationships = vec![UnitRelationship {
            id: 1,
            superior_unit_id: 1,
            subordinate_unit_id: 2,
            relationship_type: "Direct Support".into(),
            rules: RelationshipRules::DIRECT_SUPPORT,
            effective_from_turn: None,
            effective_until_turn: None,
            notes: None,
        }];
        let mut asset_totals = HashMap::new();
        asset_totals.insert(
            10,
            AssetTotals {
                weight: 2000.0,
                ..Default::default()
            },
        );

        let rollup = rollup_unit(
            1,
            &units,
            &relationships,
            None,
            &asset_totals,
            &HashMap::new(),
        );
        assert_eq!(rollup.weight, 0.0);
    }

    #[test]
    fn temporary_attachment_only_active_within_its_turn_window() {
        let units = vec![hq(1, Some(5)), leaf(2, 10)];
        let relationships = vec![UnitRelationship {
            id: 1,
            superior_unit_id: 1,
            subordinate_unit_id: 2,
            relationship_type: "Attached".into(),
            rules: RelationshipRules::ATTACHED,
            effective_from_turn: Some(5),
            effective_until_turn: Some(10),
            notes: None,
        }];
        let mut asset_totals = HashMap::new();
        asset_totals.insert(
            10,
            AssetTotals {
                weight: 1000.0,
                ..Default::default()
            },
        );

        let before = rollup_unit(
            1,
            &units,
            &relationships,
            Some(3),
            &asset_totals,
            &HashMap::new(),
        );
        let during = rollup_unit(
            1,
            &units,
            &relationships,
            Some(7),
            &asset_totals,
            &HashMap::new(),
        );
        let after = rollup_unit(
            1,
            &units,
            &relationships,
            Some(10),
            &asset_totals,
            &HashMap::new(),
        );

        assert_eq!(before.weight, 0.0);
        assert_eq!(during.weight, 1000.0);
        assert_eq!(after.weight, 0.0); // effective_until is exclusive
    }

    #[test]
    fn unit_can_have_both_own_composition_and_subordinates() {
        let mut hq_unit = hq(1, Some(5));
        hq_unit.own_assets.push(UnitAsset {
            asset_id: 99,
            quantity: 1,
        });
        let units = vec![hq_unit, leaf(2, 10)];
        let relationships = vec![organic(1, 1, 2)];
        let mut asset_totals = HashMap::new();
        asset_totals.insert(
            99,
            AssetTotals {
                weight: 500.0,
                ..Default::default()
            },
        );
        asset_totals.insert(
            10,
            AssetTotals {
                weight: 2000.0,
                ..Default::default()
            },
        );

        let rollup = rollup_unit(
            1,
            &units,
            &relationships,
            None,
            &asset_totals,
            &HashMap::new(),
        );
        assert_eq!(rollup.weight, 2500.0);
    }

    #[test]
    fn effective_subtree_unit_ids_includes_root_and_reachable_units_only() {
        // 1 -Organic-> 2 -OPCON-> 3, plus an unrelated unit 4 that shouldn't appear.
        let units = vec![hq(1, None), leaf(2, 10), leaf(3, 10), leaf(4, 10)];
        let relationships = vec![
            organic(1, 1, 2),
            UnitRelationship {
                id: 2,
                superior_unit_id: 2,
                subordinate_unit_id: 3,
                relationship_type: "OPCON".into(),
                rules: RelationshipRules::OPCON,
                effective_from_turn: None,
                effective_until_turn: None,
                notes: None,
            },
        ];

        let ids = effective_subtree_unit_ids(1, &units, &relationships, None);
        assert_eq!(ids, [1, 2, 3].into_iter().collect());
    }

    #[test]
    fn effective_subtree_unit_ids_excludes_relationships_outside_span_of_control() {
        let units = vec![hq(1, None), leaf(2, 10)];
        let relationships = vec![UnitRelationship {
            id: 1,
            superior_unit_id: 1,
            subordinate_unit_id: 2,
            relationship_type: "Direct Support".into(),
            rules: RelationshipRules::DIRECT_SUPPORT,
            effective_from_turn: None,
            effective_until_turn: None,
            notes: None,
        }];

        let ids = effective_subtree_unit_ids(1, &units, &relationships, None);
        assert_eq!(ids, [1].into_iter().collect());
    }

    #[test]
    fn effective_subtree_unit_ids_respects_as_of_window() {
        let units = vec![hq(1, None), leaf(2, 10)];
        let relationships = vec![UnitRelationship {
            id: 1,
            superior_unit_id: 1,
            subordinate_unit_id: 2,
            relationship_type: "Attached".into(),
            rules: RelationshipRules::ATTACHED,
            effective_from_turn: Some(5),
            effective_until_turn: Some(10),
            notes: None,
        }];

        assert_eq!(
            effective_subtree_unit_ids(1, &units, &relationships, Some(3)),
            [1].into_iter().collect()
        );
        assert_eq!(
            effective_subtree_unit_ids(1, &units, &relationships, Some(7)),
            [1, 2].into_iter().collect()
        );
    }
}
