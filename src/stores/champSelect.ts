// ============================================================
// Store: champSelect — Estado de seleção de campeão
// Polling da sessão LCU para detectar lock-in e buscar recomendações
// ============================================================
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { useGameBuildStore } from './gameBuild'

export interface LocalPlayerPick {
  championId:       number
  assignedPosition: string
  spell1Id:         number
  spell2Id:         number
  isLockedIn:       boolean
  pickIntent:       number
}

export interface PickRecommendation {
  role:             string
  keystone:         string
  primaryPath:      string
  secondaryPath:    string
  spell1Name:       string
  spell2Name:       string
  spell1Id:         number
  spell2Id:         number
  primaryPathId:    number
  primaryRuneIds:   number[]  // [keystone, t1, t2, t3]
  secondaryPathId:  number
  secondaryRuneIds: number[]  // [r1, r2]
  statShardIds:     number[]  // [ofensa, flex, defesa]
  metaWinRate:      number | null
  metaGames:        number | null
  patch:            string | null
}

export interface ChampionInfo {
  id:    number
  name:  string
  key:   string
  title: string
}

const POLL_MS = 2000

// ── Tipos de build (vindos de get_champion_build) ─────────────

export interface RuneOption {
  styleId:   number
  selections: number[]
  games:     number
  wins:      number
  winRate:   number
  frequency: number
}

export interface ShardOption {
  offense:   number
  flex:      number
  defense:   number
  games:     number
  winRate:   number
  frequency: number
}

export interface ItemSet {
  items:     number[]
  games:     number
  wins:      number
  winRate:   number
  frequency: number
}

export interface SkillOrder {
  order:     string
  games:     number
  winRate:   number
  frequency: number
}

export interface ChampionBuild {
  championName: string
  role:         string
  tier:         string
  runes: {
    keystones:      { perkId: number; winRate: number; frequency: number }[]
    primaryRunes:   RuneOption[]
    secondaryRunes: RuneOption[]
    shards:         ShardOption[]
  } | null
  items: {
    startingItems: ItemSet[]
    coreBuilds:    ItemSet[]
    boots:         { itemId: number; winRate: number; frequency: number }[]
  } | null
  skills: {
    skillPriority: SkillOrder[]
  } | null
}

export const useChampSelectStore = defineStore('champSelect', () => {
  const gameBuild = useGameBuildStore()
  // ── Estado ─────────────────────────────────────────────────
  const localPick      = ref<LocalPlayerPick | null>(null)
  const recommendation = ref<PickRecommendation | null>(null)
  const champion       = ref<ChampionInfo | null>(null)
  const build          = ref<ChampionBuild | null>(null)
  const showPanel      = ref(false)
  const isLoading      = ref(false)
  const pollError      = ref<string | null>(null)
  const recError       = ref<string | null>(null)

  let pollTimer: ReturnType<typeof setInterval> | null = null
  let lastLockedId = 0

  // ── Actions ────────────────────────────────────────────────

  /** Uma única passagem de poll — reutilizada pelo intervalo e pela chamada imediata. */
  async function pollOnce() {
    try {
      const pick = await invoke<LocalPlayerPick>('get_local_player_pick')
      localPick.value = pick
      pollError.value = null
      console.debug('[ChampSelect] poll ok →', JSON.stringify(pick))

      if (pick.isLockedIn && pick.championId !== lastLockedId) {
        lastLockedId = pick.championId
        await loadRecommendation(pick.championId, pick.assignedPosition)
      }
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : (e?.message ?? String(e))
      pollError.value = msg
      console.warn('[ChampSelect] poll error:', msg)
    }
  }

  /** Inicia o polling da sessão LCU durante a fase de seleção. */
  function startPolling() {
    stopPolling()
    lastLockedId = 0
    showPanel.value = false
    recommendation.value = null
    champion.value = null
    pollError.value = null
    recError.value = null

    // Primeira checagem imediata — não espera 2s
    pollOnce()
    pollTimer = setInterval(pollOnce, POLL_MS)
  }

  /** Para o polling (chamado ao sair da fase de seleção). */
  function stopPolling() {
    if (pollTimer !== null) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  /** Abre a janela overlay de runas e carrega recomendação + build após lock-in.
   *
   * Separado em duas fases independentes para que falhas na recomendação
   * não impeçam o build panel de receber o campeão:
   *   1. get_champion_data  — obrigatório para ambos os painéis
   *   2. get_pick_recommendation — apenas para rune_panel (pode falhar)
   *   3. get_champion_build — fire-and-forget para build_panel
   */
  async function loadRecommendation(championId: number, role: string) {
    isLoading.value = true
    showPanel.value = true
    recError.value  = null
    build.value     = null

    emit('rune_panel_loading').catch(() => {})
    invoke('show_rune_overlay').catch((e) => console.error('[ChampSelect] show_rune_overlay:', e))

    // ── Fase 1: dados do campeão (necessário para ambos os painéis) ──
    let champ: ChampionInfo | null = null
    try {
      champ = await invoke<ChampionInfo>('get_champion_data', { id: championId })
      champion.value = champ
    } catch (e) {
      console.warn('[ChampSelect] get_champion_data falhou:', e)
    }

    // ── Fase 2: recomendação de runas (rune_panel) ───────────────────
    try {
      const rec = await invoke<PickRecommendation>('get_pick_recommendation', { championId, role })
      recommendation.value = rec
      if (champ) emit('rune_panel_data', { recommendation: rec, champion: champ }).catch(() => {})
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : (e?.message ?? String(e))
      recError.value = msg
      emit('rune_panel_error', msg).catch(() => {})
      console.warn('[ChampSelect] get_pick_recommendation falhou:', msg)
    } finally {
      isLoading.value = false
    }

    // ── Fase 3: build do banco (build_panel) — fire-and-forget ───────
    if (!champ?.name) {
      // Campeão não disponível: notifica build panel com estado vazio
      emit('build_panel_data', { build: null, champion: champ }).catch(() => {})
      return
    }

    invoke<{ championName: string; role: string; tier: string; runes: any; items: any; skills: any }>(
      'get_champion_build',
      { championName: champ.name }
    ).then(raw => {
      const resolved: ChampionBuild = {
        championName: raw.championName,
        role:         raw.role,
        tier:         raw.tier,
        runes:   raw.runes   ? JSON.parse(typeof raw.runes   === 'string' ? raw.runes   : JSON.stringify(raw.runes))   : null,
        items:   raw.items   ? JSON.parse(typeof raw.items   === 'string' ? raw.items   : JSON.stringify(raw.items))   : null,
        skills:  raw.skills  ? JSON.parse(typeof raw.skills  === 'string' ? raw.skills  : JSON.stringify(raw.skills))  : null,
      }
      build.value = resolved
      gameBuild.set(champ!, resolved)
      emit('build_panel_data', { build: resolved, champion: champ }).catch(() => {})
    }).catch((e) => {
      console.warn('[ChampSelect] get_champion_build falhou:', e)
      // Sem build no banco — ao menos mostra o campeão no painel
      emit('build_panel_data', { build: null, champion: champ }).catch(() => {})
    })
  }

  /** Fecha o painel e a janela overlay. */
  function dismiss() {
    showPanel.value = false
    invoke('hide_rune_overlay').catch(() => {})
  }

  /** Limpa todo o estado ao sair do champ select. */
  function reset() {
    stopPolling()
    localPick.value      = null
    recommendation.value = null
    champion.value       = null
    build.value          = null
    showPanel.value      = false
    pollError.value      = null
    recError.value       = null
    lastLockedId         = 0
    emit('rune_panel_reset').catch(() => {})
    invoke('hide_rune_overlay').catch(() => {})
  }

  return {
    localPick,
    recommendation,
    champion,
    build,
    showPanel,
    isLoading,
    pollError,
    recError,
    startPolling,
    stopPolling,
    dismiss,
    reset,
    loadRecommendation,
  }
})
