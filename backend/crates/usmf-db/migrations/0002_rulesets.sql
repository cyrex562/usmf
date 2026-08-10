-- Catalogs known combat-resolution rulesets (design_doc.md §2.1, §3.7) as data,
-- mirroring relationship_type_specs. This is metadata only (display name, source,
-- which granularity it applies to) for UI/validation purposes -- it does not gate
-- which CombatResolver usmf-sim::ResolverRegistry actually registers, which stays
-- code-driven ("metadata is data, the resolution math is still Rust").
CREATE TABLE rulesets (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    source TEXT,
    supports_individual INTEGER NOT NULL DEFAULT 0,
    supports_aggregate INTEGER NOT NULL DEFAULT 0
);

-- legacy_linear_v1 (usmf-sim::combat::LEGACY_LINEAR_V1) is granularity-agnostic:
-- it just depletes a flat hit_points pool regardless of what that pool represents,
-- so it's the fallback either way until aggregate_strength_v1/cepheus_vehicle_v1
-- land as more specific rulesets (design_doc.md §3.7).
INSERT INTO rulesets (id, display_name, source, supports_individual, supports_aggregate) VALUES
    ('legacy_linear_v1', 'Legacy Linear (range-scaled hit chance)', 'usmf-sim built-in', 1, 1);
