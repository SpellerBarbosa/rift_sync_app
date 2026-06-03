<script setup lang="ts">
import { computed } from 'vue'
import { useCoachStore } from '../stores/coach'
import type { AlertCategory, AlertSeverity } from '../stores/coach'

const coach = useCoachStore()

const CATEGORY_COLOR: Record<AlertCategory, string> = {
  OBJECTIVE:   '#C89B3C',
  VISION:      '#06B6D4',
  MACRO:       '#A78BFA',
  TRADE:       '#FB923C',
  POSITIONING: '#60A5FA',
}

const CATEGORY_LABEL: Record<AlertCategory, string> = {
  OBJECTIVE:   'Objetivo',
  VISION:      'Visão',
  MACRO:       'Macro',
  TRADE:       'Trade',
  POSITIONING: 'Posição',
}

const DISMISS_MS: Record<AlertSeverity, number> = {
  CRITICAL: 10_000,
  WARNING:   6_000,
  INFO:      4_000,
}

const SEVERITY_ACCENT: Record<AlertSeverity, string> = {
  CRITICAL: '#E74C3C',
  WARNING:  '#C89B3C',
  INFO:     '#06B6D4',
}

const card      = computed(() => coach.activeCard)
const pending   = computed(() => coach.cardQueue.length)
const accent    = computed(() => card.value ? SEVERITY_ACCENT[card.value.severity] : '#06B6D4')
const catColor  = computed(() => card.value ? CATEGORY_COLOR[card.value.category] : '#C89B3C')
const catLabel  = computed(() => card.value ? CATEGORY_LABEL[card.value.category] : '')
const dismissMs = computed(() => card.value ? DISMISS_MS[card.value.severity] : 4_000)
</script>

<template>
  <div class="fc-root">
    <Transition name="fc">
      <div
        v-if="card"
        :key="card.id"
        class="fc-card"
        :style="{ '--accent': accent, '--cat': catColor }"
      >
        <!-- Corte angular hextech superior-direito -->
        <div class="fc-corner-cut" />

        <!-- Linha de acento superior (severity color) -->
        <div class="fc-top-line" />

        <!-- Conteúdo -->
        <div class="fc-inner">

          <!-- Header -->
          <div class="fc-header">
            <div class="fc-cat-row">
              <div class="fc-cat-diamond" />
              <span class="fc-category">{{ catLabel }}</span>
            </div>
            <span v-if="pending > 0" class="fc-pending">+{{ pending }}</span>
          </div>

          <!-- Mensagem -->
          <p class="fc-message">{{ card.message }}</p>

          <!-- Footer: severity badge + progress -->
          <div class="fc-footer">
            <span class="fc-severity-badge" :class="card.severity.toLowerCase()">
              {{ card.severity }}
            </span>
            <div class="fc-track">
              <div
                class="fc-fill"
                :style="{ '--dur': dismissMs + 'ms' }"
              />
            </div>
          </div>

        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.fc-root {
  position: fixed;
  top: 18px;
  right: 18px;
  width: 300px;
  pointer-events: none;
  user-select: none;
  z-index: 50;
}

/* ── Transição ───────────────────────────────────────────── */
.fc-enter-active { transition: opacity 0.20s ease, transform 0.20s cubic-bezier(0.22,1,0.36,1); }
.fc-leave-active { transition: opacity 0.14s ease, transform 0.14s ease; }
.fc-enter-from   { opacity: 0; transform: translateX(20px); }
.fc-leave-to     { opacity: 0; transform: translateX(12px); }

/* ── Card hextech ────────────────────────────────────────── */
.fc-card {
  position: relative;
  background: rgba(4, 10, 22, 0.97);
  border: 1px solid rgba(var(--accent-rgb, 6,182,212), 0.28);
  border-color: color-mix(in srgb, var(--accent) 28%, transparent);

  /* Corte angular no canto superior-direito (hextech) */
  clip-path: polygon(
    0 0,
    calc(100% - 16px) 0,
    100% 16px,
    100% 100%,
    0 100%
  );

  overflow: visible;
}

/* Linha de acento superior */
.fc-top-line {
  position: absolute;
  top: -1px;
  left: 0;
  right: 16px;
  height: 2px;
  background: var(--accent);
  opacity: 0.9;
}

/* Triângulo do canto cortado */
.fc-corner-cut {
  position: absolute;
  top: -1px;
  right: -1px;
  width: 18px;
  height: 18px;
  background: rgba(4, 10, 22, 0.97);
  clip-path: polygon(100% 0, 0 0, 100% 100%);
  border-top: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
  border-right: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
}

/* ── Inner ───────────────────────────────────────────────── */
.fc-inner {
  padding: 10px 13px 0;
}

/* ── Header ──────────────────────────────────────────────── */
.fc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}

.fc-cat-row {
  display: flex;
  align-items: center;
  gap: 5px;
}

.fc-cat-diamond {
  width: 5px;
  height: 5px;
  background: var(--cat);
  transform: rotate(45deg);
  flex-shrink: 0;
  opacity: 0.9;
}

.fc-category {
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.20em;
  text-transform: uppercase;
  color: var(--cat);
}

.fc-pending {
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: 9px;
  font-weight: 600;
  color: rgba(255,255,255,0.22);
  letter-spacing: 0.05em;
}

/* ── Mensagem ────────────────────────────────────────────── */
.fc-message {
  font-size: 12.5px;
  line-height: 1.5;
  font-weight: 500;
  color: rgba(232, 224, 210, 0.92);
  letter-spacing: 0.01em;
  margin: 0 0 9px;
  word-break: break-word;
}

/* ── Footer ──────────────────────────────────────────────── */
.fc-footer {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-bottom: 1px;
}

.fc-severity-badge {
  font-family: 'Rajdhani', 'Inter', sans-serif;
  font-size: 7px;
  font-weight: 700;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  padding: 1px 5px;
  border-radius: 1px;
  flex-shrink: 0;
}
.fc-severity-badge.critical {
  color: rgba(231,76,60,0.9);
  background: rgba(231,76,60,0.1);
  border: 1px solid rgba(231,76,60,0.25);
}
.fc-severity-badge.warning {
  color: rgba(200,155,60,0.9);
  background: rgba(200,155,60,0.1);
  border: 1px solid rgba(200,155,60,0.25);
}
.fc-severity-badge.info {
  color: rgba(6,182,212,0.8);
  background: rgba(6,182,212,0.08);
  border: 1px solid rgba(6,182,212,0.2);
}

/* ── Progress bar ────────────────────────────────────────── */
.fc-track {
  flex: 1;
  height: 2px;
  background: rgba(255,255,255,0.06);
  overflow: hidden;
}

.fc-fill {
  height: 100%;
  width: 100%;
  background: var(--accent);
  opacity: 0.75;
  animation: bar-shrink var(--dur) linear forwards;
}

@keyframes bar-shrink {
  from { width: 100%; }
  to   { width: 0%; }
}
</style>
