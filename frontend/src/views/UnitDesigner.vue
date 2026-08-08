<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useDesignStore } from '../stores/design'
import type {
  FormationKind,
  RollupScope,
  UnitAsset,
  UnitPersonnelEntry,
  UnitRelationship,
  UnitRollup,
  UnitType,
  UpsertUnitRequest,
} from '../api/types'

const store = useDesignStore()

const selectedId = ref<number | null>(null)
const rollup = ref<UnitRollup | null>(null)
const rollupLoading = ref(false)
const rollupScope = ref<RollupScope>('effective')

watch(rollupScope, () => refreshRollup())

const draft = reactive({
  name: '',
  unit_type: 'line' as UnitType,
  formation_kind: 'standing' as FormationKind,
  c2_capacity: null as number | null,
  own_assets: [] as UnitAsset[],
  personnelMode: 'simplified' as 'simplified' | 'detailed',
  simplifiedCount: 0,
  detailedEntries: [] as UnitPersonnelEntry[],
})

const unitTypes: UnitType[] = ['hq', 'line', 'support']
const formationKinds: FormationKind[] = ['standing', 'task_force']

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

function unitName(id: number): string {
  return store.units.find((u) => u.id === id)?.name ?? `#${id}`
}

// Organic tree, built from the full relationship set (design-time preview
// ignores from/until turn bounds, same as the backend's as_of: None -- there's
// no live simulation clock yet to filter against). Roots are units that never
// appear as an Organic subordinate, which naturally includes standalone units
// with no relationships at all.
const organicTree = computed(() => {
  const organic = store.relationships.filter((r) => r.relationship_type === 'Organic')
  const childrenOf = new Map<number, number[]>()
  const hasOrganicSuperior = new Set<number>()
  for (const r of organic) {
    if (!childrenOf.has(r.superior_unit_id)) childrenOf.set(r.superior_unit_id, [])
    childrenOf.get(r.superior_unit_id)!.push(r.subordinate_unit_id)
    hasOrganicSuperior.add(r.subordinate_unit_id)
  }

  const nodes: { id: number; depth: number }[] = []
  const visiting = new Set<number>()
  function visit(id: number, depth: number) {
    // Defensive: a data inconsistency shouldn't hang the UI in a loop.
    if (visiting.has(id)) return
    visiting.add(id)
    nodes.push({ id, depth })
    for (const childId of childrenOf.get(id) ?? []) visit(childId, depth + 1)
    visiting.delete(id)
  }
  for (const unit of store.units) {
    if (!hasOrganicSuperior.has(unit.id)) visit(unit.id, 0)
  }
  return nodes
})

const relationshipForm = reactive({
  otherUnitId: 0,
  direction: 'reports_to' as 'reports_to' | 'commands',
  relationship_type: 'Organic',
  effective_from_turn: null as number | null,
  effective_until_turn: null as number | null,
  notes: '',
})
const relationshipError = ref<string | null>(null)

const selectedRelationships = computed(() => {
  if (selectedId.value === null) return []
  const id = selectedId.value
  return store.relationships.filter(
    (r) => r.superior_unit_id === id || r.subordinate_unit_id === id,
  )
})

async function submitRelationship() {
  if (selectedId.value === null || !relationshipForm.otherUnitId) return
  relationshipError.value = null
  const [superior_unit_id, subordinate_unit_id] =
    relationshipForm.direction === 'commands'
      ? [selectedId.value, relationshipForm.otherUnitId]
      : [relationshipForm.otherUnitId, selectedId.value]

  try {
    await store.createRelationship({
      superior_unit_id,
      subordinate_unit_id,
      relationship_type: relationshipForm.relationship_type,
      effective_from_turn: relationshipForm.effective_from_turn,
      effective_until_turn: relationshipForm.effective_until_turn,
      notes: relationshipForm.notes || null,
    })
    relationshipForm.otherUnitId = 0
    relationshipForm.notes = ''
  } catch (err) {
    relationshipError.value = err instanceof Error ? err.message : String(err)
  }
}

async function endRelationship(rel: UnitRelationship) {
  const input = window.prompt('End this relationship at turn:', '0')
  if (input === null) return
  const turn = Number(input)
  if (!Number.isFinite(turn)) return
  relationshipError.value = null
  try {
    await store.detachRelationship(rel.id, { effective_until_turn: turn })
  } catch (err) {
    relationshipError.value = err instanceof Error ? err.message : String(err)
  }
}

function resetDraft() {
  selectedId.value = null
  draft.name = ''
  draft.unit_type = 'line'
  draft.formation_kind = 'standing'
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
  draft.formation_kind = unit.formation_kind
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
  const id = selectedId.value
  const scope = rollupScope.value
  if (id === null) return
  rollupLoading.value = true
  try {
    const result = await store.getUnitRollup(id, { scope })
    // Guard against a slower, earlier fetch resolving after the user has
    // since selected a different unit or toggled scope again.
    if (selectedId.value === id && rollupScope.value === scope) rollup.value = result
  } finally {
    if (selectedId.value === id && rollupScope.value === scope) rollupLoading.value = false
  }
}

function buildRequest(): UpsertUnitRequest {
  return {
    name: draft.name,
    unit_type: draft.unit_type,
    formation_kind: draft.formation_kind,
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
    store.fetchRelationshipTypes(),
    store.fetchRelationships(),
  ])
})
</script>

<template>
  <section>
    <h1>Unit Designer</h1>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <div class="layout">
      <div class="panel unit-list">
        <h2>Units (organic tree)</h2>
        <button type="button" @click="resetDraft">+ New unit</button>
        <ul>
          <li
            v-for="node in organicTree"
            :key="node.id"
            :class="{ active: node.id === selectedId }"
            :style="{ paddingLeft: `${0.3 + node.depth * 1.1}rem` }"
            @click="selectUnit(node.id)"
          >
            {{ unitName(node.id) }}
            <span class="tag">{{ store.units.find((u) => u.id === node.id)?.unit_type }}</span>
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
          Formation kind
          <select v-model="draft.formation_kind">
            <option v-for="f in formationKinds" :key="f" :value="f">{{ f }}</option>
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
        <h2>Commander's Dashboard</h2>
        <div class="scope-toggle" v-if="selectedId !== null">
          <label class="radio">
            <input type="radio" value="effective" v-model="rollupScope" />
            Effective command tree
          </label>
          <label class="radio">
            <input type="radio" value="organic" v-model="rollupScope" />
            Organic tree only
          </label>
        </div>
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

    <div class="panel relationships" v-if="selectedId !== null">
      <h2>Command relationships — {{ unitName(selectedId) }}</h2>
      <p v-if="relationshipError" class="error">{{ relationshipError }}</p>

      <table v-if="selectedRelationships.length">
        <thead>
          <tr>
            <th>Type</th>
            <th>Superior</th>
            <th>Subordinate</th>
            <th>From → Until</th>
            <th>Notes</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="rel in selectedRelationships" :key="rel.id">
            <td>{{ rel.relationship_type }}</td>
            <td>{{ unitName(rel.superior_unit_id) }}</td>
            <td>{{ unitName(rel.subordinate_unit_id) }}</td>
            <td>{{ rel.effective_from_turn ?? '—' }} → {{ rel.effective_until_turn ?? 'open' }}</td>
            <td>{{ rel.notes ?? '—' }}</td>
            <td><button type="button" @click="endRelationship(rel)">Detach</button></td>
          </tr>
        </tbody>
      </table>
      <p v-else>No relationships yet for this unit.</p>

      <h3>Add relationship</h3>
      <div class="relationship-form">
        <select v-model="relationshipForm.direction">
          <option value="commands">{{ unitName(selectedId) }} commands…</option>
          <option value="reports_to">{{ unitName(selectedId) }} reports to…</option>
        </select>
        <select v-model.number="relationshipForm.otherUnitId">
          <option :value="0" disabled>Select a unit…</option>
          <option v-for="u in store.units.filter((u) => u.id !== selectedId)" :key="u.id" :value="u.id">
            {{ u.name }}
          </option>
        </select>
        <select v-model="relationshipForm.relationship_type">
          <option v-for="t in store.relationshipTypes" :key="t.name" :value="t.name">
            {{ t.name }}
          </option>
        </select>
        <input
          v-model.number="relationshipForm.effective_from_turn"
          type="number"
          placeholder="From turn (optional)"
        />
        <input
          v-model.number="relationshipForm.effective_until_turn"
          type="number"
          placeholder="Until turn (optional)"
        />
        <input v-model="relationshipForm.notes" placeholder="Notes (optional)" />
        <button type="button" :disabled="!relationshipForm.otherUnitId" @click="submitRelationship">
          Add
        </button>
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
.scope-toggle {
  margin-bottom: 0.8rem;
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
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
.relationships {
  margin-top: 1.5rem;
}
.relationships table {
  border-collapse: collapse;
  width: 100%;
  margin-bottom: 1rem;
}
.relationships th,
.relationships td {
  border: 1px solid #444;
  padding: 0.4rem 0.6rem;
  text-align: left;
}
.relationship-form {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}
.relationship-form select,
.relationship-form input {
  padding: 0.4rem;
}
</style>
