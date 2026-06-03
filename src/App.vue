<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { useGameStateStore } from './stores/gameState'
import { useCoachStore }    from './stores/coach'
import { useSettingsStore } from './stores/settings'
import TitleBar     from './components/TitleBar.vue'
import UpdateModal  from './components/UpdateModal.vue'

const updateModal = ref<InstanceType<typeof UpdateModal> | null>(null)

const gameStateStore = useGameStateStore()
const coachStore     = useCoachStore()
const settingsStore  = useSettingsStore()

// Avaliado de forma síncrona — evita que TitleBar renderize antes do onMounted
// em janelas que não deveriam tê-la (rune_panel, build_panel, flashcard).
const _windowLabel   = getCurrentWindow().label
const isOverlayWindow = ref(
  _windowLabel === 'flashcard'     ||
  _windowLabel === 'ward_win'      ||
  _windowLabel === 'matchup_win'   ||
  _windowLabel === 'champ_pick_win'||
  _windowLabel === 'loading_win'   ||
  _windowLabel === 'ingame_build'  ||
  _windowLabel === 'rune_panel'    ||
  _windowLabel === 'build_panel'
)

onMounted(async () => {
  const label = _windowLabel

  // Todas as janelas não-main precisam de fundo transparente imediatamente,
  // antes de qualquer renderização, para evitar flash/fantasma branco.
  if (label !== 'main') {
    document.body.style.backgroundColor = 'transparent'
    document.documentElement.style.backgroundColor = 'transparent'
  }

  if (label === 'flashcard') {
    await settingsStore.load()
    await Promise.all([
      gameStateStore.listenPhaseOnly(),
      coachStore.initListener(),
    ])
    return
  }

  // Todas as janelas auxiliares (overlays + painéis) só precisam renderizar a
  // sua página. Nenhuma delas deve iniciar o listener completo do LCU, pois
  // múltiplas conexões WebSocket interferem no fluxo de eventos.
  if (label !== 'main') return

  // Janela principal: carrega settings e inicia o game state listener
  await settingsStore.load()
  await gameStateStore.initListener()

  // Acorda o HuggingFace Space TTS em background para reduzir latência no primeiro uso
  invoke('warm_up_tts', { voice: settingsStore.ttsVoice }).catch(() => {})

  // Verifica atualizações 5s após init para não atrasar o startup
  setTimeout(() => updateModal.value?.checkForUpdates(), 5000)
})
</script>

<template>
  <RouterView v-if="isOverlayWindow" />
  <div v-else class="app-shell">
    <TitleBar />
    <main class="app-content">
      <RouterView v-slot="{ Component, route }">
        <Transition name="fade" mode="out-in">
          <component :is="Component" :key="route.path" />
        </Transition>
      </RouterView>
    </main>
    <UpdateModal ref="updateModal" />
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}
.app-content {
  flex: 1;
  overflow: hidden;
  position: relative;
}
.fade-enter-active,
.fade-leave-active { transition: opacity 0.15s ease; }
.fade-enter-from,
.fade-leave-to { opacity: 0; }
</style>
