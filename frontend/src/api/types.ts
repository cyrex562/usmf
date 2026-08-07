// Mirrors backend/crates/usmf-core's serde types. Hand-maintained for now;
// see design_doc.md section 4.1 for the plan to generate these from Rust once
// the API surface stabilizes (ts-rs/specta).

export type ComponentType =
  | 'weapon'
  | 'engine'
  | 'power'
  | 'sensor'
  | 'armor'
  | 'comms'
  | 'logistics'

export interface ComponentStats {
  weight: number
  space: number
  cost: number
  power_gen: number
  power_draw: number
  damage: number
  range_hexes: number
  initiative: number
  capabilities: Record<string, number>
}

export interface Component {
  id: number
  name: string
  component_type: ComponentType
  stats: ComponentStats
}

export type CreateComponentRequest = {
  name: string
  component_type: ComponentType
  stats: Partial<ComponentStats>
}

export interface ChassisSpec {
  name: string
  max_weight: number
  max_space: number
  base_cost: number
}

export type CreateChassisSpecRequest = ChassisSpec

export interface AssetComponent {
  component_id: number
  quantity: number
}

export interface Asset {
  id: number
  name: string
  chassis_type: string
  components: AssetComponent[]
}

export type CreateAssetRequest = {
  name: string
  chassis_type: string
  components: AssetComponent[]
}

export interface AssetTotals {
  weight: number
  space: number
  cost: number
  power_gen: number
  power_draw: number
  initiative: number
  capabilities: Record<string, number>
}

export interface AssetValidation {
  valid: boolean
  violations: string[]
  totals: AssetTotals
}

export type ValidateAssetRequest = {
  chassis_type: string
  components: AssetComponent[]
}

export interface PersonnelLoadoutItem {
  component_id: number
  quantity: number
}

// Structurally identical to AssetTotals/AssetValidation -- both are usmf-core's
// shared LoadoutTotals/LoadoutValidation under the hood (see loadout.rs).
export type PersonnelTotals = AssetTotals
export type PersonnelValidation = AssetValidation

export interface PersonnelType {
  id: number
  name: string
  role_category: string | null
  max_carry_weight: number
  max_carry_space: number
  base_cost: number
  loadout: PersonnelLoadoutItem[]
}

export type CreatePersonnelTypeRequest = {
  name: string
  role_category?: string | null
  max_carry_weight: number
  max_carry_space: number
  base_cost?: number
  loadout: PersonnelLoadoutItem[]
}

export type ValidatePersonnelTypeRequest = {
  role_category?: string | null
  max_carry_weight: number
  max_carry_space: number
  base_cost?: number
  loadout: PersonnelLoadoutItem[]
}

export type UnitType = 'hq' | 'line' | 'support'
export type FormationKind = 'standing' | 'task_force'

export interface UnitAsset {
  asset_id: number
  quantity: number
}

export interface UnitPersonnelEntry {
  personnel_type_id: number
  quantity: number
}

// Mirrors usmf_core::unit::PersonnelComposition -- an internally-tagged enum
// (tag = "mode") over struct variants.
export type PersonnelComposition =
  | { mode: 'simplified'; count: number }
  | { mode: 'detailed'; entries: UnitPersonnelEntry[] }

export interface Unit {
  id: number
  name: string
  unit_type: UnitType
  formation_kind: FormationKind
  own_assets: UnitAsset[]
  personnel: PersonnelComposition
  c2_capacity: number | null
}

export type UpsertUnitRequest = {
  name: string
  unit_type: UnitType
  formation_kind?: FormationKind
  own_assets?: UnitAsset[]
  personnel?: PersonnelComposition
  c2_capacity?: number | null
}

export interface UnitRollup {
  weight: number
  cost: number
  personnel_headcount: number
  daily_supply_consumption: number
  capabilities: Record<string, number>
  span_of_control_warnings: string[]
}

export interface RelationshipRules {
  includes_in_span_of_control: boolean
  sustainment_transfers: boolean
  includes_in_combat_power_rollup: boolean
}

export interface RelationshipTypeSpec {
  name: string
  rules: RelationshipRules
}

export interface UnitRelationship {
  id: number
  superior_unit_id: number
  subordinate_unit_id: number
  relationship_type: string
  rules: RelationshipRules
  effective_from_turn: number | null
  effective_until_turn: number | null
  notes: string | null
}

export type CreateRelationshipRequest = {
  superior_unit_id: number
  subordinate_unit_id: number
  relationship_type: string
  effective_from_turn?: number | null
  effective_until_turn?: number | null
  notes?: string | null
}

export type DetachRelationshipRequest = {
  effective_until_turn: number
}
