<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import RuneOverlay from '../overlay/RuneOverlay.vue'
import type { PickRecommendation, ChampionInfo } from '../stores/champSelect'

const recommendation = ref<PickRecommendation | null>(null)
const champion       = ref<ChampionInfo | null>(null)
const isLoading      = ref(true)
const recError       = ref<string | null>(null)

const unlisten: UnlistenFn[] = []

onMounted(async () => {
  unlisten.push(
    await listen<void>('rune_panel_loading', () => {
      isLoading.value      = true
      recommendation.value = null
      champion.value       = null
      recError.value       = null
    }),
    await listen<{ recommendation: PickRecommendation; champion: ChampionInfo }>('rune_panel_data', (e) => {
      recommendation.value = e.payload.recommendation
      champion.value       = e.payload.champion
      isLoading.value      = false
      recError.value       = null
    }),
    await listen<string>('rune_panel_error', (e) => {
      recError.value  = e.payload
      isLoading.value = false
    }),
    await listen<void>('rune_panel_reset', () => {
      isLoading.value      = true
      recommendation.value = null
      champion.value       = null
      recError.value       = null
    }),
  )
})

onUnmounted(() => unlisten.forEach(fn => fn()))

function dismiss() {
  invoke('hide_rune_overlay').catch(() => {})
}
</script>

<template>
  <RuneOverlay
    :recommendation="recommendation"
    :champion="champion"
    :is-loading="isLoading"
    :rec-error="recError"
    @dismiss="dismiss"
  />
</template>

<style>
html, body, #app {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  background: #08080a;
  overflow: hidden;
}
</style>
