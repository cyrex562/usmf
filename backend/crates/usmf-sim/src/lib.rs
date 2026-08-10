pub mod combat;
pub mod engine;
pub mod los;
pub mod pathfinding;
pub mod rng;
pub mod spawn;

pub use combat::{
    default_registry, AggregateStrengthV1, AttackOutcome, CombatContext, CombatResolver,
    LegacyLinearV1, ResolverRegistry, RulesetId, AGGREGATE_STRENGTH_V1, CEPHEUS_VEHICLE_V1,
    LEGACY_LINEAR_V1,
};
pub use engine::{resolve_round, roll_initiative, Action, CombatantState, RoundEvent};
pub use los::has_line_of_sight;
pub use pathfinding::{find_path, path_cost};
pub use rng::round_rng;
pub use spawn::{
    expand_placement, instance_count, resolve_granularity, CombatDefaults,
    DEFAULT_AGGREGATE_THRESHOLD,
};
