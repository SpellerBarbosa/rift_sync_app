<script setup lang="ts">
import { onMounted } from 'vue'
import { useGameStateStore } from '../stores/gameState'
import ChampSelectPickOverlay from '../overlay/ChampSelectPickOverlay.vue'

const gameState = useGameStateStore()
onMounted(() => {
  document.body.style.backgroundColor = 'transparent'
  document.documentElement.style.backgroundColor = 'transparent'
  gameState.listenPhaseOnly()
})
</script>

<template>
  <div class="win-root">
    <ChampSelectPickOverlay />
  </div>
</template>

<style>
html, body, #app { margin: 0; padding: 0; width: 100%; height: 100%; background: transparent !important; overflow: hidden; }
.win-root { width: 100%; height: 100%; background: transparent !important; pointer-events: none; }
</style>
