use serde::{Deserialize, Serialize};

use crate::hex::HexCoord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartPosition {
    pub unit_id: i64,
    pub coord: HexCoord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioForce {
    pub side_name: String,
    pub root_unit_id: i64,
    pub start_positions: Vec<StartPosition>,
    pub starting_morale: i32,
    pub starting_supply: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub id: i64,
    pub name: String,
    pub map_id: i64,
    pub weather: String,
    pub duration_turns: u32,
    pub forces: Vec<ScenarioForce>,
}
