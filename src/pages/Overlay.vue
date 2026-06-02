<script setup lang="ts">
import { onMounted } from 'vue'
import { useGameStateStore } from '../stores/gameState'
import FlashcardOverlay       from '../overlay/FlashcardOverlay.vue'
import WardOverlay            from '../overlay/WardOverlay.vue'
import MatchupOverlay         from '../overlay/MatchupOverlay.vue'
import InGameBuildOverlay     from '../overlay/InGameBuildOverlay.vue'
import ChampSelectPickOverlay from '../overlay/ChampSelectPickOverlay.vue'
import LoadingOverlay         from '../overlay/LoadingOverlay.vue'

const gameState = useGameStateStore()

onMounted(() => {
  document.body.style.backgroundColor = 'transparent'
  document.documentElement.style.backgroundColor = 'transparent'
  // Sincroniza a fase do jogo na janela overlay.
  // Sem isso, gameState.phase fica 'IDLE' e LoadingOverlay/FlashcardOverlay
  // nunca são renderizados.
  gameState.listenPhaseOnly()
})
</script>

<template>
  <div class="overlay-root">
    <ChampSelectPickOverlay v-if="gameState.isInChampSelect" />
    <LoadingOverlay         v-else-if="gameState.phase === 'LOADING'" />
    <FlashcardOverlay       v-else-if="gameState.phase === 'INGAME'" />

    <!--
      WardOverlay e MatchupOverlay SEMPRE montados — fora do condicional de fase.
      Motivo: precisam ter os listeners registrados para receber eventos do backend
      (ward_spots_forced, matchup_init) mesmo quando testados fora de uma partida.
      Quando inativos, são 100% transparentes e não capturam input.
    -->
    <WardOverlay />
    <MatchupOverlay />
    <InGameBuildOverlay />
  </div>
</template>

<style scoped>
.overlay-root {
  width: 100vw;
  height: 100vh;
  background: transparent;
  pointer-events: none;
}
</style>
