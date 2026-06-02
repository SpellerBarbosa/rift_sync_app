// ============================================================
// Store: settings — Configurações persistidas no SQLite
//
// Carregado uma vez no startup (App.vue), então todos os
// stores e páginas podem usar sem fazer chamadas adicionais.
//
// Chaves no banco (tabela settings):
//   tts_voice            → "pf_dora" | "pm_alex" | "pm_santa"
//   tts_volume           → "0.0" – "1.0"
//   coach_enabled        → "true" | "false"
//   coach_categories     → "OBJECTIVE,VISION,MACRO,TRADE,POSITIONING"
//   coach_min_severity   → "INFO" | "WARNING" | "CRITICAL"
// ============================================================
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

import type { AlertCategory, AlertSeverity } from './coach'

// ── Vozes pt-BR disponíveis na API TTS ────────────────────────
export interface TtsVoice {
  id:     string
  label:  string
  gender: 'Feminino' | 'Masculino'
}

export const PT_BR_VOICES: TtsVoice[] = [
  { id: 'pf_dora',   label: 'Dora',  gender: 'Feminino'  },
  { id: 'pm_alex',   label: 'Alex',  gender: 'Masculino' },
  { id: 'pm_santa',  label: 'Santa', gender: 'Masculino' },
]

// ── Defaults ──────────────────────────────────────────────────
const DEFAULTS = {
  tts_voice:           'pf_dora',
  tts_volume:          '0.8',
  coach_enabled:       'true',
  coach_categories:    'OBJECTIVE,VISION,MACRO,TRADE,POSITIONING',
  coach_min_severity:  'INFO',
} as const

export const useSettingsStore = defineStore('settings', () => {
  // ── Estado ─────────────────────────────────────────────────
  const ttsVoice          = ref<string>(DEFAULTS.tts_voice)
  const ttsVolume         = ref<number>(0.8)
  const coachEnabled      = ref<boolean>(true)
  const enabledCategories = ref<AlertCategory[]>([
    'OBJECTIVE', 'VISION', 'MACRO', 'TRADE', 'POSITIONING',
  ])
  const minSeverity = ref<AlertSeverity>('INFO')
  const isLoaded    = ref(false)

  // ── Carregar do banco ────────────────────────────────────────
  async function load() {
    try {
      const all = await invoke<Record<string, string>>('get_all_settings')

      if (all.tts_voice)          ttsVoice.value  = all.tts_voice
      if (all.tts_volume)         ttsVolume.value = parseFloat(all.tts_volume)
      if (all.coach_enabled)      coachEnabled.value = all.coach_enabled === 'true'
      if (all.coach_categories)   enabledCategories.value =
          (all.coach_categories.split(',').filter(Boolean) as AlertCategory[])
      if (all.coach_min_severity) minSeverity.value = all.coach_min_severity as AlertSeverity
    } catch {
      // banco vazio no primeiro uso — usa defaults
    }
    isLoaded.value = true
  }

  // ── Persistir uma chave ──────────────────────────────────────
  async function save(key: string, value: string) {
    await invoke('set_setting', { key, value }).catch(console.error)
  }

  // ── Actions ──────────────────────────────────────────────────

  async function setVoice(id: string) {
    ttsVoice.value = id
    await save('tts_voice', id)
  }

  async function setVolume(vol: number) {
    ttsVolume.value = vol
    await save('tts_volume', vol.toFixed(2))
  }

  async function setCoachEnabled(enabled: boolean) {
    coachEnabled.value = enabled
    await save('coach_enabled', String(enabled))
  }

  async function toggleCategory(cat: AlertCategory) {
    const idx = enabledCategories.value.indexOf(cat)
    if (idx >= 0) enabledCategories.value.splice(idx, 1)
    else          enabledCategories.value.push(cat)
    await save('coach_categories', enabledCategories.value.join(','))
  }

  async function setMinSeverity(sev: AlertSeverity) {
    minSeverity.value = sev
    await save('coach_min_severity', sev)
  }

  // ── Helpers ──────────────────────────────────────────────────

  function isCategoryEnabled(cat: AlertCategory): boolean {
    return enabledCategories.value.includes(cat)
  }

  return {
    ttsVoice,
    ttsVolume,
    coachEnabled,
    enabledCategories,
    minSeverity,
    isLoaded,
    load,
    setVoice,
    setVolume,
    setCoachEnabled,
    toggleCategory,
    setMinSeverity,
    isCategoryEnabled,
  }
})
