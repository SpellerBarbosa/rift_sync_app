// ============================================================
// Store: champion — Cache local de dados de campeões
// Evita requisições repetidas ao backend/Data Dragon
// ============================================================
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Champion {
  id: number
  name: string
  key: string     // chave string usada pela Data Dragon (ex: "Yasuo")
  title: string
}

export interface ChampionMatchup {
  difficultyScore: number  // 1-10 (10 = muito difícil)
  isFavorable: boolean
  mainTip: string
  powerSpikes: string[]
}

export const useChampionStore = defineStore('champion', () => {
  // ── Estado ─────────────────────────────────────────────────
  // Map<championId, Champion> para acesso O(1)
  const cache     = ref<Map<number, Champion>>(new Map())
  const isLoading = ref(false)

  // ── Actions ────────────────────────────────────────────────

  /** Retorna um campeão do cache ou busca do backend se necessário. */
  async function getChampion(id: number): Promise<Champion | null> {
    if (cache.value.has(id)) return cache.value.get(id)!

    try {
      const champ = await invoke<Champion>('get_champion_data', { id })
      cache.value.set(id, champ)
      return champ
    } catch {
      return null
    }
  }

  /** Pré-carrega todos os campeões na inicialização para uso offline. */
  async function preloadAll() {
    if (isLoading.value) return
    isLoading.value = true
    try {
      const all = await invoke<Champion[]>('get_all_champions')
      all.forEach((c) => cache.value.set(c.id, c))
    } finally {
      isLoading.value = false
    }
  }

  /** Busca dados de matchup entre dois campeões. */
  async function getMatchup(
    championId: number,
    enemyId: number
  ): Promise<ChampionMatchup | null> {
    try {
      return await invoke<ChampionMatchup>('get_champion_matchup', {
        championId,
        enemyId,
      })
    } catch {
      return null
    }
  }

  return { cache, isLoading, getChampion, preloadAll, getMatchup }
})
