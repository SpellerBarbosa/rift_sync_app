<script setup lang="ts">
import { ref, computed } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

// ── Estado ────────────────────────────────────────────────────
const visible       = ref(false)
const downloading   = ref(false)
const downloaded    = ref(false)
const progress      = ref(0)
const newVersion    = ref('')
const releaseNotes  = ref('')
const error         = ref<string | null>(null)

let updateHandle: Awaited<ReturnType<typeof check>> = null

const progressLabel = computed(() => {
  if (downloaded.value) return 'Pronto para instalar'
  if (downloading.value) return `Baixando… ${progress.value}%`
  return ''
})

// ── API pública ───────────────────────────────────────────────
async function checkForUpdates() {
  try {
    const update = await check()
    if (!update?.available) return

    updateHandle       = update
    newVersion.value   = update.version
    releaseNotes.value = update.body ?? ''
    visible.value      = true
  } catch {
    // Silencioso — updater não bloqueia o uso do app
  }
}

async function startDownload() {
  if (!updateHandle || downloading.value) return
  downloading.value = true
  error.value       = null
  progress.value    = 0

  try {
    let contentLength = 0
    let received      = 0

    await updateHandle.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        contentLength = event.data.contentLength ?? 0
      } else if (event.event === 'Progress') {
        received += event.data.chunkLength
        progress.value = contentLength > 0
          ? Math.round((received / contentLength) * 100)
          : 0
      } else if (event.event === 'Finished') {
        downloaded.value = true
        progress.value   = 100
      }
    })
  } catch (e: any) {
    error.value = typeof e === 'string' ? e : (e?.message ?? 'Erro ao baixar atualização')
    downloading.value = false
  }
}

async function installAndRestart() {
  try {
    await relaunch()
  } catch {
    error.value = 'Reinicie o app manualmente para completar a atualização.'
  }
}

function dismiss() {
  visible.value = false
}

defineExpose({ checkForUpdates })
</script>

<template>
  <Transition name="modal-fade">
    <div v-if="visible" class="um-backdrop" @click.self="dismiss">
      <div class="um-panel">

        <!-- Barra decorativa superior -->
        <div class="um-top-bar" />

        <!-- Ícone + cabeçalho -->
        <div class="um-header">
          <div class="um-rune-icon">
            <svg width="36" height="36" viewBox="0 0 36 36" fill="none">
              <polygon
                points="18,2 34,10 34,26 18,34 2,26 2,10"
                stroke="rgba(200,155,60,0.6)" stroke-width="1.2" fill="none"
              />
              <polygon
                points="18,7 29,13 29,23 18,29 7,23 7,13"
                stroke="rgba(200,155,60,0.35)" stroke-width="1" fill="none"
              />
              <circle cx="18" cy="18" r="5" fill="rgba(200,155,60,0.5)" />
              <circle cx="18" cy="18" r="2.5" fill="#C89B3C" />
            </svg>
          </div>
          <div class="um-header-text">
            <p class="um-eyebrow">Nova versão disponível</p>
            <h2 class="um-version">RiftSync AI <span class="um-ver-num">v{{ newVersion }}</span></h2>
          </div>
          <button class="um-close" @click="dismiss" title="Mais tarde">
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
            </svg>
          </button>
        </div>

        <!-- Divisor -->
        <div class="um-divider" />

        <!-- Notas da versão -->
        <div class="um-notes-wrap" v-if="releaseNotes">
          <p class="um-notes-label">O que há de novo</p>
          <div class="um-notes">{{ releaseNotes }}</div>
        </div>

        <!-- Barra de progresso -->
        <div v-if="downloading || downloaded" class="um-progress-section">
          <div class="um-progress-bar-bg">
            <div
              class="um-progress-bar-fill"
              :style="{ width: `${progress}%` }"
              :class="{ 'fill-done': downloaded }"
            />
          </div>
          <p class="um-progress-label">{{ progressLabel }}</p>
        </div>

        <!-- Erro -->
        <p v-if="error" class="um-error">{{ error }}</p>

        <!-- Ações -->
        <div class="um-actions">
          <button
            v-if="!downloading && !downloaded"
            class="um-btn-secondary"
            @click="dismiss"
          >
            Mais tarde
          </button>

          <button
            v-if="!downloaded"
            class="um-btn-primary"
            :disabled="downloading"
            @click="startDownload"
          >
            <span v-if="downloading" class="um-spin">⟳</span>
            <template v-else>
              <svg width="13" height="13" viewBox="0 0 13 13" fill="none" style="flex-shrink:0">
                <path d="M6.5 1v8M3 6l3.5 3.5L10 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                <path d="M1 11h11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              </svg>
              Atualizar agora
            </template>
          </button>

          <button
            v-if="downloaded"
            class="um-btn-install"
            @click="installAndRestart"
          >
            <svg width="13" height="13" viewBox="0 0 13 13" fill="none" style="flex-shrink:0">
              <path d="M2 7l4 4 5-7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            Instalar e reiniciar
          </button>
        </div>

        <!-- Rodapé -->
        <p class="um-footer">
          As atualizações garantem coaching preciso para o patch atual.
        </p>

      </div>
    </div>
  </Transition>
</template>

<style scoped>
/* ── Backdrop ────────────────────────────────────────────────── */
.um-backdrop {
  position: fixed; inset: 0; z-index: 9999;
  display: flex; align-items: center; justify-content: center;
  background: rgba(1, 10, 19, 0.82);
  backdrop-filter: blur(4px);
}

/* ── Panel ───────────────────────────────────────────────────── */
.um-panel {
  width: 420px;
  background: linear-gradient(160deg, #0D1B2A 0%, #010A13 100%);
  border: 1px solid rgba(200,155,60,.3);
  border-radius: 8px;
  overflow: hidden;
  box-shadow:
    0 0 0 1px rgba(200,155,60,.08),
    0 24px 64px rgba(0,0,0,.7),
    0 0 80px rgba(200,155,60,.06);
  display: flex; flex-direction: column;
}

/* Barra dourada no topo */
.um-top-bar {
  height: 2px;
  background: linear-gradient(to right, transparent, #C89B3C, rgba(200,155,60,.4), transparent);
}

/* ── Header ──────────────────────────────────────────────────── */
.um-header {
  display: flex; align-items: center; gap: 14px;
  padding: 20px 20px 16px;
}

.um-rune-icon {
  flex-shrink: 0;
  width: 44px; height: 44px;
  display: flex; align-items: center; justify-content: center;
  background: rgba(200,155,60,.06);
  border: 1px solid rgba(200,155,60,.18);
  border-radius: 6px;
}

.um-header-text { flex: 1; min-width: 0; }

.um-eyebrow {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .58rem; font-weight: 700; letter-spacing: .22em;
  text-transform: uppercase; color: rgba(200,155,60,.55);
  margin-bottom: 3px;
}

.um-version {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: 1.05rem; font-weight: 700; letter-spacing: .04em;
  color: rgba(232,224,208,.9);
}

.um-ver-num { color: #C89B3C; }

.um-close {
  flex-shrink: 0; width: 28px; height: 28px;
  display: flex; align-items: center; justify-content: center;
  background: none; border: 1px solid rgba(255,255,255,.08);
  border-radius: 4px; cursor: pointer; color: rgba(232,224,208,.3);
  transition: all .15s;
}
.um-close:hover { border-color: rgba(255,255,255,.2); color: rgba(232,224,208,.7); }

/* ── Divisor ─────────────────────────────────────────────────── */
.um-divider {
  height: 1px; margin: 0 20px;
  background: linear-gradient(to right, transparent, rgba(200,155,60,.2), transparent);
}

/* ── Notas ───────────────────────────────────────────────────── */
.um-notes-wrap { padding: 14px 20px 0; }
.um-notes-label {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .58rem; font-weight: 700; letter-spacing: .18em;
  text-transform: uppercase; color: rgba(200,155,60,.4);
  margin-bottom: 8px;
}
.um-notes {
  font-size: .7rem; color: rgba(232,224,208,.5); line-height: 1.6;
  max-height: 100px; overflow-y: auto;
  white-space: pre-wrap;
}

/* ── Progresso ───────────────────────────────────────────────── */
.um-progress-section { padding: 14px 20px 0; }
.um-progress-bar-bg {
  height: 4px; border-radius: 2px;
  background: rgba(255,255,255,.08); overflow: hidden;
}
.um-progress-bar-fill {
  height: 100%; border-radius: 2px;
  background: linear-gradient(to right, rgba(200,155,60,.6), #C89B3C);
  transition: width .3s ease;
}
.um-progress-bar-fill.fill-done {
  background: linear-gradient(to right, #4ADE80, rgba(74,222,128,.7));
}
.um-progress-label {
  font-size: .62rem; color: rgba(232,224,208,.4); margin-top: 6px;
  font-family: 'Rajdhani','Inter',sans-serif; letter-spacing: .08em;
}

/* ── Erro ────────────────────────────────────────────────────── */
.um-error { padding: 10px 20px 0; font-size: .65rem; color: rgba(239,68,68,.7); }

/* ── Ações ───────────────────────────────────────────────────── */
.um-actions {
  display: flex; gap: 8px;
  padding: 16px 20px 12px;
}

.um-btn-secondary {
  flex: 1; padding: 9px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .7rem; font-weight: 700; letter-spacing: .12em; text-transform: uppercase;
  background: transparent; border: 1px solid rgba(255,255,255,.1);
  border-radius: 3px; cursor: pointer; color: rgba(232,224,208,.4);
  transition: all .15s;
}
.um-btn-secondary:hover { border-color: rgba(255,255,255,.2); color: rgba(232,224,208,.7); }

.um-btn-primary {
  flex: 2; display: flex; align-items: center; justify-content: center; gap: 7px;
  padding: 9px 16px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .72rem; font-weight: 700; letter-spacing: .14em; text-transform: uppercase;
  background: rgba(200,155,60,.12); border: 1px solid rgba(200,155,60,.5);
  border-radius: 3px; cursor: pointer; color: #C89B3C;
  transition: all .2s;
}
.um-btn-primary:hover:not(:disabled) {
  background: rgba(200,155,60,.22);
  border-color: #C89B3C;
  box-shadow: 0 0 20px rgba(200,155,60,.2);
}
.um-btn-primary:disabled { opacity: .5; cursor: default; }

.um-btn-install {
  flex: 1; display: flex; align-items: center; justify-content: center; gap: 7px;
  padding: 9px 16px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .72rem; font-weight: 700; letter-spacing: .14em; text-transform: uppercase;
  background: rgba(74,222,128,.1); border: 1px solid rgba(74,222,128,.4);
  border-radius: 3px; cursor: pointer; color: #4ADE80;
  transition: all .2s;
}
.um-btn-install:hover {
  background: rgba(74,222,128,.18);
  box-shadow: 0 0 20px rgba(74,222,128,.15);
}

.um-spin { display: inline-block; animation: spin .7s linear infinite; }

/* ── Rodapé ──────────────────────────────────────────────────── */
.um-footer {
  padding: 0 20px 16px;
  font-size: .58rem; color: rgba(232,224,208,.2);
  font-family: 'Rajdhani','Inter',sans-serif; letter-spacing: .06em;
  text-align: center;
}

/* ── Transição ───────────────────────────────────────────────── */
.modal-fade-enter-active { transition: opacity .25s ease, transform .25s ease; }
.modal-fade-leave-active { transition: opacity .2s ease, transform .2s ease; }
.modal-fade-enter-from   { opacity: 0; transform: scale(.96) translateY(8px); }
.modal-fade-leave-to     { opacity: 0; transform: scale(.96) translateY(8px); }

@keyframes spin { to { transform: rotate(360deg); } }
</style>
