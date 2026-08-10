import { defineStore } from 'pinia'
import { api } from '../api/client'
import type { HexMap, UpsertMapRequest } from '../api/types'

export const useMapStore = defineStore('map', {
  state: () => ({
    maps: [] as HexMap[],
    current: null as HexMap | null,
    loading: false,
    error: null as string | null,
  }),
  actions: {
    async fetchMaps() {
      this.loading = true
      this.error = null
      try {
        this.maps = await api.listMaps()
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err)
      } finally {
        this.loading = false
      }
    },
    async fetchMap(id: number) {
      this.loading = true
      this.error = null
      try {
        this.current = await api.getMap(id)
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err)
      } finally {
        this.loading = false
      }
    },
    async createMap(body: UpsertMapRequest) {
      const { id } = await api.createMap(body)
      await this.fetchMaps()
      return id
    },
    async updateMap(id: number, body: UpsertMapRequest) {
      await api.updateMap(id, body)
      await this.fetchMaps()
    },
  },
})
