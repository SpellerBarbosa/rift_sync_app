<script setup lang="ts">
import { watch, onMounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useGameStateStore } from '../stores/gameState'
import LoadingOverlay from '../overlay/LoadingOverlay.vue'

const gameState = useGameStateStore()
const win       = getCurrentWindow()

onMounted(() => {
  document.body.style.backgroundColor = 'transparent'
  document.documentElement.style.backgroundColor = 'transparent'
  gameState.listenPhaseOnly()
})

// Mostra a janela só durante LOADING; esconde nas demais fases
watch(() => gameState.phase, async (phase) => {
  if (phase === 'LOADING') await win.show()
  else await win.hide().catch(() => {})
}, { immediate: true })
</script>

<template>
  <div class="win-root">
    <LoadingOverlay v-if="gameState.phase === 'LOADING'" />
  </div>
</template>

<style>
html, body, #app { margin: 0; padding: 0; width: 100%; height: 100%; background: transparent !important; overflow: hidden; }
.win-root { width: 100%; height: 100%; background: transparent !important; pointer-events: none; }
</style>
