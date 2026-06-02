<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useGameStateStore } from '../stores/gameState'
import { useChampSelectStore } from '../stores/champSelect'
import RuneOverlay  from './RuneOverlay.vue'
import BuildOverlay from './BuildOverlay.vue'

const gameState  = useGameStateStore()
const champSelect = useChampSelectStore()

onMounted(() => {
  if (gameState.phase === 'PICK_PHASE') champSelect.startPolling()
})

onUnmounted(() => {
  champSelect.reset()
})

watch(() => gameState.phase, (phase) => {
  if (phase === 'PICK_PHASE') champSelect.startPolling()
  else if (phase !== 'BAN_PHASE') champSelect.reset()
})
</script>

<template>
  <div class="cs-overlay-root">
    <!-- Indicador mínimo durante ban phase -->
    <div v-if="gameState.phase === 'BAN_PHASE'" class="ban-indicator">
      <div class="ban-dot" />
      <span>Fase de Banimento</span>
    </div>

    <!-- Build overlay: lado esquerdo -->
    <BuildOverlay
      v-if="champSelect.showPanel"
      :build="champSelect.build"
      :champion="champSelect.champion"
      :is-loading="champSelect.isLoading"
      @dismiss="champSelect.dismiss()"
    />

    <!-- Painel de runas: lado direito -->
    <RuneOverlay
      v-if="champSelect.showPanel"
      :recommendation="champSelect.recommendation"
      :champion="champSelect.champion"
      :is-loading="champSelect.isLoading"
      :rec-error="champSelect.recError"
      @dismiss="champSelect.dismiss()"
    />
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
  position: fixed;
  top: 16px;
  right: 16px;
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
