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
  CreatePersonnelTypeRequest,
  CreateRelationshipRequest,
  DetachRelationshipRequest,
  PersonnelType,
  PersonnelValidation,
  RelationshipTypeSpec,
  RollupQuery,
  Unit,
  UnitRelationship,
  UnitRollup,
  UpsertUnitRequest,
  ValidateAssetRequest,
  ValidatePersonnelTypeRequest,
} from '../api/types'

export const useDesignStore = defineStore('design', {
  state: () => ({
    components: [] as Component[],
    chassisSpecs: [] as ChassisSpec[],
    assets: [] as Asset[],
    personnelTypes: [] as PersonnelType[],
    units: [] as Unit[],
    relationshipTypes: [] as RelationshipTypeSpec[],
    relationships: [] as UnitRelationship[],
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
    async fetchPersonnelTypes() {
      this.loading = true
      this.error = null
      try {
        this.personnelTypes = await api.listPersonnelTypes()
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err)
      } finally {
        this.loading = false
      }
    },
    async createPersonnelType(body: CreatePersonnelTypeRequest) {
      await api.createPersonnelType(body)
      await this.fetchPersonnelTypes()
    },
    validatePersonnelType(body: ValidatePersonnelTypeRequest): Promise<PersonnelValidation> {
      return api.validatePersonnelType(body)
    },
    async fetchUnits() {
      this.loading = true
      this.error = null
      try {
        this.units = await api.listUnits()
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err)
      } finally {
        this.loading = false
      }
    },
    async createUnit(body: UpsertUnitRequest) {
      await api.createUnit(body)
      await this.fetchUnits()
    },
    async updateUnit(id: number, body: UpsertUnitRequest) {
      await api.updateUnit(id, body)
      await this.fetchUnits()
    },
    getUnitRollup(id: number, query: RollupQuery = {}): Promise<UnitRollup> {
      return api.getUnitRollup(id, query)
    },
    async fetchRelationshipTypes() {
      this.relationshipTypes = await api.listRelationshipTypes()
    },
    async fetchRelationships() {
      this.relationships = await api.listRelationships()
    },
    // Deliberately doesn't swallow errors into store.error -- a rejected
    // cyclic relationship needs to surface right next to the attach form,
    // not just in the page-level error banner, so callers catch this directly.
    async createRelationship(body: CreateRelationshipRequest) {
      await api.createRelationship(body)
      await this.fetchRelationships()
    },
    async detachRelationship(id: number, body: DetachRelationshipRequest) {
      await api.detachRelationship(id, body)
      await this.fetchRelationships()
    },
  },
})
