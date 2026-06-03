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
import { i18n, type AppLocale } from '../i18n'

// ── Vozes pt-BR disponíveis na API TTS ────────────────────────
export interface TtsVoice {
  id:     string
  label:  string
  gender: 'Feminino' | 'Masculino'
}

export const PT_BR_VOICES: TtsVoice[] = [
  { id: 'pt_BR-cadu-medium',  label: 'Cadu',  gender: 'Masculino' },
  { id: 'pt_BR-faber-medium', label: 'Faber', gender: 'Masculino' },
  { id: 'pt_BR-jeff-medium',  label: 'Jeff',  gender: 'Masculino' },
  { id: 'pt_BR-miro-high',    label: 'Miro',  gender: 'Masculino' },
  { id: 'pt_BR-dii',          label: 'Dii',   gender: 'Masculino' },
]

export const EN_US_VOICES: TtsVoice[] = [
  { id: 'en_US-amy-medium',   label: 'Amy',   gender: 'Feminino'  },
  { id: 'en_US-bryce-medium', label: 'Bryce', gender: 'Masculino' },
  { id: 'en_US-joe-medium',   label: 'Joe',   gender: 'Masculino' },
]

const VOICES_BY_LOCALE: Record<AppLocale, TtsVoice[]> = {
  'pt-BR': PT_BR_VOICES,
  'en-US': EN_US_VOICES,
}

// ── Defaults ──────────────────────────────────────────────────
const DEFAULTS = {
  tts_voice:           'pt_BR-cadu-medium',
  tts_volume:          '0.8',
  tts_speed:           '1.0',
  coach_enabled:       'true',
  coach_categories:    'OBJECTIVE,VISION,MACRO,TRADE,POSITIONING',
  coach_min_severity:  'INFO',
} as const

export const useSettingsStore = defineStore('settings', () => {
  // ── Estado ─────────────────────────────────────────────────
  const ttsVoice          = ref<string>(DEFAULTS.tts_voice)
  const ttsVolume         = ref<number>(0.8)
  const ttsSpeed          = ref<number>(1.0)
  const coachEnabled      = ref<boolean>(true)
  const enabledCategories = ref<AlertCategory[]>([
    'OBJECTIVE', 'VISION', 'MACRO', 'TRADE', 'POSITIONING',
  ])
  const minSeverity = ref<AlertSeverity>('INFO')
  const isLoaded    = ref(false)
  const appLanguage = ref<AppLocale>('pt-BR')

  // ── Carregar do banco ────────────────────────────────────────
  async function load() {
    try {
      const all = await invoke<Record<string, string>>('get_all_settings')

      if (all.tts_voice)          ttsVoice.value  = all.tts_voice
      if (all.tts_volume)         ttsVolume.value = parseFloat(all.tts_volume)
      if (all.tts_speed)          ttsSpeed.value  = parseFloat(all.tts_speed)
      if (all.coach_enabled)      coachEnabled.value = all.coach_enabled === 'true'
      if (all.coach_categories)   enabledCategories.value =
          (all.coach_categories.split(',').filter(Boolean) as AlertCategory[])
      if (all.coach_min_severity) minSeverity.value = all.coach_min_severity as AlertSeverity
      if (all.app_language) {
        appLanguage.value = all.app_language as AppLocale
        i18n.global.locale.value = appLanguage.value
      }
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

  async function setSpeed(spd: number) {
    ttsSpeed.value = spd
    await save('tts_speed', spd.toFixed(2))
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

  async function setLanguage(lang: AppLocale) {
    appLanguage.value = lang
    i18n.global.locale.value = lang
    // auto-select first voice of the new language if current is incompatible
    const voices = VOICES_BY_LOCALE[lang]
    if (!voices.find(v => v.id === ttsVoice.value)) {
      await setVoice(voices[0].id)
    }
    await save('app_language', lang)
  }

  // ── Helpers ──────────────────────────────────────────────────

  function isCategoryEnabled(cat: AlertCategory): boolean {
    return enabledCategories.value.includes(cat)
  }

  return {
    ttsVoice,
    ttsVolume,
    ttsSpeed,
    coachEnabled,
    enabledCategories,
    minSeverity,
    isLoaded,
    appLanguage,
    load,
    setVoice,
    setVolume,
    setSpeed,
    setCoachEnabled,
    toggleCategory,
    setMinSeverity,
    setLanguage,
    isCategoryEnabled,
  }
})
