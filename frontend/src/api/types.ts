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
