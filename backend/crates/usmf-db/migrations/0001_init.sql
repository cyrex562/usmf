-- Design-time schema: Component -> Asset -> Unit, ported from the V2 prototype's
-- SQLAlchemy models (legacy/python_prototype/app/models.py).

CREATE TABLE chassis_specs (
    name TEXT PRIMARY KEY,
    max_weight REAL NOT NULL,
    max_space REAL NOT NULL,
    base_cost REAL NOT NULL
);

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

CREATE TABLE units (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    unit_type TEXT NOT NULL,
    parent_id INTEGER REFERENCES units(id) ON DELETE CASCADE,
    asset_id INTEGER REFERENCES assets(id),
    c2_capacity INTEGER
);

CREATE INDEX idx_units_parent_id ON units(parent_id);

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
