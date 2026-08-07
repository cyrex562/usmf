<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useDesignStore } from '../stores/design'
import type { PersonnelLoadoutItem, PersonnelValidation } from '../api/types'

const store = useDesignStore()

const draft = reactive({
  name: '',
  role_category: '',
  max_carry_weight: 60,
  max_carry_space: 10,
  loadout: [] as PersonnelLoadoutItem[],
})

const newItem = reactive({ component_id: 0, quantity: 1 })
const validation = ref<PersonnelValidation | null>(null)
const validating = ref(false)

function componentName(id: number): string {
  return store.components.find((c) => c.id === id)?.name ?? `#${id}`
}

function addItem() {
  if (!newItem.component_id) return
  const existing = draft.loadout.find((s) => s.component_id === newItem.component_id)
  if (existing) {
    existing.quantity += newItem.quantity
  } else {
    draft.loadout.push({ component_id: newItem.component_id, quantity: newItem.quantity })
  }
  newItem.quantity = 1
}

function removeItem(componentId: number) {
  draft.loadout = draft.loadout.filter((s) => s.component_id !== componentId)
}

async function revalidate() {
  validating.value = true
  try {
    validation.value = await store.validatePersonnelType({
      role_category: draft.role_category || null,
      max_carry_weight: draft.max_carry_weight,
      max_carry_space: draft.max_carry_space,
      loadout: draft.loadout,
    })
  } finally {
    validating.value = false
  }
}

watch(
  () => [
    draft.max_carry_weight,
    draft.max_carry_space,
    draft.loadout.map((s) => `${s.component_id}:${s.quantity}`).join(','),
  ],
  revalidate,
  { immediate: true },
)

const roleCategories = computed(() =>
  Array.from(
    new Set(store.personnelTypes.map((p) => p.role_category).filter((c): c is string => !!c)),
  ),
)

async function savePersonnelType() {
  if (!draft.name.trim()) return
  await store.createPersonnelType({
    name: draft.name,
    role_category: draft.role_category || null,
    max_carry_weight: draft.max_carry_weight,
    max_carry_space: draft.max_carry_space,
    loadout: draft.loadout,
  })
  draft.name = ''
  draft.loadout = []
}

onMounted(() => {
  store.fetchComponents()
  store.fetchPersonnelTypes()
})
</script>

<template>
  <section>
    <h1>Personnel Designer</h1>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <div class="designer">
      <div class="panel">
        <h2>Build</h2>
        <label>
          Name
          <input v-model="draft.name" placeholder="Rifleman" />
        </label>
        <label>
          Role category
          <input v-model="draft.role_category" placeholder="Infantry" list="role-categories" />
          <datalist id="role-categories">
            <option v-for="rc in roleCategories" :key="rc" :value="rc" />
          </datalist>
        </label>
        <label>
          Max carry weight
          <input v-model.number="draft.max_carry_weight" type="number" min="0" />
        </label>
        <label>
          Max carry space
          <input v-model.number="draft.max_carry_space" type="number" min="0" />
        </label>

        <h3>Loadout</h3>
        <div class="slot-form">
          <select v-model.number="newItem.component_id">
            <option :value="0" disabled>Select a component…</option>
            <option v-for="c in store.components" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
          <input v-model.number="newItem.quantity" type="number" min="1" />
          <button type="button" @click="addItem">Add</button>
        </div>
        <ul class="slot-list">
          <li v-for="item in draft.loadout" :key="item.component_id">
            {{ item.quantity }}× {{ componentName(item.component_id) }}
            <button type="button" @click="removeItem(item.component_id)">✕</button>
          </li>
        </ul>

        <button type="button" :disabled="!draft.name.trim()" @click="savePersonnelType">
          Save personnel type
        </button>
      </div>

      <div class="panel hud">
        <h2>Loadout Dashboard</h2>
        <template v-if="validation">
          <p class="status" :class="{ ok: validation.valid, bad: !validation.valid }">
            {{ validation.valid ? 'Valid' : 'Invalid' }}
          </p>
          <dl>
            <dt>Weight</dt>
            <dd>{{ validation.totals.weight }} / {{ draft.max_carry_weight }}</dd>
            <dt>Space</dt>
            <dd>{{ validation.totals.space }} / {{ draft.max_carry_space }}</dd>
            <dt>Cost</dt>
            <dd>{{ validation.totals.cost }}</dd>
          </dl>
          <ul v-if="validation.violations.length" class="violations">
            <li v-for="v in validation.violations" :key="v">{{ v }}</li>
          </ul>
        </template>
        <p v-else-if="validating">Validating…</p>
      </div>
    </div>

    <h2>Saved personnel types</h2>
    <table v-if="store.personnelTypes.length">
      <thead>
        <tr>
          <th>Name</th>
          <th>Role</th>
          <th>Carry capacity</th>
          <th>Loadout</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="p in store.personnelTypes" :key="p.id">
          <td>{{ p.name }}</td>
          <td>{{ p.role_category ?? '—' }}</td>
          <td>{{ p.max_carry_weight }} wt / {{ p.max_carry_space }} sp</td>
          <td>{{ p.loadout.map((c) => `${c.quantity}× ${componentName(c.component_id)}`).join(', ') || '—' }}</td>
        </tr>
      </tbody>
    </table>
    <p v-else-if="!store.loading">No personnel types yet — build one above.</p>
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
