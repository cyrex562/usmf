import { defineStore } from 'pinia'
import { api } from '../api/client'
import type {
  Asset,
  AssetValidation,
  ChassisSpec,
  Component,
  CreateAssetRequest,
  CreateChassisSpecRequest,
  CreateComponentRequest,
  ValidateAssetRequest,
} from '../api/types'

export const useDesignStore = defineStore('design', {
  state: () => ({
    components: [] as Component[],
    chassisSpecs: [] as ChassisSpec[],
    assets: [] as Asset[],
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
    async fetchChassisSpecs() {
      this.loading = true
      this.error = null
      try {
        this.chassisSpecs = await api.listChassisSpecs()
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err)
      } finally {
        this.loading = false
      }
    },
    async createChassisSpec(body: CreateChassisSpecRequest) {
      await api.createChassisSpec(body)
      await this.fetchChassisSpecs()
    },
    async fetchAssets() {
      this.loading = true
      this.error = null
      try {
        this.assets = await api.listAssets()
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err)
      } finally {
        this.loading = false
      }
    },
    async createAsset(body: CreateAssetRequest) {
      await api.createAsset(body)
      await this.fetchAssets()
    },
    validateAsset(body: ValidateAssetRequest): Promise<AssetValidation> {
      return api.validateAsset(body)
    },
  },
})
