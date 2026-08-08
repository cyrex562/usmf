use serde::{Deserialize, Serialize};

use crate::hex::HexCoord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainType {
    Plains,
    Forest,
    Urban,
    Water,
    Hill,
    Road,
}

impl TerrainType {
    /// Movement cost in "hexes of movement allowance" to enter this terrain.
    pub fn movement_cost(&self) -> u32 {
        match self {
            TerrainType::Road => 1,
            TerrainType::Plains => 2,
            TerrainType::Forest | TerrainType::Hill => 3,
            TerrainType::Urban => 3,
            TerrainType::Water => u32::MAX, // impassable to ground units
        }
    }

    pub fn cover_bonus(&self) -> f64 {
        match self {
            TerrainType::Forest => 0.3,
            TerrainType::Urban => 0.4,
            TerrainType::Hill => 0.2,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexCell {
    pub coord: HexCoord,
    pub terrain: TerrainType,
    pub elevation: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Map {
    pub id: i64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub cells: Vec<HexCell>,
}

impl Map {
    pub fn cell_at(&self, coord: &HexCoord) -> Option<&HexCell> {
        self.cells.iter().find(|c| &c.coord == coord)
    }
}
