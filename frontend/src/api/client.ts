import type {
  Asset,
  AssetValidation,
  ChassisSpec,
  Component,
  CreateAssetRequest,
  CreateChassisSpecRequest,
  CreateComponentRequest,
  CreatePersonnelTypeRequest,
  CreateRelationshipRequest,
  CreateRelationshipTypeRequest,
  DetachRelationshipRequest,
  HexMap,
  PersonnelType,
  PersonnelValidation,
  RelationshipTypeSpec,
  RollupQuery,
  Unit,
  UnitRelationship,
  UnitRollup,
  UpsertMapRequest,
  UpsertUnitRequest,
  ValidateAssetRequest,
  ValidatePersonnelTypeRequest,
} from './types'

const BASE_URL = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080'

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE_URL}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  if (!response.ok) {
    // 4xx responses (e.g. a rejected cyclic relationship) carry a
    // {"error": "..."} body with the actual reason -- surface it instead of
    // just the status code, since that's what the UI shows the user.
    const detail = await response
      .clone()
      .json()
      .then((body) => (typeof body?.error === 'string' ? body.error : null))
      .catch(() => null)
    throw new Error(detail ?? `${init?.method ?? 'GET'} ${path} failed: ${response.status}`)
  }
  return (await response.json()) as T
}

export const api = {
  listComponents: () => request<Component[]>('/api/components'),
  getComponent: (id: number) => request<Component>(`/api/components/${id}`),
  createComponent: (body: CreateComponentRequest) =>
    request<{ id: number }>('/api/components', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  listChassisSpecs: () => request<ChassisSpec[]>('/api/chassis-specs'),
  createChassisSpec: (body: CreateChassisSpecRequest) =>
    request<ChassisSpec>('/api/chassis-specs', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  listAssets: () => request<Asset[]>('/api/assets'),
  getAsset: (id: number) => request<Asset>(`/api/assets/${id}`),
  createAsset: (body: CreateAssetRequest) =>
    request<{ id: number }>('/api/assets', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  validateAsset: (body: ValidateAssetRequest) =>
    request<AssetValidation>('/api/assets/validate', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  listPersonnelTypes: () => request<PersonnelType[]>('/api/personnel-types'),
  getPersonnelType: (id: number) => request<PersonnelType>(`/api/personnel-types/${id}`),
  createPersonnelType: (body: CreatePersonnelTypeRequest) =>
    request<{ id: number }>('/api/personnel-types', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  validatePersonnelType: (body: ValidatePersonnelTypeRequest) =>
    request<PersonnelValidation>('/api/personnel-types/validate', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  listUnits: () => request<Unit[]>('/api/units'),
  getUnit: (id: number) => request<Unit>(`/api/units/${id}`),
  createUnit: (body: UpsertUnitRequest) =>
    request<{ id: number }>('/api/units', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  updateUnit: (id: number, body: UpsertUnitRequest) =>
    request<{ id: number }>(`/api/units/${id}`, {
      method: 'PUT',
      body: JSON.stringify(body),
    }),
  getUnitRollup: (id: number, query: RollupQuery = {}) => {
    const params = new URLSearchParams()
    if (query.as_of !== undefined) params.set('as_of', String(query.as_of))
    if (query.scope !== undefined) params.set('scope', query.scope)
    const qs = params.toString()
    return request<UnitRollup>(`/api/units/${id}/rollup${qs ? `?${qs}` : ''}`)
  },
  listRelationshipTypes: () => request<RelationshipTypeSpec[]>('/api/relationship-types'),
  createRelationshipType: (body: CreateRelationshipTypeRequest) =>
    request<RelationshipTypeSpec>('/api/relationship-types', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  listRelationships: () => request<UnitRelationship[]>('/api/relationships'),
  listRelationshipsForUnit: (id: number) =>
    request<UnitRelationship[]>(`/api/units/${id}/relationships`),
  createRelationship: (body: CreateRelationshipRequest) =>
    request<{ id: number }>('/api/relationships', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  detachRelationship: (id: number, body: DetachRelationshipRequest) =>
    request<{ id: number }>(`/api/relationships/${id}/detach`, {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  listMaps: () => request<HexMap[]>('/api/maps'),
  getMap: (id: number) => request<HexMap>(`/api/maps/${id}`),
  createMap: (body: UpsertMapRequest) =>
    request<{ id: number }>('/api/maps', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  updateMap: (id: number, body: UpsertMapRequest) =>
    request<{ id: number }>(`/api/maps/${id}`, {
      method: 'PUT',
      body: JSON.stringify(body),
    }),
}
