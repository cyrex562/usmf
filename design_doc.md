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

**Rulesets — how Component stats plug into combat resolution.** A **ruleset** (`RulesetId`, e.g.
`"cepheus_vehicle_v1"`, `"aggregate_strength_v1"`) is a named, versioned combat-resolution model: its
own to-hit method, its own damage/penetration math, its own way of depleting a combatant's health
pool. This is the mechanism for plugging in a different rule system — including material from a
future source book — without redesigning `usmf-core`'s shapes per book: a new ruleset is an
additional `CombatResolver` implementation in `usmf-sim` (§3.7) plus, where needed, an additional
namespaced block in `Component.stats`, not a schema change.

A weapon/armor Component's `stats` blob can carry a `rulesets` map alongside its existing flat
fields, one entry per ruleset it has data for:
```
stats: { weight, space, cost, ..., rulesets: {
  "cepheus_vehicle_v1": { damage_dice: "6D6", damage_type: "Sap", armor_front: 40, armor_side: 25, armor_rear: 15 },
  "aggregate_strength_v1": { combat_power: 12 },
} }
```
The same Component can carry data for several rulesets at once (a tank gun has both a granular
Cepheus profile and a coarse combat-power number) — which one gets read at combat time is chosen by
the *defender's* granularity, not hardcoded per weapon (§3.7). A Component with no entry for the
active ruleset simply can't damage that target under that ruleset — a design-time validation/UI
concern for Phase 2+, not an engine crash.

`CombatProfile` — the rolled-up, per-ruleset combat picture for an Asset or PersonnelType — is
computed by `rollup_unit`'s existing aggregation pass the same way capability tags already roll up
("This Brigade has Level 4 Cyber," above): sum armor components' `armor_*` fields, sum weapon
components' `combat_power`, etc., per ruleset. An Asset doesn't need a hand-authored parallel combat
data structure — it falls out of its loadout the same way weight/cost/capabilities already do.

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

**Combat granularity — individual vs. aggregate.** The same Unit definition (design-time) can be
*placed* at different resolutions depending on the scenario's scale — a "Tank Platoon: 4× M1A5" TO&E
entry might be four separately-tracked vehicles in a platoon-level skirmish, or one strength-point
stack in a division-scale battle (a division is ~50,000 people and thousands of vehicles; nothing
about the engine should require simulating each one as a full individual). This is a placement-time
choice, not a property of the Unit itself:
```
start_positions: [(unit_id, q, r, granularity: Individual | Aggregate)]
```
mirroring the `Simplified(count) | Detailed([...])` choice PersonnelType composition already makes
above — same idea, applied at the combat-resolution layer instead of the composition layer.
`granularity` defaults to `Aggregate` above a configurable headcount/quantity threshold and
`Individual` below it, overridable per placement.

At scenario start, each placement is **expanded** into one or more `CombatantState`s:
- **Individual** — one `CombatantState` per Asset/PersonnelType instance, each carrying its own
  `CombatProfile` (armor, hull points, structure points, crew) under whichever ruleset that
  Component data supports (e.g. `cepheus_vehicle_v1`) — this is what gives named vehicles/crews
  the Component Damage Table-style texture worth having at small scale. That texture is
  whole-combatant status (a `crew` pool plus a small set of effect flags — see §3.7's
  `ComponentDamageEffect`), not identified sub-entities: every Asset/PersonnelType loadout is
  already fully summed into scalar totals before a placement is expanded (§2.1's `validate_loadout`),
  so there is no surviving "this vehicle's engine Component" to damage individually, and building
  that would mean threading raw Component lists through the whole rollup pipeline instead of
  pre-summed totals — out of proportion to what the Cepheus material's effects actually need. Each
  individually-tracked combatant still gets its own independent `CombatantState`, so per-instance
  divergence ("this specific tank is on fire, that one isn't") is free — it just isn't sub-divided
  any further within one combatant.
- **Aggregate** — one `CombatantState` per placement representing the whole stack, carrying a
  single `strength_points` pool seeded from the *existing* `rollup_unit` combat-power aggregation
  (§2.1) — no new number to invent, it's the same figure the "This Brigade has Level 4 Cyber"-style
  rollup already computes — resolved under an aggregate ruleset (e.g. `aggregate_strength_v1`,
  §3.7) that does strength-vs-strength attrition instead of per-shot penetration.

`UnitState`/`CombatantState` gains `granularity: Individual | Aggregate` and `ruleset_id: RulesetId`
so the engine knows which `CombatResolver` to invoke for a given combatant without re-deriving it
every attack.

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

This was intentionally the smallest version of "hybrid control" that works: no standing-orders/rules-
of-engagement concept yet. Design settled by #29:

- **Vocabulary** — a `StandingOrder { stance: EngagementStance, objective: Option<HexCoord> }`, two
  independent axes rather than one combined vocabulary:
  - `stance` ∈ `Aggressive` (today's exact behavior — the default when no order is set, so nothing
    regresses for a unit without one) | `Defensive` (attack if an enemy is already in range and LOS,
    but never *advance* to close the distance) | `HoldFire` (never initiate an `Attack`, regardless of
    range).
  - `objective: Option<HexCoord>` — when not actively attacking, advance toward this hex instead of
    the nearest enemy. Independent of `stance`, so "advance to objective" and "free fire vs. hold
    fire" compose (e.g. `Aggressive` + an objective still fires opportunistically at anything in range
    while marching toward the objective, matching "commander sets objectives, AI executes them").
  - Target-type prioritization (the issue's fourth suggested item) is deliberately **not** in this
    first vocabulary — `decide_ai_action`'s target selection is a single `(distance, combatant_id)`
    sort with nothing on `CombatantState` to filter by (no target-type/threat tag exists anywhere
    today); adding one is its own follow-up once this smaller vocabulary is proven, not guessed at
    alongside it.
- **Where orders live** — associated with the design-time `Unit` for persistence/editing (so a
  commander sets an objective once, not every round, and it survives across rounds without
  resubmission — the opposite of `overrides`' per-round nature), but supplied to `resolve_round` as a
  flattened `HashMap<i64, StandingOrder>` **keyed by `source_unit_id`**, mirroring exactly how
  `overrides` is already a caller-supplied per-round parameter rather than something the engine looks
  up itself. Not a new `CombatantState` field: `source_unit_id` already exists on every combatant and
  is enough to look up its unit's order, and keying by unit (not by individual combatant) matches how
  a commander actually gives orders — one order to "1st Platoon," not to each of its nine separately-
  expanded riflemen. This also sidesteps a real constraint the design pass found: `source_unit_id` is
  provenance-only today (`expand_placement` runs once at scenario start and nothing keeps a live link
  from a `CombatantState` back to its `Unit`), so baking orders directly into `CombatantState` at
  expansion time would mean a changed order never takes effect mid-run; a sibling lookup parameter
  does not have that problem.
- **Interaction with `overrides`** — standing orders become the new content of "default AI," not a
  third control tier: `overrides` present for a combatant's turn still wins outright and runs
  unchanged (direct control always takes precedence); when absent, `decide_ai_action` now reads the
  combatant's `source_unit_id`'s order (defaulting to `Aggressive`/no objective, i.e. today's exact
  behavior, if none is set) instead of always chasing the nearest enemy. `resolve_round`'s own
  override-vs-AI branch doesn't change at all — only `decide_ai_action`'s internal logic and its
  signature (a new `orders` parameter, sibling to the `states`/`map` it already takes) do.

Implementation is tracked separately (#50).

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
  `attack_ap_cost`. Resolution is dispatched by the defender's `ruleset_id` (§2.2) to the matching
  `CombatResolver` (§3.7) — to-hit and damage math live entirely inside that resolver, not in the
  Attack action itself. Today's linear-range-scaled hit chance against a flat `hit_points` pool
  becomes the `legacy_linear_v1` resolver (the only one registered until §3.7's granular/aggregate
  rulesets land); cover/suppression modifiers are a per-resolver concern (confirmed by §3.8's design
  pass), not wired into any resolver yet.
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

### 3.8 Cepheus vehicle-combat mechanics, phase 2 (design settled by #28)

The original `cepheus_vehicle_v1` design conversation covered more than the penetration pipeline
(§3.7): spotting, movement/drive checks, terrain/cover/elevation to-hit modifiers, suppression,
autofire, HE burst radius, ramming, and chases/dogfights. All were scoped out of the combat-resolver
milestone (#11) as "a second full pass once the Hull/Structure/Armor core is in." This section is
that pass's structural answers — implementation is tracked issue-by-issue (see the end of this
section), not landed here.

**Where each mechanic hooks in**, and why: `engine::apply_action`'s `Attack` arm already has exactly
one chokepoint every attack passes through — the range/LOS gate, right before `CombatContext` is
built and handed to the resolver — and its `Move` arm already threads the same round-seeded
`ChaCha8Rng` used for to-hit rolls (§3.5) without yet using it for anything. Every mechanic below
reuses one of these two existing hooks rather than inventing new ones, the same way `cover_bonus()`'s
own doc comment and §3.3's existing "cover/suppression modifiers are a per-resolver concern" note
already anticipated:

- **Spotting** — a new precondition alongside the existing LOS check, not a new `Action` variant or
  persistent per-combatant "spotted" state. Recomputed fresh on every `Attack` attempt, the same way
  `has_line_of_sight` already is, rather than cached across rounds — the smallest addition consistent
  with how LOS already works, not the richer "spend AP to spot, stays spotted" alternative (that's a
  bigger follow-up if playtesting shows it's needed). Individual-granularity `cepheus_vehicle_v1`
  combatants only (see granularity note below).
- **Movement/drive checks** — a new roll inside the `Move` arm, gated on the path's terrain
  difficulty, using the AP-debiting logic that's already there. Individual-granularity vehicles only
  — a "drive check" is a specific vehicle/crew skill concept that doesn't apply to dismounted
  personnel or to an abstracted strength-point stack.
- **Terrain to-hit modifiers (cover, elevation)** — `CombatContext` gains new fields, populated at its
  existing construction site (`engine.rs`, right before the resolver call) from `map.cell_at`, which
  is already in scope there. Each resolver folds the new context into its own hit-chance math
  independently (`legacy_linear_v1`/`cepheus_vehicle_v1` both already share `range_scaled_hit_chance`;
  `aggregate_strength_v1` ignores `ctx` entirely today and can keep doing so) — a per-resolver
  concern, not a shared global modifier-stacking system, matching `CombatContext`'s own doc comment
  and §3.3's existing note. Applies to both granularities — cover protects whatever occupies a hex,
  whether that's one vehicle or a whole stack.
- **Suppression** — unlike cover/elevation, this needs genuinely new state (a level that
  builds/decays over time), not just a read of existing static Map data — closer in weight to the
  Component Damage Table (#25) than to the terrain-modifier bullet above. Tracked as its own
  follow-up rather than bundled with cover/elevation.
- **Autofire** — multiple damage rolls against the same single target within one `Attack` action;
  still one attacker/one defender, so it extends `CepheusVehicleV1::resolve_attack`'s existing
  single-pair shape rather than requiring a new dispatch mechanism.
- **HE burst radius** — the one mechanic with no existing hook at all: every resolver, and the
  `Attack` action itself, is hard-wired to exactly one attacker and one defender
  (`target_combatant_id: i64`, not a hex or a list of targets). Needs either a new `Action` variant
  (e.g. targeting a hex rather than a combatant) or a multi-defender resolution loop inside the
  existing `Attack` arm — the largest structural change of this whole batch, and the one most likely
  to need its own follow-up design pass once the others are built and the resolver-extension pattern
  is proven out.
- **Ramming** — a `Move`-into-an-occupied-hex-as-attack hybrid; combines both existing hooks rather
  than needing a third.
- **Chases/dogfights** — parked, not scoped. A sustained multi-round pursuer/evader relationship
  (escape conditions, contested movement) is a different shape of mechanic from everything else in
  this list, and nothing in the roadmap has called for a fast-mover/pursuit scenario yet (no
  aircraft/naval concept exists anywhere in the domain model). Revisit if a concrete scenario need
  shows up, the same treatment §8 already gives #31/#32.

**Granularity**: `CombatantState`, `Action`, `apply_action`, `pathfinding`, and LOS are all already
100% uniform across `Individual`/`Aggregate` — nothing in the engine branches on granularity today,
so any new precondition applies to both by default unless a mechanic explicitly exempts one. Spotting
and movement/drive checks are exempted for `Aggregate` above, following the same precedent §2.2's
Component Damage Table decision (#25) already set: extra per-combatant texture piles onto
`Individual` via new scalar/flag state, `Aggregate` stays coarse. Terrain modifiers apply to both,
since they need no new per-combatant state to begin with.

Implementation is tracked as a `#11`-style milestone (tracking issue + one sub-issue per mechanic),
not one PR — the same reasoning §3.7's own resolvers used: each mechanic's actual numbers (spotting
odds, drive-check DCs, cover-bonus-to-hit-chance formula, autofire's shot count, HE's damage falloff)
still need their own origination and, per #26's precedent, their own balance pass once built.

### 3.7 Combat resolver architecture (pluggable rulesets)

Combat resolution is a `CombatResolver` trait in `usmf-sim`, not a single hardcoded formula, so a
new rule system — a different book's vehicle combat rules, a different era's aggregate CRT, a
homebrew variant — plugs in as one more implementation instead of a rewrite of the Attack action:
```
trait CombatResolver {
    fn ruleset_id(&self) -> RulesetId;
    fn resolve_attack(&self, attacker: &CombatantState, defender: &CombatantState,
                       ctx: &CombatContext, rng: &mut ChaCha8Rng) -> AttackOutcome;
}
```
`AttackOutcome` is wide enough to cover both granularities without either resolver lying about the
other's shape:
```
enum AttackOutcome {
    Miss,
    IndividualHit { hull_lost: u32, structure_lost: u32, component_effects: Vec<ComponentDamageEffect> },
    AggregateHit { strength_lost: u32 },
}
enum ComponentDamageEffect {
    WeaponDisabled,
    Immobilized,
    CrewCasualty(u32),
    FireBreakout,
    ElectronicsKnockedOut,
}
```
`ComponentDamageEffect` (design settled by #25) is a closed set of whole-combatant status results,
not identified crew members or subsystem instances — see §2.2's note on why per-instance Component
identity doesn't survive to combat time. `resolve_attack` returns these as data only; `apply_action`
is what actually mutates the defender (decrementing a new `crew: Option<f64>` pool the same way
`armor`/`hull_points`/`structure_points` already are, or setting a status flag/counter), keeping
resolvers pure — the same split `hull_lost`/`structure_lost` already use today. `crew` needs no new
rollup plumbing: `CombatProfile`'s `rulesets` JSON already round-trips a plain numeric `crew` field
through `merge_combat_profiles` (nothing currently reads it into `CombatantState`; it just becomes
one more field `spawn.rs::cepheus_numeric_fields` derives, alongside armor/hull/structure). The
`# of rolls` column from the source Penetration Table material becomes a third element on
`PENETRATION_TABLE`'s existing `(bound, hull_damage)` rows, driving that many rolls against a new
`COMPONENT_DAMAGE_TABLE` (placeholder odds, same not-final treatment as `CRT`/`PENETRATION_TABLE`,
pending #26's balance pass) per penetrating hit. Applies uniformly to Individual-granularity
vehicles and Individual-granularity Detailed personnel (§2.2 already treats both the same way) —
`crew` simply stays unpopulated for a PersonnelType whose loadout has no Component declaring one,
while the other effects (weapon jammed, wounded/immobilized, on fire) apply just as sensibly to an
individual soldier as to a vehicle.
A `ResolverRegistry` (`HashMap<RulesetId, Box<dyn CombatResolver>>`), built once at engine init, is
what the Attack action consults using the *defender's* `ruleset_id` — resolution is chosen by who's
being shot at, not by the attacker's weapon type, since a mixed engagement (an individually-tracked
tank firing into an aggregate infantry-company stack) still needs exactly one outcome shape to
apply.

Two resolvers ship as the built-in set:
- **`legacy_linear_v1`** — today's range-scaled hit chance against a flat `hit_points` pool; the
  default for any combatant without a more specific ruleset assigned, so nothing regresses while
  granular/aggregate rulesets are rolled out incrementally. Deliberately stays the simple baseline —
  attacker-side risk (below) is out of scope for it.
- **`aggregate_strength_v1`** — a combat-power-ratio CRT (classic wargame odds table: attacker:
  defender combat-power ratio → a results row like "defender −X% strength," "defender eliminated,"
  "no effect," "attacker −X% strength"), reading `CombatProfile.combat_power` (§2.1) on both sides.
  #16's first cut shipped without the "attacker −X% strength" row this paragraph already sketched —
  a known simplification, not a design change. #27 confirmed attacker-side loss is wanted here after
  all: it's this resolver's own stated genre convention, not a novel addition, and initiative-order
  turn structure (§3.1) doesn't substitute for it — a defender can be destroyed by one attack before
  ever getting a turn to return fire, so "wait for the round to come back around" isn't equivalent
  risk. Implementation (redistributing some of the bad-odds columns' "no effect" probability mass
  into a new `AttackOutcome` variant applied to the *attacker's* `strength_points`, plus a follow-up
  balance-pass on the changed table per #26's precedent) is tracked separately. `cepheus_vehicle_v1`
  does not get this — a per-shot penetration pipeline doesn't have an equivalent "the assault itself
  went wrong" moment the way a CRT roll does; a jam/breakdown/counter-fire concept was considered but
  needs new mechanical state (jam status, counter-fire targeting) disconnected from a simple outcome
  addition, so it's left to #28 (the catch-all for cepheus_vehicle_v1's next mechanics pass) if ever
  picked up, not bundled into this decision.

`cepheus_vehicle_v1` — the granular penetration pipeline (damage dice, SAP/AP armor-ignore, hull/
structure points, Component Damage Table) — is the first non-legacy resolver to build, once Hull
Points and the full penetration/component-damage tables are confirmed (open design questions
tracked outside this doc).

Adding a ruleset from a new source book means: a new `CombatResolver` impl in `usmf-sim`, a
namespaced entry in the relevant Components' `stats.rulesets` (§2.1), and a row in a new `rulesets`
DB table (id, display name, source, granularity support) mirroring how `relationship_type_specs`
makes relationship types data-driven (§2.1) — metadata is data, the resolution math is still Rust,
the same boundary the relationship-type system already draws between "configurable" and "requires
code."

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

Resolved since this section was last written:
- ~~Pluggable combat rule sets~~ — resolved by §3.7's `CombatResolver` trait + registry and §2.2's
  individual/aggregate granularity split, landed end-to-end as the combat-resolver milestone (issues
  #12–#17, tracked in #11). `old/`'s prior rule-system iterations were mined (#15) and confirmed to
  have nothing reusable — both `aggregate_strength_v1` and `cepheus_vehicle_v1`'s numeric tables are
  originated, not migrated, and explicitly marked placeholder pending #26's balance pass.
- ~~Cycle prevention for `Organic` relationships~~ — implemented and tested in `UnitRelationshipRepo`
  (issue #6): an `Organic` relationship that would make a unit its own ancestor is rejected before
  insert; non-organic types aren't subject to the check, per §2.1's doctrinal-effects table.
- ~~`max_action_points`/`attack_ap_cost`/weapon stats/`hit_points` were still flat per-combatant
  values~~ — resolved (#24, merged): `usmf-sim::spawn::expand_placement` now derives all of these
  from real Component/Asset/PersonnelType rollups (`WeaponProfiles`/`LegacyWeaponProfiles`), the same
  way `cepheus_vehicle_v1`'s armor/hull/weapon data already was; `CombatDefaults` narrowed to just
  the per-weapon fallback for Aggregate combatants and Individual instances with no profile entry.
- ~~The Component Damage Table (per-crew/per-subsystem hit effects)~~ — design settled (#25): see
  §2.2 and §3.7's `ComponentDamageEffect` — a closed set of whole-combatant status results (a `crew`
  pool plus weapon/mobility/fire/electronics flags), not identified sub-entities, since per-instance
  Component identity doesn't survive to combat time. Implementation tracked separately (#37).
- ~~Balance pass on the placeholder CRT and Penetration Table numbers~~ — resolved (#26,
  `docs/combat-balance-pass.md`): a Monte Carlo playtest across a spread of odds ratios and
  weapon/armor matchups found both tables already smooth, monotonic, and free of degenerate
  all-or-nothing outcomes — no retuning needed. The `PLACEHOLDER NUMBERS` doc comments on `CRT`/
  `PENETRATION_TABLE` (`usmf-sim::combat`) are removed; the properties confirmed are pinned by the
  `balance_pass` test module. #37's future `COMPONENT_DAMAGE_TABLE` will need its own such pass once
  it exists.
- ~~Whether attacker-side losses are wanted at all~~ — decided (#27): yes for
  `aggregate_strength_v1` (see §3.7 — restores the "attacker −X% strength" row this section's own
  prose already sketched, and is standard for the genre this resolver is modeled on), no for
  `cepheus_vehicle_v1` or `legacy_linear_v1`. Implementation (new `AttackOutcome` variant, CRT
  redistribution, a fresh balance pass on the changed table) tracked separately (#39).
- ~~The default AI (§3.2) was a one-line heuristic with no standing orders/rules-of-engagement/doctrine
  concept~~ — design settled (#29, see §3.2): a two-axis `StandingOrder` (`stance` ∈
  `Aggressive`/`Defensive`/`HoldFire`, plus an independent `objective: Option<HexCoord>`), associated
  with the design-time `Unit` for persistence but supplied to `resolve_round` as a flattened lookup
  keyed by `source_unit_id`, becoming the new content of "default AI" rather than a third control
  tier — `overrides` still wins outright when present. Target-type prioritization deliberately left
  out of this first vocabulary (nothing on `CombatantState` to filter by yet). Implementation tracked
  separately (#50).

Still open, now tracked as individual issues rather than bullets here:
- Multiplayer vs. single-player-vs-AI vs. pure sandbox replay, and its effect on the WebSocket
  protocol's auth/session model — #32.
- Map size ceiling and whether SVG rendering holds up, or a Canvas/PixiJS rewrite of `HexGrid.vue`
  becomes necessary — #31 (blocked on Phase 3 landing first).
- UI for editing `relationship_type_specs` itself (a custom relationship type beyond the seeded six)
  — #30.
- ~~To-hit (§3.3) was still a simple range-scaled chance with no cover, suppression, or elevation
  modifiers~~ — design settled (#28, see §3.8): terrain modifiers extend `CombatContext`, each
  resolver folds them in independently. Spotting/movement checks/suppression/autofire/HE/ramming all
  scoped as sub-issues of #41; chases/dogfights explicitly parked. A jam/breakdown/counter-fire
  concept for `cepheus_vehicle_v1`'s own version of attacker-side risk (#27) was folded into this
  design pass too and stayed out of scope — nothing in §3.8's mechanic list covers it either, so it
  remains unaddressed if ever picked up.
