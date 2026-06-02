// ============================================================
// Store: coach — Fila de flashcards + TTS via API externa
//
// Fluxo:
//   coach_alert (Tauri) → enqueue() → advance() → activeCard
//   FlashCard.vue observa activeCard e controla show/hide da janela
//
// TTS usa a API spell2014-riftsyncai.hf.space/tts (vozes pt-BR).
// speak() é fire-and-forget; o dismiss timer é independente do áudio.
// ============================================================
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useSettingsStore } from './settings'

export type AlertCategory = 'VISION' | 'MACRO' | 'TRADE' | 'OBJECTIVE' | 'POSITIONING'
export type AlertSeverity  = 'INFO' | 'WARNING' | 'CRITICAL'

export interface CoachAlert {
  id:        string
  category:  AlertCategory
  severity:  AlertSeverity
  message:   string
  timestamp: number
}

const DISMISS_MS: Record<AlertSeverity, number> = {
  CRITICAL: 10_000,
  WARNING:   6_000,
  INFO:      4_000,
}

const SEVERITY_RANK: Record<AlertSeverity, number> = {
  INFO: 1, WARNING: 2, CRITICAL: 3,
}

export const useCoachStore = defineStore('coach', () => {
  // ── Estado ─────────────────────────────────────────────────
  const activeCard     = ref<CoachAlert | null>(null)
  const cardQueue      = ref<CoachAlert[]>([])
  const isMuted        = ref(false)
  const isVoiceEnabled = ref(true)

  let dismissTimer: ReturnType<typeof setTimeout> | null = null

  // ── Áudio atual ────────────────────────────────────────────
  let currentAudio: HTMLAudioElement | null = null

  // ── Geração de TTS ─────────────────────────────────────────
  // Cada speak() recebe um token único. Quando o áudio carrega (pode
  // demorar segundos), verifica se o token ainda é o mais recente.
  // Se um card novo já começou enquanto o áudio antigo carregava, o
  // áudio antigo é descartado silenciosamente — evita sobreposição.
  let speakGen = 0

  // ── Guarda de inicialização ────────────────────────────────
  let _unlistenAlert: UnlistenFn | null = null

  // ── Verifica janela de voz (síncrono, cacheado) ────────────
  let _isVoiceWindow: boolean | null = null
  function isVoiceWindow(): boolean {
    if (_isVoiceWindow === null) {
      try { _isVoiceWindow = getCurrentWindow().label === 'flashcard' }
      catch { _isVoiceWindow = false }
    }
    return _isVoiceWindow
  }

  // ── TTS — chama API e toca áudio com proteção contra sobreposição ──
  //
  // Problema resolvido: speak() é async mas o dismiss timer não espera.
  // Se o áudio demora a carregar (cache miss, rede lenta), pode chegar
  // depois que o próximo card já começou e tocar por cima.
  //
  // Solução: token de geração. Cada speak() incrementa speakGen e captura
  // o valor em myGen. Quando o áudio finalmente carrega, compara myGen
  // com speakGen — se divergirem, um card mais novo já assumiu → descarta.
  function speak(text: string) {
    if (!isVoiceEnabled.value || isMuted.value) return
    if (!isVoiceWindow()) return

    const settings = useSettingsStore()
    if (!settings.coachEnabled) return

    // Para o áudio atual imediatamente (novo card = novo áudio)
    if (currentAudio) {
      currentAudio.pause()
      currentAudio.src = ''
      currentAudio = null
    }

    const myGen = ++speakGen

    ;(async () => {
      try {
        const dataUrl = await invoke<string>('speak_tts', {
          text,
          voice: settings.ttsVoice,
        })

        // Outro card foi ativado enquanto esse carregava → descarta
        if (myGen !== speakGen) return

        const audio  = new Audio(dataUrl)
        audio.volume = settings.ttsVolume
        currentAudio = audio
        audio.onended = () => { if (currentAudio === audio) currentAudio = null }
        await audio.play()
      } catch (e) {
        console.warn('[TTS] falha:', e)
      }
    })()
  }

  // ── Fila de flashcards ──────────────────────────────────────

  function advance() {
    if (dismissTimer) { clearTimeout(dismissTimer); dismissTimer = null }

    if (cardQueue.value.length === 0) {
      activeCard.value = null
      return
    }

    const card = cardQueue.value.shift()!
    activeCard.value = card

    speak(card.message)  // para o áudio anterior e carrega o novo

    dismissTimer = setTimeout(advance, DISMISS_MS[card.severity])
  }

  /** Enfileira um alerta respeitando filtros de settings, prioridade e deduplicação.
   *  Síncrono — sem imports dinâmicos para não engolir erros silenciosamente. */
  function enqueue(alert: CoachAlert) {
    const settings = useSettingsStore()

    // Filtros de settings
    if (!settings.coachEnabled) return
    if (!settings.isCategoryEnabled(alert.category)) return
    if (SEVERITY_RANK[alert.severity] < SEVERITY_RANK[settings.minSeverity]) return

    // Deduplicação: mesma mensagem ativa ou na fila → descarta
    if (activeCard.value?.message === alert.message) return
    if (cardQueue.value.some(c => c.message === alert.message)) return

    if (alert.severity === 'CRITICAL') {
      cardQueue.value.unshift(alert)
      advance()
    } else {
      cardQueue.value.push(alert)
      if (!activeCard.value) advance()
    }
  }

  // ── Listeners Tauri ─────────────────────────────────────────

  async function initListener() {
    if (_unlistenAlert !== null) return

    _unlistenAlert = await listen<CoachAlert>('coach_alert', (event) => {
      enqueue(event.payload)  // síncrono — sem risco de Promise engolida
    })

    await listen<string>('game_state_changed', (event) => {
      const inGame = ['LOADING', 'INGAME']
      if (!inGame.includes(event.payload)) clearAlerts()
    })
  }

  // ── Actions públicas ────────────────────────────────────────

  function dismissCurrent() {
    if (currentAudio) { currentAudio.pause(); currentAudio.src = ''; currentAudio = null }
    advance()
  }

  function clearAlerts() {
    if (dismissTimer) { clearTimeout(dismissTimer); dismissTimer = null }
    if (currentAudio) { currentAudio.pause(); currentAudio.src = ''; currentAudio = null }
    speakGen++        // invalida qualquer TTS em flight — evita tocar depois do clear
    cardQueue.value  = []
    activeCard.value = null
  }

  async function toggleMute() {
    isMuted.value = !isMuted.value
    if (isMuted.value && currentAudio) {
      currentAudio.pause(); currentAudio.src = ''; currentAudio = null
    }
    await invoke('set_coach_muted', { muted: isMuted.value })
  }

  function toggleVoice() {
    isVoiceEnabled.value = !isVoiceEnabled.value
    if (!isVoiceEnabled.value && currentAudio) {
      currentAudio.pause(); currentAudio.src = ''; currentAudio = null
    }
  }

  return {
    activeCard,
    cardQueue,
    isMuted,
    isVoiceEnabled,
    initListener,
    dismissCurrent,
    clearAlerts,
    toggleMute,
    toggleVoice,
  }
})
