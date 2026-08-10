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

const MIN_SCALE = 0.2
const MAX_SCALE = 4

const host = ref<HTMLDivElement | null>(null)
let app: Application | null = null
let world: Container | null = null
let terrainLayer: Graphics | null = null
let overlayLayer: Container | null = null
let resizeObserver: ResizeObserver | null = null

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

// Stage-space (post pan/zoom) -> world-space (the coordinate system
// axialToPixel/pixelToAxial work in), computed from `world`'s own
// position/scale rather than PixiJS's toLocal, since a Container's
// documented position/scale fields are all this needs.
function toWorld(x: number, y: number): { x: number; y: number } {
  if (!world) return { x, y }
  return { x: (x - world.position.x) / world.scale.x, y: (y - world.position.y) / world.scale.y }
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

// Scales+centers `world` so every hex is visible and the grid isn't stuck in
// one corner of a much larger canvas -- the default view on mount and
// whenever the map itself changes (a fresh/different map, not a paint).
function fitToView() {
  if (!app || !world) return
  const cells = props.map.cells
  if (!cells.length) {
    world.scale.set(1)
    world.position.set(app.screen.width / 2, app.screen.height / 2)
    return
  }
  let minX = Infinity
  let minY = Infinity
  let maxX = -Infinity
  let maxY = -Infinity
  for (const cell of cells) {
    const { x, y } = axialToPixel(cell.coord.q, cell.coord.r, props.hexSize)
    minX = Math.min(minX, x - props.hexSize)
    maxX = Math.max(maxX, x + props.hexSize)
    minY = Math.min(minY, y - props.hexSize)
    maxY = Math.max(maxY, y + props.hexSize)
  }
  const contentWidth = Math.max(maxX - minX, 1)
  const contentHeight = Math.max(maxY - minY, 1)
  const scale = clampScale(
    Math.min(app.screen.width / contentWidth, app.screen.height / contentHeight) * 0.92,
  )
  world.scale.set(scale)
  world.position.set(
    app.screen.width / 2 - ((minX + maxX) / 2) * scale,
    app.screen.height / 2 - ((minY + maxY) / 2) * scale,
  )
}

function clampScale(scale: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale))
}

// True once the user has zoomed or panned by hand -- gates whether a
// container resize (window resize, sidebar toggle, ResizeObserver firing)
// is allowed to call fitToView() and silently wipe out that manual view.
// Before any manual interaction, resizing keeps auto-fitting (so the
// initial load looks right regardless of viewport size); the explicit
// "fit and center" button resets this back to auto-fit-on-resize mode.
let userAdjustedView = false

function zoomBy(factor: number, center?: { x: number; y: number }) {
  if (!app || !world) return
  userAdjustedView = true
  const pivot = center ?? { x: app.screen.width / 2, y: app.screen.height / 2 }
  const before = toWorld(pivot.x, pivot.y)
  const newScale = clampScale(world.scale.x * factor)
  world.scale.set(newScale)
  world.position.set(pivot.x - before.x * newScale, pivot.y - before.y * newScale)
}

function recenter() {
  userAdjustedView = false
  fitToView()
}

let dragState: { startX: number; startY: number; originX: number; originY: number; moved: boolean } | null =
  null

onMounted(async () => {
  app = new Application()
  await app.init({
    background: '#141414',
    antialias: true,
    resizeTo: host.value ?? undefined,
  })
  host.value?.appendChild(app.canvas)

  world = new Container()
  terrainLayer = new Graphics()
  overlayLayer = new Container()
  world.addChild(terrainLayer)
  world.addChild(overlayLayer)
  app.stage.addChild(world)

  app.stage.eventMode = 'static'
  app.stage.hitArea = app.screen
  app.stage.on('wheel', (event) => {
    const local = event.getLocalPosition(app!.stage)
    zoomBy(event.deltaY < 0 ? 1.1 : 1 / 1.1, { x: local.x, y: local.y })
  })
  app.stage.on('pointerdown', (event) => {
    const p = event.getLocalPosition(app!.stage)
    dragState = { startX: p.x, startY: p.y, originX: world!.position.x, originY: world!.position.y, moved: false }
  })
  app.stage.on('globalpointermove', (event) => {
    if (!dragState || !world) return
    const p = event.getLocalPosition(app!.stage)
    const dx = p.x - dragState.startX
    const dy = p.y - dragState.startY
    if (Math.hypot(dx, dy) > 4) dragState.moved = true
    if (dragState.moved) {
      userAdjustedView = true
      world.position.set(dragState.originX + dx, dragState.originY + dy)
    }
  })
  function handlePointerUp(x: number, y: number) {
    if (dragState && !dragState.moved) {
      const worldPoint = toWorld(x, y)
      emit('hex-click', pixelToAxial(worldPoint.x, worldPoint.y, props.hexSize))
    }
    dragState = null
  }
  app.stage.on('pointerup', (event) => {
    const p = event.getLocalPosition(app!.stage)
    handlePointerUp(p.x, p.y)
  })
  app.stage.on('pointerupoutside', (event) => {
    const p = event.getLocalPosition(app!.stage)
    handlePointerUp(p.x, p.y)
  })

  resizeObserver = new ResizeObserver(() => {
    if (!userAdjustedView) fitToView()
  })
  if (host.value) resizeObserver.observe(host.value)

  drawTerrain()
  drawOverlays()
  fitToView()
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
  app?.destroy(true, { children: true })
  app = null
  world = null
  terrainLayer = null
  overlayLayer = null
})

watch(() => props.map.cells, drawTerrain, { deep: true })
watch(
  () => props.map.id,
  () => {
    userAdjustedView = false
    drawTerrain()
    fitToView()
  },
)
watch(() => props.overlays, drawOverlays, { deep: true })
</script>

<template>
  <div class="hex-grid-wrap">
    <div ref="host" class="hex-grid"></div>
    <div class="hex-grid-controls">
      <button type="button" title="Zoom in" @click="zoomBy(1.25)">+</button>
      <button type="button" title="Zoom out" @click="zoomBy(1 / 1.25)">−</button>
      <button type="button" title="Fit and center" @click="recenter">⤢</button>
    </div>
  </div>
</template>

<style scoped>
.hex-grid-wrap {
  position: relative;
  width: 100%;
  height: min(75vh, 720px);
}
.hex-grid {
  width: 100%;
  height: 100%;
  border: 1px solid #444;
  cursor: grab;
}
.hex-grid-controls {
  position: absolute;
  top: 0.5rem;
  right: 0.5rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
.hex-grid-controls button {
  width: 2rem;
  height: 2rem;
  line-height: 1;
  cursor: pointer;
}
</style>
