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
stats: { weight, space, cost, power_gen, power_draw, damage, range_hexes, rof, initiative, capabilities: {tag: level} }
```
`stats` stays a flexible JSON blob (mirrors V2) because component types are heterogeneous — a fusion
core and a ration pack don't share a schema. `capabilities` is a tag→level map (`"cyber": 2`,
`"indirect_fire": 1`) that rolls up through Asset → Unit for the "This Brigade has: Level 4 Cyber"
aggregation feature. `initiative` is additive and feeds the simulation's activation order (§3.1) —
purely a numeric contribution, not itself meaningful at design time.

**Asset** — a physical platform or team, built from a chassis + slotted components.
```
id, name, chassis_type -> ChassisSpec { max_weight, max_space, base_cost }
components: [(component_id, quantity)]
```
Validation rule (unchanged from V2): `sum(component.weight * qty) <= chassis.max_weight`, same for
space; `power_draw <= power_gen`. Chassis specs move from a hardcoded Python dict into a DB table so
new chassis types don't require a code change.

**PersonnelType** — an individually-modeled role/position (e.g. "Rifleman", "Squad Leader", "Combat
Medic"). Structurally the same idea as an Asset — a capacity slotted with Components — except the
capacity is a soldier's carry limit rather than a vehicle chassis envelope:
```
id, name, role_category (optional, e.g. "Infantry" | "Medical" | "Signal")
max_carry_weight, max_carry_space, base_cost
loadout: [(component_id, quantity)]   -- weapon, armor, radio, pack, ...
```
Asset and PersonnelType both validate/total through the same shared logic (`usmf-core::loadout`) —
"a capacity slotted with components" is the same problem whether the capacity belongs to a vehicle
or a person.

**Unit** — a node in the force structure. Composition and command are two separate concerns, not one
`parent_id` column:
```
id, name, unit_type (HQ | Line | Support | ...), formation_kind (Standing | TaskForce)
own_assets: [(asset_id, quantity)]
personnel: Simplified(count) | Detailed([(personnel_type_id, quantity)])
c2_capacity   -- direct-command capacity, for span-of-control warnings
```
A unit can hold multiple Assets directly (a "Tank Platoon" unit holds 4× "M1A5 Tank," no need for 4
sibling nodes) and, independently, either a bare personnel headcount (`Simplified`) when detail
isn't needed, or a quantified list of `PersonnelType`s (`Detailed`) when it is — e.g. a rifle squad
as "8× Rifleman, 1× Squad Leader," each with its own loadout rolling up into the unit's weight/cost/
capabilities. A unit can carry its own composition *and* have subordinates at the same time — a
Company HQ typically has a small HQ section of its own (own_assets/personnel) while also commanding
subordinate platoons (via relationships, below). `formation_kind` is informational: `Standing` units
are permanent force structure, `TaskForce` units are stood up ad hoc for a mission (often around a
borrowed HQ) — structurally both are ordinary units, so this is a reporting/UI tag, not a different
code path.

**UnitRelationship** — a typed, time-bounded command relationship between two units. This is the
piece that replaces a single `parent_id`. Real TO&E isn't a strict tree: a battalion's *organic*
subordinates are its companies, but battalions, companies, brigades, and divisions also routinely
gain units from elsewhere — attached, OPCON, TACON, in direct or general support — for anywhere from
one mission to an extended period, and a "task force" is nothing more than a unit (often an existing
HQ) that has gained others this way. Modeling that as data instead of a hardcoded tree is the point:
```
id, superior_unit_id, subordinate_unit_id
relationship_type: "Organic" | "Attached" | "OPCON" | "TACON" | "Direct Support" | "General Support" | <custom>
rules: { includes_in_span_of_control, sustainment_transfers, includes_in_combat_power_rollup }
effective_from_turn, effective_until_turn   -- both null = permanent (this is how "Organic" is expressed)
notes
```
`relationship_type` is a free-form label for display; `rules` is what the rollup logic actually
reads, and it's *data* — stored per label in a `relationship_type_specs` table (seeded with the
doctrinal set below), not matched on in code. That's what makes "different kinds of rules about
units and their relationships" configurable: adding a new relationship type (a custom support
relationship, a project-specific attachment rule) is a data row, not a Rust change.

| Relationship | In gaining unit's span of control? | Sustainment responsibility | In gaining unit's combat-power rollup? |
|---|---|---|---|
| Organic | Yes, permanently | Transfers (it's home) | Yes |
| Attached | Yes | Transfers to gaining unit | Yes |
| OPCON | Yes | Stays with organic parent | Yes |
| TACON | Yes, scoped to the task | Stays with organic parent | Yes |
| Direct Support | No — stays under its own chain | Stays with organic parent | No (support relationship, not command) |
| General Support | No | Stays with organic parent | No |

The **permanent TO&E tree** is just every unit's `Organic` relationship to its parent formation. The
**effective command tree** at a given point in time is every relationship active at that time (see
below) with `includes_in_span_of_control = true` — this is what span-of-control checks and the
turn-engine's C2-severance logic (section 3) actually walk, not the permanent tree alone. A Task
Force is simply a unit that has gained others via `Attached`/`OPCON`/`TACON` relationships for its
mission window; no separate schema.

Time-bounding uses `effective_from_turn`/`effective_until_turn` as an opaque ordinal (a turn number
during a running simulation, or a designer-assigned sequence at design time) with `None` on both
sides meaning "always in effect" — how a permanent Organic link is expressed. A rollup query can pass
`as_of: None` to ignore time bounds entirely (a full "what's configured" view, useful at design
time) or `as_of: Some(turn)` for the temporally accurate view during simulation.

Computed, recursive properties (effective span of control, combat-power aggregation, capability
aggregation, sustainment draw) are pure functions over units + relationships in `usmf-core`
(`rollup_unit`) — same idea V2 had in `calculate_unit_stats`, but walking typed relationships instead
of a `parent_id` column, and operating on real numbers instead of `# Dummy value`.

Known simplification (revisit only if a future relationship type needs it): `rollup_unit` only
traverses relationships flagged `includes_in_span_of_control`, so a hypothetical sustainment-only,
no-command relationship wouldn't be picked up. None of the six doctrinal types above need that.

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
`start_positions` only needs to name units that carry their own composition (`own_assets`/
`personnel` — the actual fighting elements) — pure command nodes with no composition of their own
don't occupy a hex themselves; their position for C2-range purposes is derived as the centroid (or
nearest-subordinate) of their effective subordinates, computed at simulation time.

**SimulationRun / UnitState / SimulationEvent** — kept from V2, `UnitState` gains spatial fields:
```
UnitState: ... existing morale/supply/personnel/is_destroyed/is_hq_connected fields, plus
  position (q, r), facing (optional), ammo_remaining, suppression_level, orders (current turn's order)
```

## 3. Simulation design (initiative-order hex tactical)

**Terminology, since this replaced an earlier phase-based design:** a **round** is one full pass
through the activation order — every living combatant gets exactly one turn per round. A **turn** is
one combatant's slice of time within a round, during which it spends an action-point (AP) budget on
one or more actions. This is not V2's WEGO phase model (all orders submitted, then Movement/
Engagement/Propagation resolve simultaneously) — combatants act one at a time, in initiative order,
against the *current* board state, so a fast unit can genuinely move and shoot before a slower enemy
that hasn't acted yet even gets a chance to react.

### 3.1 Initiative

Every combatant that carries its own composition (§2.2 — the units actually placed on the map) has a
`base_initiative`: the max individual-item initiative total among its own directly-held assets/
personnel (`usmf_core::base_initiative` — "fastest/most alert element sets the pace," not a sum).
Initiative itself is a `Component` stat (§2.1) like `damage` or `range_hexes`, so it's designed the
same way everything else is — a scout vehicle's sensor suite or a well-drilled soldier's kit can add
to it.

At the start of **every round**, initiative is **recalculated**: each combatant's `base_initiative`
plus a fresh seeded random draw produces that round's effective initiative, highest first (ties break
on unit ID for reproducibility). Recalculating every round — rather than fixing the order once at
battle start — means casualties, suppression, and morale shifts can reshuffle who acts when as the
fight develops, not just who's fastest on paper.

### 3.2 Control: AI by default, explicit override per activation

When a combatant's turn comes up, **a simple default AI decides its actions** unless the controlling
side has explicitly overridden that specific unit for this round:

- **Default AI** (`usmf-sim::engine::decide_ai_action`): if an enemy is in weapon range and line of
  sight, attack it; otherwise advance as far toward the nearest enemy as the remaining AP budget
  allows; otherwise pass. This is deliberately simple — a heuristic, not a doctrine engine — and is
  what gives every unit *some* sensible default behavior without the player having to hand-hold each
  one every round.
- **Override**: the controlling side can supply an explicit queued list of actions for a specific
  unit's upcoming turn (`Move`, `Attack`, `Pass`, in order); when present, that queue runs instead of
  the default AI, stopping early if AP runs out or an action doesn't apply. This is how a human player
  takes direct control of one of their own units for a round without the engine needing to pause and
  block mid-round waiting on input — overrides are submitted *before* the round resolves, alongside
  (or instead of) letting AI handle everything else.

This is intentionally the smallest version of "hybrid control" that works: no standing-orders/rules-
of-engagement editor yet (see §8) — just "AI unless told otherwise for this unit, this round."

### 3.3 Actions and the AP economy

Each combatant gets an `action_points` budget (`max_action_points`, currently a flat per-combatant
value — deriving it from component stats the way weapon range/damage already are is listed in §8) at
the start of its turn, reset every round. Actions spend AP and a turn continues — "one or more
actions" — until AP runs out, an action fails to apply, or the combatant explicitly passes:

- **Move** — A* pathfind (`usmf-sim::pathfinding::find_path`) over the hex grid to the requested
  hex, cost = sum of `TerrainType::movement_cost()` for each hex entered (impassable terrain simply
  isn't reachable). Debits AP by the path's actual cost.
- **Attack** — requires the target within the attacker's weapon `range_hexes` and line of sight
  (`usmf-sim::los::has_line_of_sight`, elevation-aware hex line-trace); costs a fixed
  `attack_ap_cost`. To-hit currently scales linearly with range (closer = more likely); cover/
  suppression modifiers are not wired in yet (§8).
- **Use ability** — not yet implemented; the `Action` enum has room to grow (abilities tied to a
  unit's `capabilities` tags from §2.1, e.g. an "indirect_fire" capability unlocking an indirect-fire
  action) once there's a concrete ability to build against.
- **Pass** — ends the turn immediately, regardless of remaining AP.

A hit that drops a target's hit points to zero destroys it immediately, mid-round — a unit destroyed
early in the initiative order is simply skipped when its own (later) turn comes up, and every
subsequent combatant's turn sees the updated board.

### 3.4 End-of-round propagation

After every living combatant has had its turn, an end-of-round pass runs the aggregate consequences —
largely unchanged from V2's logic, just retargeted at the *effective* command tree (§2.1) instead of a
flat parent_id tree: morale shock propagates up to a unit's current superior (which, for an OPCON'd
unit, is the gaining HQ it's actually reporting to, not necessarily its organic parent), supply drains
and triggers morale penalties at zero, and C2 severance drops a unit's active command relationship to
a destroyed superior (an attached/OPCON/TACON unit reverts to its organic parent if that's still alive
and connected; an organic unit whose HQ was destroyed goes fully disconnected).

### 3.5 Determinism

A round's `ChaCha8Rng` is seeded from the `SimulationRun` id and round number
(`usmf-sim::rng::round_rng`) and used for both that round's initiative rolls and its to-hit checks, in
a fixed order (unit IDs sorted before drawing) so the sequence — and therefore the whole round's
outcome — is reproducible regardless of `HashMap` iteration order. Same scenario, same round number,
same overrides in, same events out: needed for a usable replay/event log.

### 3.6 Pacing

The client drives pacing: submit this round's overrides (if any) and call `step` to resolve one round
(like V2's `/api/simulations/{id}/step`, now resolving a round instead of a WEGO turn), or request
auto-play where the server steps on a timer and pushes each round's events over WebSocket — see §5.

## 4. System architecture

### 4.1 Rust backend — Cargo workspace at `backend/`

```
backend/
  Cargo.toml                 # workspace manifest
  crates/
    usmf-core/                # domain types + pure business logic, no I/O
      Component, Asset, PersonnelType, Unit, UnitRelationship, ChassisSpec, Map, Hex, Scenario, ...
      loadout validation (shared by Asset and PersonnelType), unit-tree rollup over typed
      time-bounded relationships (effective span of control, combat power, sustainment, capabilities)
      all serde Serialize/Deserialize — this is the single source of truth for the wire format
    usmf-sim/                 # the simulation engine, depends on usmf-core only
      hex math (axial coords, distance, line-trace for LOS, A* pathfinding)
      turn orchestration (orders -> movement -> engagement -> propagation)
      combat resolution, seeded RNG
      fully unit-testable without a DB or web server
    usmf-db/                  # persistence, depends on usmf-core
      sqlx (SQLite), migrations/, repository structs per aggregate
      (ComponentRepo, AssetRepo, PersonnelTypeRepo, UnitRepo, UnitRelationshipRepo, MapRepo,
      ScenarioRepo, SimulationRepo)
    usmf-api/                 # binary crate, axum HTTP+WS server
      depends on core+db+sim, thin controllers, no business logic of its own
      `serve-frontend` feature (off by default): embeds frontend/dist via rust-embed and serves
      it as a fallback route alongside /api/*, so a release build is a single deployable binary.
      Dev workflow (cargo run + `npm run dev` on separate ports) is unaffected either way.
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
- `GET/POST /api/personnel-types`, `POST /api/personnel-types/:id/validate`
- `GET/POST /api/units`, `GET /api/units/:id/rollup?as_of=<turn>`
- `GET/POST /api/units/:id/relationships`, `DELETE /api/relationships/:id` (attach/detach, i.e. add
  or end a `UnitRelationship`)
- `GET/POST /api/relationship-types` (manage the `relationship_type_specs` rule table)
- `GET/POST /api/maps`, `PUT /api/maps/:id/hexes`
- `GET/POST /api/scenarios`, `POST /api/scenarios/:id/simulations` (start a run)
- `GET /api/simulations/:id`, `GET /api/simulations/:id/events`

WebSocket:
- `WS /api/simulations/:id/stream` — client sends
  `{ "overrides": { "<unit_id>": [Action, ...] }, "action": "step" | "play" | "pause" }` (`overrides`
  is optional and only needs entries for units the controlling side wants to hand-direct this round —
  see §3.2; everything else resolves via the default AI); server pushes
  `{ "round": n, "unit_states": [...], "events": [RoundEvent, ...] }` per round (`RoundEvent` per
  §3 — `InitiativeRolled`, `TurnStarted`, `Moved`, `AttackResolved`, `UnitDestroyed`, `UnitPassed`,
  `ActionBlocked`).

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

**Phase 2 — Design tools.** Component Library, Asset Designer (with live HUD validation), Personnel
Designer (same HUD pattern for `PersonnelType` loadouts), Unit Designer (drag-drop *organic* TO&E
tree, own-composition editor, and an attach/detach panel for non-organic `UnitRelationship`s) —
functional parity with V2 plus the relationship model from section 2.1, real Rust logic instead of
dummy stats.

**Phase 3 — Map.** Map Editor (hex terrain painting) and the `usmf-db`/`usmf-api` layer for Maps.
Hex math, A* pathfinding, and LOS are already implemented and unit-tested in `usmf-sim` ahead of
this phase (`usmf-sim::pathfinding`, `usmf-sim::los`) — this phase is mainly the persistence/UI
around them.

**Phase 4 — Scenario & Simulation.** Scenario Editor (force placement on a Map), the
`usmf-db`/`usmf-api` layer for Scenarios/SimulationRuns, Simulation Viewer with step/play and event
log, WebSocket streaming (§5). The initiative/AP round engine itself (§3) is already implemented and
unit-tested in `usmf-sim::engine` ahead of this phase — this phase wires it to real persisted
scenarios and a live UI instead of in-memory test fixtures.

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
- Cycle prevention for `Organic` relationships: nothing yet stops a unit from organically reporting
  to its own descendant. Not a problem for the non-command relationship types (a unit can be
  Attached/OPCON in a loop-free way even if graph cycles are technically representable), but the
  permanent TO&E tree specifically needs to stay acyclic — needs a validation pass before the Unit
  Designer's attach/detach UI ships in Phase 2.
- UI for editing `relationship_type_specs` itself (adding a custom relationship type beyond the
  seeded six) — deferred to whenever a real use case for a custom type shows up, per section 2.1.
- `max_action_points`/`attack_ap_cost` (§3.3) are currently flat per-combatant values passed into
  `CombatantState` directly, not derived from Component/Asset/PersonnelType stats the way weapon
  range/damage/initiative already are. Wiring an `action_points` (or similar) Component stat through
  the same loadout-totals mechanism is the natural next step once there's real Asset/PersonnelType
  data to derive it from (Phase 2 issues).
- The default AI (§3.2) is a one-line heuristic (attack if in range, else advance, else pass) with no
  concept of standing orders, rules of engagement, or unit-specific doctrine. A richer
  orders/behavior system (and the "commander sets objectives, AI executes them" framing from the
  original ask) is real future work, not represented in the engine yet — the override mechanism
  covers "player wants direct control of this unit this round" but not "player wants to steer AI
  behavior without micromanaging every turn."
- To-hit (§3.3) is still a simple range-scaled chance with no cover, suppression, or elevation
  modifiers — the `TerrainType::cover_bonus()` value already exists on the Map model (§2.2) but isn't
  read by combat resolution yet.
