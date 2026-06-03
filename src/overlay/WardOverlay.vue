<script setup lang="ts">
// ============================================================
// WardOverlay — Smart Vision System
//
// Painel que aparece no centro-inferior da tela com slide-up,
// mostrando o minimapa do SR (DDragon) com dots coloridos nos
// pontos exatos onde wardar.
//
// Combina a atenção do slide-in com a clareza espacial do mapa:
// o jogador vê ONDE no mapa cada ward deve ser colocado.
//
// Posições: xPct = game_x / 14450 * 100
//           yPct = (1 - game_y / 14450) * 100  (y invertido)
//
// Blue/Red side: posições espelhadas para wards dependentes de lado.
// ============================================================
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn }               from '@tauri-apps/api/event'
import minimapSrc from '../assets/minimap_sr.png'

// Asset bundlado pelo Vite — sem dependência de rede, sempre disponível
const MINIMAP_URL = minimapSrc
const imgFailed   = ref(false)
function onImgError() { imgFailed.value = true }

interface WardSpot {
  xPct:    number
  yPct:    number
  label:   string
  sublabel:string
  color:   string
}

/** Spot dinâmico vindo da SpellCoach API (coordenadas reais de alto elo). */
interface SmartWardSpot {
  xPct:     number
  yPct:     number
  label:    string
  sublabel: string
  color:    string
  wardType: string
}

// ── Spots fixos (mesma posição para ambos os lados) ───────────
const SPOTS_FIXED: Record<string, WardSpot> = {
  dragon_pit:      { xPct: 70.9, yPct: 71.6, label: 'Drake',     sublabel: 'Pit do Dragão',     color: '#E55A3A' },
  baron_pit:       { xPct: 34.6, yPct: 27.3, label: 'Baron',     sublabel: 'Pit do Baron',       color: '#A78BFA' },
  river_bot_brush: { xPct: 62.3, yPct: 80.6, label: 'Rio Bot',   sublabel: 'Arbusto rio bot',    color: '#06B6D4' },
  tribush_bot:     { xPct: 73.4, yPct: 86.8, label: 'Tri Bot',   sublabel: 'Tri-bush bot',       color: '#06B6D4' },
  pixel_ward:      { xPct: 67.0, yPct: 75.8, label: 'Pixel',     sublabel: 'Bush do pixel',      color: '#22D3EE' },
  mid_river_drake: { xPct: 55.8, yPct: 74.5, label: 'Mid Bot',   sublabel: 'Rio mid lado drake', color: '#22D3EE' },
  baron_river:     { xPct: 31.8, yPct: 18.3, label: 'Rio Top',   sublabel: 'Rio lado baron',     color: '#06B6D4' },
  mid_river_baron: { xPct: 44.2, yPct: 25.5, label: 'Mid Top',   sublabel: 'Rio mid lado baron', color: '#22D3EE' },
}

// ── Spots dependentes do lado ─────────────────────────────────
//
// ATENÇÃO — simetria rotacional do mapa SR (180°):
//   Camp aliado em (x, y) → camp inimigo em (14450-x, 14450-y)
//   yPct = (1 - game_y/14450) * 100
//
// Red Buff (Brambleback) aliado — BLUE side:
//   game (3550, 3700) → xPct=24.6%, yPct=74.4%  (sul-esquerda, bot jungle)
// Red Buff inimigo — BLUE side:
//   game (10900, 10750) → xPct=75.4%, yPct=25.6% (norte-direita, bot jungle DELES)
//
// Blue Buff aliado — BLUE side:
//   game (3750, 7600) → xPct=26.0%, yPct=47.4%  (centro-esquerda)
// Blue Buff inimigo — BLUE side:
//   game (10700, 6850) → xPct=74.0%, yPct=52.6% (centro-direita)
const SPOTS_BLUE: Record<string, WardSpot> = {
  // Deep wards (ward avançado para visão do jungle inimigo)
  deep_bot_enemy:  { xPct: 78.5, yPct: 73.2, label: 'Deep Bot',  sublabel: 'Jungle inimigo sul',    color: '#FB923C' },
  deep_top_enemy:  { xPct: 68.0, yPct: 21.5, label: 'Deep Top',  sublabel: 'Jungle inimigo norte',  color: '#FB923C' },
  // Tribush e rio da top lane (lado baron, blue perspective = noroeste)
  tribush_top:     { xPct: 26.6, yPct: 13.2, label: 'Tri Top',   sublabel: 'Tri-bush top lane',     color: '#06B6D4' },
  top_river:       { xPct: 17.8, yPct: 30.4, label: 'Rio Top',   sublabel: 'Entrada rio top',       color: '#06B6D4' },
  // Buffs inimigos — lado NORTE do mapa (Red Buff deles = bot jungle deles = norte)
  enemy_red_buff:  { xPct: 75.4, yPct: 25.6, label: 'Red Ini.',  sublabel: 'Red Buff inimigo (N)',  color: '#EF4444' },
  // Blue Buff inimigo — lado CENTRO-DIREITA do mapa
  enemy_blue_buff: { xPct: 74.0, yPct: 52.6, label: 'Blue Ini.', sublabel: 'Blue Buff inimigo',     color: '#3B82F6' },
}
const SPOTS_RED: Record<string, WardSpot> = {
  // Deep wards espelhados
  deep_bot_enemy:  { xPct: 21.5, yPct: 73.2, label: 'Deep Bot',  sublabel: 'Jungle inimigo sul',    color: '#FB923C' },
  deep_top_enemy:  { xPct: 32.0, yPct: 21.5, label: 'Deep Top',  sublabel: 'Jungle inimigo norte',  color: '#FB923C' },
  // Tribush e rio top (red perspective = sudeste)
  tribush_top:     { xPct: 73.4, yPct: 86.8, label: 'Tri Top',   sublabel: 'Tri-bush top lane',     color: '#06B6D4' },
  top_river:       { xPct: 82.2, yPct: 69.6, label: 'Rio Top',   sublabel: 'Entrada rio top',       color: '#06B6D4' },
  // Buffs inimigos (blue team) — Red Buff deles = bot jungle deles = SUL do mapa
  enemy_red_buff:  { xPct: 24.6, yPct: 74.4, label: 'Red Ini.',  sublabel: 'Red Buff inimigo (S)',  color: '#EF4444' },
  enemy_blue_buff: { xPct: 26.0, yPct: 47.4, label: 'Blue Ini.', sublabel: 'Blue Buff inimigo',     color: '#3B82F6' },
}

// ── Spots dinâmicos (SpellCoach) ──────────────────────────────
const smartSpots      = ref<SmartWardSpot[]>([])
let   smartDismissTimer: ReturnType<typeof setTimeout> | null = null

function showSmartSpots(spots: SmartWardSpot[]) {
  smartSpots.value = spots.slice(0, 5)
  clearTimeout(smartDismissTimer ?? undefined)
  smartDismissTimer = setTimeout(() => { smartSpots.value = [] }, DISPLAY_MS)
}

const isBlueSide = ref(true)
const SPOTS = computed<Record<string, WardSpot>>(() => ({
  ...SPOTS_FIXED,
  ...(isBlueSide.value ? SPOTS_BLUE : SPOTS_RED),
}))

const DISPLAY_MS  = 12_000
const COOLDOWN_MS = 60_000

const activeSpots  = ref<string[]>([])
const isVisible    = computed(() => activeSpots.value.length > 0 || smartSpots.value.length > 0)
const displaySpots = computed(() =>
  activeSpots.value.filter(id => SPOTS.value[id]).slice(0, 5)
)

let dismissTimer: ReturnType<typeof setTimeout> | null = null
let lastShownAt  = 0
let lastSpotKey  = ''

let unlistenSpots:       UnlistenFn | null = null
let unlistenSpotsForced: UnlistenFn | null = null
let unlistenSmartSpots:  UnlistenFn | null = null
let unlistenTeam:        UnlistenFn | null = null

function showSpots(spots: string[]) {
  const valid = spots.filter(id => SPOTS.value[id])
  if (valid.length === 0) { hideSpots(); return }

  const key = [...valid].sort().join(',')
  const now  = Date.now()
  if (key === lastSpotKey && now - lastShownAt < COOLDOWN_MS) return

  _applySpots(valid)
}

function showSpotsForced(spots: string[]) {
  const valid = spots.filter(id => SPOTS.value[id])
  if (valid.length === 0) return
  _applySpots(valid)
}

function _applySpots(valid: string[]) {
  activeSpots.value = valid
  lastShownAt       = Date.now()
  lastSpotKey       = [...valid].sort().join(',')
  clearTimeout(dismissTimer ?? undefined)
  dismissTimer = setTimeout(hideSpots, DISPLAY_MS)
}

function hideSpots() {
  activeSpots.value = []
  if (dismissTimer) { clearTimeout(dismissTimer); dismissTimer = null }
}

onMounted(async () => {
  unlistenSpots       = await listen<string[]>('ward_spots',        e => showSpots(e.payload))
  unlistenSpotsForced = await listen<string[]>('ward_spots_forced', e => showSpotsForced(e.payload))
  unlistenSmartSpots  = await listen<SmartWardSpot[]>('ward_spots_smart', e => showSmartSpots(e.payload))
  unlistenTeam        = await listen<boolean>( 'team_side',         e => { isBlueSide.value = e.payload })
})

onUnmounted(() => {
  unlistenSpots?.()
  unlistenSpotsForced?.()
  unlistenSmartSpots?.()
  unlistenTeam?.()
  clearTimeout(dismissTimer ?? undefined)
  clearTimeout(smartDismissTimer ?? undefined)
})
</script>

<template>
  <!--
    Painel de wards — bottom-center, desliza de baixo para cima.
    Mostra o minimapa real do SR (DDragon) com dots nos pontos exatos.
    Considera blue/red side para posicionamento correto dos wards de lado.
  -->
  <Transition name="ward-slide">
    <div v-if="isVisible" class="wb-root">

      <!-- ── Header ──────────────────────────────────────── -->
      <div class="wb-header">
        <span class="wb-header-icon">👁</span>
        <span class="wb-header-text">Wards Recomendados</span>
        <span class="wb-side-badge" :class="isBlueSide ? 'side-blue' : 'side-red'">
          {{ isBlueSide ? 'BLUE' : 'RED' }}
        </span>
      </div>

      <!-- ── Body: mapa + legenda ──────────────────────── -->
      <div class="wb-body">

        <!-- Minimapa SR com dots posicionados ────────────── -->
        <div class="wb-map-wrap">
          <!-- Imagem do mapa (asset bundlado pelo Vite) -->
          <img
            v-if="!imgFailed"
            :src="MINIMAP_URL"
            class="wb-map-img"
            alt=""
            draggable="false"
            @error="onImgError"
          />
          <div v-else class="wb-map-fallback" />

          <!-- Dots estáticos (proc engine) -->
          <div
            v-for="id in displaySpots"
            :key="'static-' + id"
            class="wb-dot"
            :style="{
              left: SPOTS[id].xPct + '%',
              top:  SPOTS[id].yPct + '%',
              '--c': SPOTS[id].color,
            }"
          />

          <!-- Dots dinâmicos (SpellCoach API — alto elo) -->
          <div
            v-for="(spot, i) in smartSpots"
            :key="'smart-' + i"
            class="wb-dot wb-dot-smart"
            :title="`${spot.label} · ${spot.sublabel}`"
            :style="{
              left: spot.xPct + '%',
              top:  spot.yPct + '%',
              '--c': spot.color,
            }"
          />
        </div>

        <!-- Legenda lateral ──────────────────────────────── -->
        <div class="wb-legend">
          <!-- Spots estáticos (proc engine) -->
          <div
            v-for="id in displaySpots"
            :key="'leg-' + id"
            class="wb-legend-row"
            :style="{ '--c': SPOTS[id].color }"
          >
            <span class="wb-legend-dot" />
            <div class="wb-legend-text">
              <span class="wb-legend-label">{{ SPOTS[id].label }}</span>
              <span class="wb-legend-sub">{{ SPOTS[id].sublabel }}</span>
            </div>
          </div>

          <!-- Divider se tiver ambos -->
          <div v-if="displaySpots.length > 0 && smartSpots.length > 0" class="wb-legend-divider" />

          <!-- Spots dinâmicos (SpellCoach) -->
          <div
            v-for="(spot, i) in smartSpots"
            :key="'leg-smart-' + i"
            class="wb-legend-row"
            :style="{ '--c': spot.color }"
          >
            <span class="wb-legend-dot wb-dot-smart-dot" />
            <div class="wb-legend-text">
              <span class="wb-legend-label">
                {{ spot.label }}
                <span class="wb-sc-badge">SC</span>
              </span>
              <span class="wb-legend-sub">{{ spot.sublabel }}</span>
            </div>
          </div>
        </div>

      </div>

      <!-- ── Barra de progresso ──────────────────────────── -->
      <div class="wb-progress">
        <div class="wb-progress-fill" :style="{ '--dur': DISPLAY_MS + 'ms' }" />
      </div>

    </div>
  </Transition>
</template>

<style scoped>
/* ── Painel — preenche a janela dedicada ward_win ─────────── */
.wb-root {
  position:       relative;
  pointer-events: none;
  user-select:    none;

  /* Visual hextech */
  background:   rgba(5, 12, 28, 0.96);
  border:       1px solid rgba(6, 182, 212, 0.30);
  border-radius: 4px;
  /* Corte angular hextech no canto superior-direito */
  clip-path: polygon(
    0 0,
    calc(100% - 14px) 0,
    100% 14px,
    100% 100%,
    0 100%
  );
  display:      flex;
  flex-direction: column;
  min-width:    340px;
}

/* ── Header ─────────────────────────────────────────────── */
.wb-header {
  display:     flex;
  align-items: center;
  gap:         7px;
  padding:     8px 12px 6px;
  border-bottom: 1px solid rgba(6, 182, 212, 0.15);
  flex-shrink: 0;
}

.wb-header-icon {
  font-size: 13px;
  opacity:   0.85;
}

.wb-header-text {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      9.5px;
  font-weight:    700;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  color:          rgba(6, 182, 212, 0.90);
  flex:           1;
}

.wb-side-badge {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      7.5px;
  font-weight:    700;
  letter-spacing: 0.15em;
  padding:        1px 6px;
  border-radius:  2px;
  flex-shrink:    0;
}
.side-blue { color: rgba(100, 160, 255, 0.9); background: rgba(100, 160, 255, 0.12); border: 1px solid rgba(100, 160, 255, 0.30); }
.side-red  { color: rgba(255, 100, 100, 0.9); background: rgba(255, 100, 100, 0.12); border: 1px solid rgba(255, 100, 100, 0.30); }

/* ── Body: mapa + legenda ────────────────────────────────── */
.wb-body {
  display:     flex;
  align-items: stretch;
  gap:         0;
  flex:        1;
}

/* ── Minimap ─────────────────────────────────────────────── */
.wb-map-wrap {
  position:    relative;
  width:       160px;
  height:      160px;
  flex-shrink: 0;
  border-right: 1px solid rgba(6, 182, 212, 0.12);
}

.wb-map-img {
  position: absolute;
  inset:    0;
  width:    100%;
  height:   100%;
  object-fit: fill;
  display:  block;
  border-radius: 0 0 0 3px;
  /* Escurece levemente para os dots ficarem mais visíveis */
  filter:   brightness(0.75) contrast(1.1);
}

.wb-map-fallback {
  position:   absolute;
  inset:      0;
  background: rgba(8, 15, 32, 0.95);
  border-radius: 0 0 0 3px;
}

/* ── Dot sobre o mapa ────────────────────────────────────── */
.wb-dot {
  position:      absolute;
  width:         10px;
  height:        10px;
  border-radius: 50%;
  background:    var(--c);
  opacity:       0.95;
  transform:     translate(-50%, -50%);
  /* SEM box-shadow — evita artefatos WebView2 em janela transparente */
  animation:     dot-pulse 1.8s ease-in-out infinite;
  /* outline em vez de box-shadow para o glow */
  outline:       2px solid transparent;
  outline-offset: 2px;
}

/* Anel via ::after para não usar box-shadow */
.wb-dot::after {
  content:       '';
  position:      absolute;
  inset:         -2px;
  border-radius: 50%;
  border:        1px solid var(--c);
  opacity:       0;
  animation:     dot-ring 1.8s ease-out infinite;
}

@keyframes dot-pulse {
  0%, 100% { transform: translate(-50%, -50%) scale(1.0); opacity: 0.95; }
  50%       { transform: translate(-50%, -50%) scale(1.2); opacity: 1.0; }
}

@keyframes dot-ring {
  0%   { transform: scale(1.0); opacity: 0.5; }
  100% { transform: scale(1.8); opacity: 0;   }
}

/* ── Legenda ─────────────────────────────────────────────── */
.wb-legend {
  flex:            1;
  display:         flex;
  flex-direction:  column;
  justify-content: center;
  gap:             2px;
  padding:         8px 12px;
  min-width:       170px;
}

.wb-legend-row {
  display:     flex;
  align-items: center;
  gap:         7px;
}

.wb-legend-dot {
  width:         7px;
  height:        7px;
  border-radius: 50%;
  background:    var(--c);
  flex-shrink:   0;
  opacity:       0.9;
}

.wb-legend-text {
  display:        flex;
  flex-direction: column;
  gap:            0;
  min-width:      0;
}

.wb-legend-label {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      10px;
  font-weight:    700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color:          var(--c);
  line-height:    1.2;
  white-space:    nowrap;
}

.wb-legend-sub {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      8px;
  font-weight:    400;
  color:          rgba(200, 215, 230, 0.40);
  line-height:    1.2;
  white-space:    nowrap;
}

/* ── Barra de progresso ──────────────────────────────────── */
.wb-progress {
  height:     2px;
  background: rgba(255, 255, 255, 0.07);
  flex-shrink: 0;
}

.wb-progress-fill {
  height:     100%;
  width:      100%;
  background: rgba(6, 182, 212, 0.65);
  animation:  bar-shrink var(--dur) linear forwards;
}

@keyframes bar-shrink {
  from { width: 100%; }
  to   { width:   0%; }
}

/* ── Dot SpellCoach: diamante em vez de círculo ─────────── */
.wb-dot-smart {
  border-radius: 2px;
  transform:     translate(-50%, -50%) rotate(45deg);
  width:         9px;
  height:        9px;
}
.wb-dot-smart::after {
  border-radius: 2px;
}

/* ── Badge SC na legenda ─────────────────────────────────── */
.wb-sc-badge {
  display:        inline-block;
  font-size:      6px;
  font-weight:    700;
  letter-spacing: 0.1em;
  padding:        0 3px;
  margin-left:    4px;
  border-radius:  2px;
  background:     rgba(6, 182, 212, 0.18);
  color:          rgba(6, 182, 212, 0.85);
  border:         1px solid rgba(6, 182, 212, 0.30);
  vertical-align: middle;
  line-height:    1.6;
}

/* ── Dot da legenda para smart spots ─────────────────────── */
.wb-dot-smart-dot {
  border-radius: 1px;
  transform:     rotate(45deg);
  width:         6px;
  height:        6px;
}

/* ── Divider entre spots estáticos e SpellCoach ──────────── */
.wb-legend-divider {
  height:     1px;
  background: rgba(6, 182, 212, 0.12);
  margin:     3px 0;
}

/* ── Transição: slide de baixo para cima ─────────────────── */
.ward-slide-enter-active {
  transition: transform 0.30s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.22s ease;
}
.ward-slide-leave-active {
  transition: transform 0.20s cubic-bezier(0.55, 0, 1, 0.45), opacity 0.18s ease;
}
.ward-slide-enter-from {
  transform: translateX(-50%) translateY(28px);
  opacity:   0;
}
.ward-slide-leave-to {
  transform: translateX(-50%) translateY(28px);
  opacity:   0;
}


</style>
