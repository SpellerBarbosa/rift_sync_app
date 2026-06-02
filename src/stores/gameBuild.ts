// ============================================================
// stores/gameBuild.ts — Build persistente entre fases do jogo
//
// Sobrevive ao champSelect.reset() para que a build carregada
// durante o champ select fique disponível in-game.
// Limpo apenas quando a partida termina (IDLE/POSTGAME).
// ============================================================
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { ChampionBuild, ChampionInfo } from './champSelect'

export const useGameBuildStore = defineStore('gameBuild', () => {
  const champion = ref<ChampionInfo | null>(null)
  const build    = ref<ChampionBuild | null>(null)

  function set(champ: ChampionInfo, b: ChampionBuild) {
    champion.value = champ
    build.value    = b
  }

  function clear() {
    champion.value = null
    build.value    = null
  }

  return { champion, build, set, clear }
})
