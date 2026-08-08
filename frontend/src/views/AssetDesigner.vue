<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useDesignStore } from '../stores/design'
import type { AssetComponent, AssetValidation } from '../api/types'

const store = useDesignStore()

const draft = reactive({
  name: '',
  chassis_type: '',
  slots: [] as AssetComponent[],
})

const newSlot = reactive({ component_id: 0, quantity: 1 })
const validation = ref<AssetValidation | null>(null)
const validating = ref(false)

function componentName(id: number): string {
  return store.components.find((c) => c.id === id)?.name ?? `#${id}`
}

function addSlot() {
  if (!newSlot.component_id) return
  const existing = draft.slots.find((s) => s.component_id === newSlot.component_id)
  if (existing) {
    existing.quantity += newSlot.quantity
  } else {
    draft.slots.push({ component_id: newSlot.component_id, quantity: newSlot.quantity })
  }
  newSlot.quantity = 1
}

function removeSlot(componentId: number) {
  draft.slots = draft.slots.filter((s) => s.component_id !== componentId)
}

async function revalidate() {
  if (!draft.chassis_type) {
    validation.value = null
    return
  }
  validating.value = true
  try {
    validation.value = await store.validateAsset({
      chassis_type: draft.chassis_type,
      components: draft.slots,
    })
  } finally {
    validating.value = false
  }
}

watch(() => [draft.chassis_type, draft.slots.map((s) => `${s.component_id}:${s.quantity}`).join(',')], revalidate, {
  immediate: true,
})

const chassisSpec = computed(() => store.chassisSpecs.find((c) => c.name === draft.chassis_type))

async function saveAsset() {
  if (!draft.name.trim() || !draft.chassis_type) return
  await store.createAsset({
    name: draft.name,
    chassis_type: draft.chassis_type,
    components: draft.slots,
  })
  draft.name = ''
  draft.slots = []
}

onMounted(async () => {
  await Promise.all([store.fetchComponents(), store.fetchChassisSpecs(), store.fetchAssets()])
  if (!draft.chassis_type && store.chassisSpecs.length) {
    draft.chassis_type = store.chassisSpecs[0].name
  }
})
</script>

<template>
  <section>
    <h1>Asset Designer</h1>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <div class="designer">
      <div class="panel">
        <h2>Build</h2>
        <label>
          Name
          <input v-model="draft.name" placeholder="Scout Car" />
        </label>
        <label>
          Chassis
          <select v-model="draft.chassis_type">
            <option v-for="spec in store.chassisSpecs" :key="spec.name" :value="spec.name">
              {{ spec.name }}
            </option>
          </select>
        </label>

        <h3>Components</h3>
        <div class="slot-form">
          <select v-model.number="newSlot.component_id">
            <option :value="0" disabled>Select a component…</option>
            <option v-for="c in store.components" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
          <input v-model.number="newSlot.quantity" type="number" min="1" />
          <button type="button" @click="addSlot">Add</button>
        </div>
        <ul class="slot-list">
          <li v-for="slot in draft.slots" :key="slot.component_id">
            {{ slot.quantity }}× {{ componentName(slot.component_id) }}
            <button type="button" @click="removeSlot(slot.component_id)">✕</button>
          </li>
        </ul>

        <button type="button" :disabled="!draft.name.trim() || !draft.chassis_type" @click="saveAsset">
          Save asset
        </button>
      </div>

      <div class="panel hud">
        <h2>Engineer's Dashboard</h2>
        <template v-if="validation">
          <p class="status" :class="{ ok: validation.valid, bad: !validation.valid }">
            {{ validation.valid ? 'Valid' : 'Invalid' }}
          </p>
          <dl>
            <dt>Weight</dt>
            <dd>{{ validation.totals.weight }} / {{ chassisSpec?.max_weight ?? '?' }}</dd>
            <dt>Space</dt>
            <dd>{{ validation.totals.space }} / {{ chassisSpec?.max_space ?? '?' }}</dd>
            <dt>Power</dt>
            <dd>{{ validation.totals.power_gen }} gen / {{ validation.totals.power_draw }} draw</dd>
            <dt>Cost</dt>
            <dd>{{ validation.totals.cost }}</dd>
          </dl>
          <ul v-if="validation.violations.length" class="violations">
            <li v-for="v in validation.violations" :key="v">{{ v }}</li>
          </ul>
        </template>
        <p v-else-if="validating">Validating…</p>
        <p v-else>Pick a chassis to see live stats.</p>
      </div>
    </div>

    <h2>Saved assets</h2>
    <table v-if="store.assets.length">
      <thead>
        <tr>
          <th>Name</th>
          <th>Chassis</th>
          <th>Components</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="a in store.assets" :key="a.id">
          <td>{{ a.name }}</td>
          <td>{{ a.chassis_type }}</td>
          <td>{{ a.components.map((c) => `${c.quantity}× ${componentName(c.component_id)}`).join(', ') }}</td>
        </tr>
      </tbody>
    </table>
    <p v-else-if="!store.loading">No assets yet — build one above.</p>
  </section>
</template>

<style scoped>
.designer {
  display: flex;
  gap: 1.5rem;
  margin-bottom: 2rem;
}
.panel {
  flex: 1;
  border: 1px solid #444;
  border-radius: 6px;
  padding: 1rem;
}
.panel label {
  display: block;
  margin-bottom: 0.6rem;
}
.panel input,
.panel select {
  display: block;
  width: 100%;
  padding: 0.4rem;
  margin-top: 0.2rem;
  box-sizing: border-box;
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
.status {
  font-weight: 600;
}
.status.ok {
  color: #4caf50;
}
.status.bad {
  color: #ff6b6b;
}
.violations {
  color: #ff6b6b;
  margin-top: 0.8rem;
}
table {
  border-collapse: collapse;
  width: 100%;
}
th,
td {
  border: 1px solid #444;
  padding: 0.4rem 0.6rem;
  text-align: left;
}
.error {
  color: #ff6b6b;
}
</style>
