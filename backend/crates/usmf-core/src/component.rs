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
    /// Contribution to whatever carries this component's baseline initiative
    /// (see `usmf-sim`'s round loop) -- e.g. a lightweight recon sensor or a
    /// well-drilled radio operator's kit might add a little, heavy armor might
    /// be modeled as a negative delta. Purely additive; the unit's own
    /// `base_initiative` takes the max across its own_assets/personnel, not a
    /// sum, so one fast element doesn't get diluted by slower ones.
    #[serde(default)]
    pub initiative: f64,
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
