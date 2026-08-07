import type { Component, CreateComponentRequest } from './types'

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
}
