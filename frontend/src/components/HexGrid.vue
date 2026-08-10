<script setup lang="ts">
// Canvas (PixiJS) hex-grid renderer, shared by MapEditor.vue and (once Phase
// 4 lands) SimulationViewer.vue -- see design_doc.md §4.2. Chosen over plain
// SVG (issue #31) because both consumers need more than terrain fill: unit
// markers, movement paths, LOS lines, range rings, and eventually MIL-STD-
// 2525/APP-6 symbology all need to draw as vector shapes over the grid, at a
// scale (a full simulation-run event log, hundreds of hexes) SVG's DOM-per-
// element cost doesn't hold up well against. Pointy-top axial coordinates,
// matching usmf-core::HexCoord's own doc comment.
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Application, Container, Graphics } from 'pixi.js'
import type { HexCoord, HexMap, TerrainType } from '../api/types'

// A deliberately small, generic vocabulary -- enough for range rings, unit
// markers, and paths/LOS lines today, extensible for real unit symbology
// (MIL-STD-2525/APP-6) later without changing HexGrid's own contract: a
// symbology layer can render into the same overlay Container this component
// exposes, or this shape can grow a `symbol` kind when that work lands.
export type HexOverlayShape =
  | { kind: 'marker'; at: HexCoord; color?: number; radius?: number }
  | { kind: 'ring'; at: HexCoord; color?: number; radius?: number }
  | { kind: 'line'; from: HexCoord; to: HexCoord; color?: number; width?: number }

const props = withDefaults(
  defineProps<{
    map: HexMap
    overlays?: HexOverlayShape[]
    hexSize?: number
  }>(),
  { overlays: () => [], hexSize: 24 },
)

const emit = defineEmits<{ (e: 'hex-click', coord: HexCoord): void }>()

const TERRAIN_COLORS: Record<TerrainType, number> = {
  plains: 0x9acd6e,
  forest: 0x2e7d32,
  urban: 0x757575,
  water: 0x4a90d9,
  hill: 0xb08d57,
  road: 0xd8c48a,
}

const host = ref<HTMLDivElement | null>(null)
let app: Application | null = null
let terrainLayer: Graphics | null = null
let overlayLayer: Container | null = null

function axialToPixel(q: number, r: number, size: number): { x: number; y: number } {
  return {
    x: size * (Math.sqrt(3) * q + (Math.sqrt(3) / 2) * r),
    y: size * (1.5 * r),
  }
}

function hexCorners(cx: number, cy: number, size: number): number[] {
  const points: number[] = []
  for (let i = 0; i < 6; i++) {
    const angle = (Math.PI / 180) * (60 * i - 30)
    points.push(cx + size * Math.cos(angle), cy + size * Math.sin(angle))
  }
  return points
}

function axialRound(q: number, r: number): HexCoord {
  const s = -q - r
  let rq = Math.round(q)
  let rr = Math.round(r)
  const rs = Math.round(s)
  const qDiff = Math.abs(rq - q)
  const rDiff = Math.abs(rr - r)
  const sDiff = Math.abs(rs - s)
  if (qDiff > rDiff && qDiff > sDiff) rq = -rr - rs
  else if (rDiff > sDiff) rr = -rq - rs
  return { q: rq, r: rr }
}

function pixelToAxial(x: number, y: number, size: number): HexCoord {
  const q = ((Math.sqrt(3) / 3) * x - (1 / 3) * y) / size
  const r = ((2 / 3) * y) / size
  return axialRound(q, r)
}

function drawTerrain() {
  if (!terrainLayer) return
  terrainLayer.clear()
  for (const cell of props.map.cells) {
    const { x, y } = axialToPixel(cell.coord.q, cell.coord.r, props.hexSize)
    const points = hexCorners(x, y, props.hexSize)
    terrainLayer.poly(points).fill({ color: TERRAIN_COLORS[cell.terrain] ?? 0xcccccc })
    terrainLayer.poly(points).stroke({ width: 1, color: 0x000000, alpha: 0.25 })
  }
}

function drawOverlays() {
  if (!overlayLayer) return
  overlayLayer.removeChildren()
  for (const shape of props.overlays) {
    const g = new Graphics()
    if (shape.kind === 'marker') {
      const { x, y } = axialToPixel(shape.at.q, shape.at.r, props.hexSize)
      g.circle(x, y, shape.radius ?? props.hexSize * 0.4).fill({ color: shape.color ?? 0xff3b30 })
    } else if (shape.kind === 'ring') {
      const { x, y } = axialToPixel(shape.at.q, shape.at.r, props.hexSize)
      g.circle(x, y, shape.radius ?? props.hexSize * 0.9).stroke({
        width: 2,
        color: shape.color ?? 0xffd60a,
      })
    } else if (shape.kind === 'line') {
      const a = axialToPixel(shape.from.q, shape.from.r, props.hexSize)
      const b = axialToPixel(shape.to.q, shape.to.r, props.hexSize)
      g.moveTo(a.x, a.y)
        .lineTo(b.x, b.y)
        .stroke({ width: shape.width ?? 2, color: shape.color ?? 0xffffff })
    }
    overlayLayer.addChild(g)
  }
}

onMounted(async () => {
  app = new Application()
  await app.init({
    background: '#141414',
    antialias: true,
    resizeTo: host.value ?? undefined,
  })
  host.value?.appendChild(app.canvas)

  terrainLayer = new Graphics()
  overlayLayer = new Container()
  app.stage.addChild(terrainLayer)
  app.stage.addChild(overlayLayer)

  app.stage.eventMode = 'static'
  app.stage.hitArea = app.screen
  app.stage.on('pointertap', (event) => {
    const local = event.getLocalPosition(app!.stage)
    emit('hex-click', pixelToAxial(local.x, local.y, props.hexSize))
  })

  drawTerrain()
  drawOverlays()
})

onBeforeUnmount(() => {
  app?.destroy(true, { children: true })
  app = null
  terrainLayer = null
  overlayLayer = null
})

watch(() => props.map, drawTerrain, { deep: true })
watch(() => props.overlays, drawOverlays, { deep: true })
</script>

<template>
  <div ref="host" class="hex-grid"></div>
</template>

<style scoped>
.hex-grid {
  width: 100%;
  height: 480px;
  border: 1px solid #444;
}
</style>
