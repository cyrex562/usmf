import { defineStore } from 'pinia'
import { api } from '../api/client'
import type { Component, CreateComponentRequest } from '../api/types'

export const useDesignStore = defineStore('design', {
  state: () => ({
    components: [] as Component[],
    loading: false,
    error: null as string | null,
  }),
  actions: {
    async fetchComponents() {
      this.loading = true
      this.error = null
      try {
        this.components = await api.listComponents()
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err)
      } finally {
        this.loading = false
      }
    },
    async createComponent(body: CreateComponentRequest) {
      await api.createComponent(body)
      await this.fetchComponents()
    },
  },
})
