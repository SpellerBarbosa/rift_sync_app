<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
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
  _windowLabel === 'flashcard' || _windowLabel === 'rune_panel' || _windowLabel === 'build_panel'
)

onMounted(async () => {
  const label = _windowLabel

  if (label === 'flashcard') {
    document.body.style.backgroundColor = 'transparent'
    document.documentElement.style.backgroundColor = 'transparent'
    await settingsStore.load()
    await Promise.all([
      gameStateStore.listenPhaseOnly(),
      coachStore.initListener(),
    ])
    return
  }

  if (label === 'rune_panel' || label === 'build_panel') {
    // Janelinhas independentes: recebem dados via eventos Tauri, sem init adicional
    return
  }

  // Janela principal: carrega settings e inicia o game state listener
  await settingsStore.load()
  await gameStateStore.initListener()

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
