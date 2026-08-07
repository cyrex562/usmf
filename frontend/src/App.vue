<script setup lang="ts">
import { RouterLink, RouterView } from 'vue-router'
</script>

<template>
  <div class="app-shell">
    <nav>
      <span class="brand">USMF</span>
      <RouterLink to="/components">Components</RouterLink>
      <RouterLink to="/assets">Assets</RouterLink>
      <RouterLink to="/personnel">Personnel</RouterLink>
      <RouterLink to="/units">Units</RouterLink>
      <RouterLink to="/map">Map</RouterLink>
      <RouterLink to="/scenarios">Scenarios</RouterLink>
      <RouterLink to="/simulation">Simulation</RouterLink>
    </nav>
    <main>
      <RouterView />
    </main>
  </div>
</template>

<style scoped>
.app-shell {
  max-width: 1100px;
  margin: 0 auto;
  padding: 1rem;
}
nav {
  display: flex;
  gap: 1rem;
  align-items: center;
  padding-bottom: 1rem;
  margin-bottom: 1.5rem;
  border-bottom: 1px solid #444;
}
.brand {
  font-weight: 700;
  margin-right: 1rem;
}
nav a {
  color: inherit;
  text-decoration: none;
  opacity: 0.7;
}
nav a.router-link-active {
  opacity: 1;
  font-weight: 600;
}
</style>
