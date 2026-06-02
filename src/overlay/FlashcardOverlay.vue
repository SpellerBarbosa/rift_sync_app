<script setup lang="ts">
import { computed } from 'vue'
import { useCoachStore } from '../stores/coach'
import type { AlertCategory, AlertSeverity } from '../stores/coach'

const coach = useCoachStore()

const CATEGORY_COLOR: Record<AlertCategory, string> = {
  OBJECTIVE:    '#C89B3C',
  VISION:       '#06B6D4',
  MACRO:        '#A78BFA',
  TRADE:        '#FB923C',
  POSITIONING:  '#60A5FA',
}

const CATEGORY_LABEL: Record<AlertCategory, string> = {
  OBJECTIVE:    'Objetivo',
  VISION:       'Visão',
  MACRO:        'Macro',
  TRADE:        'Trade',
  POSITIONING:  'Posição',
}

const DISMISS_MS: Record<AlertSeverity, number> = {
  CRITICAL: 10_000,
  WARNING:   6_000,
  INFO:      4_000,
}

const SEVERITY_ACCENT: Record<AlertSeverity, string> = {
  CRITICAL: '#E74C3C',
  WARNING:  '#C89B3C',
  INFO:     '#4A6080',
}

const SEVERITY_BG: Record<AlertSeverity, string> = {
  CRITICAL: 'rgba(231,76,60,0.10)',
  WARNING:  'rgba(10,14,20,0.93)',
  INFO:     'rgba(6,8,12,0.90)',
}

const card       = computed(() => coach.activeCard)
const pending    = computed(() => coach.cardQueue.length)
const accent     = computed(() => card.value ? SEVERITY_ACCENT[card.value.severity] : '#4A6080')
const catColor   = computed(() => card.value ? CATEGORY_COLOR[card.value.category] : '#C89B3C')
const catLabel   = computed(() => card.value ? CATEGORY_LABEL[card.value.category] : '')
const bgColor    = computed(() => card.value ? SEVERITY_BG[card.value.severity] : 'transparent')
const dismissMs  = computed(() => card.value ? DISMISS_MS[card.value.severity] : 4_000)
</script>

<template>
  <div class="fixed top-5 right-5 z-50 w-[320px] pointer-events-none select-none">
    <Transition name="fc">
      <div
        v-if="card"
        :key="card.id"
        class="fc-card"
        :style="{ background: bgColor }"
      >
        <!-- Accent bar esquerdo (severity) -->
        <div class="fc-accent" :style="{ background: accent }" />

        <!-- Corpo -->
        <div class="fc-body">

          <!-- Header: categoria + pending -->
          <div class="fc-header">
            <span class="fc-category" :style="{ color: catColor }">
              {{ catLabel }}
            </span>
            <span v-if="pending > 0" class="fc-pending">
              +{{ pending }}
            </span>
          </div>

          <!-- Mensagem principal -->
          <p class="fc-message">{{ card.message }}</p>

          <!-- Progress bar -->
          <div class="fc-track">
            <div
              class="fc-fill progress-run"
              :style="{
                '--dur':   dismissMs + 'ms',
                '--color': accent,
              }"
            />
          </div>

        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* ── Transição ───────────────────────────────────────────── */
.fc-enter-active { transition: opacity 0.18s ease, transform 0.18s ease; }
.fc-leave-active { transition: opacity 0.12s ease, transform 0.12s ease; }
.fc-enter-from   { opacity: 0; transform: translateX(16px); }
.fc-leave-to     { opacity: 0; transform: translateX(16px); }

/* ── Card ────────────────────────────────────────────────── */
.fc-card {
  display: flex;
  border-radius: 3px;
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.055);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.55), 0 1px 0 rgba(255,255,255,0.03);
  backdrop-filter: blur(12px);
}

/* ── Barra lateral de severidade ─────────────────────────── */
.fc-accent {
  width: 3px;
  flex-shrink: 0;
}

/* ── Corpo do card ───────────────────────────────────────── */
.fc-body {
  flex: 1;
  min-width: 0;
  padding: 10px 13px 0;
}

/* ── Header ──────────────────────────────────────────────── */
.fc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 5px;
}

.fc-category {
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  font-family: 'Rajdhani', 'Inter', sans-serif;
}

.fc-pending {
  font-size: 9px;
  font-weight: 600;
  color: rgba(255, 255, 255, 0.22);
  font-family: 'Rajdhani', 'Inter', sans-serif;
  letter-spacing: 0.05em;
}

/* ── Mensagem ────────────────────────────────────────────── */
.fc-message {
  font-size: 12.5px;
  line-height: 1.5;
  font-weight: 500;
  color: rgba(232, 224, 210, 0.92);
  letter-spacing: 0.01em;
  margin: 0 0 10px;
  word-break: break-word;
}

/* ── Progress bar ────────────────────────────────────────── */
.fc-track {
  height: 2px;
  background: rgba(255, 255, 255, 0.06);
  margin: 0 -13px;
  overflow: hidden;
}

.fc-fill {
  height: 100%;
  width: 100%;
  background: var(--color);
  opacity: 0.7;
  animation: bar-shrink var(--dur) linear forwards;
}

@keyframes bar-shrink {
  from { width: 100%; }
  to   { width: 0%; }
}
</style>
