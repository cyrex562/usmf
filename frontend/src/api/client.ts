import type {
  Asset,
  AssetValidation,
  ChassisSpec,
  Component,
  CreateAssetRequest,
  CreateChassisSpecRequest,
  CreateComponentRequest,
  CreatePersonnelTypeRequest,
  PersonnelType,
  PersonnelValidation,
  Unit,
  UnitRollup,
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
    throw new Error(`${init?.method ?? 'GET'} ${path} failed: ${response.status}`)
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
  getUnitRollup: (id: number) => request<UnitRollup>(`/api/units/${id}/rollup`),
}
