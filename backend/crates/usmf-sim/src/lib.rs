pub mod combat;
pub mod engine;
pub mod los;
pub mod pathfinding;
pub mod rng;

pub use combat::{
    default_registry, AttackOutcome, CombatContext, CombatResolver, LegacyLinearV1,
    ResolverRegistry, RulesetId, LEGACY_LINEAR_V1,
};
pub use engine::{resolve_round, roll_initiative, Action, CombatantState, RoundEvent};
pub use los::has_line_of_sight;
pub use pathfinding::{find_path, path_cost};
pub use rng::round_rng;
