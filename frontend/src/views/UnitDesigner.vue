<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { useDesignStore } from '../stores/design'
import type { UnitAsset, UnitPersonnelEntry, UnitRollup, UnitType, UpsertUnitRequest } from '../api/types'

const store = useDesignStore()

const selectedId = ref<number | null>(null)
const rollup = ref<UnitRollup | null>(null)
const rollupLoading = ref(false)

const draft = reactive({
  name: '',
  unit_type: 'line' as UnitType,
  c2_capacity: null as number | null,
  own_assets: [] as UnitAsset[],
  personnelMode: 'simplified' as 'simplified' | 'detailed',
  simplifiedCount: 0,
  detailedEntries: [] as UnitPersonnelEntry[],
})

const unitTypes: UnitType[] = ['hq', 'line', 'support']

const newAsset = reactive({ asset_id: 0, quantity: 1 })
const newPersonnel = reactive({ personnel_type_id: 0, quantity: 1 })

function assetName(id: number): string {
  return store.assets.find((a) => a.id === id)?.name ?? `#${id}`
}
function personnelTypeName(id: number): string {
  return store.personnelTypes.find((p) => p.id === id)?.name ?? `#${id}`
}

function addAsset() {
  if (!newAsset.asset_id) return
  const existing = draft.own_assets.find((a) => a.asset_id === newAsset.asset_id)
  if (existing) {
    existing.quantity += newAsset.quantity
  } else {
    draft.own_assets.push({ asset_id: newAsset.asset_id, quantity: newAsset.quantity })
  }
  newAsset.quantity = 1
}
function removeAsset(assetId: number) {
  draft.own_assets = draft.own_assets.filter((a) => a.asset_id !== assetId)
}

function addPersonnel() {
  if (!newPersonnel.personnel_type_id) return
  const existing = draft.detailedEntries.find(
    (e) => e.personnel_type_id === newPersonnel.personnel_type_id,
  )
  if (existing) {
    existing.quantity += newPersonnel.quantity
  } else {
    draft.detailedEntries.push({
      personnel_type_id: newPersonnel.personnel_type_id,
      quantity: newPersonnel.quantity,
    })
  }
  newPersonnel.quantity = 1
}
function removePersonnel(personnelTypeId: number) {
  draft.detailedEntries = draft.detailedEntries.filter(
    (e) => e.personnel_type_id !== personnelTypeId,
  )
}

function resetDraft() {
  selectedId.value = null
  draft.name = ''
  draft.unit_type = 'line'
  draft.c2_capacity = null
  draft.own_assets = []
  draft.personnelMode = 'simplified'
  draft.simplifiedCount = 0
  draft.detailedEntries = []
  rollup.value = null
}

async function selectUnit(id: number) {
  const unit = store.units.find((u) => u.id === id)
  if (!unit) return
  selectedId.value = unit.id
  draft.name = unit.name
  draft.unit_type = unit.unit_type
  draft.c2_capacity = unit.c2_capacity
  draft.own_assets = unit.own_assets.map((a) => ({ ...a }))
  if (unit.personnel.mode === 'detailed') {
    draft.personnelMode = 'detailed'
    draft.detailedEntries = unit.personnel.entries.map((e) => ({ ...e }))
    draft.simplifiedCount = 0
  } else {
    draft.personnelMode = 'simplified'
    draft.simplifiedCount = unit.personnel.count
    draft.detailedEntries = []
  }
  await refreshRollup()
}

async function refreshRollup() {
  if (selectedId.value === null) return
  rollupLoading.value = true
  try {
    rollup.value = await store.getUnitRollup(selectedId.value)
  } finally {
    rollupLoading.value = false
  }
}

function buildRequest(): UpsertUnitRequest {
  return {
    name: draft.name,
    unit_type: draft.unit_type,
    c2_capacity: draft.c2_capacity,
    own_assets: draft.own_assets,
    personnel:
      draft.personnelMode === 'simplified'
        ? { mode: 'simplified', count: draft.simplifiedCount }
        : { mode: 'detailed', entries: draft.detailedEntries },
  }
}

async function saveUnit() {
  if (!draft.name.trim()) return
  if (selectedId.value !== null) {
    await store.updateUnit(selectedId.value, buildRequest())
  } else {
    await store.createUnit(buildRequest())
    const created = store.units[store.units.length - 1]
    if (created) selectedId.value = created.id
  }
  await refreshRollup()
}

onMounted(async () => {
  await Promise.all([
    store.fetchComponents(),
    store.fetchAssets(),
    store.fetchPersonnelTypes(),
    store.fetchUnits(),
  ])
})
</script>

<template>
  <section>
    <h1>Unit Designer</h1>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <div class="layout">
      <div class="panel unit-list">
        <h2>Units</h2>
        <button type="button" @click="resetDraft">+ New unit</button>
        <ul>
          <li
            v-for="u in store.units"
            :key="u.id"
            :class="{ active: u.id === selectedId }"
            @click="selectUnit(u.id)"
          >
            {{ u.name }} <span class="tag">{{ u.unit_type }}</span>
          </li>
        </ul>
        <p v-if="!store.units.length && !store.loading">No units yet.</p>
      </div>

      <div class="panel">
        <h2>{{ selectedId === null ? 'New Unit' : 'Edit Unit' }}</h2>
        <label>
          Name
          <input v-model="draft.name" placeholder="1st Rifle Squad" />
        </label>
        <label>
          Unit type
          <select v-model="draft.unit_type">
            <option v-for="t in unitTypes" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
        <label>
          C2 capacity (span of control, blank = unlimited)
          <input v-model.number="draft.c2_capacity" type="number" min="0" />
        </label>

        <h3>Own assets</h3>
        <div class="slot-form">
          <select v-model.number="newAsset.asset_id">
            <option :value="0" disabled>Select an asset…</option>
            <option v-for="a in store.assets" :key="a.id" :value="a.id">{{ a.name }}</option>
          </select>
          <input v-model.number="newAsset.quantity" type="number" min="1" />
          <button type="button" @click="addAsset">Add</button>
        </div>
        <ul class="slot-list">
          <li v-for="a in draft.own_assets" :key="a.asset_id">
            {{ a.quantity }}× {{ assetName(a.asset_id) }}
            <button type="button" @click="removeAsset(a.asset_id)">✕</button>
          </li>
        </ul>

        <h3>Personnel</h3>
        <label class="radio">
          <input type="radio" value="simplified" v-model="draft.personnelMode" />
          Simplified headcount
        </label>
        <label class="radio">
          <input type="radio" value="detailed" v-model="draft.personnelMode" />
          Detailed roster
        </label>

        <div v-if="draft.personnelMode === 'simplified'">
          <label>
            Headcount
            <input v-model.number="draft.simplifiedCount" type="number" min="0" />
          </label>
        </div>
        <div v-else>
          <div class="slot-form">
            <select v-model.number="newPersonnel.personnel_type_id">
              <option :value="0" disabled>Select a personnel type…</option>
              <option v-for="p in store.personnelTypes" :key="p.id" :value="p.id">
                {{ p.name }}
              </option>
            </select>
            <input v-model.number="newPersonnel.quantity" type="number" min="1" />
            <button type="button" @click="addPersonnel">Add</button>
          </div>
          <ul class="slot-list">
            <li v-for="e in draft.detailedEntries" :key="e.personnel_type_id">
              {{ e.quantity }}× {{ personnelTypeName(e.personnel_type_id) }}
              <button type="button" @click="removePersonnel(e.personnel_type_id)">✕</button>
            </li>
          </ul>
        </div>

        <button type="button" :disabled="!draft.name.trim()" @click="saveUnit">
          {{ selectedId === null ? 'Create unit' : 'Save changes' }}
        </button>
      </div>

      <div class="panel hud">
        <h2>Rollup</h2>
        <template v-if="rollup">
          <dl>
            <dt>Weight</dt>
            <dd>{{ rollup.weight }}</dd>
            <dt>Cost</dt>
            <dd>{{ rollup.cost }}</dd>
            <dt>Personnel</dt>
            <dd>{{ rollup.personnel_headcount }}</dd>
            <dt>Daily supply draw</dt>
            <dd>{{ rollup.daily_supply_consumption }}</dd>
          </dl>
          <p v-if="Object.keys(rollup.capabilities).length">
            <strong>Capabilities:</strong>
            {{ Object.entries(rollup.capabilities).map(([k, v]) => `${k}: ${v}`).join(', ') }}
          </p>
          <ul v-if="rollup.span_of_control_warnings.length" class="violations">
            <li v-for="w in rollup.span_of_control_warnings" :key="w">{{ w }}</li>
          </ul>
        </template>
        <p v-else-if="rollupLoading">Loading…</p>
        <p v-else-if="selectedId === null">Save this unit to see its rollup.</p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.layout {
  display: flex;
  gap: 1.5rem;
  align-items: flex-start;
}
.panel {
  flex: 1;
  border: 1px solid #444;
  border-radius: 6px;
  padding: 1rem;
}
.unit-list {
  flex: 0 0 220px;
}
.unit-list ul {
  list-style: none;
  padding: 0;
  margin: 0.6rem 0 0;
}
.unit-list li {
  padding: 0.4rem 0.3rem;
  cursor: pointer;
  border-radius: 4px;
}
.unit-list li:hover {
  background: rgba(255, 255, 255, 0.05);
}
.unit-list li.active {
  background: rgba(100, 150, 255, 0.15);
  font-weight: 600;
}
.tag {
  opacity: 0.6;
  font-size: 0.85em;
}
.panel label {
  display: block;
  margin-bottom: 0.6rem;
}
.panel label.radio {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  margin-right: 1rem;
  width: auto;
}
.panel input,
.panel select {
  display: block;
  width: 100%;
  padding: 0.4rem;
  margin-top: 0.2rem;
  box-sizing: border-box;
}
.panel label.radio input {
  display: inline;
  width: auto;
}
.slot-form {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 0.6rem;
}
.slot-form select {
  flex: 1;
}
.slot-form input {
  width: 4rem;
}
.slot-list {
  list-style: none;
  padding: 0;
  margin: 0 0 1rem;
}
.slot-list li {
  display: flex;
  justify-content: space-between;
  padding: 0.2rem 0;
}
.hud dl {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 0.3rem 0.8rem;
}
.hud dt {
  opacity: 0.7;
}
.violations {
  color: #ff6b6b;
  margin-top: 0.8rem;
}
.error {
  color: #ff6b6b;
}
</style>
