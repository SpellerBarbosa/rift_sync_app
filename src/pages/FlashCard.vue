<script setup lang="ts">
import { computed, watch, onMounted } from 'vue'
import { invoke }                    from '@tauri-apps/api/core'
import { useCoachStore }             from '../stores/coach'
import type { AlertCategory, AlertSeverity } from '../stores/coach'

const coach   = useCoachStore()
const card    = computed(() => coach.activeCard)
const pending = computed(() => coach.cardQueue.length)

onMounted(() => {
  if (card.value) invoke('show_flashcard_window').catch(() => {})
})

watch(card, (newCard, oldCard) => {
  if (newCard && !oldCard) invoke('show_flashcard_window').catch(() => {})
})

function onAfterLeave() {
  if (!coach.activeCard && coach.cardQueue.length === 0) {
    invoke('hide_flashcard_window').catch(() => {})
  }
}

// ── Mapas visuais ────────────────────────────────────────────

const CAT_ICON: Record<AlertCategory, string> = {
  OBJECTIVE:   '⚔',
  VISION:      '◉',
  MACRO:       '◈',
  TRADE:       '⚡',
  POSITIONING: '◎',
}

const CAT_LABEL: Record<AlertCategory, string> = {
  OBJECTIVE:   'Objetivo',
  VISION:      'Visão',
  MACRO:       'Macro',
  TRADE:       'Trade',
  POSITIONING: 'Posição',
}

const CAT_COLOR: Record<AlertCategory, string> = {
  OBJECTIVE:   '#C89B3C',
  VISION:      '#22D3EE',
  MACRO:       '#A78BFA',
  TRADE:       '#FB923C',
  POSITIONING: '#60A5FA',
}

// A severidade comanda o esquema de cor principal do card
const SEV_COLOR: Record<AlertSeverity, string> = {
  CRITICAL: '#E84057',
  WARNING:  '#F0B429',
  INFO:     '#0BC4EF',
}

const SEV_GLOW: Record<AlertSeverity, string> = {
  CRITICAL: 'rgba(232, 64, 87,  0.55)',
  WARNING:  'rgba(240, 180, 41, 0.40)',
  INFO:     'rgba(11, 196, 239, 0.30)',
}

const SEV_TINT: Record<AlertSeverity, string> = {
  CRITICAL: 'rgba(232, 64, 87,  0.07)',
  WARNING:  'rgba(240, 180, 41, 0.05)',
  INFO:     'rgba(11, 196, 239, 0.04)',
}

const SEV_BORDER: Record<AlertSeverity, string> = {
  CRITICAL: 'rgba(232, 64, 87,  0.40)',
  WARNING:  'rgba(240, 180, 41, 0.30)',
  INFO:     'rgba(11, 196, 239, 0.22)',
}

const DISMISS_MS: Record<AlertSeverity, number> = {
  CRITICAL: 10_000,
  WARNING:   6_000,
  INFO:      4_000,
}

const sevColor   = computed(() => card.value ? SEV_COLOR[card.value.severity]   : '#0BC4EF')
const sevGlow    = computed(() => card.value ? SEV_GLOW[card.value.severity]    : 'transparent')
const sevTint    = computed(() => card.value ? SEV_TINT[card.value.severity]    : 'transparent')
const sevBorder  = computed(() => card.value ? SEV_BORDER[card.value.severity]  : 'transparent')
const catColor   = computed(() => card.value ? CAT_COLOR[card.value.category]   : '#C89B3C')
const catLabel   = computed(() => card.value ? CAT_LABEL[card.value.category]   : '')
const catIcon    = computed(() => card.value ? CAT_ICON[card.value.category]    : '')
const dismissMs  = computed(() => card.value ? DISMISS_MS[card.value.severity]  : 4_000)
const isCritical = computed(() => card.value?.severity === 'CRITICAL')
const sevLabel   = computed(() => card.value?.severity ?? '')
</script>

<template>
  <!-- Root transparente — ocupa toda a janela Tauri -->
  <div class="fc-root">
    <Transition name="fc-slide" @after-leave="onAfterLeave">
      <div
        v-if="card"
        :key="card.id"
        class="fc-card"
        :class="{ 'is-critical': isCritical }"
        :style="{
          '--sev':    sevColor,
          '--glow':   sevGlow,
          '--tint':   sevTint,
          '--border': sevBorder,
          '--cat':    catColor,
          '--dur':    dismissMs + 'ms',
        }"
      >
        <!-- ── Linha superior: barra de severidade (estreita) ── -->
        <div class="fc-top-bar" />

        <!-- ── Corpo principal ─────────────────────────────── -->
        <div class="fc-body">

          <!-- Barra lateral esquerda (severidade) -->
          <div class="fc-side-bar" />

          <!-- Conteúdo -->
          <div class="fc-content">

            <!-- Header: categoria | severity + fila -->
            <div class="fc-header">
              <div class="fc-cat">
                <span class="fc-cat-icon">{{ catIcon }}</span>
                <span class="fc-cat-label">{{ catLabel }}</span>
              </div>
              <div class="fc-right">
                <span class="fc-sev-tag">{{ sevLabel }}</span>
                <span v-if="pending > 0" class="fc-pending">+{{ pending }}</span>
              </div>
            </div>

            <!-- Mensagem -->
            <p class="fc-message">{{ card.message }}</p>

          </div>
        </div>

        <!-- ── Barra de progresso (baixo, largura total) ──── -->
        <div class="fc-progress-track">
          <div class="fc-progress-fill" />
        </div>

        <!-- ── Cantoneiras hextech ──────────────────────── -->
        <div class="fc-corner tl" />
        <div class="fc-corner br" />
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* ── Root — preenche a janela inteiramente ──────────────── */
.fc-root {
  width:  100vw;
  height: 100vh;
  background: transparent;
  overflow: hidden;
  pointer-events: none;
  user-select: none;
}

/* ── Slide (entrada / saída pela direita) ───────────────── */
.fc-slide-enter-active { transition: transform 0.28s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.20s ease; }
.fc-slide-leave-active { transition: transform 0.18s cubic-bezier(0.55, 0, 1, 0.45), opacity 0.15s ease; }
.fc-slide-enter-from,
.fc-slide-leave-to     { transform: translateX(105%); opacity: 0; }

/* ── Card — ocupa 100% da janela ────────────────────────── */
.fc-card {
  position: relative;
  width:  100%;
  height: 100%;
  display: flex;
  flex-direction: column;

  background:
    linear-gradient(160deg, rgba(16, 28, 56, 0.97) 0%, rgba(5, 10, 22, 0.98) 100%),
    var(--tint);
  background-blend-mode: normal;
  border: 1px solid var(--border);
  box-shadow:
    inset 0 0 60px var(--tint),
    0 0 24px rgba(0, 0, 0, 0.8);
}

/* CRITICAL: pulso suave na borda */
.is-critical {
  animation: crit-pulse 1.8s ease-in-out infinite;
}
@keyframes crit-pulse {
  0%, 100% { box-shadow: inset 0 0 60px var(--tint), 0 0 24px rgba(0,0,0,.8); }
  50%       { box-shadow: inset 0 0 60px var(--tint), 0 0 24px rgba(0,0,0,.8), 0 0 14px var(--glow); }
}

/* ── Barra superior (linha hextech fina no topo) ────────── */
.fc-top-bar {
  flex-shrink: 0;
  height: 2px;
  background: linear-gradient(90deg, transparent 0%, var(--sev) 40%, var(--sev) 60%, transparent 100%);
  opacity: 0.9;
}

/* ── Corpo (barra lateral + conteúdo) ───────────────────── */
.fc-body {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
}

/* Barra lateral esquerda (severidade) com glow */
.fc-side-bar {
  flex-shrink: 0;
  width: 3px;
  background: var(--sev);
  box-shadow: 2px 0 12px var(--glow), 0 0 6px var(--glow);
}

/* Área de conteúdo */
.fc-content {
  flex: 1;
  min-width: 0;
  padding: 9px 12px 8px 12px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}

/* ── Header: categoria + severity ───────────────────────── */
.fc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
}

.fc-cat {
  display: flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
}

.fc-cat-icon {
  font-size: 11px;
  color: var(--cat);
  line-height: 1;
  flex-shrink: 0;
  filter: drop-shadow(0 0 4px var(--cat));
}

.fc-cat-label {
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: 9.5px;
  font-weight: 700;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  color: var(--cat);
  white-space: nowrap;
}

.fc-right {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.fc-sev-tag {
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: 8px;
  font-weight: 700;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--sev);
  background: rgba(0, 0, 0, 0.35);
  border: 1px solid var(--border);
  padding: 1px 5px 0;
  border-radius: 1px;
}

.fc-pending {
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: 9px;
  font-weight: 600;
  color: rgba(255, 255, 255, 0.28);
  letter-spacing: 0.05em;
}

/* ── Mensagem ────────────────────────────────────────────── */
.fc-message {
  font-size: 12.5px;
  line-height: 1.45;
  font-weight: 500;
  color: rgba(224, 218, 206, 0.92);
  letter-spacing: 0.01em;
  margin: 0;
  word-break: break-word;
  /* Limita a 2 linhas para não vazar da janela */
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/* ── Barra de progresso (largura total do card) ─────────── */
.fc-progress-track {
  flex-shrink: 0;
  height: 3px;
  background: rgba(255, 255, 255, 0.06);
  overflow: hidden;
}

.fc-progress-fill {
  height: 100%;
  width: 100%;
  background: linear-gradient(90deg, var(--sev) 0%, color-mix(in srgb, var(--sev) 60%, transparent) 100%);
  box-shadow: 0 0 8px var(--glow);
  animation: progress-shrink var(--dur) linear forwards;
}

@keyframes progress-shrink {
  from { width: 100%; }
  to   { width: 0%;   }
}

/* ── Cantoneiras hextech (decorativas) ──────────────────── */
.fc-corner {
  position: absolute;
  width: 10px;
  height: 10px;
  pointer-events: none;
}

.fc-corner.tl {
  top:  3px;
  left: 4px;
  border-top:  1.5px solid var(--sev);
  border-left: 1.5px solid var(--sev);
  opacity: 0.7;
}

.fc-corner.br {
  bottom: 5px;
  right:  4px;
  border-bottom: 1.5px solid var(--sev);
  border-right:  1.5px solid var(--sev);
  opacity: 0.7;
}
</style>
