<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useRouter } from 'vue-router'
import { ref, onMounted, onUnmounted } from 'vue'
import { useSettingsStore } from '../stores/settings'

const win      = getCurrentWindow()
const router   = useRouter()
const settings = useSettingsStore()

const maximized = ref(false)
let unlisten: (() => void) | null = null

onMounted(async () => {
  maximized.value = await win.isMaximized()
  unlisten = await win.onResized(async () => {
    maximized.value = await win.isMaximized()
  })
})

onUnmounted(() => unlisten?.())

function minimize()  { win.minimize() }
function toggleMax() { maximized.value ? win.unmaximize() : win.maximize() }
function close()     { win.close() }
function toggleLang() {
  settings.setLanguage(settings.appLanguage === 'pt-BR' ? 'en-US' : 'pt-BR')
}
</script>

<template>
  <div class="titlebar" data-tauri-drag-region>

    <!-- Drag region label (purely decorative) -->
    <div class="tb-label" data-tauri-drag-region>
      <span class="tb-diamond">◆</span>
      <span class="tb-name">RiftSync AI</span>
    </div>

    <!-- Window controls — NOT inside drag region -->
    <div class="tb-controls">

      <!-- Language toggle -->
      <button class="tb-btn tb-lang" @click="toggleLang" :title="$t('titleBar.switchLang')">
        <span class="tb-lang-text">{{ settings.appLanguage === 'pt-BR' ? 'PT' : 'EN' }}</span>
        <span class="tb-lang-sep">·</span>
        <span class="tb-lang-other">{{ settings.appLanguage === 'pt-BR' ? 'EN' : 'PT' }}</span>
      </button>
      <div class="tb-sep" />

      <!-- Settings button -->
      <button class="tb-btn tb-settings" @click="router.push('/settings')" :title="$t('titleBar.settings')">
        <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
          <circle cx="6.5" cy="6.5" r="2" stroke="currentColor" stroke-width="1.2"/>
          <path d="M6.5 1v1.5M6.5 10.5V12M1 6.5h1.5M10.5 6.5H12M2.6 2.6l1.05 1.05M9.35 9.35l1.05 1.05M2.6 10.4l1.05-1.05M9.35 3.65l1.05-1.05"
            stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
        </svg>
      </button>
      <div class="tb-sep" />
      <button class="tb-btn" @click="minimize" :title="$t('titleBar.minimize')">
        <svg width="10" height="1" viewBox="0 0 10 1">
          <line x1="0" y1="0.5" x2="10" y2="0.5" stroke="currentColor" stroke-width="1.5"/>
        </svg>
      </button>

      <button class="tb-btn" @click="toggleMax" :title="$t('titleBar.maximize')">
        <!-- Restore icon when maximized -->
        <svg v-if="maximized" width="10" height="10" viewBox="0 0 10 10">
          <rect x="2" y="0" width="8" height="8" fill="none" stroke="currentColor" stroke-width="1.2"/>
          <rect x="0" y="2" width="8" height="8" fill="#010A13" stroke="currentColor" stroke-width="1.2"/>
        </svg>
        <!-- Maximize icon otherwise -->
        <svg v-else width="10" height="10" viewBox="0 0 10 10">
          <rect x="0.6" y="0.6" width="8.8" height="8.8" fill="none" stroke="currentColor" stroke-width="1.2"/>
        </svg>
      </button>

      <button class="tb-btn close-btn" @click="close" :title="$t('titleBar.close')">
        <svg width="10" height="10" viewBox="0 0 10 10">
          <line x1="0.5" y1="0.5" x2="9.5" y2="9.5" stroke="currentColor" stroke-width="1.4"/>
          <line x1="9.5" y1="0.5" x2="0.5" y2="9.5" stroke="currentColor" stroke-width="1.4"/>
        </svg>
      </button>
    </div>

  </div>
</template>

<style scoped>
.titlebar {
  height: 30px;
  min-height: 30px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: #010A13;
  border-bottom: 1px solid rgba(200, 155, 60, 0.18);
  user-select: none;
  flex-shrink: 0;
}

.tb-label {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-left: 12px;
  pointer-events: none; /* drag handled by parent */
}

.tb-diamond {
  font-size: 7px;
  color: rgba(200, 155, 60, 0.55);
  line-height: 1;
}

.tb-name {
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: rgba(200, 155, 60, 0.4);
}

/* Controls */
.tb-controls {
  display: flex;
  height: 100%;
}

.tb-btn {
  width: 44px;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: rgba(200, 155, 60, 0.4);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.tb-btn:hover {
  background: rgba(200, 155, 60, 0.08);
  color: rgba(200, 155, 60, 0.9);
}

.close-btn:hover {
  background: rgba(192, 57, 43, 0.75);
  color: #fff;
}

.tb-settings { color: rgba(200, 155, 60, 0.35); }
.tb-settings:hover { color: rgba(200, 155, 60, 0.85); background: rgba(200,155,60,.08); }

.tb-sep {
  width: 1px;
  height: 14px;
  background: rgba(200,155,60,.12);
  align-self: center;
}

/* Language toggle */
.tb-lang {
  width: 54px;
  gap: 3px;
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.1em;
}

.tb-lang-text {
  color: rgba(200, 155, 60, 0.9);
}

.tb-lang-sep {
  color: rgba(200, 155, 60, 0.25);
  font-size: 8px;
}

.tb-lang-other {
  color: rgba(200, 155, 60, 0.3);
}

.tb-lang:hover .tb-lang-text {
  color: #C89B3C;
}

.tb-lang:hover .tb-lang-other {
  color: rgba(200, 155, 60, 0.55);
}
</style>
