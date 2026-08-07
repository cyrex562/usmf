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
