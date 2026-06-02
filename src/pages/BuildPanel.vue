<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import BuildOverlay from '../overlay/BuildOverlay.vue'
import type { ChampionBuild } from '../stores/champSelect'

interface ChampionInfo { id: number; name: string; key: string; title: string }

const build     = ref<ChampionBuild | null>(null)
const champion  = ref<ChampionInfo | null>(null)
const isLoading = ref(true)

const unlisten: UnlistenFn[] = []

onMounted(async () => {
  unlisten.push(
    await listen<void>('rune_panel_loading', () => {
      isLoading.value = true
      build.value     = null
      champion.value  = null
    }),
    await listen<{ build: ChampionBuild; champion: ChampionInfo }>('build_panel_data', (e) => {
      build.value     = e.payload.build
      champion.value  = e.payload.champion
      isLoading.value = false
    }),
    await listen<void>('rune_panel_reset', () => {
      isLoading.value = true
      build.value     = null
      champion.value  = null
    }),
  )
})

onUnmounted(() => unlisten.forEach(fn => fn()))

function dismiss() {
  invoke('hide_rune_overlay').catch(() => {})
}
</script>

<template>
  <BuildOverlay
    :build="build"
    :champion="champion"
    :is-loading="isLoading"
    @dismiss="dismiss"
  />
</template>

<style>
html, body, #app {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  background: #05061a;
  overflow: hidden;
}
</style>
