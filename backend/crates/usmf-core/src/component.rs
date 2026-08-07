use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentType {
    Weapon,
    Engine,
    Power,
    Sensor,
    Armor,
    Comms,
    Logistics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentStats {
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub space: f64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub power_gen: f64,
    #[serde(default)]
    pub power_draw: f64,
    #[serde(default)]
    pub damage: f64,
    #[serde(default)]
    pub range_hexes: u32,
    #[serde(default)]
    pub capabilities: HashMap<String, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub id: i64,
    pub name: String,
    pub component_type: ComponentType,
    pub stats: ComponentStats,
}
