use serde::{Deserialize, Serialize};

use crate::hex::HexCoord;

/// Whether a placement resolves to one `CombatantState` per individual
/// Asset/PersonnelType instance, or one per placement carrying a summed
/// strength-point pool (design_doc.md §2.2) -- a division-scale battle
/// (~50,000 people, thousands of vehicles) never needs to simulate every
/// individual, so this is a per-placement choice, not a property of the
/// Unit design itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    Individual,
    Aggregate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartPosition {
    pub unit_id: i64,
    pub coord: HexCoord,
    /// `None` = pick a default based on how many individual instances this
    /// placement would expand into (`usmf-sim::spawn::resolve_granularity`);
    /// `Some` overrides that default explicitly.
    #[serde(default)]
    pub granularity: Option<Granularity>,
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
