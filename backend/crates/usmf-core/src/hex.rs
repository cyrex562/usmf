use serde::{Deserialize, Serialize};

/// Axial coordinates, pointy-top orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

impl HexCoord {
    pub fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Cube-coordinate distance, standard for axial hex grids.
    pub fn distance(&self, other: &HexCoord) -> u32 {
        let dq = (self.q - other.q).abs();
        let dr = (self.r - other.r).abs();
        let ds = ((self.q + self.r) - (other.q + other.r)).abs();
        (dq.max(dr).max(ds)) as u32
    }

    pub fn neighbors(&self) -> [HexCoord; 6] {
        const DIRS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];
        DIRS.map(|(dq, dr)| HexCoord::new(self.q + dq, self.r + dr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_between_adjacent_hexes_is_one() {
        let origin = HexCoord::new(0, 0);
        for neighbor in origin.neighbors() {
            assert_eq!(origin.distance(&neighbor), 1);
        }
    }

    #[test]
    fn distance_is_symmetric_and_scales() {
        let a = HexCoord::new(0, 0);
        let b = HexCoord::new(3, -1);
        assert_eq!(a.distance(&b), b.distance(&a));
        assert_eq!(a.distance(&b), 3);
    }
}
