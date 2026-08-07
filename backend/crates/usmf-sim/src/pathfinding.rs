use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use usmf_core::{HexCoord, Map};

#[derive(Eq, PartialEq)]
struct Frontier {
    cost: u32,
    coord: HexCoord,
}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap; reverse so lowest cost pops first.
        other.cost.cmp(&self.cost)
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A* over the hex grid. Movement cost per hex comes from `TerrainType::movement_cost`;
/// a cell not present on the map is treated as impassable. Returns `None` if the goal
/// is unreachable within `max_cost` movement points.
pub fn find_path(
    map: &Map,
    start: HexCoord,
    goal: HexCoord,
    max_cost: u32,
) -> Option<Vec<HexCoord>> {
    if start == goal {
        return Some(vec![start]);
    }

    let mut open = BinaryHeap::new();
    open.push(Frontier {
        cost: 0,
        coord: start,
    });

    let mut came_from: HashMap<HexCoord, HexCoord> = HashMap::new();
    let mut cost_so_far: HashMap<HexCoord, u32> = HashMap::new();
    cost_so_far.insert(start, 0);

    while let Some(Frontier { cost, coord }) = open.pop() {
        if coord == goal {
            return Some(reconstruct_path(&came_from, start, goal));
        }
        if cost > max_cost {
            continue;
        }

        for neighbor in coord.neighbors() {
            let Some(cell) = map.cell_at(&neighbor) else {
                continue;
            };
            let step_cost = cell.terrain.movement_cost();
            if step_cost == u32::MAX {
                continue;
            }
            let new_cost = cost + step_cost;
            if new_cost > max_cost {
                continue;
            }
            let better = cost_so_far
                .get(&neighbor)
                .map(|&existing| new_cost < existing)
                .unwrap_or(true);
            if better {
                cost_so_far.insert(neighbor, new_cost);
                came_from.insert(neighbor, coord);
                open.push(Frontier {
                    cost: new_cost + neighbor.distance(&goal),
                    coord: neighbor,
                });
            }
        }
    }

    None
}

fn reconstruct_path(
    came_from: &HashMap<HexCoord, HexCoord>,
    start: HexCoord,
    goal: HexCoord,
) -> Vec<HexCoord> {
    let mut path = vec![goal];
    let mut current = goal;
    while current != start {
        current = came_from[&current];
        path.push(current);
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn finds_direct_path_on_open_terrain() {
        let map = flat_map(5, 5);
        let path = find_path(&map, HexCoord::new(0, 0), HexCoord::new(2, 0), 10).unwrap();
        assert_eq!(*path.first().unwrap(), HexCoord::new(0, 0));
        assert_eq!(*path.last().unwrap(), HexCoord::new(2, 0));
    }

    #[test]
    fn returns_none_when_goal_out_of_reach() {
        let map = flat_map(5, 5);
        // Plains costs 2/hex, so reaching a hex 3 hexes away needs 6 movement.
        let path = find_path(&map, HexCoord::new(0, 0), HexCoord::new(3, 0), 3);
        assert!(path.is_none());
    }

    #[test]
    fn water_is_impassable() {
        let mut map = flat_map(3, 1);
        map.cells[1].terrain = TerrainType::Water; // between (0,0) and (2,0)
        let path = find_path(&map, HexCoord::new(0, 0), HexCoord::new(2, 0), 20);
        assert!(path.is_none());
    }
}
