<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useMapStore } from '../stores/map'
import HexGrid from '../components/HexGrid.vue'
import type { HexCell, HexCoord, TerrainType } from '../api/types'

const store = useMapStore()

const selectedId = ref<number | null>(null)
const selectedTerrain = ref<TerrainType>('plains')
const dirty = ref(false)

const terrainTypes: TerrainType[] = ['plains', 'forest', 'urban', 'water', 'hill', 'road']

const newMap = reactive({
  name: '',
  width: 8,
  height: 8,
})

function cellKey(coord: HexCoord): string {
  return `${coord.q},${coord.r}`
}

// A width x height rectangle of hexes, offset so the grid reads left-to-right,
// top-to-bottom like the width/height inputs imply -- axial coordinates
// don't naturally form a rectangle, so each row's starting q shifts by
// -floor(r/2) to keep the visual bounding box rectangular (matches the
// standard "offset axial" layout for pointy-top hexes).
function blankGrid(width: number, height: number): HexCell[] {
  const cells: HexCell[] = []
  for (let r = 0; r < height; r++) {
    const qOffset = -Math.floor(r / 2)
    for (let q = 0; q < width; q++) {
      cells.push({ coord: { q: q + qOffset, r }, terrain: 'plains', elevation: 0 })
    }
  }
  return cells
}

async function createMap() {
  if (!newMap.name.trim()) return
  const id = await store.createMap({
    name: newMap.name,
    width: newMap.width,
    height: newMap.height,
    cells: blankGrid(newMap.width, newMap.height),
  })
  newMap.name = ''
  await selectMap(id)
}

async function selectMap(id: number) {
  selectedId.value = id
  dirty.value = false
  await store.fetchMap(id)
}

function paint(coord: HexCoord) {
  if (!store.current) return
  const cell = store.current.cells.find((c) => c.coord.q === coord.q && c.coord.r === coord.r)
  if (!cell) return
  cell.terrain = selectedTerrain.value
  dirty.value = true
}

async function save() {
  if (!store.current || selectedId.value === null) return
  await store.updateMap(selectedId.value, {
    name: store.current.name,
    width: store.current.width,
    height: store.current.height,
    cells: store.current.cells,
  })
  dirty.value = false
}

const cellCount = computed(
  () => new Set((store.current?.cells ?? []).map((c) => cellKey(c.coord))).size,
)

onMounted(() => store.fetchMaps())
</script>

<template>
  <section>
    <h1>Map Editor</h1>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <div class="layout">
      <aside>
        <h2>Maps</h2>
        <ul class="map-list">
          <li
            v-for="m in store.maps"
            :key="m.id"
            :class="{ active: m.id === selectedId }"
            @click="selectMap(m.id)"
          >
            {{ m.name }} <span class="dim">{{ m.width }}×{{ m.height }}</span>
          </li>
        </ul>

        <form class="new-map-form" @submit.prevent="createMap">
          <h3>New map</h3>
          <input v-model="newMap.name" placeholder="Name" required />
          <label>Width <input v-model.number="newMap.width" type="number" min="1" /></label>
          <label>Height <input v-model.number="newMap.height" type="number" min="1" /></label>
          <button type="submit">Create map</button>
        </form>
      </aside>

      <div class="editor" v-if="store.current">
        <div class="toolbar">
          <span>Paint:</span>
          <label v-for="t in terrainTypes" :key="t">
            <input v-model="selectedTerrain" type="radio" :value="t" />
            {{ t }}
          </label>
          <button type="button" :disabled="!dirty" @click="save">
            {{ dirty ? 'Save changes' : 'Saved' }}
          </button>
          <span class="dim">{{ cellCount }} hexes</span>
        </div>
        <HexGrid :map="store.current" @hex-click="paint" />
      </div>
      <p v-else-if="!store.loading">Select or create a map to start painting terrain.</p>
    </div>
  </section>
</template>

<style scoped>
.layout {
  display: flex;
  gap: 1.5rem;
  align-items: flex-start;
}
aside {
  flex: 0 0 220px;
}
.editor {
  flex: 1;
  min-width: 0;
}
.map-list {
  list-style: none;
  padding: 0;
  margin: 0 0 1rem;
}
.map-list li {
  padding: 0.35rem 0.5rem;
  cursor: pointer;
  border-radius: 4px;
}
.map-list li:hover {
  background: rgba(255, 255, 255, 0.06);
}
.map-list li.active {
  background: rgba(255, 255, 255, 0.12);
  font-weight: 600;
}
.new-map-form {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}
.new-map-form label {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
}
.new-map-form input[type='number'] {
  width: 5rem;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 1rem;
  margin-bottom: 0.75rem;
  flex-wrap: wrap;
}
.dim {
  opacity: 0.6;
  font-size: 0.9em;
}
.error {
  color: #ff6b6b;
}
</style>
