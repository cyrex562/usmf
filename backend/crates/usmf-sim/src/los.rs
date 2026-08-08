use usmf_core::{HexCoord, Map};

#[derive(Debug, Clone, Copy)]
struct Cube {
    x: f64,
    y: f64,
    z: f64,
}

fn to_cube(c: HexCoord) -> Cube {
    Cube {
        x: c.q as f64,
        y: (-c.q - c.r) as f64,
        z: c.r as f64,
    }
}

fn round_cube(c: Cube) -> HexCoord {
    let mut rx = c.x.round();
    let ry = c.y.round();
    let mut rz = c.z.round();

    let dx = (rx - c.x).abs();
    let dy = (ry - c.y).abs();
    let dz = (rz - c.z).abs();

    if dx > dy && dx > dz {
        rx = -ry - rz;
    } else if dy <= dz {
        rz = -rx - ry;
    }

    HexCoord::new(rx as i32, rz as i32)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Traces the hex line between `from` and `to`. Line of sight is blocked if any
/// intermediate hex's elevation exceeds the higher of the two endpoint elevations
/// (a simple ridge-blocks-sight model; it does not account for unit height).
pub fn has_line_of_sight(map: &Map, from: HexCoord, to: HexCoord) -> bool {
    let distance = from.distance(&to);
    if distance == 0 {
        return true;
    }

    let from_elevation = map.cell_at(&from).map(|c| c.elevation).unwrap_or(0);
    let to_elevation = map.cell_at(&to).map(|c| c.elevation).unwrap_or(0);
    let sight_height = from_elevation.max(to_elevation);

    let a = to_cube(from);
    let b = to_cube(to);

    for step in 1..distance {
        let t = step as f64 / distance as f64;
        let interpolated = Cube {
            x: lerp(a.x, b.x, t),
            y: lerp(a.y, b.y, t),
            z: lerp(a.z, b.z, t),
        };
        let coord = round_cube(interpolated);
        if coord == from || coord == to {
            continue;
        }
        if let Some(cell) = map.cell_at(&coord) {
            if cell.elevation > sight_height {
                return false;
            }
        }
    }

    true
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
    fn flat_terrain_always_has_los() {
        let map = flat_map(6, 1);
        assert!(has_line_of_sight(
            &map,
            HexCoord::new(0, 0),
            HexCoord::new(5, 0)
        ));
    }

    #[test]
    fn tall_ridge_blocks_los() {
        let mut map = flat_map(5, 1);
        map.cells[2].elevation = 10; // sits between (0,0) and (4,0)
        assert!(!has_line_of_sight(
            &map,
            HexCoord::new(0, 0),
            HexCoord::new(4, 0)
        ));
    }
}
