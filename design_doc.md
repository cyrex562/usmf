# USMF Design Document (V3) — Rust + Vue Rewrite

## 0. Status

This supersedes the FastAPI/HTMX prototype ("Proving Ground V2"). That prototype is preserved at
`legacy/python_prototype/` (code) with its own design doc at
`legacy/python_prototype/design_doc_v2.md`. It proved out the core domain model
(Component → Asset → Unit hierarchy, recursive TO&E, turn-based morale/supply/C2 propagation)
against a SQLite DB seeded with real data. This document keeps that domain model, ports it to
Rust, and adds the piece the prototype never had: a **map**.

Two product goals, unchanged from the original ask:

1. **Unit design** — build equipment/vehicles from components, then organize them into military
   formations (TO&E).
2. **Simulation** — place designed forces on a map and fight them out, TABS-style but resolved as a
   turn-based hex-grid tactical sim rather than real-time physics.

## 1. What carries over from the prototype, and what changes

| Concept | V2 (Python prototype) | V3 (this doc) |
|---|---|---|
| Component/Asset/Unit hierarchy | Present, working | Kept as-is conceptually, ported to Rust structs |
| Asset slot validation (weight/space/power vs chassis) | Present, working | Kept, same rules |
| Unit tree (recursive TO&E, span of control, capability aggregation) | Present, partially wired (some dummy stat values) | Kept, and the dummy values (`stats["weight"] += 5000 # Dummy value`) get replaced with real aggregation from actual Asset component stats |
| Scenario / Forces | Present, abstract (`terrain_type` is just a string) | Kept, `terrain_type` becomes a real hex map reference |
| Turn-based simulation loop (morale, supply, C2 severance) | Present, non-spatial: random casualties, no positions | Kept as the *rules* layer, but combat resolution becomes spatial — casualties come from actual engagements between units in range/LOS of each other, not `random.uniform(0.01, 0.05)` |
| Map / positions / movement / range / line-of-sight | **Absent** | **New** — this is the core addition in V3 |
| Backend | Python, FastAPI, SQLAlchemy async, Jinja2+HTMX | Rust, Axum, sqlx, JSON/WebSocket API |
| Frontend | Server-rendered Jinja2 templates + HTMX partials | Vue 3 SPA (Vite, TypeScript, Pinia) |
| DB | SQLite (`usmf.db`) | SQLite (kept — single-user local tool, no reason to add Postgres ops overhead), schema extended with map/position tables |

The data in `usmf.db` (10 seed components, 4 units) is small enough that it's not worth writing an
automated migrator; the seed script (`legacy/python_prototype/scripts/seed_components.py`) is the
reference for re-seeding the new schema by hand once the Rust `usmf-db` migrations exist.

## 2. Domain model

### 2.1 Design-time entities (unit & equipment design)

**Component** — smallest building block (weapon, engine, sensor, armor plate, radio, ration pack).
```
id, name, component_type (Weapon | Engine | Sensor | Armor | Comms | Logistics | ...)
stats: { weight, space, cost, power_gen, power_draw, damage, range_hexes, rof, capabilities: {tag: level} }
```
`stats` stays a flexible JSON blob (mirrors V2) because component types are heterogeneous — a fusion
core and a ration pack don't share a schema. `capabilities` is a tag→level map (`"cyber": 2`,
`"indirect_fire": 1`) that rolls up through Asset → Unit for the "This Brigade has: Level 4 Cyber"
aggregation feature.

**Asset** — a physical platform or team, built from a chassis + slotted components.
```
id, name, chassis_type -> ChassisSpec { max_weight, max_space, base_cost }
components: [(component_id, quantity)]
```
Validation rule (unchanged from V2): `sum(component.weight * qty) <= chassis.max_weight`, same for
space; `power_draw <= power_gen`. Chassis specs move from a hardcoded Python dict into a DB table so
new chassis types don't require a code change.

**Unit** — an organizational node in a recursive TO&E tree.
```
id, name, unit_type (HQ | Line | Support | ...), parent_id (nullable, self-referential), asset_id (nullable)
c2_capacity, c2_cost
```
Leaf units reference an Asset (or a personnel-only entry, e.g. a rifle squad with no vehicle).
Internal nodes are pure organizational (a Battalion HQ has no Asset of its own). Computed,
recursive properties (span of control, C2 load, logistics throughput vs. consumption, capability
aggregation) are pure functions over the tree in `usmf-core` — same logic V2 had in
`calculate_unit_stats`, but operating on real numbers instead of `# Dummy value`.

### 2.2 Simulation-time entities (the new part)

**Map** — a hex grid.
```
id, name, width, height
hexes: [{ q, r, terrain: Plains|Forest|Urban|Water|Hill|Road, elevation, movement_cost, cover_bonus }]
```
Axial coordinates (`q, r`), flat-top or pointy-top chosen once and fixed (pointy-top, standard
offset for on-screen row alignment). Movement cost and cover are per-hex so terrain-editing in the
Scenario Editor is just painting hex properties.

**Scenario** — a Map + starting force placements + win conditions.
```
id, name, map_id, weather, duration_turns
forces: [{ side_name, root_unit_id, start_positions: [(unit_id, q, r)], starting_morale, starting_supply }]
```
`start_positions` only needs to name the *leaf* units (the actual fighting Assets/squads) — HQ and
intermediate formation nodes don't occupy a hex themselves; their position for C2-range purposes is
derived as the centroid (or nearest-subordinate) of their children, computed at simulation time.

**SimulationRun / UnitState / SimulationEvent** — kept from V2, `UnitState` gains spatial fields:
```
UnitState: ... existing morale/supply/personnel/is_destroyed/is_hq_connected fields, plus
  position (q, r), facing (optional), ammo_remaining, suppression_level, orders (current turn's order)
```

## 3. Simulation design (turn-based hex tactical)

Each turn has four phases, run in this order, mirroring V2's `run_turn()` phase list but adding
movement/engagement as spatial operations instead of abstract ones:

1. **Orders phase** — client submits per-unit orders for the turn (`MoveTo(q,r)`, `Attack(target_unit_id)`,
   `Hold`). This is the only phase that takes client input; the rest is deterministic simulation.
2. **Movement phase** — each unit with a `MoveTo` order pathfinds (A* over the hex grid, cost =
   terrain `movement_cost`, blocked by impassable terrain and unit stacking limits) up to its
   movement allowance (derived from Asset component stats, e.g. engine power/chassis type).
3. **Engagement phase** — for each unit with an `Attack` order or an enemy in range: check
   line-of-sight (hex line-trace, blocked by elevation/terrain), check range against the unit's
   weapon components (`range_hexes` from Component stats), resolve to-hit and damage
   (weapon `damage` stat, modified by target's cover bonus and suppression), apply casualties.
   This is where V2's `_process_combat` random-casualty stub gets replaced with a real resolution
   step — but the *output* (casualties → morale loss → propagate up tree) reuses V2's logic as-is.
4. **Propagation phase** — unchanged from V2: morale shock propagates to parent units (50% of
   loss), supply drains and triggers morale penalties at zero, C2 severance detaches children when
   their HQ is destroyed. This phase is spatially aware only in that "HQ destroyed" now also means
   "HQ's hex was overrun," but the propagation math itself doesn't change.

Determinism: combat resolution uses a seeded RNG (`ChaCha8Rng` seeded from the `SimulationRun` id +
turn number) so a given scenario + order sequence always replays identically — needed for a usable
event log and for any future "what changed" diffing between runs.

The client drives pacing: it can submit orders and call `step` turn-by-turn (like V2's
`/api/simulations/{id}/step`), or request auto-play where the server steps on a timer and pushes
state over WebSocket. Given orders happen once per turn per side, WebSocket push (not polling) is
the natural fit for keeping the map view live during auto-play.

## 4. System architecture

### 4.1 Rust backend — Cargo workspace at `backend/`

```
backend/
  Cargo.toml                 # workspace manifest
  crates/
    usmf-core/                # domain types + pure business logic, no I/O
      Component, Asset, Unit, ChassisSpec, Map, Hex, Scenario, ...
      asset validation, unit-tree aggregation (span of control, C2, logistics, capabilities)
      all serde Serialize/Deserialize — this is the single source of truth for the wire format
    usmf-sim/                 # the simulation engine, depends on usmf-core only
      hex math (axial coords, distance, line-trace for LOS, A* pathfinding)
      turn orchestration (orders -> movement -> engagement -> propagation)
      combat resolution, seeded RNG
      fully unit-testable without a DB or web server
    usmf-db/                  # persistence, depends on usmf-core
      sqlx (SQLite), migrations/, repository structs per aggregate
      (ComponentRepo, AssetRepo, UnitRepo, MapRepo, ScenarioRepo, SimulationRepo)
    usmf-api/                 # binary crate, axum HTTP+WS server
      depends on core+db+sim, thin controllers, no business logic of its own
```

Why four crates instead of one: `usmf-sim` needs to be testable in isolation (feed it a scenario,
assert on the resulting event log) without spinning up a database or HTTP server, and `usmf-core`
needs to be shared by both `usmf-db` (for row mapping) and `usmf-sim` (for domain types) without
either depending on the other.

Key dependencies: `axum`, `tokio`, `serde`/`serde_json`, `sqlx` (sqlite, runtime-tokio, macros,
migrate), `thiserror`, `anyhow`, `tracing`/`tracing-subscriber`, `tower-http` (CORS), `rand`/`rand_chacha`.

Deliberately **not** included yet: an OpenAPI generator (`utoipa`) or a Rust→TypeScript type
exporter (`ts-rs`/`specta`). Both are good fits for this project once the API surface stabilizes —
worth revisiting after the first vertical slice works end-to-end, not before.

### 4.2 Vue frontend — `frontend/` (Vite + Vue 3 + TypeScript + Pinia + vue-router)

```
frontend/src/
  views/
    ComponentLibrary.vue      # browse/create components
    AssetDesigner.vue         # chassis + slots + live validation HUD (port of V2's asset_designer.html)
    UnitDesigner.vue          # recursive drag-drop TO&E tree (port of V2's unit_designer.html)
    MapEditor.vue             # NEW — paint hex terrain, define a Map
    ScenarioEditor.vue        # NEW — place forces from the Unit tree onto a Map
    SimulationViewer.vue      # NEW — hex map render, order entry, step/play controls, event log
  components/                 # shared widgets (HexGrid.vue, StatBar.vue, TreeNode.vue, ...)
  stores/                     # Pinia: useDesignStore, useMapStore, useScenarioStore, useSimStore
  api/
    client.ts                 # typed REST client
    simSocket.ts               # WebSocket client for live sim state
```

Map rendering: plain SVG with hand-rolled axial-hex math to start (a tactical map of a few hundred
hexes doesn't need WebGL). `HexGrid.vue` takes a `Map` + per-hex `UnitState` overlay and renders
both the Map Editor and the Simulation Viewer, since they're the same rendering problem with
different edit affordances layered on top.

### 4.3 Data flow

```
Vue SPA  <--REST (CRUD: components/assets/units/maps/scenarios)-->  Axum (usmf-api)
Vue SPA  <--WebSocket (order submission, turn-state push)-->        Axum (usmf-api)
                                                                       |
                                                          usmf-sim (turn engine)  <-- usmf-core (domain types/rules)
                                                                       |
                                                          usmf-db (sqlx/SQLite)
```

## 5. API surface (initial)

REST:
- `GET/POST /api/components`, `GET/PUT/DELETE /api/components/:id`
- `GET/POST /api/assets`, `POST /api/assets/:id/validate`
- `GET/POST /api/units`, `PATCH /api/units/:id/move`, `GET /api/units/:id/stats`
- `GET/POST /api/maps`, `PUT /api/maps/:id/hexes`
- `GET/POST /api/scenarios`, `POST /api/scenarios/:id/simulations` (start a run)
- `GET /api/simulations/:id`, `GET /api/simulations/:id/events`

WebSocket:
- `WS /api/simulations/:id/stream` — client sends `{ "orders": [...] }` or `{ "action": "step" | "play" | "pause" }`;
  server pushes `{ "turn": n, "unit_states": [...], "events": [...] }` per turn.

## 6. Repo layout after this change

```
usmf/
  design_doc.md                      # this file
  backend/                           # Rust workspace (see 4.1)
  frontend/                          # Vue app (see 4.2)
  legacy/python_prototype/           # V2 FastAPI/HTMX prototype, kept for reference
    design_doc_v2.md
    app/, scripts/, requirements.txt, usmf.db
  old/                               # pre-2020 design spreadsheets/docs (unchanged)
```

## 7. Roadmap

**Phase 1 — Foundation.** Rust workspace + Vue scaffold building and talking to each other
(health-check round trip). SQLite schema + migrations for Component/Asset/Unit ported from V2.

**Phase 2 — Design tools.** Component Library, Asset Designer (with live HUD validation), Unit
Designer (drag-drop TO&E tree) — functional parity with V2, real Rust logic instead of dummy stats.

**Phase 3 — Map.** Map Editor (hex terrain painting), hex math + pathfinding + LOS in `usmf-sim`,
unit-tested in isolation.

**Phase 4 — Scenario & Simulation.** Scenario Editor (force placement on a Map), turn engine wired
end-to-end (orders → movement → engagement → propagation), Simulation Viewer with step/play and
event log, WebSocket streaming.

**Phase 5 — Polish.** Capability aggregation UI ("this Brigade has Level 4 Cyber"), replay/scrub
through a completed run, chassis-type management UI (replacing the old hardcoded dict).

## 8. Open questions (deferred, not blocking Phase 1)

- Multiplayer (two humans, each ordering one side) vs. single-player-vs-AI vs. pure sandbox replay —
  affects whether the WebSocket protocol needs auth/session separation per side.
- Map size ceiling and whether SVG rendering holds up, or a Canvas/PixiJS rewrite of `HexGrid.vue`
  becomes necessary — defer until Phase 3 gives real hex counts to benchmark against.
- Whether `usmf-sim`'s combat resolution should support pluggable rule sets (e.g. different damage
  models per era/genre) — the old spreadsheets in `old/` suggest this project has iterated through
  several rule systems before; worth a skim before Phase 4 locks in one resolution formula.
