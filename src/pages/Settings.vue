<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore, PT_BR_VOICES } from '../stores/settings'
import { debugFireReplay, debugFireWard } from '../services/coach'
import type { AlertCategory, AlertSeverity } from '../stores/coach'

const router   = useRouter()
const settings = useSettingsStore()

// ── Preview de voz ────────────────────────────────────────────
const previewLoading = ref<string | null>(null)
const previewError   = ref(false)

// ── Teste de alertas e wards ──────────────────────────────────
const testLoading     = ref(false)
const testDone        = ref(false)
const testWardLoading = ref(false)
const testWardDone    = ref(false)

async function runTestAlerts() {
  if (testLoading.value) return
  testLoading.value = true
  testDone.value    = false
  try {
    await debugFireReplay()
    testDone.value = true
    setTimeout(() => { testDone.value = false }, 4000)
  } catch {
    // ignora — comando não disponível em alguns builds
  } finally {
    testLoading.value = false
  }
}

async function runTestWard() {
  if (testWardLoading.value) return
  testWardLoading.value = true
  testWardDone.value    = false
  try {
    await debugFireWard()
    testWardDone.value = true
    setTimeout(() => { testWardDone.value = false }, 5000)
  } catch {
    // ignora
  } finally {
    testWardLoading.value = false
  }
}


async function previewVoice(voiceId: string) {
  if (previewLoading.value) return
  previewLoading.value = voiceId
  previewError.value   = false
  try {
    const dataUrl = await invoke<string>('speak_tts', {
      text:  'Dragão disponível agora — vá agora!',
      voice: voiceId,
    })
    const audio  = new Audio(dataUrl)
    audio.volume = settings.ttsVolume
    audio.play().catch(() => {})
  } catch {
    previewError.value = true
  } finally {
    previewLoading.value = null
  }
}

// ── SpellCoach sync ───────────────────────────────────────────
const syncLoading = ref(false)
const syncResult  = ref<{ synced: number; errors: number } | null>(null)
const syncError   = ref<string | null>(null)

async function runSpellCoachSync() {
  if (syncLoading.value) return
  syncLoading.value = true
  syncResult.value  = null
  syncError.value   = null
  try {
    const r = await invoke<{ synced: number; errors: number; combinationsOk: string[] }>(
      'sync_spellcoach_data'
    )
    syncResult.value = { synced: r.synced, errors: r.errors }
    setTimeout(() => { syncResult.value = null }, 8000)
  } catch (e: any) {
    syncError.value = typeof e === 'string' ? e : (e?.message ?? 'Erro desconhecido')
    setTimeout(() => { syncError.value = null }, 6000)
  } finally {
    syncLoading.value = false
  }
}

// ── Categorias ────────────────────────────────────────────────
const CATEGORIES: { id: AlertCategory; label: string; color: string }[] = [
  { id: 'OBJECTIVE',   label: 'Objetivos',    color: '#C89B3C' },
  { id: 'VISION',      label: 'Visão',        color: '#06B6D4' },
  { id: 'MACRO',       label: 'Macro',        color: '#A78BFA' },
  { id: 'TRADE',       label: 'Trades',       color: '#FB923C' },
  { id: 'POSITIONING', label: 'Posicionamento', color: '#60A5FA' },
]

const SEVERITIES: { id: AlertSeverity; label: string; desc: string }[] = [
  { id: 'INFO',     label: 'Tudo',      desc: 'Recebe dicas informativas, avisos e alertas críticos' },
  { id: 'WARNING',  label: 'Avisos +',  desc: 'Apenas avisos e alertas críticos (sem dicas informativas)' },
  { id: 'CRITICAL', label: 'Críticos',  desc: 'Apenas alertas de máxima urgência' },
]
</script>

<template>
  <div class="s-root">
    <div class="s-glow" />

    <!-- Header -->
    <header class="s-header">
      <button class="back-btn" @click="router.back()">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path d="M9 2L4 7L9 12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        Voltar
      </button>
      <div class="s-title">
        <span class="s-diamond">◆</span>
        Configurações
      </div>
      <div class="w-20" />
    </header>

    <div class="s-scroll">
      <div class="s-content">

        <!-- ── Seção: Voz do Coach ─────────────────────────── -->
        <section class="s-section">
          <h2 class="s-section-title">Voz do Coach</h2>

          <!-- Cards de voz -->
          <div class="voice-grid">
            <button
              v-for="voice in PT_BR_VOICES"
              :key="voice.id"
              class="voice-card"
              :class="{ 'voice-card--active': settings.ttsVoice === voice.id }"
              @click="settings.setVoice(voice.id)"
            >
              <!-- Radio dot -->
              <div class="voice-radio">
                <div v-if="settings.ttsVoice === voice.id" class="voice-radio-dot" />
              </div>

              <div class="voice-info">
                <span class="voice-name">{{ voice.label }}</span>
                <span class="voice-gender">{{ voice.gender }}</span>
              </div>

              <!-- Preview button -->
              <button
                class="preview-btn"
                :disabled="previewLoading !== null"
                @click.stop="previewVoice(voice.id)"
                title="Ouvir prévia"
              >
                <span v-if="previewLoading === voice.id" class="spin-small">⟳</span>
                <svg v-else width="12" height="12" viewBox="0 0 12 12" fill="currentColor">
                  <path d="M2 2.5L10 6L2 9.5V2.5Z"/>
                </svg>
              </button>
            </button>
          </div>

          <p v-if="previewError" class="preview-err">
            API TTS offline — verifique a conexão
          </p>

          <!-- Volume -->
          <div class="volume-row">
            <label class="volume-label">
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" style="flex-shrink:0">
                <path d="M2 5H4L7 2V12L4 9H2V5Z" stroke="currentColor" stroke-width="1.2" fill="none"/>
                <path d="M9.5 4.5C10.5 5.5 10.5 8.5 9.5 9.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
                <path d="M11 3C12.5 4.5 12.5 9.5 11 11" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
              </svg>
              Volume
            </label>
            <input
              type="range"
              min="0" max="1" step="0.05"
              :value="settings.ttsVolume"
              class="volume-slider"
              @input="settings.setVolume(parseFloat(($event.target as HTMLInputElement).value))"
            />
            <span class="volume-val">{{ Math.round(settings.ttsVolume * 100) }}%</span>
          </div>
        </section>

        <div class="s-divider" />

        <!-- ── Seção: Alertas ──────────────────────────────── -->
        <section class="s-section">
          <div class="section-header-row">
            <h2 class="s-section-title">Alertas de Coaching</h2>
            <!-- Toggle coach enabled -->
            <button
              class="toggle-btn"
              :class="settings.coachEnabled ? 'toggle-on' : 'toggle-off'"
              @click="settings.setCoachEnabled(!settings.coachEnabled)"
            >
              <div class="toggle-thumb" />
            </button>
          </div>

          <div :class="{ 'disabled-section': !settings.coachEnabled }">

            <!-- Categorias -->
            <p class="s-sublabel">Categorias ativas</p>
            <div class="category-grid">
              <button
                v-for="cat in CATEGORIES"
                :key="cat.id"
                class="cat-chip"
                :class="settings.isCategoryEnabled(cat.id) ? 'cat-on' : 'cat-off'"
                :style="settings.isCategoryEnabled(cat.id) ? { borderColor: cat.color + '60', color: cat.color } : {}"
                @click="settings.toggleCategory(cat.id)"
              >
                <span class="cat-dot" :style="{ background: settings.isCategoryEnabled(cat.id) ? cat.color : 'rgba(255,255,255,0.15)' }" />
                {{ cat.label }}
              </button>
            </div>

            <!-- Severidade mínima -->
            <p class="s-sublabel" style="margin-top:16px">Nível mínimo de alerta</p>
            <div class="severity-row">
              <button
                v-for="sev in SEVERITIES"
                :key="sev.id"
                class="sev-btn"
                :class="settings.minSeverity === sev.id ? 'sev-active' : 'sev-inactive'"
                :title="sev.desc"
                @click="settings.setMinSeverity(sev.id)"
              >
                {{ sev.label }}
              </button>
            </div>
            <p class="sev-desc">
              {{ SEVERITIES.find(s => s.id === settings.minSeverity)?.desc }}
            </p>

          </div>
        </section>

        <div class="s-divider" />

        <!-- ── Seção: OCR / Modo de Tela ──────────────────── -->
        <section class="s-section">
          <h2 class="s-section-title">Captura de Tela (OCR)</h2>

          <div class="ocr-banner">
            <div class="ocr-icon">⚠</div>
            <div class="ocr-text">
              <p class="ocr-title">Jogue em modo Janela Sem Bordas</p>
              <p class="ocr-desc">
                O OCR captura a tela via GDI (Windows) e <strong>não funciona em fullscreen exclusivo</strong>.
                Para receber todas as dicas sobre cooldowns de spells e minimapa, configure o League of Legends para
                <strong>Janela Sem Bordas (Borderless)</strong> em <em>Vídeo → Modo de Tela</em>.
              </p>
            </div>
          </div>
        </section>

        <div class="s-divider" />

        <!-- ── Seção: SpellCoach ──────────────────────────── -->
        <section class="s-section">
          <div class="section-header-row">
            <h2 class="s-section-title">SpellCoach — Meta Data</h2>
            <span class="sc-badge-label">API</span>
          </div>
          <p class="s-sublabel" style="margin-bottom:12px">
            Sincroniza estatísticas de meta, builds e wards de alto elo com o banco local.
            O app sincroniza automaticamente 8s após abrir e a cada 6h. Use o botão para
            forçar uma atualização imediata.
          </p>

          <button
            class="test-btn sc-sync-btn"
            :class="{ 'test-done': !!syncResult, 'test-err': !!syncError }"
            :disabled="syncLoading"
            @click="runSpellCoachSync"
          >
            <span v-if="syncLoading" class="spin-small">⟳</span>
            <template v-else-if="syncResult">
              ✓ {{ syncResult.synced.toLocaleString() }} registros
              <span v-if="syncResult.errors > 0" class="sc-err-count">
                · {{ syncResult.errors }} erros
              </span>
            </template>
            <span v-else-if="syncError" class="sc-err-text">{{ syncError }}</span>
            <template v-else>
              Sincronizar agora
            </template>
          </button>

          <p class="s-sublabel" style="margin-top:10px;font-size:.52rem">
            Cada chamada percorre 5 roles × 9 tiers (45 requests). Pode levar até 60s.
          </p>
        </section>

        <div class="s-divider" />

        <!-- ── Seção: Diagnóstico ──────────────────────────── -->
        <section class="s-section">
          <h2 class="s-section-title">Diagnóstico</h2>
          <p class="s-sublabel" style="margin-bottom:12px">
            Dispara alertas de teste para verificar flashcards, TTS e barra de wards.
          </p>
          <div style="display:flex;flex-direction:column;gap:8px;">
            <button class="test-btn" :class="{ 'test-done': testDone }" :disabled="testLoading" @click="runTestAlerts">
              <span v-if="testLoading" class="spin-small">⟳</span>
              <span v-else-if="testDone">✓ Alertas enviados</span>
              <span v-else>Testar alertas + TTS</span>
            </button>
            <button class="test-btn" :class="{ 'test-done': testWardDone }" :disabled="testWardLoading" @click="runTestWard">
              <span v-if="testWardLoading" class="spin-small">⟳</span>
              <span v-else-if="testWardDone">✓ Barra de ward enviada</span>
              <span v-else>Testar barra de wards</span>
            </button>
          </div>
        </section>

      </div>
    </div>
  </div>
</template>

<style scoped>
/* ── Root ─────────────────────────────────────────────── */
.s-root {
  height: 100%;
  background: #010A13;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
  user-select: none;
}
.s-glow {
  position: absolute; inset: 0; pointer-events: none;
  background: radial-gradient(ellipse 70% 40% at 50% 0%, rgba(10,30,60,.5) 0%, transparent 60%);
}

/* ── Header ───────────────────────────────────────────── */
.s-header {
  position: relative; z-index: 1;
  display: flex; align-items: center; justify-content: space-between;
  padding: 14px 20px 12px;
  border-bottom: 1px solid rgba(200,155,60,.1);
  flex-shrink: 0;
}
.back-btn {
  display: flex; align-items: center; gap: 5px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .65rem; font-weight: 600; letter-spacing: .14em; text-transform: uppercase;
  color: rgba(200,155,60,.5); background: none; border: 1px solid rgba(200,155,60,.15);
  padding: 3px 10px; border-radius: 2px; cursor: pointer; transition: all .15s;
}
.back-btn:hover { color: rgba(200,155,60,.9); border-color: rgba(200,155,60,.45); }
.s-title {
  display: flex; align-items: center; gap: 7px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .78rem; font-weight: 700; letter-spacing: .22em; text-transform: uppercase;
  color: rgba(200,155,60,.6);
}
.s-diamond { font-size: 7px; color: rgba(200,155,60,.5); }

/* ── Scroll ───────────────────────────────────────────── */
.s-scroll { flex: 1; overflow-y: auto; position: relative; z-index: 1; }
.s-content { max-width: 560px; margin: 0 auto; padding: 20px 24px 40px; }

/* ── Seções ───────────────────────────────────────────── */
.s-section { margin-bottom: 4px; }
.s-section-title {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .62rem; font-weight: 700; letter-spacing: .22em; text-transform: uppercase;
  color: rgba(200,155,60,.45); margin-bottom: 14px;
}
.section-header-row {
  display: flex; align-items: center; justify-content: space-between; margin-bottom: 14px;
}
.section-header-row .s-section-title { margin-bottom: 0; }
.s-sublabel {
  font-size: .6rem; font-family: 'Rajdhani','Inter',sans-serif;
  letter-spacing: .14em; text-transform: uppercase;
  color: rgba(232,224,208,.3); margin-bottom: 8px;
}
.s-divider {
  height: 1px; margin: 20px 0;
  background: linear-gradient(to right,transparent,rgba(200,155,60,.2),transparent);
}
.disabled-section { opacity: .4; pointer-events: none; }

/* ── SpellCoach sync ──────────────────────────────────────── */
.sc-badge-label {
  font-family:    'Rajdhani','Inter',sans-serif;
  font-size:      .48rem;
  font-weight:    700;
  letter-spacing: .16em;
  padding:        2px 7px;
  border-radius:  2px;
  background:     rgba(6,182,212,.1);
  color:          rgba(6,182,212,.75);
  border:         1px solid rgba(6,182,212,.25);
}
.sc-sync-btn { width: 100%; justify-content: center; }
.sc-sync-btn.test-done { border-color: rgba(39,174,96,.4); color: rgba(39,174,96,.9); }
.sc-sync-btn.test-err  { border-color: rgba(192,57,43,.4); color: rgba(192,57,43,.8); }
.sc-err-count { color: rgba(251,146,60,.7); margin-left: 4px; }
.sc-err-text  { font-size: .62rem; }

/* ── Voice cards ──────────────────────────────────────── */
.voice-grid { display: flex; flex-direction: column; gap: 6px; margin-bottom: 16px; }
.voice-card {
  display: flex; align-items: center; gap: 10px;
  padding: 10px 12px; border-radius: 4px; cursor: pointer; transition: all .15s;
  background: rgba(28,42,58,.4); border: 1px solid rgba(200,155,60,.1);
  text-align: left;
}
.voice-card:hover { border-color: rgba(200,155,60,.3); background: rgba(28,42,58,.7); }
.voice-card--active { border-color: rgba(200,155,60,.55); background: rgba(200,155,60,.06); }

.voice-radio {
  width: 14px; height: 14px; border-radius: 50%;
  border: 1px solid rgba(200,155,60,.4);
  display: flex; align-items: center; justify-content: center;
  flex-shrink: 0; transition: border-color .15s;
}
.voice-card--active .voice-radio { border-color: #C89B3C; }
.voice-radio-dot { width: 7px; height: 7px; border-radius: 50%; background: #C89B3C; }

.voice-info { flex: 1; min-width: 0; }
.voice-name { display: block; font-size: .8rem; font-weight: 600; color: rgba(232,224,208,.85); }
.voice-gender { display: block; font-size: .62rem; color: rgba(232,224,208,.35); margin-top: 1px; }

.preview-btn {
  width: 28px; height: 28px; border-radius: 50%;
  display: flex; align-items: center; justify-content: center;
  background: rgba(200,155,60,.1); border: 1px solid rgba(200,155,60,.2);
  color: rgba(200,155,60,.6); cursor: pointer; transition: all .15s;
  flex-shrink: 0;
}
.preview-btn:hover:not(:disabled) { background: rgba(200,155,60,.2); color: #C89B3C; }
.preview-btn:disabled { opacity: .4; cursor: default; }
.spin-small { display: inline-block; animation: spin .7s linear infinite; font-size: .9rem; }
@keyframes spin { to { transform: rotate(360deg); } }

.preview-err { font-size: .62rem; color: rgba(192,57,43,.7); margin-top: -8px; margin-bottom: 12px; }

/* ── Volume ───────────────────────────────────────────── */
.volume-row { display: flex; align-items: center; gap: 10px; margin-top: 2px; }
.volume-label {
  display: flex; align-items: center; gap: 5px;
  font-size: .62rem; font-family: 'Rajdhani','Inter',sans-serif;
  letter-spacing: .1em; text-transform: uppercase;
  color: rgba(232,224,208,.35); flex-shrink: 0; min-width: 70px;
}
.volume-slider {
  flex: 1; -webkit-appearance: none; appearance: none;
  height: 3px; border-radius: 2px;
  background: linear-gradient(
    to right,
    rgba(200,155,60,.7) calc(var(--val, 80) * 1%),
    rgba(255,255,255,.1) calc(var(--val, 80) * 1%)
  );
  outline: none; cursor: pointer;
}
.volume-slider::-webkit-slider-thumb {
  -webkit-appearance: none; width: 12px; height: 12px; border-radius: 50%;
  background: #C89B3C; border: 1px solid rgba(200,155,60,.5); cursor: pointer;
}
.volume-val { font-size: .62rem; font-family: monospace; color: rgba(200,155,60,.5); min-width: 32px; text-align: right; }

/* ── Toggle ───────────────────────────────────────────── */
.toggle-btn {
  width: 36px; height: 20px; border-radius: 10px;
  border: none; cursor: pointer; position: relative;
  transition: background .2s;
}
.toggle-on  { background: rgba(200,155,60,.45); }
.toggle-off { background: rgba(255,255,255,.1); }
.toggle-thumb {
  position: absolute; top: 3px; width: 14px; height: 14px;
  border-radius: 50%; background: #fff; transition: left .2s;
}
.toggle-on  .toggle-thumb { left: 19px; }
.toggle-off .toggle-thumb { left: 3px; }

/* ── Categorias ───────────────────────────────────────── */
.category-grid { display: flex; flex-wrap: wrap; gap: 6px; }
.cat-chip {
  display: flex; align-items: center; gap: 5px;
  padding: 4px 10px; border-radius: 2px; font-size: .65rem; font-weight: 600;
  font-family: 'Rajdhani','Inter',sans-serif; letter-spacing: .1em; text-transform: uppercase;
  cursor: pointer; transition: all .15s; border: 1px solid transparent;
}
.cat-on  { background: rgba(255,255,255,.05); }
.cat-off { background: transparent; border-color: rgba(255,255,255,.08); color: rgba(255,255,255,.25); }
.cat-dot { width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0; }

/* ── Severidade ───────────────────────────────────────── */
.severity-row { display: flex; gap: 6px; }
.sev-btn {
  flex: 1; padding: 6px; border-radius: 3px;
  font-size: .65rem; font-weight: 700; font-family: 'Rajdhani','Inter',sans-serif;
  letter-spacing: .12em; text-transform: uppercase;
  cursor: pointer; transition: all .15s; border: 1px solid transparent;
}
.sev-active  { background: rgba(200,155,60,.12); border-color: rgba(200,155,60,.45); color: rgba(200,155,60,.9); }
.sev-inactive { background: transparent; border-color: rgba(255,255,255,.08); color: rgba(255,255,255,.3); }
.sev-inactive:hover { border-color: rgba(200,155,60,.25); color: rgba(200,155,60,.6); }
.sev-desc { font-size: .6rem; color: rgba(232,224,208,.3); margin-top: 6px; min-height: 14px; }

/* ── OCR banner ───────────────────────────────────────── */
.ocr-banner {
  display: flex; gap: 12px; align-items: flex-start;
  padding: 12px 14px; border-radius: 4px;
  background: rgba(200,155,60,.05);
  border: 1px solid rgba(200,155,60,.2);
}
.ocr-icon { font-size: 1.1rem; line-height: 1; flex-shrink: 0; margin-top: 1px; color: rgba(200,155,60,.7); }
.ocr-title { font-size: .72rem; font-weight: 600; color: rgba(200,155,60,.75); margin-bottom: 5px; }
.ocr-desc { font-size: .68rem; line-height: 1.55; color: rgba(232,224,208,.45); }
.ocr-desc strong { color: rgba(232,224,208,.75); font-weight: 600; }
.ocr-desc em { font-style: normal; color: rgba(200,155,60,.6); }

/* ── Test button ──────────────────────────────────────────── */
.test-btn {
  display: flex; align-items: center; justify-content: center; gap: 6px;
  width: 100%; padding: 9px 16px; border-radius: 3px; cursor: pointer;
  font-family: 'Rajdhani','Inter',sans-serif; font-size: .72rem; font-weight: 700;
  letter-spacing: .14em; text-transform: uppercase; transition: all .2s;
  background: rgba(200,155,60,.08); border: 1px solid rgba(200,155,60,.35);
  color: rgba(200,155,60,.8);
}
.test-btn:hover:not(:disabled) { background: rgba(200,155,60,.15); border-color: rgba(200,155,60,.6); color: #C89B3C; }
.test-btn:disabled { opacity: .5; cursor: default; }
.test-btn.test-done { background: rgba(39,174,96,.1); border-color: rgba(39,174,96,.4); color: rgba(39,174,96,.85); }
</style>
