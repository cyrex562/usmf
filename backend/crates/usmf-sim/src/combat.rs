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
        let hit_chance =
            1.0 - (ctx.range_hexes as f64 / attacker.weapon_range_hexes.max(1) as f64) * 0.5;
        if rng.gen::<f64>() < hit_chance {
            AttackOutcome::LegacyHit {
                damage: attacker.weapon_damage,
            }
        } else {
            AttackOutcome::Miss
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

/// The registry every simulation run should start from: `legacy_linear_v1`
/// registered, ready for `aggregate_strength_v1`/`cepheus_vehicle_v1`
/// (issues #16, #17) to be added the same way once their numbers are
/// confirmed.
pub fn default_registry() -> ResolverRegistry {
    let mut registry = ResolverRegistry::new();
    registry.register(Box::new(LegacyLinearV1));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::round_rng;
    use usmf_core::HexCoord;

    fn combatant(ruleset_id: &str) -> CombatantState {
        CombatantState {
            unit_id: 1,
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
        }
    }

    #[test]
    fn default_registry_resolves_legacy_linear_v1() {
        let registry = default_registry();
        assert!(registry.get(LEGACY_LINEAR_V1).is_some());
        assert!(registry.get("does_not_exist").is_none());
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
}
