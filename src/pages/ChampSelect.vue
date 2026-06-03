<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useGameStateStore } from '../stores/gameState'
import { useChampSelectStore } from '../stores/champSelect'

const gameStateStore   = useGameStateStore()
const champSelectStore = useChampSelectStore()


onMounted(() => {
  champSelectStore.startPolling()
})

onUnmounted(() => {
  champSelectStore.reset()
})

watch(() => gameStateStore.phase, (phase) => {
  if (phase === 'PICK_PHASE') champSelectStore.startPolling()
  else if (phase !== 'BAN_PHASE') champSelectStore.reset()
})
</script>

<template>
  <div class="cs-root">
    <div class="cs-glow" />

    <div class="cs-status">
      <!-- Fase badge -->
      <div class="phase-badge">
        <div class="phase-dot" :class="gameStateStore.phase === 'BAN_PHASE' ? 'dot-ban' : 'dot-pick'" />
        {{ gameStateStore.phase === 'BAN_PHASE' ? 'Fase de Banimento' : 'Seleção de Campeão' }}
      </div>

      <!-- Estado do poll -->
      <div class="poll-state" v-if="!champSelectStore.showPanel">
        <template v-if="champSelectStore.pollError">
          <span class="err-text">⚠ {{ champSelectStore.pollError }}</span>
        </template>
        <template v-else-if="champSelectStore.localPick">
          <span v-if="champSelectStore.localPick.isLockedIn" class="ok-text">
            Campeão travado (id {{ champSelectStore.localPick.championId }}) — carregando...
          </span>
          <span v-else-if="champSelectStore.localPick.pickIntent > 0" class="hint-text">
            Hover: campeão {{ champSelectStore.localPick.pickIntent }} · role: {{ champSelectStore.localPick.assignedPosition || '—' }}
          </span>
          <span v-else class="hint-text">
            Aguardando seleção · role: {{ champSelectStore.localPick.assignedPosition || '—' }}
          </span>
        </template>
        <template v-else>
          <span class="hint-text">Conectando à sessão...</span>
        </template>
      </div>

    </div>

  </div>
</template>

<style scoped>
.cs-root {
  height: 100%;
  background: #010A13;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
}
.cs-glow {
  position: absolute; inset: 0; pointer-events: none;
  background: radial-gradient(ellipse 60% 50% at 50% 0%, rgba(10,30,60,.5) 0%, transparent 60%);
}
.cs-status {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  z-index: 1;
}
.phase-badge {
  display: flex;
  align-items: center;
  gap: 7px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .75rem;
  font-weight: 600;
  letter-spacing: .22em;
  text-transform: uppercase;
  color: rgba(200,155,60,.7);
  border: 1px solid rgba(200,155,60,.2);
  padding: 5px 14px;
  border-radius: 2px;
}
.phase-dot { width: 6px; height: 6px; border-radius: 50%; }
.dot-ban   { background: #C0392B; box-shadow: 0 0 6px #C0392B; animation: ping .9s ease infinite; }
.dot-pick  { background: #C89B3C; box-shadow: 0 0 6px #C89B3C; animation: ping .9s ease infinite; }

.poll-state { font-size: .62rem; letter-spacing: .04em; }
.hint-text  { color: rgba(232,224,208,.25); }
.ok-text    { color: rgba(200,155,60,.55); }
.err-text   { color: rgba(192,57,43,.7); font-family: monospace; font-size: .6rem; }

@keyframes ping { 0%,100%{opacity:1} 50%{opacity:.4} }
</style>
