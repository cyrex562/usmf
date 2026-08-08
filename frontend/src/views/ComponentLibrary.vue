<script setup lang="ts">
import { onMounted, reactive } from 'vue'
import { useDesignStore } from '../stores/design'
import type { ComponentType } from '../api/types'

const store = useDesignStore()

const form = reactive({
  name: '',
  component_type: 'weapon' as ComponentType,
  weight: 0,
  space: 0,
  cost: 0,
  power_gen: 0,
  power_draw: 0,
  damage: 0,
  range_hexes: 0,
})

const componentTypes: ComponentType[] = [
  'weapon',
  'engine',
  'power',
  'sensor',
  'armor',
  'comms',
  'logistics',
]

async function submit() {
  if (!form.name.trim()) return
  await store.createComponent({
    name: form.name,
    component_type: form.component_type,
    stats: {
      weight: form.weight,
      space: form.space,
      cost: form.cost,
      power_gen: form.power_gen,
      power_draw: form.power_draw,
      damage: form.damage,
      range_hexes: form.range_hexes,
    },
  })
  form.name = ''
}

onMounted(() => store.fetchComponents())
</script>

<template>
  <section>
    <h1>Component Library</h1>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <form class="component-form" @submit.prevent="submit">
      <input v-model="form.name" placeholder="Name" required />
      <select v-model="form.component_type">
        <option v-for="type in componentTypes" :key="type" :value="type">{{ type }}</option>
      </select>
      <input v-model.number="form.weight" type="number" placeholder="Weight" />
      <input v-model.number="form.space" type="number" placeholder="Space" />
      <input v-model.number="form.cost" type="number" placeholder="Cost" />
      <input v-model.number="form.power_gen" type="number" placeholder="Power gen" />
      <input v-model.number="form.power_draw" type="number" placeholder="Power draw" />
      <button type="submit">Add component</button>
    </form>

    <table v-if="store.components.length">
      <thead>
        <tr>
          <th>Name</th>
          <th>Type</th>
          <th>Weight</th>
          <th>Space</th>
          <th>Cost</th>
          <th>Power gen</th>
          <th>Power draw</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="c in store.components" :key="c.id">
          <td>{{ c.name }}</td>
          <td>{{ c.component_type }}</td>
          <td>{{ c.stats.weight }}</td>
          <td>{{ c.stats.space }}</td>
          <td>{{ c.stats.cost }}</td>
          <td>{{ c.stats.power_gen }}</td>
          <td>{{ c.stats.power_draw }}</td>
        </tr>
      </tbody>
    </table>
    <p v-else-if="!store.loading">No components yet — add one above.</p>
  </section>
</template>

<style scoped>
.component-form {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-bottom: 1.5rem;
}
.component-form input,
.component-form select {
  padding: 0.4rem;
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
