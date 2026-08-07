import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/components' },
    {
      path: '/components',
      name: 'component-library',
      component: () => import('../views/ComponentLibrary.vue'),
    },
    {
      path: '/assets',
      name: 'asset-designer',
      component: () => import('../views/AssetDesigner.vue'),
    },
    {
      path: '/units',
      name: 'unit-designer',
      component: () => import('../views/UnitDesigner.vue'),
    },
    {
      path: '/map',
      name: 'map-editor',
      component: () => import('../views/MapEditor.vue'),
    },
    {
      path: '/scenarios',
      name: 'scenario-editor',
      component: () => import('../views/ScenarioEditor.vue'),
    },
    {
      path: '/simulation/:id?',
      name: 'simulation-viewer',
      component: () => import('../views/SimulationViewer.vue'),
    },
  ],
})

export default router
