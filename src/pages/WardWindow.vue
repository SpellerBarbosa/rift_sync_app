<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useGameStateStore } from '../stores/gameState'
import WardOverlay from '../overlay/WardOverlay.vue'

const gameState = useGameStateStore()
const win       = getCurrentWindow()

const HIDE_DELAY = 13_000 // um pouco mais que DISPLAY_MS do WardOverlay (12s)
let hideTimer: ReturnType<typeof setTimeout> | null = null
const unlisteners: UnlistenFn[] = []

async function show() {
  if (hideTimer) clearTimeout(hideTimer)
  await win.show()
  hideTimer = setTimeout(() => { win.hide().catch(() => {}) }, HIDE_DELAY)
}

onMounted(async () => {
  document.body.style.backgroundColor = 'transparent'
  document.documentElement.style.backgroundColor = 'transparent'
  gameState.listenPhaseOnly()

  // Mostra quando chegam ward spots de qualquer origem
  unlisteners.push(
    await listen('ward_spots',        show),
    await listen('ward_spots_smart',  show),
    await listen('ward_spots_forced', show),
  )
})

onUnmounted(() => {
  unlisteners.forEach(fn => fn())
  if (hideTimer) clearTimeout(hideTimer)
})
</script>

<template>
  <div class="win-root">
    <WardOverlay />
  </div>
</template>

<style>
html, body, #app { margin: 0; padding: 0; width: 100%; height: 100%; background: transparent !important; overflow: hidden; }
.win-root { width: 100%; height: 100%; background: transparent !important; pointer-events: none; }
</style>
