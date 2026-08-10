use std::collections::HashMap;

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::engine::CombatantState;

/// Identifies a combat-resolution model (design_doc.md §3.7). A plain string
/// rather than an enum, so adding a ruleset -- including one drawn from a
/// future source book -- never requires touching this type, only registering
/// a new `CombatResolver` in a `ResolverRegistry`. Mirrors `relationship_type`
/// in `usmf-core`, which makes the same "label is data" choice for the same
/// reason (see `RelationshipTypeSpec`).
pub type RulesetId = String;

/// `legacy_linear_v1`'s id, kept as a constant since it doubles as the
/// default `CombatantState::ruleset_id` for any combatant that hasn't opted
/// into a more specific ruleset.
pub const LEGACY_LINEAR_V1: &str = "legacy_linear_v1";

/// The id `aggregate_strength_v1` (issue #16) will register under, booked
/// ahead of time so `crate::spawn` can point `Aggregate`-granularity
/// combatants at it now instead of leaving them on `legacy_linear_v1`. No
/// resolver is registered for it yet -- an attack against one of these
/// combatants surfaces as an `ActionBlocked` "no CombatResolver registered"
/// event (`engine::apply_action`) until #16 lands.
pub const AGGREGATE_STRENGTH_V1: &str = "aggregate_strength_v1";

/// The id `cepheus_vehicle_v1` (issue #17) will register under -- same
/// forward-reference reasoning as `AGGREGATE_STRENGTH_V1`, for
/// `Individual`-granularity combatants whose Component data has a
/// `cepheus_vehicle_v1` entry.
pub const CEPHEUS_VEHICLE_V1: &str = "cepheus_vehicle_v1";

/// Read-only context an attack is resolved against. Currently just the range
/// the Attack action already computed before dispatching to a resolver --
/// kept as its own struct (rather than passing extra scalars alongside
/// attacker/defender) so a future resolver that needs more context (cover,
/// suppression -- design_doc.md §8) extends this in one place instead of
/// every `CombatResolver::resolve_attack` signature.
#[derive(Debug, Clone, Copy)]
pub struct CombatContext {
    pub range_hexes: u32,
}

/// What resolving one attack produced. Wide enough to cover every
/// granularity a ruleset might resolve against (design_doc.md §2.2, §3.7)
/// without any one resolver lying about another's shape: `legacy_linear_v1`
/// only ever produces `Miss`/`LegacyHit`; `IndividualHit`/`AggregateHit` exist
/// so `cepheus_vehicle_v1` (issue #17) and `aggregate_strength_v1` (issue
/// #16) have somewhere to land without another enum-shape change. The engine
/// doesn't yet know how to apply the latter two to a `CombatantState` --
/// that lands with whichever of #16/#17 first needs it.
#[derive(Debug, Clone, PartialEq)]
pub enum AttackOutcome {
    Miss,
    /// Flat hit-points loss -- what every attack resolves to today via
    /// `legacy_linear_v1`.
    LegacyHit {
        damage: f64,
    },
    /// Granular per-vehicle damage (`cepheus_vehicle_v1`, issue #17):
    /// depletes Hull Points then Structure Points; `component_effects` holds
    /// Component Damage Table results once that (separately-tracked, not yet
    /// designed) system exists.
    IndividualHit {
        hull_lost: u32,
        structure_lost: u32,
        component_effects: Vec<String>,
    },
    /// Aggregate strength-point attrition (`aggregate_strength_v1`, issue
    /// #16).
    AggregateHit {
        strength_lost: u32,
    },
}

/// A pluggable combat-resolution model (design_doc.md §3.7): its own to-hit
/// method and its own way of depleting a combatant's health pool. Dispatch is
/// keyed by the *defender's* `ruleset_id` (`CombatantState::ruleset_id`), not
/// the attacker's weapon, since a mixed engagement (e.g. an individually-
/// tracked vehicle firing into an aggregate stack) still needs exactly one
/// outcome shape to apply.
pub trait CombatResolver: Send + Sync {
    fn ruleset_id(&self) -> &str;

    fn resolve_attack(
        &self,
        attacker: &CombatantState,
        defender: &CombatantState,
        ctx: &CombatContext,
        rng: &mut ChaCha8Rng,
    ) -> AttackOutcome;
}

/// Range-scaled hit chance: linear falloff from a certain hit at range 0 to
/// a coin-flip at the weapon's max range. Shared by `LegacyLinearV1` and
/// `CepheusVehicleV1` (issue #17 kept this half of combat resolution as-is
/// per the original design conversation -- "core penetration only, keep the
/// existing continuous to-hit roll rather than the full 2D6 range-band
/// subsystem" -- only the *damage* side of `cepheus_vehicle_v1` is
/// Cepheus-derived).
fn range_scaled_hit_chance(range_hexes: u32, weapon_range_hexes: u32) -> f64 {
    1.0 - (range_hexes as f64 / weapon_range_hexes.max(1) as f64) * 0.5
}

/// Today's range-scaled hit chance against a flat `hit_points` pool -- the
/// default for any combatant without a more specific ruleset assigned, so
/// nothing regresses while granular/aggregate rulesets (issues #16, #17) are
/// rolled out incrementally. A straight extraction of what the Attack action
/// used to compute inline; behavior is unchanged.
pub struct LegacyLinearV1;

impl CombatResolver for LegacyLinearV1 {
    fn ruleset_id(&self) -> &str {
        LEGACY_LINEAR_V1
    }

    fn resolve_attack(
        &self,
        attacker: &CombatantState,
        _defender: &CombatantState,
        ctx: &CombatContext,
        rng: &mut ChaCha8Rng,
    ) -> AttackOutcome {
        let hit_chance = range_scaled_hit_chance(ctx.range_hexes, attacker.weapon_range_hexes);
        if rng.gen::<f64>() < hit_chance {
            AttackOutcome::LegacyHit {
                damage: attacker.weapon_damage,
            }
        } else {
            AttackOutcome::Miss
        }
    }
}

/// **PLACEHOLDER NUMBERS -- not a final design.** Classic wargame combat-
/// results-table shape (attacker:defender odds ratio -> a results row), per
/// `design_doc.md` §3.7. Issue #15 (`docs/combat-resolver-research.md`)
/// confirmed `old/`'s prior USMF rule iterations have nothing usable to seed
/// this from -- no CRT, no odds table, nothing -- so these are originated,
/// not migrated, and need a real balance/design pass before being treated as
/// final (tracked by issue #16). Six odds columns, each a monotonically
/// improving distribution over four outcomes: no effect, and the defender
/// losing a quarter/half/all of its current `strength_points`.
///
/// Each row is `(cumulative_probability, defender_strength_loss_fraction)`;
/// a uniform roll in `[0, 1)` picks the first row whose cumulative
/// probability exceeds it. Deliberately continuous (matching
/// `LegacyLinearV1`'s `rng.gen::<f64>()` style) rather than simulated
/// discrete dice -- there's no existing dice-rolling helper in this crate,
/// and one isn't worth introducing for numbers that are placeholders anyway.
const CRT: [[(f64, f64); 4]; 6] = [
    // 1:2 -- no effect .70, -25% .25, -50% .05, eliminated 0 (unreachable)
    [(0.70, 0.0), (0.95, 0.25), (1.00, 0.5), (1.00, 1.0)],
    // 1:1
    [(0.50, 0.0), (0.80, 0.25), (0.95, 0.5), (1.00, 1.0)],
    // 2:1
    [(0.30, 0.0), (0.60, 0.25), (0.85, 0.5), (1.00, 1.0)],
    // 3:1
    [(0.15, 0.0), (0.40, 0.25), (0.70, 0.5), (1.00, 1.0)],
    // 4:1
    [(0.05, 0.0), (0.20, 0.25), (0.50, 0.5), (1.00, 1.0)],
    // 5:1 and better
    [(0.00, 0.0), (0.10, 0.25), (0.35, 0.5), (1.00, 1.0)],
];

/// The odds ratio each `CRT` column represents -- a ratio is assigned to the
/// best (highest-index) column its ratio still meets or exceeds, floored to
/// the worst column (index 0, "1:2 or worse") rather than extrapolating
/// below it.
const ODDS_COLUMNS: [f64; 6] = [0.5, 1.0, 2.0, 3.0, 4.0, 5.0];

fn odds_column(ratio: f64) -> usize {
    ODDS_COLUMNS
        .iter()
        .rposition(|&threshold| ratio >= threshold)
        .unwrap_or(0)
}

fn defender_loss_fraction(ratio: f64, roll: f64) -> f64 {
    CRT[odds_column(ratio)]
        .iter()
        .find(|(cumulative, _)| roll < *cumulative)
        .map(|(_, loss_fraction)| *loss_fraction)
        .unwrap_or(1.0)
}

/// Aggregate-granularity counterpart to `LegacyLinearV1` (design_doc.md
/// §2.2, §3.7): a combat-power-ratio CRT instead of per-shot penetration,
/// depleting `strength_points` instead of `hit_points`. See `CRT`'s doc
/// comment for the placeholder-numbers caveat.
pub struct AggregateStrengthV1;

impl CombatResolver for AggregateStrengthV1 {
    fn ruleset_id(&self) -> &str {
        AGGREGATE_STRENGTH_V1
    }

    fn resolve_attack(
        &self,
        attacker: &CombatantState,
        defender: &CombatantState,
        _ctx: &CombatContext,
        rng: &mut ChaCha8Rng,
    ) -> AttackOutcome {
        // `strength_points: None` shouldn't happen here -- expand_placement
        // (issue #14) only ever assigns this ruleset to Aggregate-
        // granularity combatants, which always carry Some(_) -- but resolve
        // to a harmless Miss rather than panicking if some future caller
        // wires it up differently.
        let (Some(attacker_strength), Some(defender_strength)) =
            (attacker.strength_points, defender.strength_points)
        else {
            return AttackOutcome::Miss;
        };
        if defender_strength <= 0.0 {
            return AttackOutcome::Miss;
        }

        let ratio = attacker_strength / defender_strength;
        let roll = rng.gen::<f64>();
        let loss_fraction = defender_loss_fraction(ratio, roll);

        if loss_fraction <= 0.0 {
            AttackOutcome::Miss
        } else {
            let strength_lost = (defender_strength * loss_fraction).round().max(1.0) as u32;
            AttackOutcome::AggregateHit { strength_lost }
        }
    }
}

/// SAP ignores half its dice count in armor (rounded down); AP ignores its
/// dice count times its tier. Confirmed against the two worked examples from
/// the design conversation that produced this ruleset: 6D6 SAP ignores 3
/// (6/2); 9D6 AP3 ignores 27 (9*3) -- see `combat::tests` for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CepheusDamageType {
    Sap,
    Ap(u32),
}

/// A specific weapon's Cepheus-derived attack profile -- per-weapon data
/// (design_doc.md §2.1: "describes one specific component's own attack, not
/// something meaningful to sum across a whole loadout"), so this lives on
/// the individual `CombatantState` that fired it, not in the rolled-up
/// `CombatProfile` the way `armor`/`hull_points` do (both are plain numbers
/// and *are* summable, so those ride the existing rollup -- see
/// `CombatantState::armor`/`hull_points`/`structure_points`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CepheusWeapon {
    pub dice_count: u32,
    pub dice_sides: u32,
    pub damage_type: CepheusDamageType,
}

impl CepheusWeapon {
    fn armor_ignore(&self) -> u32 {
        match self.damage_type {
            CepheusDamageType::Sap => self.dice_count / 2,
            CepheusDamageType::Ap(tier) => self.dice_count * tier,
        }
    }

    fn roll_damage(&self, rng: &mut ChaCha8Rng) -> u32 {
        (0..self.dice_count)
            .map(|_| rng.gen_range(1..=self.dice_sides.max(1)))
            .sum()
    }
}

/// **PLACEHOLDER NUMBERS -- not a final design**, same caveat as
/// `AggregateStrengthV1`'s CRT: issue #15 confirmed `old/` has no
/// penetration table to seed this from, so these are originated, not
/// migrated (tracked by issue #17). `(upper_bound, hull_points_lost)` rows,
/// sorted ascending by `upper_bound`; a `penetration_damage` value picks the
/// first row whose bound it doesn't exceed, or the last row (the highest
/// bracket) if it exceeds them all.
const PENETRATION_TABLE: [(u32, u32); 6] =
    [(3, 1), (7, 2), (14, 4), (24, 8), (40, 15), (u32::MAX, 25)];

fn penetration_table_lookup(penetration_damage: u32) -> u32 {
    PENETRATION_TABLE
        .iter()
        .find(|(upper_bound, _)| penetration_damage <= *upper_bound)
        .map(|(_, hull_lost)| *hull_lost)
        .unwrap_or(25)
}

/// Granular, per-vehicle counterpart to `AggregateStrengthV1` (design_doc.md
/// §2.2, §3.7): roll the attacking weapon's damage dice, subtract SAP/AP
/// armor-ignore from the target's armor, subtract the remainder from the
/// roll, look up the result in `PENETRATION_TABLE` for Hull Point loss, then
/// spill anything Hull Points can't absorb into Structure Points. To-hit
/// itself is unchanged from `LegacyLinearV1` (`range_scaled_hit_chance`) --
/// only the damage/penetration side is Cepheus-derived, per the original
/// design conversation's scope decision. See `PENETRATION_TABLE`'s doc
/// comment for the placeholder-numbers caveat, and `CepheusWeapon`'s for why
/// the attacker's dice/type live on `CombatantState` directly rather than in
/// a rolled-up `CombatProfile`.
///
/// "Knocked Out" (Hull Points at 0) deliberately does *not* set
/// `destroyed` -- a hulled-out vehicle can still be hit again, spilling
/// further damage into Structure Points (design_doc.md §3.7's pipeline:
/// "Hull Points -> 0 = Knocked Out; further damage spills to Structure
/// Points -> 0 = Destroyed"). Only Structure Points reaching 0 destroys it;
/// see `engine::apply_action`'s `IndividualHit` handling.
pub struct CepheusVehicleV1;

impl CombatResolver for CepheusVehicleV1 {
    fn ruleset_id(&self) -> &str {
        CEPHEUS_VEHICLE_V1
    }

    fn resolve_attack(
        &self,
        attacker: &CombatantState,
        defender: &CombatantState,
        ctx: &CombatContext,
        rng: &mut ChaCha8Rng,
    ) -> AttackOutcome {
        // Shouldn't happen in practice -- expand_placement (#14/#17) only
        // assigns this ruleset to Individual-granularity combatants with
        // real weapon/armor/hull data -- but degrade to a harmless Miss
        // rather than panicking if some future caller wires it up
        // differently.
        let (Some(weapon), Some(armor), Some(hull_points), Some(structure_points)) = (
            attacker.weapon,
            defender.armor,
            defender.hull_points,
            defender.structure_points,
        ) else {
            return AttackOutcome::Miss;
        };
        if hull_points <= 0.0 && structure_points <= 0.0 {
            return AttackOutcome::Miss;
        }

        let hit_chance = range_scaled_hit_chance(ctx.range_hexes, attacker.weapon_range_hexes);
        if rng.gen::<f64>() >= hit_chance {
            return AttackOutcome::Miss;
        }

        let effective_armor = (armor - weapon.armor_ignore() as f64).max(0.0);
        let roll = weapon.roll_damage(rng) as f64;
        let penetration_damage = (roll - effective_armor).max(0.0) as u32;

        if penetration_damage == 0 {
            return AttackOutcome::Miss;
        }

        let raw_hull_damage = penetration_table_lookup(penetration_damage);
        // Can't lose more Hull Points than remain; the rest spills into
        // Structure Points.
        let hull_lost = raw_hull_damage.min(hull_points.max(0.0) as u32);
        let structure_lost = raw_hull_damage - hull_lost;

        AttackOutcome::IndividualHit {
            hull_lost,
            structure_lost,
            component_effects: Vec::new(),
        }
    }
}

/// Looks up the right `CombatResolver` for a defender's `ruleset_id`, built
/// once at engine init. Registering a ruleset from a new source book means
/// adding one more `Box<dyn CombatResolver>` here, not touching the Attack
/// action (design_doc.md §3.7).
#[derive(Default)]
pub struct ResolverRegistry {
    resolvers: HashMap<RulesetId, Box<dyn CombatResolver>>,
}

impl ResolverRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, resolver: Box<dyn CombatResolver>) {
        self.resolvers
            .insert(resolver.ruleset_id().to_string(), resolver);
    }

    pub fn get(&self, ruleset_id: &str) -> Option<&dyn CombatResolver> {
        self.resolvers.get(ruleset_id).map(|b| b.as_ref())
    }
}

/// The registry every simulation run should start from: `legacy_linear_v1`,
/// `aggregate_strength_v1`, and `cepheus_vehicle_v1` all registered.
pub fn default_registry() -> ResolverRegistry {
    let mut registry = ResolverRegistry::new();
    registry.register(Box::new(LegacyLinearV1));
    registry.register(Box::new(AggregateStrengthV1));
    registry.register(Box::new(CepheusVehicleV1));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::round_rng;
    use usmf_core::{Granularity, HexCoord};

    fn combatant(ruleset_id: &str) -> CombatantState {
        CombatantState {
            combatant_id: 1,
            source_unit_id: 1,
            side: "blue".to_string(),
            position: HexCoord::new(0, 0),
            base_initiative: 0.0,
            max_action_points: 10,
            action_points: 10,
            weapon_range_hexes: 4,
            weapon_damage: 50.0,
            attack_ap_cost: 4,
            hit_points: 100.0,
            destroyed: false,
            ruleset_id: ruleset_id.to_string(),
            granularity: Granularity::Individual,
            strength_points: None,
            armor: None,
            hull_points: None,
            structure_points: None,
            weapon: None,
        }
    }

    fn aggregate_combatant(strength_points: f64) -> CombatantState {
        CombatantState {
            granularity: Granularity::Aggregate,
            strength_points: Some(strength_points),
            ..combatant(AGGREGATE_STRENGTH_V1)
        }
    }

    fn cepheus_defender(armor: f64, hull_points: f64, structure_points: f64) -> CombatantState {
        CombatantState {
            armor: Some(armor),
            hull_points: Some(hull_points),
            structure_points: Some(structure_points),
            ..combatant(CEPHEUS_VEHICLE_V1)
        }
    }

    fn cepheus_attacker(weapon: CepheusWeapon) -> CombatantState {
        CombatantState {
            weapon: Some(weapon),
            ..combatant(LEGACY_LINEAR_V1) // attacker's own ruleset_id is irrelevant to dispatch
        }
    }

    #[test]
    fn default_registry_resolves_legacy_linear_v1() {
        let registry = default_registry();
        assert!(registry.get(LEGACY_LINEAR_V1).is_some());
        assert!(registry.get("does_not_exist").is_none());
    }

    #[test]
    fn default_registry_resolves_aggregate_strength_v1() {
        let registry = default_registry();
        assert!(registry.get(AGGREGATE_STRENGTH_V1).is_some());
    }

    #[test]
    fn odds_column_floors_to_the_worst_column_below_1_to_2() {
        assert_eq!(odds_column(0.1), 0);
        assert_eq!(odds_column(0.49), 0);
    }

    #[test]
    fn odds_column_picks_the_best_qualifying_column() {
        assert_eq!(odds_column(0.5), 0); // exactly 1:2
        assert_eq!(odds_column(0.99), 0); // just short of 1:1
        assert_eq!(odds_column(1.0), 1); // exactly 1:1
        assert_eq!(odds_column(2.5), 2); // between 2:1 and 3:1 -> floors to 2:1
        assert_eq!(odds_column(10.0), 5); // far past 5:1 -> caps at the last column
    }

    #[test]
    fn defender_loss_fraction_at_worst_odds_is_usually_no_effect() {
        // Column 0 (1:2): rows are (.70, 0.0), (.95, .25), (1.00, .5), (1.00, 1.0).
        assert_eq!(defender_loss_fraction(0.3, 0.0), 0.0);
        assert_eq!(defender_loss_fraction(0.3, 0.69), 0.0);
        assert_eq!(defender_loss_fraction(0.3, 0.80), 0.25);
        assert_eq!(defender_loss_fraction(0.3, 0.99), 0.5);
        // Elimination is unreachable at 1:2 -- the .5 row's cumulative
        // already reaches 1.00, so no roll in [0, 1) can fall through to it.
    }

    #[test]
    fn defender_loss_fraction_at_best_odds_never_no_effect() {
        // Column 5 (5:1+): rows are (0.0, 0.0), (.10, .25), (.35, .5), (1.00, 1.0)
        // -- the no-effect row's cumulative is 0.0, so it's unreachable too;
        // every roll in [0, 1) produces some loss.
        assert!(defender_loss_fraction(5.0, 0.0) > 0.0);
        assert!(defender_loss_fraction(5.0, 0.99) > 0.0);
        assert_eq!(defender_loss_fraction(5.0, 0.05), 0.25);
        assert_eq!(defender_loss_fraction(5.0, 0.20), 0.5);
        assert_eq!(defender_loss_fraction(5.0, 0.50), 1.0);
    }

    #[test]
    fn resolve_attack_at_overwhelming_odds_always_hits() {
        // ratio >= 5.0 guarantees loss_fraction > 0 (see the property test
        // above), so this holds for every RNG draw, not just this seed.
        let resolver = AggregateStrengthV1;
        let attacker = aggregate_combatant(1000.0);
        let defender = aggregate_combatant(10.0);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert!(
            matches!(outcome, AttackOutcome::AggregateHit { strength_lost } if strength_lost > 0)
        );
    }

    #[test]
    fn resolve_attack_against_destroyed_defender_is_a_miss() {
        let resolver = AggregateStrengthV1;
        let attacker = aggregate_combatant(10.0);
        let defender = aggregate_combatant(0.0);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert_eq!(outcome, AttackOutcome::Miss);
    }

    #[test]
    fn resolve_attack_without_strength_points_is_a_miss_not_a_panic() {
        // Shouldn't happen in practice (expand_placement always sets
        // strength_points for Aggregate combatants) -- guard against a
        // misconfigured caller rather than panicking mid-round.
        let resolver = AggregateStrengthV1;
        let attacker = combatant(AGGREGATE_STRENGTH_V1); // strength_points: None
        let defender = aggregate_combatant(10.0);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert_eq!(outcome, AttackOutcome::Miss);
    }

    #[test]
    fn legacy_linear_v1_hits_at_point_blank_range() {
        let resolver = LegacyLinearV1;
        let attacker = combatant(LEGACY_LINEAR_V1);
        let defender = combatant(LEGACY_LINEAR_V1);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        // At range 0, hit_chance = 1.0 - (0/4)*0.5 = 1.0 -- always a hit
        // regardless of the RNG draw.
        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert_eq!(
            outcome,
            AttackOutcome::LegacyHit {
                damage: attacker.weapon_damage
            }
        );
    }

    #[test]
    fn legacy_linear_v1_never_hits_beyond_double_weapon_range() {
        let resolver = LegacyLinearV1;
        let attacker = combatant(LEGACY_LINEAR_V1);
        let defender = combatant(LEGACY_LINEAR_V1);
        // hit_chance = 1.0 - (8/4)*0.5 = 0.0 -- always a miss.
        let ctx = CombatContext { range_hexes: 8 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert_eq!(outcome, AttackOutcome::Miss);
    }

    #[test]
    fn default_registry_resolves_cepheus_vehicle_v1() {
        let registry = default_registry();
        assert!(registry.get(CEPHEUS_VEHICLE_V1).is_some());
    }

    /// The two worked examples confirmed during the design conversation
    /// that produced this ruleset (design_doc.md §3.7): 6D6 SAP ignores 3
    /// armor (6/2); 9D6 AP3 ignores 27 (9*3).
    #[test]
    fn cepheus_armor_ignore_matches_the_two_worked_examples() {
        let sap = CepheusWeapon {
            dice_count: 6,
            dice_sides: 6,
            damage_type: CepheusDamageType::Sap,
        };
        assert_eq!(sap.armor_ignore(), 3);

        let ap3 = CepheusWeapon {
            dice_count: 9,
            dice_sides: 6,
            damage_type: CepheusDamageType::Ap(3),
        };
        assert_eq!(ap3.armor_ignore(), 27);
    }

    #[test]
    fn penetration_table_lookup_is_monotonic_and_caps_at_the_top_bracket() {
        assert_eq!(penetration_table_lookup(1), 1);
        assert_eq!(penetration_table_lookup(3), 1);
        assert_eq!(penetration_table_lookup(4), 2);
        assert_eq!(penetration_table_lookup(40), 15);
        assert_eq!(penetration_table_lookup(41), 25);
        assert_eq!(penetration_table_lookup(u32::MAX), 25);
    }

    #[test]
    fn cepheus_resolve_attack_stopped_cold_by_overwhelming_armor_is_always_a_miss() {
        // 6D6 rolls at most 36; armor 1000 with SAP's tiny ignore (3) makes
        // effective_armor far exceed any possible roll, so penetration_damage
        // is 0 regardless of the RNG draw.
        let resolver = CepheusVehicleV1;
        let attacker = cepheus_attacker(CepheusWeapon {
            dice_count: 6,
            dice_sides: 6,
            damage_type: CepheusDamageType::Sap,
        });
        let defender = cepheus_defender(1000.0, 20.0, 20.0);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert_eq!(outcome, AttackOutcome::Miss);
    }

    #[test]
    fn cepheus_resolve_attack_against_zero_armor_always_penetrates() {
        // 6D6's minimum roll is 6, always > 0 effective_armor, so this
        // always produces an IndividualHit regardless of the RNG draw.
        let resolver = CepheusVehicleV1;
        let attacker = cepheus_attacker(CepheusWeapon {
            dice_count: 6,
            dice_sides: 6,
            damage_type: CepheusDamageType::Sap,
        });
        let defender = cepheus_defender(0.0, 20.0, 20.0);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert!(matches!(
            outcome,
            AttackOutcome::IndividualHit { hull_lost, .. } if hull_lost > 0
        ));
    }

    #[test]
    fn cepheus_resolve_attack_spills_overflow_into_structure() {
        // Hull Points too low to absorb even the smallest table bracket (1)
        // -- whatever the roll, all of it spills into Structure Points.
        let resolver = CepheusVehicleV1;
        let attacker = cepheus_attacker(CepheusWeapon {
            dice_count: 6,
            dice_sides: 6,
            damage_type: CepheusDamageType::Sap,
        });
        let defender = cepheus_defender(0.0, 0.0, 30.0);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert!(matches!(
            outcome,
            AttackOutcome::IndividualHit { hull_lost: 0, structure_lost, .. } if structure_lost > 0
        ));
    }

    #[test]
    fn cepheus_resolve_attack_against_already_destroyed_defender_is_a_miss() {
        let resolver = CepheusVehicleV1;
        let attacker = cepheus_attacker(CepheusWeapon {
            dice_count: 6,
            dice_sides: 6,
            damage_type: CepheusDamageType::Sap,
        });
        let defender = cepheus_defender(0.0, 0.0, 0.0);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert_eq!(outcome, AttackOutcome::Miss);
    }

    #[test]
    fn cepheus_resolve_attack_without_weapon_or_armor_data_is_a_miss_not_a_panic() {
        let resolver = CepheusVehicleV1;
        let attacker = combatant(LEGACY_LINEAR_V1); // weapon: None
        let defender = cepheus_defender(10.0, 20.0, 20.0);
        let ctx = CombatContext { range_hexes: 0 };
        let mut rng = round_rng(1, 1);

        let outcome = resolver.resolve_attack(&attacker, &defender, &ctx, &mut rng);
        assert_eq!(outcome, AttackOutcome::Miss);
    }
}
