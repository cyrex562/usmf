use std::collections::HashMap;

use rand::Rng;
use rand_chacha::ChaCha8Rng;
use usmf_core::{Granularity, HexCoord, Map};

use crate::combat::{AttackOutcome, CepheusWeapon, CombatContext, ResolverRegistry, RulesetId};
use crate::los::has_line_of_sight;
use crate::pathfinding::{find_path, path_cost};

#[derive(Debug, Clone)]
pub enum Action {
    Move { to: HexCoord },
    Attack { target_combatant_id: i64 },
    Pass,
}

#[derive(Debug, Clone)]
pub struct CombatantState {
    /// The engine's own identity for this combatant -- also the key it's
    /// stored under in `resolve_round`'s `states` map. Not necessarily equal
    /// to `source_unit_id`: under `Granularity::Individual` (design_doc.md
    /// §2.2), one Unit placement expands into several `CombatantState`s that
    /// all share a `source_unit_id` but each need their own distinct
    /// `combatant_id` (see `crate::spawn`).
    pub combatant_id: i64,
    /// Which design-time Unit (TO&E entry) this combatant was expanded from
    /// -- for tracing/UI, not consulted by the round engine itself.
    pub source_unit_id: i64,
    pub side: String,
    pub position: HexCoord,
    /// Composition-derived baseline from `usmf_core::base_initiative`; combined
    /// with a fresh random draw each round to produce that round's activation
    /// order (design_doc.md §3).
    pub base_initiative: f64,
    pub max_action_points: u32,
    /// Reset to `max_action_points` at the start of every round, then spent as
    /// this unit takes actions during its turn.
    pub action_points: u32,
    pub weapon_range_hexes: u32,
    pub weapon_damage: f64,
    pub attack_ap_cost: u32,
    pub hit_points: f64,
    pub destroyed: bool,
    /// Which `CombatResolver` (§3.7) applies when this combatant is on the
    /// *defending* side of an attack -- dispatch is keyed by the defender,
    /// not the attacker's weapon (`crate::combat`). Defaults to
    /// `LEGACY_LINEAR_V1` for every combatant that hasn't opted into a more
    /// specific ruleset, so today's behavior is unchanged until #16/#17 land.
    pub ruleset_id: RulesetId,
    /// Individual vs. aggregate resolution (design_doc.md §2.2) -- set once
    /// at scenario-start expansion (`crate::spawn::expand_placement`) and
    /// read by whichever `CombatResolver` this combatant's `ruleset_id`
    /// dispatches to.
    pub granularity: Granularity,
    /// Aggregate-granularity strength-point pool, seeded from the placed
    /// Unit's rolled-up combat power (`crate::spawn`). `None` for
    /// `Individual`-granularity combatants, which fight off `hit_points`
    /// (or, under `cepheus_vehicle_v1`, `hull_points`/`structure_points`)
    /// instead. Depleted by `AttackOutcome::AggregateHit` (`aggregate_strength_v1`,
    /// #16) below.
    pub strength_points: Option<f64>,
    /// `cepheus_vehicle_v1` (#17) defender pools: current armor, Hull
    /// Points, and Structure Points. All three ride the existing
    /// `CombatProfile` rollup (they're plain numbers, unlike `weapon` below)
    /// -- populated at expansion time (`crate::spawn`) from whichever
    /// Component contributed `cepheus_vehicle_v1` data. `None` for any
    /// combatant not using that ruleset.
    pub armor: Option<f64>,
    pub hull_points: Option<f64>,
    pub structure_points: Option<f64>,
    /// `cepheus_vehicle_v1` (#17) attacker data: the specific weapon this
    /// combatant fires with. Per-component, not summable (`CepheusWeapon`'s
    /// doc comment), so unlike `armor`/`hull_points` it isn't part of the
    /// rolled-up `CombatProfile` -- `crate::spawn` threads it through
    /// separately. `None` for any combatant not using that ruleset, or with
    /// no weapon Component carrying `cepheus_vehicle_v1` data.
    pub weapon: Option<CepheusWeapon>,
}

#[derive(Debug, Clone)]
pub enum RoundEvent {
    InitiativeRolled {
        order: Vec<i64>,
    },
    TurnStarted {
        combatant_id: i64,
        action_points: u32,
    },
    Moved {
        combatant_id: i64,
        path: Vec<HexCoord>,
        ap_spent: u32,
    },
    AttackResolved {
        attacker_combatant_id: i64,
        target_combatant_id: i64,
        hit: bool,
        damage: f64,
    },
    UnitDestroyed {
        combatant_id: i64,
    },
    UnitPassed {
        combatant_id: i64,
    },
    ActionBlocked {
        combatant_id: i64,
        reason: String,
    },
}

/// This round's activation order: `base_initiative` plus a fresh random draw per
/// combatant (in combatant-id order, so the draw sequence -- and therefore the
/// result -- is reproducible regardless of `HashMap` iteration order), highest
/// effective initiative first. Ties break on combatant_id ascending, also for
/// reproducibility. Destroyed combatants are excluded. Recalculated fresh every
/// round per design_doc.md §3, so casualties/suppression/morale shifts can
/// reshuffle who acts when.
pub fn roll_initiative(states: &HashMap<i64, CombatantState>, rng: &mut ChaCha8Rng) -> Vec<i64> {
    let mut combatant_ids: Vec<i64> = states
        .values()
        .filter(|s| !s.destroyed)
        .map(|s| s.combatant_id)
        .collect();
    combatant_ids.sort_unstable();

    let mut rolled: Vec<(i64, f64)> = combatant_ids
        .into_iter()
        .map(|id| {
            let roll: f64 = rng.gen_range(0.0..10.0);
            (id, states[&id].base_initiative + roll)
        })
        .collect();

    rolled.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    rolled.into_iter().map(|(id, _)| id).collect()
}

/// Runs one full round: resets every living combatant's action points, rolls
/// this round's initiative order, then gives each living combatant its slice
/// of time in that order. A unit with a queued `overrides` entry executes those
/// actions in order (stopping early if AP runs out or an action doesn't apply);
/// otherwise a simple default AI decides its actions -- engage the nearest
/// enemy if in range and LOS, else advance toward the nearest enemy with
/// whatever AP remains, else pass. This is deliberately simple ("hybrid"
/// control per design_doc.md §3: AI by default, explicit player override per
/// activation via `overrides`); a richer standing-orders/rules-of-engagement
/// system is future work, not needed for this vertical slice.
pub fn resolve_round(
    map: &Map,
    states: &mut HashMap<i64, CombatantState>,
    overrides: &HashMap<i64, Vec<Action>>,
    registry: &ResolverRegistry,
    rng: &mut ChaCha8Rng,
) -> Vec<RoundEvent> {
    let mut events = Vec::new();

    for state in states.values_mut() {
        if !state.destroyed {
            state.action_points = state.max_action_points;
        }
    }

    let order = roll_initiative(states, rng);
    events.push(RoundEvent::InitiativeRolled {
        order: order.clone(),
    });

    for combatant_id in order {
        let Some(state) = states.get(&combatant_id) else {
            continue;
        };
        if state.destroyed {
            continue;
        }
        events.push(RoundEvent::TurnStarted {
            combatant_id,
            action_points: state.action_points,
        });

        if let Some(queue) = overrides.get(&combatant_id) {
            for action in queue {
                if states[&combatant_id].destroyed {
                    break;
                }
                let consumed = apply_action(
                    map,
                    states,
                    combatant_id,
                    action.clone(),
                    registry,
                    rng,
                    &mut events,
                );
                if !consumed {
                    break;
                }
            }
        } else {
            loop {
                let state = &states[&combatant_id];
                if state.destroyed || state.action_points == 0 {
                    break;
                }
                let action = decide_ai_action(combatant_id, states, map);
                let is_pass = matches!(action, Action::Pass);
                let consumed = apply_action(
                    map,
                    states,
                    combatant_id,
                    action,
                    registry,
                    rng,
                    &mut events,
                );
                if is_pass || !consumed {
                    break;
                }
            }
        }
    }

    events
}

fn apply_action(
    map: &Map,
    states: &mut HashMap<i64, CombatantState>,
    combatant_id: i64,
    action: Action,
    registry: &ResolverRegistry,
    rng: &mut ChaCha8Rng,
    events: &mut Vec<RoundEvent>,
) -> bool {
    match action {
        Action::Pass => {
            events.push(RoundEvent::UnitPassed { combatant_id });
            false
        }
        Action::Move { to } => {
            let (from, ap_available) = {
                let state = &states[&combatant_id];
                (state.position, state.action_points)
            };
            match find_path(map, from, to, ap_available) {
                Some(path) => {
                    let cost = path_cost(map, &path);
                    let state = states.get_mut(&combatant_id).unwrap();
                    state.position = to;
                    state.action_points = state.action_points.saturating_sub(cost);
                    events.push(RoundEvent::Moved {
                        combatant_id,
                        path,
                        ap_spent: cost,
                    });
                    true
                }
                None => {
                    events.push(RoundEvent::ActionBlocked {
                        combatant_id,
                        reason: "no reachable path within remaining action points".to_string(),
                    });
                    false
                }
            }
        }
        Action::Attack {
            target_combatant_id,
        } => {
            let Some(attacker) = states.get(&combatant_id).cloned() else {
                return false;
            };
            if attacker.action_points < attacker.attack_ap_cost {
                events.push(RoundEvent::ActionBlocked {
                    combatant_id,
                    reason: "insufficient action points to attack".to_string(),
                });
                return false;
            }
            let Some(target) = states.get(&target_combatant_id).cloned() else {
                return false;
            };
            if attacker.destroyed || target.destroyed {
                return false;
            }

            let range = attacker.position.distance(&target.position);
            if range > attacker.weapon_range_hexes
                || !has_line_of_sight(map, attacker.position, target.position)
            {
                events.push(RoundEvent::ActionBlocked {
                    combatant_id,
                    reason: "target out of range or line of sight".to_string(),
                });
                return false;
            }

            let Some(resolver) = registry.get(&target.ruleset_id) else {
                events.push(RoundEvent::ActionBlocked {
                    combatant_id,
                    reason: format!(
                        "no CombatResolver registered for ruleset '{}'",
                        target.ruleset_id
                    ),
                });
                return false;
            };
            let ctx = CombatContext { range_hexes: range };
            let outcome = resolver.resolve_attack(&attacker, &target, &ctx, rng);
            let (hit, damage) = match &outcome {
                AttackOutcome::Miss => (false, 0.0),
                AttackOutcome::LegacyHit { damage } => (true, *damage),
                AttackOutcome::AggregateHit { strength_lost } => (true, *strength_lost as f64),
                // Total points of damage dealt across both pools, for the
                // event log -- actual per-pool application happens below,
                // where hull_lost/structure_lost are applied individually.
                AttackOutcome::IndividualHit {
                    hull_lost,
                    structure_lost,
                    ..
                } => (true, (*hull_lost + *structure_lost) as f64),
            };

            if let Some(attacker_state) = states.get_mut(&combatant_id) {
                attacker_state.action_points = attacker_state
                    .action_points
                    .saturating_sub(attacker.attack_ap_cost);
            }

            events.push(RoundEvent::AttackResolved {
                attacker_combatant_id: combatant_id,
                target_combatant_id,
                hit,
                damage,
            });

            if hit {
                if let Some(target_state) = states.get_mut(&target_combatant_id) {
                    // Each granularity/ruleset fights off its own pool
                    // (design_doc.md §2.2) -- deplete whichever one this
                    // outcome actually represents. `cepheus_vehicle_v1`
                    // (#17): Hull Points reaching 0 is "Knocked Out," not
                    // destroyed -- it can still be hit again, spilling
                    // further damage into Structure Points -- so only
                    // Structure Points reaching 0 destroys it.
                    let destroyed_now = match &outcome {
                        AttackOutcome::AggregateHit { .. } => {
                            let remaining = target_state.strength_points.unwrap_or(0.0) - damage;
                            target_state.strength_points = Some(remaining);
                            remaining <= 0.0
                        }
                        AttackOutcome::IndividualHit {
                            hull_lost,
                            structure_lost,
                            ..
                        } => {
                            let hull_remaining = (target_state.hull_points.unwrap_or(0.0)
                                - *hull_lost as f64)
                                .max(0.0);
                            target_state.hull_points = Some(hull_remaining);
                            let structure_remaining = target_state.structure_points.unwrap_or(0.0)
                                - *structure_lost as f64;
                            target_state.structure_points = Some(structure_remaining);
                            structure_remaining <= 0.0
                        }
                        _ => {
                            target_state.hit_points -= damage;
                            target_state.hit_points <= 0.0
                        }
                    };
                    if destroyed_now && !target_state.destroyed {
                        target_state.destroyed = true;
                        events.push(RoundEvent::UnitDestroyed {
                            combatant_id: target_combatant_id,
                        });
                    }
                }
            }

            true
        }
    }
}

const GENEROUS_MOVE_CAP: u32 = 10_000;

/// Default AI: attack the nearest enemy if it's in range and LOS; otherwise
/// advance as far toward the nearest enemy as this turn's remaining AP allows;
/// otherwise pass. See `resolve_round`'s doc comment for how this fits the
/// hybrid AI/override control model.
fn decide_ai_action(combatant_id: i64, states: &HashMap<i64, CombatantState>, map: &Map) -> Action {
    let me = &states[&combatant_id];

    let mut enemies: Vec<&CombatantState> = states
        .values()
        .filter(|s| !s.destroyed && s.side != me.side)
        .collect();
    enemies.sort_by_key(|e| (me.position.distance(&e.position), e.combatant_id));

    let Some(target) = enemies.first() else {
        return Action::Pass;
    };

    let range = me.position.distance(&target.position);
    if range <= me.weapon_range_hexes
        && me.action_points >= me.attack_ap_cost
        && has_line_of_sight(map, me.position, target.position)
    {
        return Action::Attack {
            target_combatant_id: target.combatant_id,
        };
    }

    if let Some(full_path) = find_path(map, me.position, target.position, GENEROUS_MOVE_CAP) {
        let reachable = truncate_path_to_budget(map, &full_path, me.action_points);
        if reachable.len() > 1 {
            return Action::Move {
                to: *reachable.last().unwrap(),
            };
        }
    }

    Action::Pass
}

fn truncate_path_to_budget(map: &Map, path: &[HexCoord], budget: u32) -> Vec<HexCoord> {
    let mut truncated = vec![path[0]];
    let mut spent = 0u32;
    for coord in path.iter().skip(1) {
        let Some(cell) = map.cell_at(coord) else {
            break;
        };
        let cost = cell.terrain.movement_cost();
        if spent + cost > budget {
            break;
        }
        spent += cost;
        truncated.push(*coord);
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{
        default_registry, CepheusDamageType, AGGREGATE_STRENGTH_V1, CEPHEUS_VEHICLE_V1,
        LEGACY_LINEAR_V1,
    };
    use crate::rng::round_rng;
    use usmf_core::{HexCell, TerrainType};

    fn flat_map(width: u32, height: u32) -> Map {
        let mut cells = Vec::new();
        for q in 0..width as i32 {
            for r in 0..height as i32 {
                cells.push(HexCell {
                    coord: HexCoord::new(q, r),
                    terrain: TerrainType::Plains,
                    elevation: 0,
                });
            }
        }
        Map {
            id: 1,
            name: "test".into(),
            width,
            height,
            cells,
        }
    }

    fn combatant(
        combatant_id: i64,
        side: &str,
        position: HexCoord,
        base_initiative: f64,
    ) -> CombatantState {
        CombatantState {
            combatant_id,
            source_unit_id: combatant_id,
            side: side.to_string(),
            position,
            base_initiative,
            max_action_points: 10,
            action_points: 10,
            weapon_range_hexes: 3,
            weapon_damage: 50.0,
            attack_ap_cost: 4,
            hit_points: 100.0,
            destroyed: false,
            ruleset_id: LEGACY_LINEAR_V1.to_string(),
            granularity: Granularity::Individual,
            strength_points: None,
            armor: None,
            hull_points: None,
            structure_points: None,
            weapon: None,
        }
    }

    #[test]
    fn initiative_order_favors_higher_base_initiative() {
        let mut states = HashMap::new();
        states.insert(1, combatant(1, "blue", HexCoord::new(0, 0), 0.0));
        states.insert(2, combatant(2, "blue", HexCoord::new(1, 0), 100.0));
        let mut rng = round_rng(1, 1);

        let order = roll_initiative(&states, &mut rng);
        assert_eq!(order, vec![2, 1]);
    }

    #[test]
    fn initiative_is_recalculated_and_can_differ_round_to_round() {
        let mut states = HashMap::new();
        states.insert(1, combatant(1, "blue", HexCoord::new(0, 0), 5.0));
        states.insert(2, combatant(2, "blue", HexCoord::new(1, 0), 5.0));

        let mut rng_round_1 = round_rng(99, 1);
        let mut rng_round_2 = round_rng(99, 2);
        let order_1 = roll_initiative(&states, &mut rng_round_1);
        let order_2 = roll_initiative(&states, &mut rng_round_2);

        // Same seed/round always reproduces the same order...
        let mut rng_round_1_again = round_rng(99, 1);
        assert_eq!(order_1, roll_initiative(&states, &mut rng_round_1_again));
        // ...but different rounds are free to differ (equal base initiative here,
        // so the random draw alone decides it).
        assert_ne!(order_1, order_2);
    }

    #[test]
    fn destroyed_combatants_are_excluded_from_initiative() {
        let mut states = HashMap::new();
        states.insert(1, combatant(1, "blue", HexCoord::new(0, 0), 0.0));
        let mut dead = combatant(2, "blue", HexCoord::new(1, 0), 100.0);
        dead.destroyed = true;
        states.insert(2, dead);

        let mut rng = round_rng(1, 1);
        assert_eq!(roll_initiative(&states, &mut rng), vec![1]);
    }

    #[test]
    fn ai_attacks_when_target_in_range_and_los() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        states.insert(1, combatant(1, "blue", HexCoord::new(0, 0), 100.0));
        states.insert(2, combatant(2, "red", HexCoord::new(2, 0), 0.0));

        let registry = default_registry();
        let mut rng = round_rng(1, 1);
        let events = resolve_round(&map, &mut states, &HashMap::new(), &registry, &mut rng);

        assert!(events.iter().any(|e| matches!(
            e,
            RoundEvent::AttackResolved {
                attacker_combatant_id: 1,
                target_combatant_id: 2,
                ..
            }
        )));
    }

    #[test]
    fn ai_advances_toward_distant_enemy_using_available_ap() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        // Plains cost 2/hex; 10 AP affords 5 hexes of movement, well short of
        // reaching (9,0) but the AI should still move as far as it can.
        states.insert(1, combatant(1, "blue", HexCoord::new(0, 0), 100.0));
        states.insert(2, combatant(2, "red", HexCoord::new(9, 0), 0.0));

        let registry = default_registry();
        let mut rng = round_rng(1, 1);
        let events = resolve_round(&map, &mut states, &HashMap::new(), &registry, &mut rng);

        assert!(events.iter().any(|e| matches!(
            e,
            RoundEvent::Moved {
                combatant_id: 1,
                ..
            }
        )));
        assert_ne!(states[&1].position, HexCoord::new(0, 0));
        assert_eq!(states[&1].action_points, 0);
    }

    #[test]
    fn unit_can_move_then_attack_in_the_same_turn_within_ap_budget() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        // Move 1 hex (cost 2 on plains) then attack (cost 4) = 6 AP of the 10 available.
        states.insert(1, combatant(1, "blue", HexCoord::new(0, 0), 100.0));
        states.insert(2, combatant(2, "red", HexCoord::new(2, 0), 0.0));

        let overrides = HashMap::from([(
            1,
            vec![
                Action::Move {
                    to: HexCoord::new(1, 0),
                },
                Action::Attack {
                    target_combatant_id: 2,
                },
            ],
        )]);
        let registry = default_registry();
        let mut rng = round_rng(1, 1);
        let events = resolve_round(&map, &mut states, &overrides, &registry, &mut rng);

        assert!(events.iter().any(|e| matches!(
            e,
            RoundEvent::Moved {
                combatant_id: 1,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            RoundEvent::AttackResolved {
                attacker_combatant_id: 1,
                ..
            }
        )));
        assert_eq!(states[&1].position, HexCoord::new(1, 0));
        assert_eq!(states[&1].action_points, 4);
    }

    #[test]
    fn override_action_is_used_instead_of_default_ai() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        // In range/LOS -- default AI would attack -- but the override says pass.
        // Unit 2 is also overridden to pass so its own (otherwise-legitimate)
        // default-AI attack on unit 1 doesn't confound the assertion below.
        states.insert(1, combatant(1, "blue", HexCoord::new(0, 0), 100.0));
        states.insert(2, combatant(2, "red", HexCoord::new(2, 0), 0.0));

        let overrides = HashMap::from([(1, vec![Action::Pass]), (2, vec![Action::Pass])]);
        let registry = default_registry();
        let mut rng = round_rng(1, 1);
        let events = resolve_round(&map, &mut states, &overrides, &registry, &mut rng);

        assert!(events
            .iter()
            .any(|e| matches!(e, RoundEvent::UnitPassed { combatant_id: 1 })));
        assert!(!events
            .iter()
            .any(|e| matches!(e, RoundEvent::AttackResolved { .. })));
    }

    #[test]
    fn destroyed_mid_round_unit_is_skipped_when_its_turn_comes() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        // Attacker goes first (high initiative) and one-shots the defender
        // before the defender's own (lower-initiative) turn arrives.
        states.insert(1, combatant(1, "blue", HexCoord::new(0, 0), 100.0));
        let mut defender = combatant(2, "red", HexCoord::new(1, 0), 0.0);
        defender.hit_points = 10.0;
        states.insert(2, defender);

        let registry = default_registry();
        let mut rng = round_rng(1, 1);
        let events = resolve_round(&map, &mut states, &HashMap::new(), &registry, &mut rng);

        assert!(events
            .iter()
            .any(|e| matches!(e, RoundEvent::UnitDestroyed { combatant_id: 2 })));
        // The dead unit never gets a TurnStarted of its own.
        assert!(!events.iter().any(|e| matches!(
            e,
            RoundEvent::TurnStarted {
                combatant_id: 2,
                ..
            }
        )));
    }

    #[test]
    fn aggregate_stack_destroyed_by_strength_loss_is_skipped_when_its_turn_comes() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        // Attacker goes first (high initiative). Its strength (1000) vs. the
        // defender's (1.0) is an overwhelming ratio (>= 5:1), which
        // combat::AggregateStrengthV1's CRT guarantees a hit on regardless
        // of the RNG draw (see combat::tests) -- and at strength_points =
        // 1.0, any nonzero loss fraction rounds up to exactly 1 point lost,
        // zeroing the stack out in a single hit deterministically.
        let mut attacker = combatant(1, "blue", HexCoord::new(0, 0), 100.0);
        attacker.ruleset_id = AGGREGATE_STRENGTH_V1.to_string();
        attacker.granularity = Granularity::Aggregate;
        attacker.strength_points = Some(1000.0);
        states.insert(1, attacker);

        let mut defender = combatant(2, "red", HexCoord::new(1, 0), 0.0);
        defender.ruleset_id = AGGREGATE_STRENGTH_V1.to_string();
        defender.granularity = Granularity::Aggregate;
        defender.strength_points = Some(1.0);
        states.insert(2, defender);

        let registry = default_registry();
        let mut rng = round_rng(1, 1);
        let events = resolve_round(&map, &mut states, &HashMap::new(), &registry, &mut rng);

        assert_eq!(states[&2].strength_points, Some(0.0));
        assert!(states[&2].destroyed);
        assert!(events
            .iter()
            .any(|e| matches!(e, RoundEvent::UnitDestroyed { combatant_id: 2 })));
        assert!(!events.iter().any(|e| matches!(
            e,
            RoundEvent::TurnStarted {
                combatant_id: 2,
                ..
            }
        )));
    }

    fn cepheus_weapon() -> CepheusWeapon {
        CepheusWeapon {
            dice_count: 6,
            dice_sides: 6,
            damage_type: CepheusDamageType::Sap,
        }
    }

    #[test]
    fn cepheus_individual_knocked_out_but_not_destroyed_spills_to_structure() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        // Same hex (range 0) for a guaranteed hit, matching the resolver's
        // own deterministic-Miss/-Hit tests in combat::tests. 6D6's minimum
        // roll is 6, always > 0 armor, so this is always an IndividualHit.
        let mut attacker = combatant(1, "blue", HexCoord::new(0, 0), 100.0);
        attacker.weapon = Some(cepheus_weapon());
        states.insert(1, attacker);

        let mut defender = combatant(2, "red", HexCoord::new(0, 0), 0.0);
        defender.ruleset_id = CEPHEUS_VEHICLE_V1.to_string();
        defender.granularity = Granularity::Individual;
        defender.armor = Some(0.0);
        defender.hull_points = Some(1.0);
        // 25 is the highest possible table bracket, so 30 always survives
        // regardless of how much of the roll spills past hull_points.
        defender.structure_points = Some(30.0);
        states.insert(2, defender);

        let registry = default_registry();
        let mut rng = round_rng(1, 1);
        let events = resolve_round(&map, &mut states, &HashMap::new(), &registry, &mut rng);

        assert_eq!(states[&2].hull_points, Some(0.0));
        assert!(
            states[&2].structure_points.unwrap() < 30.0,
            "overflow past Hull Points should have spilled into Structure Points"
        );
        assert!(!states[&2].destroyed, "Knocked Out is not Destroyed");
        assert!(events.iter().any(|e| matches!(
            e,
            RoundEvent::AttackResolved {
                attacker_combatant_id: 1,
                target_combatant_id: 2,
                hit: true,
                ..
            }
        )));
    }

    #[test]
    fn cepheus_individual_destroyed_when_structure_points_reach_zero() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        let mut attacker = combatant(1, "blue", HexCoord::new(0, 0), 100.0);
        attacker.weapon = Some(cepheus_weapon());
        states.insert(1, attacker);

        let mut defender = combatant(2, "red", HexCoord::new(0, 0), 0.0);
        defender.ruleset_id = CEPHEUS_VEHICLE_V1.to_string();
        defender.granularity = Granularity::Individual;
        defender.armor = Some(0.0);
        // Already Knocked Out (Hull Points at 0) from some earlier hit, and
        // Structure Points too low to survive any further nonzero hit --
        // the guaranteed penetration (armor 0) always destroys it here.
        defender.hull_points = Some(0.0);
        defender.structure_points = Some(1.0);
        states.insert(2, defender);

        let registry = default_registry();
        let mut rng = round_rng(1, 1);
        let events = resolve_round(&map, &mut states, &HashMap::new(), &registry, &mut rng);

        assert!(states[&2].structure_points.unwrap() <= 0.0);
        assert!(states[&2].destroyed);
        assert!(events
            .iter()
            .any(|e| matches!(e, RoundEvent::UnitDestroyed { combatant_id: 2 })));
    }
}
