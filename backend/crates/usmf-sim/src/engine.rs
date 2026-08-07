use std::collections::HashMap;

use rand::Rng;
use rand_chacha::ChaCha8Rng;
use usmf_core::{HexCoord, Map};

use crate::los::has_line_of_sight;
use crate::pathfinding::find_path;

#[derive(Debug, Clone)]
pub enum Order {
    MoveTo(HexCoord),
    Attack { target_unit_id: i64 },
    Hold,
}

#[derive(Debug, Clone)]
pub struct CombatantState {
    pub unit_id: i64,
    pub side: String,
    pub position: HexCoord,
    pub movement_allowance: u32,
    pub weapon_range_hexes: u32,
    pub weapon_damage: f64,
    pub hit_points: f64,
    pub destroyed: bool,
}

#[derive(Debug, Clone)]
pub enum TurnEvent {
    Moved {
        unit_id: i64,
        path: Vec<HexCoord>,
    },
    MoveBlocked {
        unit_id: i64,
        reason: String,
    },
    AttackResolved {
        attacker_unit_id: i64,
        target_unit_id: i64,
        hit: bool,
        damage: f64,
    },
    UnitDestroyed {
        unit_id: i64,
    },
}

/// Runs the movement + engagement phases for one turn. Propagation (morale/supply/C2)
/// is deliberately not here yet — it operates over the `Unit` tree in `usmf-core`
/// rather than over spatial `CombatantState`, and gets wired in once the API layer
/// can supply both together.
pub fn resolve_turn(
    map: &Map,
    states: &mut HashMap<i64, CombatantState>,
    orders: &HashMap<i64, Order>,
    rng: &mut ChaCha8Rng,
) -> Vec<TurnEvent> {
    let mut events = Vec::new();

    // Movement phase.
    for (unit_id, order) in orders {
        let Order::MoveTo(goal) = order else {
            continue;
        };
        let Some(state) = states.get(unit_id) else {
            continue;
        };
        if state.destroyed {
            continue;
        }
        match find_path(map, state.position, *goal, state.movement_allowance) {
            Some(path) => {
                if let Some(state) = states.get_mut(unit_id) {
                    state.position = *goal;
                }
                events.push(TurnEvent::Moved {
                    unit_id: *unit_id,
                    path,
                });
            }
            None => events.push(TurnEvent::MoveBlocked {
                unit_id: *unit_id,
                reason: "no reachable path within movement allowance".to_string(),
            }),
        }
    }

    // Engagement phase.
    for (unit_id, order) in orders {
        let Order::Attack { target_unit_id } = order else {
            continue;
        };
        let Some(attacker) = states.get(unit_id).cloned() else {
            continue;
        };
        let Some(target) = states.get(target_unit_id).cloned() else {
            continue;
        };
        if attacker.destroyed || target.destroyed {
            continue;
        }

        let range = attacker.position.distance(&target.position);
        if range > attacker.weapon_range_hexes {
            continue;
        }
        if !has_line_of_sight(map, attacker.position, target.position) {
            continue;
        }

        // Simple to-hit: chance decreases linearly with range.
        let hit_chance = 1.0 - (range as f64 / attacker.weapon_range_hexes.max(1) as f64) * 0.5;
        let hit = rng.gen::<f64>() < hit_chance;
        let damage = if hit { attacker.weapon_damage } else { 0.0 };

        events.push(TurnEvent::AttackResolved {
            attacker_unit_id: *unit_id,
            target_unit_id: *target_unit_id,
            hit,
            damage,
        });

        if hit {
            if let Some(target_state) = states.get_mut(target_unit_id) {
                target_state.hit_points -= damage;
                if target_state.hit_points <= 0.0 && !target_state.destroyed {
                    target_state.destroyed = true;
                    events.push(TurnEvent::UnitDestroyed {
                        unit_id: *target_unit_id,
                    });
                }
            }
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::turn_rng;
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

    #[test]
    fn unit_moves_along_valid_path() {
        let map = flat_map(5, 5);
        let mut states = HashMap::new();
        states.insert(
            1,
            CombatantState {
                unit_id: 1,
                side: "blue".into(),
                position: HexCoord::new(0, 0),
                movement_allowance: 10,
                weapon_range_hexes: 3,
                weapon_damage: 10.0,
                hit_points: 100.0,
                destroyed: false,
            },
        );
        let mut orders = HashMap::new();
        orders.insert(1, Order::MoveTo(HexCoord::new(2, 0)));

        let mut rng = turn_rng(1, 1);
        let events = resolve_turn(&map, &mut states, &orders, &mut rng);

        assert!(matches!(events[0], TurnEvent::Moved { unit_id: 1, .. }));
        assert_eq!(states[&1].position, HexCoord::new(2, 0));
    }

    #[test]
    fn attack_out_of_range_does_nothing() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        states.insert(
            1,
            CombatantState {
                unit_id: 1,
                side: "blue".into(),
                position: HexCoord::new(0, 0),
                movement_allowance: 0,
                weapon_range_hexes: 2,
                weapon_damage: 50.0,
                hit_points: 100.0,
                destroyed: false,
            },
        );
        states.insert(
            2,
            CombatantState {
                unit_id: 2,
                side: "red".into(),
                position: HexCoord::new(8, 0),
                movement_allowance: 0,
                weapon_range_hexes: 2,
                weapon_damage: 50.0,
                hit_points: 100.0,
                destroyed: false,
            },
        );
        let mut orders = HashMap::new();
        orders.insert(1, Order::Attack { target_unit_id: 2 });

        let mut rng = turn_rng(1, 1);
        let events = resolve_turn(&map, &mut states, &orders, &mut rng);
        assert!(events.is_empty());
        assert_eq!(states[&2].hit_points, 100.0);
    }

    #[test]
    fn in_range_attack_can_destroy_target() {
        let map = flat_map(10, 1);
        let mut states = HashMap::new();
        states.insert(
            1,
            CombatantState {
                unit_id: 1,
                side: "blue".into(),
                position: HexCoord::new(0, 0),
                movement_allowance: 0,
                weapon_range_hexes: 5,
                weapon_damage: 100.0,
                hit_points: 100.0,
                destroyed: false,
            },
        );
        states.insert(
            2,
            CombatantState {
                unit_id: 2,
                side: "red".into(),
                position: HexCoord::new(1, 0),
                movement_allowance: 0,
                weapon_range_hexes: 5,
                weapon_damage: 100.0,
                hit_points: 100.0,
                destroyed: false,
            },
        );
        let mut orders = HashMap::new();
        orders.insert(1, Order::Attack { target_unit_id: 2 });

        // Adjacent hex + short range relative to max range => near-certain hit chance,
        // seed chosen from a run that lands under the threshold.
        let mut rng = turn_rng(7, 1);
        let events = resolve_turn(&map, &mut states, &orders, &mut rng);
        assert!(matches!(
            events[0],
            TurnEvent::AttackResolved { hit: true, .. }
        ));
        assert!(states[&2].destroyed);
    }
}
