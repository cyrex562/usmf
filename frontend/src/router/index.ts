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
      path: '/personnel',
      name: 'personnel-designer',
      component: () => import('../views/PersonnelDesigner.vue'),
    },
    {
      path: '/units',
      name: 'unit-designer',
      component: () => import('../views/UnitDesigner.vue'),
    },
    {
      path: '/relationship-types',
      name: 'relationship-type-library',
      component: () => import('../views/RelationshipTypeLibrary.vue'),
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
