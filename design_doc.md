# Design Document: Proving Ground (V2)

## 1. Core Concepts & Hierarchy

To accommodate your feedback, we will strictly enforce a 3-tier hierarchy:

1. **Components (The "Parts"):** The smallest building blocks.
* *Examples:* "120mm Smoothbore Cannon", "Fusion Core Generator", "Standard Ration Pack", "Cyberdeck Mk1".
* *Data:* Weight, Cost, Power Draw, Space Required, Capability Tags.


2. **Assets (The "Entities"):** The atomic fighting/functional elements designed in the **Asset Designer**.
* *Examples:* "M1A5 Tank" (Platform), "Rifle Squad" (Crew), "Logistics Drone" (Platform).
* *Constraint Logic:* Sum of Components cannot exceed Asset Capacity (Weight, Power, Space).


3. **Units (The "Formations"):** The organizational hierarchy designed in the **Formation Designer**.
* *Examples:* "1st Armored Brigade", "Recon Platoon".
* *Constraint Logic:* Span of Control (C2 Burden), Logistics Throughput vs. Consumption.



## 2. The User Interface Architecture

We will implement two distinct "Designer" workflows in the Web App.

### A. The Asset Designer (Physical Engineering)

* **Goal:** Build a functional platform or team.
* **Input:** Select a Chassis/Framework (e.g., "Heavy Tracked Chassis" or "Infantry Fireteam Slots").
* **Action:** Slot **Components** into the Chassis.
* **Real-time Feedback (The "Engineer's Dashboard"):**
* **Weight/Space:** Progress bars showing `Current / Max`.
* **Power Grid:** Graph showing `Generation vs. Draw` (Idle vs. Combat Load).
* **Cost:** Running total of production cost.
* **C2 Cost:** "This asset requires 2 Command Points to manage."



### B. The Unit Designer (Force Structure)

* **Goal:** Organize Assets into effective fighting forces (TO&E).
* **View:** Recursive Tree View (Drag-and-Drop).
* **Logic:**
* **Root:** The Division/Brigade HQ.
* **Branches:** Sub-units (Battalions, Companies).
* **Leaves:** The Assets (Tanks, Squads).


* **Real-time Feedback (The "Commander's Dashboard"):**
* **Span of Control:** If a HQ unit has `C2_Cap: 10` but is linked to 12 sub-units, trigger a warning.
* **Logistics Pulse:** Calculate `Daily_Consumption` (Fuel/Food) vs `Organic_Supply_Cap` (Trucks assigned to unit).
* **Capability Aggregation:** "This Brigade has: Level 4 Cyber, Level 2 Indirect Fire, Level 0 Anti-Air."



## 3. Data Model Strategy (Enhanced)

We need a recursive structure for Units and a component-slot system for Assets.

### Tables (SQLAlchemy)

* **Component:** `id`, `name`, `type` (Sensor, Weapon, Engine), `attributes` (JSON: weight, power, cost).
* **Asset:** `id`, `name`, `chassis_type`.
* **AssetComponents:** Link table. `asset_id`, `component_id`, `quantity`.
* **Unit:** `id`, `name`, `parent_unit_id` (Self-referential FK), `asset_id` (Nullable - if this node is a physical asset), `unit_type` (HQ, Line, Support).

### Computed Properties (Python/Rust Logic)

* `Unit.get_total_supply_draw()`: Recursively sums the consumption of all child nodes.
* `Unit.get_c2_load()`: Sum of C2 cost of immediate children.

## 4. Revised Task List

This is the updated roadmap for your IDE/Agent.

### Phase 1: Foundation & Component Library

* [x] **1.0 Setup:** Initialize FastAPI, SQLAlchemy (Async), Jinja2, HTMX.
* [x] **1.1 Component Model:** Create `Component` model with a JSON `stats` field (flexible for "Fuel" vs "Antimatter").
* [x] **1.2 Component Seeder:** Create a script to populate the DB with 10 basic items (Engine, Rifle, Radio, Rations) so we have test data.

### Phase 2: The Asset Designer (The "Physical" Layer)

* [x] **2.0 Asset Schema:** Create `Asset` and `AssetComponent` models.
* [x] **2.1 Designer UI:** Create `asset_designer.html`.
* Left Panel: Component Library (Searchable list).
* Center Panel: The "Chassis" (Slots).
* Right Panel: "The HUD" (Live updates of Weight, Power, Cost).


* [x] **2.2 Validation Logic:** Write a Python service `validate_asset(asset_id)` that returns `valid: boolean` and `violations: list` (e.g., "Power draw exceeds generator output").

### Phase 3: The Unit Designer (The "Strategic" Layer)

* [x] **3.0 Unit Schema:** Create the recursive `Unit` model.
* [x] **3.1 Tree View UI:** Create `unit_designer.html`. Use a recursive Jinja template or a JS tree library to visualize the hierarchy.
* [x] **3.2 Drag-and-Drop API:** Implement endpoints to `move_unit(child_id, new_parent_id)`.
* [x] **3.3 Calculation Services:**
* `calculate_logistics(unit_id)`: Sums ammo/fuel usage vs supply capacity.
* `calculate_c2(unit_id)`: Checks Span of Control limits.


* [x] **3.4 Capability Tags:** Implement logic to aggregate tags. (e.g., If 3 sub-units have "Cyber", the Parent Unit gets "Cyber Support: High").

### Phase 4: Simulation Integration (The "Test")

* [x] **4.0 Scenario Definition:** Update to accept full **Unit Trees** instead of just lists of assets.
* [x] **4.1 Logic Update:**
* *Morale Propagation:* If a sub-unit takes losses, propagate morale shock up the tree.
* *Supply Check:* If the parent unit runs out of generic "Supply Points", child units suffer penalties.
* *C2 Severance:* If a HQ unit is destroyed, detach child units (AI takes over locally with penalties).
