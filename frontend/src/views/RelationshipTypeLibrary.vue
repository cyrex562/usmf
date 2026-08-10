<script setup lang="ts">
import { onMounted, reactive } from 'vue'
import { useDesignStore } from '../stores/design'

const store = useDesignStore()

const form = reactive({
  name: '',
  includes_in_span_of_control: false,
  sustainment_transfers: false,
  includes_in_combat_power_rollup: false,
})

async function submit() {
  if (!form.name.trim()) return
  await store.createRelationshipType({
    name: form.name,
    rules: {
      includes_in_span_of_control: form.includes_in_span_of_control,
      sustainment_transfers: form.sustainment_transfers,
      includes_in_combat_power_rollup: form.includes_in_combat_power_rollup,
    },
  })
  form.name = ''
  form.includes_in_span_of_control = false
  form.sustainment_transfers = false
  form.includes_in_combat_power_rollup = false
}

onMounted(() => store.fetchRelationshipTypes())
</script>

<template>
  <section>
    <h1>Relationship Types</h1>
    <p class="hint">
      The six doctrinal types (Organic, Attached, OPCON, TACON, Direct Support, General Support)
      cover most task organization. Add a custom type here only when none of those fit.
    </p>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <form class="relationship-type-form" @submit.prevent="submit">
      <input v-model="form.name" placeholder="Name" required />
      <label><input v-model="form.includes_in_span_of_control" type="checkbox" /> Span of control</label>
      <label><input v-model="form.sustainment_transfers" type="checkbox" /> Sustainment transfers</label>
      <label><input v-model="form.includes_in_combat_power_rollup" type="checkbox" /> Combat power rollup</label>
      <button type="submit">Add relationship type</button>
    </form>

    <table v-if="store.relationshipTypes.length">
      <thead>
        <tr>
          <th>Name</th>
          <th>Span of control</th>
          <th>Sustainment transfers</th>
          <th>Combat power rollup</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="t in store.relationshipTypes" :key="t.name">
          <td>{{ t.name }}</td>
          <td>{{ t.rules.includes_in_span_of_control ? 'yes' : 'no' }}</td>
          <td>{{ t.rules.sustainment_transfers ? 'yes' : 'no' }}</td>
          <td>{{ t.rules.includes_in_combat_power_rollup ? 'yes' : 'no' }}</td>
        </tr>
      </tbody>
    </table>
    <p v-else-if="!store.loading">No relationship types yet.</p>
  </section>
</template>

<style scoped>
.hint {
  opacity: 0.7;
  max-width: 60ch;
}
.relationship-type-form {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 1rem;
  margin-bottom: 1.5rem;
}
.relationship-type-form input[type='text'],
.relationship-type-form input:not([type]) {
  padding: 0.4rem;
}
.relationship-type-form label {
  display: flex;
  align-items: center;
  gap: 0.35rem;
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
