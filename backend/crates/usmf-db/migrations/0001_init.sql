-- Design-time schema: Component -> Asset -> Unit, ported from the V2 prototype's
-- SQLAlchemy models (legacy/python_prototype/app/models.py).

CREATE TABLE chassis_specs (
    name TEXT PRIMARY KEY,
    max_weight REAL NOT NULL,
    max_space REAL NOT NULL,
    base_cost REAL NOT NULL
);

-- Starter chassis types mirroring the V2 prototype's hardcoded CHASSIS_SPECS
-- dict (legacy/python_prototype/app/services.py), so the Asset Designer isn't
-- empty on first run. Users can add more via the chassis-specs API.
INSERT INTO chassis_specs (name, max_weight, max_space, base_cost) VALUES
    ('Heavy Tracked', 8000, 20, 5000),
    ('Light Wheeled', 2000, 8, 1000),
    ('Infantry Squad', 500, 10, 0);

CREATE TABLE components (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    component_type TEXT NOT NULL,
    stats TEXT NOT NULL DEFAULT '{}' -- JSON: ComponentStats
);

CREATE TABLE assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    chassis_type TEXT NOT NULL REFERENCES chassis_specs(name)
);

CREATE TABLE asset_components (
    asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    component_id INTEGER NOT NULL REFERENCES components(id),
    quantity INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (asset_id, component_id)
);

-- An individually-modeled role/position (e.g. "Rifleman", "Squad Leader"),
-- structurally the same idea as a chassis: a carry capacity slotted with
-- components. See usmf-core::personnel.
CREATE TABLE personnel_types (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    role_category TEXT,
    max_carry_weight REAL NOT NULL,
    max_carry_space REAL NOT NULL,
    base_cost REAL NOT NULL DEFAULT 0
);

CREATE TABLE personnel_loadout (
    personnel_type_id INTEGER NOT NULL REFERENCES personnel_types(id) ON DELETE CASCADE,
    component_id INTEGER NOT NULL REFERENCES components(id),
    quantity INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (personnel_type_id, component_id)
);

-- No parent_id/asset_id here: composition (own_assets/personnel below) and
-- command relationships (unit_relationships below) are both separate from the
-- unit itself. See design_doc.md section 2.1.
CREATE TABLE units (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    unit_type TEXT NOT NULL,
    formation_kind TEXT NOT NULL DEFAULT 'standing', -- 'standing' | 'task_force'
    c2_capacity INTEGER,
    personnel_mode TEXT NOT NULL DEFAULT 'simplified', -- 'simplified' | 'detailed'
    personnel_simplified_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE unit_assets (
    unit_id INTEGER NOT NULL REFERENCES units(id) ON DELETE CASCADE,
    asset_id INTEGER NOT NULL REFERENCES assets(id),
    quantity INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (unit_id, asset_id)
);

-- Only populated when the owning unit's personnel_mode = 'detailed'.
CREATE TABLE unit_personnel (
    unit_id INTEGER NOT NULL REFERENCES units(id) ON DELETE CASCADE,
    personnel_type_id INTEGER NOT NULL REFERENCES personnel_types(id),
    quantity INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (unit_id, personnel_type_id)
);

-- The doctrinal effect of a relationship type, as data rather than hardcoded --
-- see usmf-core::unit::RelationshipRules. Seeded with the standard set below;
-- a user can add custom rows (e.g. a project-specific relationship) without a
-- code change.
CREATE TABLE relationship_type_specs (
    name TEXT PRIMARY KEY,
    includes_in_span_of_control INTEGER NOT NULL,
    sustainment_transfers INTEGER NOT NULL,
    includes_in_combat_power_rollup INTEGER NOT NULL
);

INSERT INTO relationship_type_specs
    (name, includes_in_span_of_control, sustainment_transfers, includes_in_combat_power_rollup)
VALUES
    ('Organic', 1, 1, 1),
    ('Attached', 1, 1, 1),
    ('OPCON', 1, 0, 1),
    ('TACON', 1, 0, 1),
    ('Direct Support', 0, 0, 0),
    ('General Support', 0, 0, 0);

-- A typed, time-bounded command relationship between two units. The permanent
-- TO&E tree is every unit's 'Organic' relationship to its parent formation
-- (effective_from_turn/effective_until_turn both NULL = always in effect);
-- everything else layers temporary task organization (attachments, OPCON,
-- TACON, a Task Force's gained units, ...) on top without disturbing it.
CREATE TABLE unit_relationships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    superior_unit_id INTEGER NOT NULL REFERENCES units(id) ON DELETE CASCADE,
    subordinate_unit_id INTEGER NOT NULL REFERENCES units(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL REFERENCES relationship_type_specs(name),
    effective_from_turn INTEGER,
    effective_until_turn INTEGER,
    notes TEXT
);

CREATE INDEX idx_unit_relationships_superior ON unit_relationships(superior_unit_id);
CREATE INDEX idx_unit_relationships_subordinate ON unit_relationships(subordinate_unit_id);

-- Simulation-time schema: Map/Scenario/SimulationRun. Table shapes are settled now
-- (see design_doc.md section 2.2) but the Rust repository layer for these lands in
-- Phase 3/4 -- see design_doc.md's roadmap.

CREATE TABLE maps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL
);

CREATE TABLE hex_cells (
    map_id INTEGER NOT NULL REFERENCES maps(id) ON DELETE CASCADE,
    q INTEGER NOT NULL,
    r INTEGER NOT NULL,
    terrain TEXT NOT NULL,
    elevation INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (map_id, q, r)
);

CREATE TABLE scenarios (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    map_id INTEGER NOT NULL REFERENCES maps(id),
    weather TEXT NOT NULL DEFAULT 'Clear',
    duration_turns INTEGER NOT NULL DEFAULT 24
);

CREATE TABLE scenario_forces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scenario_id INTEGER NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    side_name TEXT NOT NULL,
    root_unit_id INTEGER NOT NULL REFERENCES units(id),
    starting_morale INTEGER NOT NULL DEFAULT 100,
    starting_supply INTEGER NOT NULL DEFAULT 1000
);

CREATE TABLE scenario_start_positions (
    scenario_force_id INTEGER NOT NULL REFERENCES scenario_forces(id) ON DELETE CASCADE,
    unit_id INTEGER NOT NULL REFERENCES units(id),
    q INTEGER NOT NULL,
    r INTEGER NOT NULL,
    PRIMARY KEY (scenario_force_id, unit_id)
);

CREATE TABLE simulation_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scenario_id INTEGER NOT NULL REFERENCES scenarios(id),
    status TEXT NOT NULL DEFAULT 'running',
    current_turn INTEGER NOT NULL DEFAULT 0,
    max_turns INTEGER NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE TABLE unit_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    simulation_run_id INTEGER NOT NULL REFERENCES simulation_runs(id) ON DELETE CASCADE,
    unit_id INTEGER NOT NULL REFERENCES units(id),
    turn_number INTEGER NOT NULL,
    personnel_count INTEGER NOT NULL,
    morale INTEGER NOT NULL,
    supply_level INTEGER NOT NULL,
    is_hq_connected INTEGER NOT NULL DEFAULT 1,
    is_destroyed INTEGER NOT NULL DEFAULT 0,
    casualties_this_turn INTEGER NOT NULL DEFAULT 0,
    position_q INTEGER,
    position_r INTEGER
);

CREATE INDEX idx_unit_states_run_turn ON unit_states(simulation_run_id, turn_number);

CREATE TABLE simulation_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    simulation_run_id INTEGER NOT NULL REFERENCES simulation_runs(id) ON DELETE CASCADE,
    turn_number INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    unit_id INTEGER REFERENCES units(id),
    description TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info'
);

CREATE INDEX idx_simulation_events_run ON simulation_events(simulation_run_id);
