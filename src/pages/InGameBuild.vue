<script setup lang="ts">
import { computed, watch, onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useGameStateStore } from '../stores/gameState'
import { useGameBuildStore } from '../stores/gameBuild'
import type { ChampionBuild, ChampionInfo } from '../stores/champSelect'
import InGameBuildOverlay from '../overlay/InGameBuildOverlay.vue'

const gameState = useGameStateStore()
const gameBuild = useGameBuildStore()
const win       = getCurrentWindow()
const unlisteners: UnlistenFn[] = []

onMounted(async () => {
  document.body.style.backgroundColor = 'transparent'
  document.documentElement.style.backgroundColor = 'transparent'
  gameState.listenPhaseOnly()

  // Recebe build do champ select (emitido por champSelect.ts na janela main)
  // O gameBuild store é por-janela, então precisa ser populado via evento Tauri
  unlisteners.push(
    await listen<{ build: ChampionBuild | null; champion: ChampionInfo }>(
      'build_panel_data',
      (e) => {
        if (e.payload.champion) {
          gameBuild.set(e.payload.champion, e.payload.build)
        }
      }
    )
  )
})

onUnmounted(() => unlisteners.forEach(fn => fn()))

const hasContent = computed(() => gameState.phase === 'INGAME' && !!gameBuild.champion)

watch(hasContent, async (val) => {
  if (val) await win.show()
  else     await win.hide().catch(() => {})
}, { immediate: true })
</script>

<template>
  <div class="ib-root">
    <InGameBuildOverlay />
  </div>
</template>

<style>
html, body, #app {
  margin: 0; padding: 0; width: 100%; height: 100%;
  background: transparent !important; overflow: hidden;
}
.ib-root {
  width: 100%; height: 100%;
  background: transparent !important; pointer-events: none;
}
</style>
