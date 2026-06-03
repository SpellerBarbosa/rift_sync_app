<script setup lang="ts">
import { useGameStateStore } from '../stores/gameState'

const gameState = useGameStateStore()
</script>

<template>
  <div class="cs-overlay-root">
    <!-- Indicador mínimo durante ban phase -->
    <div v-if="gameState.phase === 'BAN_PHASE'" class="ban-indicator">
      <div class="ban-dot" />
      <span>Fase de Banimento</span>
    </div>
  </div>
</template>

<style scoped>
.cs-overlay-root {
  width: 100vw;
  height: 100vh;
  background: transparent;
  pointer-events: none;
}

.ban-indicator {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: .6rem;
  letter-spacing: .2em;
  text-transform: uppercase;
  color: rgba(192,57,43,.6);
  background: rgba(10,10,11,.8);
  border: 1px solid rgba(192,57,43,.2);
  padding: 4px 10px;
  border-radius: 2px;
  pointer-events: auto;
  backdrop-filter: blur(8px);
}
.ban-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: #C0392B;
  box-shadow: 0 0 5px #C0392B;
  animation: ping .9s ease infinite;
}
@keyframes ping { 0%,100%{opacity:1} 50%{opacity:.3} }
</style>
