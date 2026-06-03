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

  // ── Normaliza texto antes de enviar ao TTS ───────────────────
  // Corrige abreviações e símbolos que o TTS lê de forma estranha.
  function normalizeTts(text: string): string {
    return text
      // "30s" → "30 segundos" | "30 s" → "30 segundos"
      .replace(/\b(\d+)\s*s\b/gi, '$1 segundos')
      // "1min" / "2min" → "1 minuto" / "2 minutos"
      .replace(/\b(\d+)\s*min\b/gi, (_, n) => `${n} ${n === '1' ? 'minuto' : 'minutos'}`)
      // "km/h", "m/s" — lê mal; substitui por forma escrita
      .replace(/\bkm\/h\b/gi, 'quilômetros por hora')
  }

  // ── Divide texto em segmentos para síntese paralela ──────────
  // Regra 1: separa em `.!?` (sentenças)
  // Regra 2: dentro de cada sentença longa (>40 chars), separa em `,`
  // Isso garante que nenhum segmento fique longo demais e "embole" no TTS.
  function splitSentences(text: string): string[] {
    const primary = text
      .split(/(?<=[.!?])\s+/)
      .map(s => s.trim())
      .filter(s => s.length >= 2)

    const result: string[] = []
    for (const seg of primary) {
      if (seg.length > 40 && seg.includes(',')) {
        const parts = seg.split(/,\s+/)
          .map(s => s.trim())
          .filter(s => s.length >= 3)
        result.push(...parts)
      } else {
        result.push(seg)
      }
    }
    return result
  }

  const PAUSE_BETWEEN_SEGMENTS_MS = 350

  // ── TTS — queue que não corta o áudio atual ──────────────────
  //
  // Fluxo:
  //   • Se não está tocando → começa imediatamente
  //   • Se está tocando     → salva como pendente (substitui anterior)
  //   • Ao terminar cada dica → toca a pendente se houver
  //   • speakGen permite parar tudo (mute / saída de jogo)
  let isSpeaking  = false
  let pendingText: string | null = null

  function speak(text: string) {
    if (!isVoiceEnabled.value || isMuted.value) return
    if (!isVoiceWindow()) return
    const settings = useSettingsStore()
    if (!settings.coachEnabled) return

    if (isSpeaking) {
      pendingText = text   // guarda como próxima (substitui anterior pendente)
      return
    }

    doSpeak(text)
  }

  async function doSpeak(rawText: string) {
    isSpeaking = true
    const myGen   = ++speakGen
    const settings = useSettingsStore()
    const segments = splitSentences(normalizeTts(rawText))

    try {
      // Dispara geração de TODOS os segmentos em paralelo
      const pending = segments.map(seg =>
        invoke<string>('speak_tts', {
          text:  seg,
          voice: settings.ttsVoice,
          speed: settings.ttsSpeed,
        }).catch(() => null)
      )

      // Toca em ordem, com pausa de 350ms entre segmentos
      for (let i = 0; i < pending.length; i++) {
        if (myGen !== speakGen) break   // mute/stop acionado

        const dataUrl = await pending[i]
        if (!dataUrl || myGen !== speakGen) break

        const isLast = i === pending.length - 1

        await new Promise<void>((resolve) => {
          const audio  = new Audio(dataUrl)
          audio.volume = settings.ttsVolume
          currentAudio = audio
          audio.onended = () => {
            if (currentAudio === audio) currentAudio = null
            if (isLast) resolve()
            else setTimeout(resolve, PAUSE_BETWEEN_SEGMENTS_MS)
          }
          audio.onerror = () => resolve()
          audio.play().catch(() => resolve())
        })
      }
    } catch (e) {
      console.warn('[TTS] falha:', e)
    } finally {
      isSpeaking = false
      // Toca a dica que chegou enquanto esta estava rodando
      if (pendingText && myGen === speakGen) {
        const next = pendingText
        pendingText = null
        doSpeak(next)
      }
    }
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

    // Cap de fila: máximo 2 pendentes para não spammar em rajada.
    // CRITICAL sempre entra (pode substituir o início da fila se cheia).
    if (cardQueue.value.length >= 2 && alert.severity !== 'CRITICAL') return

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
