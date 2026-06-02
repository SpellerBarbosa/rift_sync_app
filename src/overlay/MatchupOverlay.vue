<script setup lang="ts">
// ============================================================
// MatchupOverlay — Análise de matchup do oponente de lane
//
// Ativação: evento matchup_init (Rust) quando o oponente é
// identificado pela LCU Live API (primeiro fast-poll após spawn).
//
// Atualiza em tempo real:
//   matchup_level_update → nível atual do oponente
//   coach_alert (TRADE/WARNING) → pisca ao spike crítico
//
// Dados de win-rate: SpellCoach API via invoke get_matchup_stats
// (chamada única após identificação, best-effort — sem crash).
// ============================================================

import { ref, computed, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn }               from '@tauri-apps/api/event'
import { invoke }                                 from '@tauri-apps/api/core'

// ── Tipos ────────────────────────────────────────────────────

interface MatchupInitPayload {
  myChampionId: number
  enemyId:      number
  enemyName:    string
  enemyLevel:   number
  enemySpell1:  string
  enemySpell2:  string
}

interface LevelUpdatePayload {
  level:   number
  enemyId: number
  myId:    number
}

interface MatchupStats {
  championAId: number
  championBId: number
  patch:       string
  elo:         string
  gamesPlayed: number
  winRateA:    number
  winRateB:    number
}

// ── Estado reativo ───────────────────────────────────────────

const enemyName   = ref('')
const enemyKey    = ref('')    // CamelCase para ícone DDragon
const enemyLevel  = ref(0)
const enemySpell1 = ref('')
const enemySpell2 = ref('')
const myId        = ref(-1)
const enemyId     = ref(-1)

const winRateA    = ref<number | null>(null)  // WR do meu campeão vs inimigo
const winRateB    = ref<number | null>(null)
const gamesPlayed = ref(0)
const statsLoaded = ref(false)
const statsError  = ref(false)

// Pulsa quando o oponente atinge spike crítico (animação)
const spikeFlash  = ref(false)

// ── Listeners Tauri ──────────────────────────────────────────

let unlistenInit:        UnlistenFn | null = null
let unlistenLevelUpdate: UnlistenFn | null = null
let unlistenAlert:       UnlistenFn | null = null

onMounted(async () => {
  unlistenInit = await listen<MatchupInitPayload>('matchup_init', async ({ payload }) => {
    enemyName.value   = payload.enemyName
    enemyKey.value    = payload.enemyName.replace(/\s+/g, '')
    enemyLevel.value  = payload.enemyLevel
    enemySpell1.value = payload.enemySpell1
    enemySpell2.value = payload.enemySpell2
    myId.value        = payload.myChampionId
    enemyId.value     = payload.enemyId
    statsLoaded.value = false
    statsError.value  = false
    winRateA.value    = null

    // Busca win-rate uma vez (best-effort — não bloqueia se falhar)
    if (payload.myChampionId > 0 && payload.enemyId > 0) {
      try {
        const stats = await invoke<MatchupStats>('get_matchup_stats', {
          championAId: payload.myChampionId,
          championBId: payload.enemyId,
        })
        winRateA.value    = stats.winRateA
        winRateB.value    = stats.winRateB
        gamesPlayed.value = stats.gamesPlayed
        statsLoaded.value = true
      } catch {
        statsError.value = true
      }
    }
  })

  unlistenLevelUpdate = await listen<LevelUpdatePayload>('matchup_level_update', ({ payload }) => {
    enemyLevel.value = payload.level
  })

  // Pulsa visualmente quando um spike do inimigo é emitido
  unlistenAlert = await listen<{ category: string; severity: string }>('coach_alert', ({ payload }) => {
    if (
      payload.category === 'TRADE' &&
      payload.severity  === 'WARNING' &&
      enemyName.value    !== ''
    ) {
      triggerSpikeFlash()
    }
  })
})

onUnmounted(() => {
  unlistenInit?.()
  unlistenLevelUpdate?.()
  unlistenAlert?.()
})

// ── Helpers de UI ────────────────────────────────────────────

const SPIKE_LEVELS = [6, 11, 16]

function isSpikeLevel(lvl: number): boolean {
  return SPIKE_LEVELS.includes(lvl)
}

function triggerSpikeFlash() {
  spikeFlash.value = true
  setTimeout(() => { spikeFlash.value = false }, 1200)
}

// Ícone DDragon — versão fixada para evitar request extra durante a partida
const DDRAGON = 'https://ddragon.leagueoflegends.com/cdn/14.14.1/img/champion'
const iconUrl = computed(() =>
  enemyKey.value ? `${DDRAGON}/${enemyKey.value}.png` : ''
)

// Dificuldade derivada do win-rate
const difficulty = computed<{ label: string; color: string; score: number } | null>(() => {
  if (winRateA.value === null) return null
  const wr = winRateA.value
  if (wr >= 55) return { label: 'Favorável',     color: '#27AE60', score: Math.round((wr - 50) * 2) }
  if (wr >= 50) return { label: 'Levemente fav.', color: '#2ECC71', score: Math.round((wr - 50) * 2) }
  if (wr >= 45) return { label: 'Neutro',         color: '#C89B3C', score: 5 }
  if (wr >= 40) return { label: 'Desfavorável',   color: '#E67E22', score: Math.round((50 - wr) * 2) }
  return         { label: 'Muito difícil',         color: '#C0392B', score: Math.min(10, Math.round((50 - wr) * 2)) }
})

const mainTip = computed<string>(() => {
  if (winRateA.value === null) return ''
  const wr = winRateA.value
  if (wr >= 55) return 'Aplique pressão consistente — você tem vantagem neste matchup'
  if (wr >= 50) return 'Matchup ligeiramente favorável — construa vantagem aos poucos'
  if (wr >= 45) return 'Matchup equilibrado — depende de execução e decisão'
  if (wr >= 40) return 'Matchup desfavorável — jogue seguro e foque em farm'
  return              'Matchup muito difícil — evite trades diretas e solicite jungle'
})

// Barra de nível: 18 slots, destaca spikes
const levelSlots = computed(() =>
  Array.from({ length: 18 }, (_, i) => {
    const lvl = i + 1
    return {
      lvl,
      filled:  lvl <= enemyLevel.value,
      spike:   isSpikeLevel(lvl),
      current: lvl === enemyLevel.value,
    }
  })
)

function spellIcon(spellName: string): string {
  const map: Record<string, string> = {
    Flash:     '🔵', Ignite:   '🔥', Teleport: '🟣',
    Heal:      '💚', Exhaust:  '🟤', Barrier:  '🛡️',
    Ghost:     '👻', Cleanse:  '✨', Smite:    '🍃',
    Snowball:  '❄️',
  }
  return map[spellName] ?? '⚡'
}
</script>

<template>
  <!-- Overlay visível apenas quando o oponente for identificado -->
  <div class="min-h-screen bg-transparent p-3 pointer-events-none select-none">
    <Transition name="slide-in">
      <div
        v-if="enemyName"
        class="fixed right-3 top-3 w-60"
        :class="{ 'spike-pulse': spikeFlash }"
      >
        <div
          class="bg-rs-black/92 backdrop-blur-md rounded-xl overflow-hidden"
          style="border: 1px solid rgba(200,155,60,0.35); box-shadow: 0 4px 24px rgba(0,0,0,0.6)"
        >

          <!-- Cabeçalho: ícone + nome + spells -->
          <div class="flex items-center gap-2 px-3 pt-3 pb-2">
            <div class="relative shrink-0">
              <img
                v-if="iconUrl"
                :src="iconUrl"
                :alt="enemyName"
                class="w-10 h-10 rounded-lg object-cover"
                style="border: 1px solid rgba(200,155,60,0.4)"
                @error="($event.target as HTMLImageElement).style.display = 'none'"
              />
              <div
                v-else
                class="w-10 h-10 rounded-lg bg-rs-blue flex items-center justify-center text-rs-gold text-xs font-bold"
              >
                {{ enemyName.slice(0, 2).toUpperCase() }}
              </div>
            </div>

            <div class="flex-1 min-w-0">
              <p class="text-rs-white text-sm font-semibold truncate leading-tight">
                {{ enemyName }}
              </p>
              <p class="text-rs-white/40 text-[10px] uppercase tracking-widest">
                Oponente de lane
              </p>
            </div>

            <!-- Summoner spells -->
            <div class="flex flex-col gap-0.5 text-base leading-none">
              <span :title="enemySpell1">{{ spellIcon(enemySpell1) }}</span>
              <span :title="enemySpell2">{{ spellIcon(enemySpell2) }}</span>
            </div>
          </div>

          <!-- Separador -->
          <div class="h-px mx-3 bg-[rgba(200,155,60,0.15)]" />

          <!-- Barra de nível com spikes -->
          <div class="px-3 pt-2 pb-1">
            <div class="flex items-center justify-between mb-1">
              <span class="text-rs-white/50 text-[10px] uppercase tracking-widest">Nível</span>
              <span
                class="text-sm font-bold transition-colors duration-300"
                :class="isSpikeLevel(enemyLevel) ? 'text-amber-400' : 'text-rs-white'"
              >
                {{ enemyLevel > 0 ? enemyLevel : '—' }}
                <span v-if="isSpikeLevel(enemyLevel)" class="text-[10px] ml-0.5">⚡</span>
              </span>
            </div>

            <div class="flex gap-0.5">
              <div
                v-for="slot in levelSlots"
                :key="slot.lvl"
                class="flex-1 rounded-sm transition-all duration-300"
                :class="[
                  slot.filled
                    ? slot.spike
                      ? 'bg-amber-400 h-2'
                      : 'bg-rs-gold/70 h-1.5'
                    : 'bg-rs-blue h-1.5',
                  slot.current && 'ring-1 ring-white/40',
                ]"
                :title="`Nível ${slot.lvl}${slot.spike ? ' ⚡ Power spike' : ''}`"
              />
            </div>

            <!-- Legenda de spikes -->
            <div class="flex justify-between mt-1">
              <span
                v-for="spike in SPIKE_LEVELS"
                :key="spike"
                class="text-[9px]"
                :class="enemyLevel >= spike ? 'text-amber-400/80' : 'text-rs-white/25'"
              >
                {{ spike }}
              </span>
            </div>
          </div>

          <!-- Dificuldade + win-rate -->
          <div v-if="statsLoaded && difficulty" class="px-3 pb-2">
            <div class="h-px mb-2 bg-[rgba(200,155,60,0.15)]" />
            <div class="flex items-center justify-between mb-1.5">
              <span
                class="text-xs font-semibold"
                :style="{ color: difficulty.color }"
              >
                {{ difficulty.label }}
              </span>
              <span class="text-[10px] text-rs-white/40">
                WR {{ winRateA!.toFixed(1) }}%
                <span class="text-rs-white/25">({{ gamesPlayed.toLocaleString() }} partidas)</span>
              </span>
            </div>

            <!-- Barra de win-rate -->
            <div class="relative h-1.5 rounded-full bg-rs-blue overflow-hidden">
              <div
                class="absolute inset-y-0 left-0 rounded-full transition-all duration-700"
                :style="{
                  width: `${Math.min(100, winRateA!)}%`,
                  background: difficulty.color,
                }"
              />
              <!-- Linha de 50% -->
              <div class="absolute inset-y-0 left-1/2 w-px bg-white/20" />
            </div>
          </div>

          <!-- Dica de matchup -->
          <div v-if="mainTip" class="px-3 pb-3">
            <div class="h-px mb-2 bg-[rgba(200,155,60,0.15)]" />
            <p class="text-rs-white/75 text-[11px] leading-relaxed">
              {{ mainTip }}
            </p>
          </div>

          <!-- Carregando dados -->
          <div v-if="!statsLoaded && !statsError && enemyId > 0" class="px-3 pb-3">
            <div class="h-px mb-2 bg-[rgba(200,155,60,0.15)]" />
            <p class="text-rs-white/30 text-[10px]">Carregando dados de matchup…</p>
          </div>

        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* Entrada do overlay: desliza da direita */
.slide-in-enter-active { transition: all 0.35s cubic-bezier(0.25, 0.46, 0.45, 0.94); }
.slide-in-leave-active { transition: all 0.25s ease-in; }
.slide-in-enter-from   { opacity: 0; transform: translateX(20px); }
.slide-in-leave-to     { opacity: 0; transform: translateX(12px); }

/* Pulso dourado ao spike do inimigo */
@keyframes spike-pulse {
  0%   { box-shadow: 0 0 0 0 rgba(200, 155, 60, 0.6); }
  50%  { box-shadow: 0 0 0 8px rgba(200, 155, 60, 0); }
  100% { box-shadow: 0 0 0 0 rgba(200, 155, 60, 0); }
}
.spike-pulse > div { animation: spike-pulse 1.2s ease-out; }
</style>
