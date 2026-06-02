import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { router } from '../router'

export type GamePhase =
  | 'IDLE'
  | 'LOBBY'
  | 'MATCHMAKING'
  | 'MATCH_FOUND'
  | 'BAN_PHASE'
  | 'PICK_PHASE'
  | 'LOADING'
  | 'INGAME'
  | 'POSTGAME'

// Fases em que a janela principal minimiza
const ACTIVE_PHASES       = new Set<GamePhase>(['BAN_PHASE', 'PICK_PHASE', 'LOADING', 'INGAME'])
const CHAMP_SELECT_PHASES = new Set<GamePhase>(['BAN_PHASE', 'PICK_PHASE'])
const GAME_PHASES         = new Set<GamePhase>(['LOADING', 'INGAME'])

const PHASE_ROUTES: Partial<Record<GamePhase, string>> = {
  IDLE:        '/',
  LOBBY:       '/',
  MATCHMAKING: '/',
  MATCH_FOUND: '/',
  POSTGAME:    '/post-game',
}

export const useGameStateStore = defineStore('gameState', () => {
  const phase         = ref<GamePhase>('IDLE')
  const isLolRunning  = ref(false)
  const isInitialized = ref(false)

  const isInGame = computed(() =>
    phase.value === 'INGAME' || phase.value === 'LOADING'
  )
  const isInChampSelect = computed(() =>
    phase.value === 'BAN_PHASE' || phase.value === 'PICK_PHASE'
  )

  // ── Usado pela janela principal ────────────────────────────
  async function initListener() {
    await listen<GamePhase>('game_state_changed', (event) => {
      applyPhaseChange(event.payload).catch(console.error)
    })
    await listen<boolean>('lcu_connected', (event) => {
      isLolRunning.value = event.payload
      if (!event.payload) applyPhaseChange('IDLE').catch(console.error)
    })
    try {
      const [currentPhase, connected] = await Promise.all([
        invoke<GamePhase>('get_game_phase'),
        invoke<boolean>('get_lcu_connection_status'),
      ])
      isLolRunning.value = connected
      await applyPhaseChange(currentPhase)
    } catch {
      phase.value = 'IDLE'
    }
    isInitialized.value = true
  }

  // ── Usado pela janela overlay (só atualiza phase, sem navegar) ──
  async function listenPhaseOnly() {
    await listen<GamePhase>('game_state_changed', (event) => {
      phase.value = event.payload
    })
    await listen<boolean>('lcu_connected', (event) => {
      isLolRunning.value = event.payload
      if (!event.payload) phase.value = 'IDLE'
    })
    try {
      const [currentPhase, connected] = await Promise.all([
        invoke<GamePhase>('get_game_phase'),
        invoke<boolean>('get_lcu_connection_status'),
      ])
      isLolRunning.value = connected
      phase.value = currentPhase
    } catch {
      phase.value = 'IDLE'
    }
    isInitialized.value = true
  }

  async function applyPhaseChange(newPhase: GamePhase) {
    const previous = phase.value
    phase.value = newPhase

    if (newPhase === previous) return

    const wasActive   = ACTIVE_PHASES.has(previous)
    const isNowActive = ACTIVE_PHASES.has(newPhase)

    if (isNowActive) {
      const enteringChampSelect = CHAMP_SELECT_PHASES.has(newPhase) && !CHAMP_SELECT_PHASES.has(previous)
      const enteringGame        = GAME_PHASES.has(newPhase) && !GAME_PHASES.has(previous)

      if (enteringChampSelect) {
        if (router.currentRoute.value.path !== '/champ-select') {
          router.push('/champ-select')
        }
        try { await invoke('minimize_main') } catch (e) { console.error(e) }
      } else if (enteringGame) {
        try { await invoke('enter_game_mode') } catch (e) { console.error(e) }
        try {
          const { useSettingsStore } = await import('./settings')
          const s = useSettingsStore()
          invoke('warm_up_tts', { voice: s.ttsVoice }).catch(() => {})
        } catch { /* não bloqueia o enter_game_mode */ }
      }
      return
    }

    if (wasActive && !isNowActive) {
      try { await invoke('exit_game_mode') } catch (e) { console.error(e) }
    }

    // Ao entrar no POSTGAME: sincroniza a partida e recalcula padrões em background
    if (newPhase === 'POSTGAME' && previous === 'INGAME') {
      syncPostGame()
    }

    const target = PHASE_ROUTES[newPhase]
    if (target && router.currentRoute.value.path !== target) {
      router.push(target)
    }
  }

  // Fire-and-forget: salva a partida no SQLite e recalcula player_patterns
  function syncPostGame() {
    invoke('sync_match_history')
      .then(() => invoke('compute_player_patterns'))
      .catch(() => { /* falha silenciosa — o PostGame.vue tem botão de sync manual */ })
  }

  return {
    phase,
    isLolRunning,
    isInitialized,
    isInGame,
    isInChampSelect,
    initListener,
    listenPhaseOnly,
  }
})
