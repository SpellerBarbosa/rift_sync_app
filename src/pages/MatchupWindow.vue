<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useGameStateStore } from '../stores/gameState'
import MatchupOverlay from '../overlay/MatchupOverlay.vue'

const gameState = useGameStateStore()
const win       = getCurrentWindow()
const unlisteners: UnlistenFn[] = []

onMounted(async () => {
  document.body.style.backgroundColor = 'transparent'
  document.documentElement.style.backgroundColor = 'transparent'
  gameState.listenPhaseOnly()

  // Mostra quando o matchup é identificado pela LCU
  unlisteners.push(
    await listen('matchup_init', () => win.show()),
  )
})

onUnmounted(() => unlisteners.forEach(fn => fn()))
</script>

<template>
  <div class="win-root">
    <MatchupOverlay />
  </div>
</template>

<style>
html, body, #app { margin: 0; padding: 0; width: 100%; height: 100%; background: transparent !important; overflow: hidden; }
.win-root { width: 100%; height: 100%; background: transparent !important; pointer-events: none; }
</style>
