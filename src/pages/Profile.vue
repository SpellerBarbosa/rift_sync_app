<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useRouter } from 'vue-router'
import { usePlayerStore } from '../stores/player'

const router      = useRouter()
const playerStore = usePlayerStore()

// ── Tipos ─────────────────────────────────────────────────────
interface PlayerPattern {
  aggressionScore:      number
  deathsWithoutVision:  number
  avgDeaths1015:        number
  laneDominance:        number
  objectiveControl:     number
  roamFrequency:        number
  tpEfficiency:         number
  wardScore:            number
}

interface MatchStats {
  totalGames:   number
  wins:         number
  losses:       number
  winrate:      number
  avgKda:       number
  avgCs:        number
  avgVision:    number
  topChampions: ChampionStat[]
}

interface ChampionStat {
  id:      number
  name:    string
  key:     string
  games:   number
  wins:    number
  winrate: number
  avgKda:  number
}

interface PlayerInsights {
  playstyle:    string
  strengths:    string[]
  weaknesses:   string[]
  priorityTip:  string
  progressNote: string
}

// ── Estado ────────────────────────────────────────────────────
const loading      = ref(true)
const aiLoading    = ref(false)
const pattern      = ref<PlayerPattern | null>(null)
const stats        = ref<MatchStats | null>(null)
const insights     = ref<PlayerInsights | null>(null)
const aiError      = ref<string | null>(null)
const ddVersion    = ref('14.24.1')

// ── Helpers ───────────────────────────────────────────────────
const pct = (v: number) => `${Math.round(v * 100)}%`

// Converte score 0-1 em nível textual
const scoreLevel = (v: number) => {
  if (v >= 0.75) return { label: 'Alto',  color: '#4ADE80' }
  if (v >= 0.45) return { label: 'Médio', color: '#F59E0B' }
  return               { label: 'Baixo', color: '#EF4444' }
}

const playstyleLabel = (ps: string) => ({
  aggressive: 'Agressivo',
  passive:    'Passivo',
  balanced:   'Equilibrado',
}[ps] ?? ps)

const champIconUrl = (key: string) =>
  `https://ddragon.leagueoflegends.com/cdn/${ddVersion.value}/img/champion/${key}.png`

const MIN_GAMES_FOR_PATTERNS = 5
const hasEnoughData = computed(() => (stats.value?.totalGames ?? 0) >= MIN_GAMES_FOR_PATTERNS)

// ── Métricas de perfil ─────────────────────────────────────────
const metrics = computed(() => {
  if (!pattern.value) return []
  const p = pattern.value
  return [
    { label: 'Ward Score',        value: p.wardScore,         desc: 'Frequência de wards colocados' },
    { label: 'Controle de Obj.',  value: p.objectiveControl,  desc: 'Participação em drakes e baron' },
    { label: 'Domínio de Lane',   value: p.laneDominance,     desc: 'Vantagem de CS e poke na lane' },
    { label: 'Frequência de Roam',value: p.roamFrequency,     desc: 'Rotações e assistências em outras lanes' },
    { label: 'Agressividade',     value: p.aggressionScore,   desc: 'Tendência a iniciar trades e fights' },
    { label: 'Eficiência de TP',  value: p.tpEfficiency,      desc: 'Impacto dos Portais de Teletransporte' },
  ]
})

// ── Actions ───────────────────────────────────────────────────
async function loadData() {
  loading.value = true
  try {
    const [pat, st, ver] = await Promise.allSettled([
      invoke<PlayerPattern | null>('get_player_patterns'),
      invoke<MatchStats>('get_match_stats'),
      invoke<string>('get_setting', { key: 'dd_version' }),
    ])
    if (pat.status === 'fulfilled')  pattern.value  = pat.value
    if (st.status  === 'fulfilled')  stats.value    = st.value
    if (ver.status === 'fulfilled')  ddVersion.value = ver.value
    await playerStore.fetchLastPlayer()
  } finally {
    loading.value = false
  }
}

async function runProfileAnalysis() {
  if (aiLoading.value || !pattern.value) return
  aiLoading.value = true
  aiError.value   = null
  insights.value  = null
  try {
    // Constrói resumo de partidas para o Groq
    const recentMatches = await invoke<any[]>('get_cached_matches').catch(() => [])
    const summary = recentMatches.slice(0, 5).map((m: any) =>
      `${m.championName} ${m.win ? 'V' : 'D'} ${m.kills}/${m.deaths}/${m.assists}`
    ).join(', ')

    insights.value = await invoke<PlayerInsights>('analyze_player_profile_command', {
      summary,
    })
  } catch (e: any) {
    aiError.value = typeof e === 'string' ? e : (e?.message ?? 'Erro na análise de perfil')
  } finally {
    aiLoading.value = false
  }
}

onMounted(loadData)
</script>

<template>
  <div class="pf-root">
    <div class="pf-glow" />

    <!-- Header -->
    <header class="pf-header">
      <button class="back-btn" @click="router.push('/')">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path d="M9 2L4 7L9 12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        Dashboard
      </button>
      <div class="pf-title">
        <span class="pf-diamond">◆</span>
        Perfil do Jogador
      </div>
      <div class="w-20" />
    </header>

    <div class="pf-scroll">
      <div class="pf-content">

        <!-- Loading -->
        <div v-if="loading" class="pf-center">
          <span class="spin">⟳</span>
        </div>

        <template v-else>

          <!-- ── Card do jogador ─────────────────────────────── -->
          <div class="player-card" v-if="playerStore.currentPlayer">
            <div class="player-inner">
              <div class="avatar-wrap">
                <img
                  :src="playerStore.profileIconUrl(ddVersion)"
                  class="avatar-img"
                  alt="icon"
                  @error="($event.target as HTMLImageElement).src=''"
                />
              </div>
              <div class="player-info">
                <p class="player-name">{{ playerStore.displayName }}</p>
                <p class="player-rank">{{ playerStore.rankDisplay }}</p>
              </div>
              <div v-if="stats" class="player-winrate">
                <span class="wr-val">{{ stats.winrate.toFixed(1) }}%</span>
                <span class="wr-lbl">Win Rate</span>
              </div>
            </div>
          </div>

          <!-- ── Stats gerais ───────────────────────────────── -->
          <div v-if="stats && stats.totalGames > 0" class="section">
            <h2 class="section-title">Histórico</h2>
            <div class="stats-row">
              <div class="stat-block">
                <span class="stat-num">{{ stats.totalGames }}</span>
                <span class="stat-nm">Partidas</span>
              </div>
              <div class="stat-block">
                <span class="stat-num">{{ stats.avgKda.toFixed(2) }}</span>
                <span class="stat-nm">KDA Médio</span>
              </div>
              <div class="stat-block">
                <span class="stat-num">{{ stats.avgCs.toFixed(0) }}</span>
                <span class="stat-nm">CS Médio</span>
              </div>
              <div class="stat-block">
                <span class="stat-num">{{ stats.avgVision.toFixed(0) }}</span>
                <span class="stat-nm">Visão Média</span>
              </div>
            </div>

            <!-- Top campeões -->
            <div v-if="stats.topChampions.length > 0" class="champs-row">
              <div
                v-for="champ in stats.topChampions"
                :key="champ.id"
                class="champ-chip"
              >
                <img
                  :src="champIconUrl(champ.key)"
                  :alt="champ.name"
                  class="champ-chip-icon"
                  @error="($event.target as HTMLImageElement).style.display='none'"
                />
                <div class="champ-chip-info">
                  <span class="champ-chip-name">{{ champ.name }}</span>
                  <span class="champ-chip-wr">{{ champ.winrate.toFixed(0) }}% ({{ champ.games }}g)</span>
                </div>
              </div>
            </div>
          </div>

          <!-- ── Sem dados suficientes ───────────────────────── -->
          <div v-else-if="!stats || stats.totalGames === 0" class="no-data-card">
            <p class="no-data-title">Histórico insuficiente</p>
            <p class="no-data-desc">
              Sincronize o histórico de partidas no Dashboard para ver suas estatísticas e perfil comportamental.
            </p>
          </div>

          <!-- ── Métricas comportamentais ───────────────────── -->
          <div v-if="pattern" class="section">
            <h2 class="section-title">Perfil Comportamental</h2>
            <div class="metrics-list">
              <div
                v-for="m in metrics"
                :key="m.label"
                class="metric-row"
                :title="m.desc"
              >
                <div class="metric-label-row">
                  <span class="metric-label">{{ m.label }}</span>
                  <div class="metric-right">
                    <span class="metric-level" :style="{ color: scoreLevel(m.value).color }">
                      {{ scoreLevel(m.value).label }}
                    </span>
                    <span class="metric-pct">{{ pct(m.value) }}</span>
                  </div>
                </div>
                <div class="metric-bar-bg">
                  <div
                    class="metric-bar-fill"
                    :style="{
                      width: pct(m.value),
                      background: scoreLevel(m.value).color,
                    }"
                  />
                </div>
              </div>
            </div>

            <!-- Mortes sem visão -->
            <div class="deaths-vision-row">
              <div class="dv-label">
                <span class="dv-title">Mortes sem Visão</span>
                <span class="dv-desc">Total histórico de mortes em zonas sem ward</span>
              </div>
              <span
                class="dv-val"
                :style="{ color: pattern.deathsWithoutVision > 10 ? '#EF4444' : pattern.deathsWithoutVision > 5 ? '#F59E0B' : '#4ADE80' }"
              >
                {{ pattern.deathsWithoutVision }}
              </span>
            </div>
          </div>

          <!-- ── Mensagem: dados insuficientes ─────────────── -->
          <div v-else class="patterns-pending-card">
            <div class="patterns-pending-icon">
              <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
                <circle cx="14" cy="14" r="12" stroke="rgba(200,155,60,0.3)" stroke-width="1.5"/>
                <path d="M14 8v7l4 2" stroke="rgba(200,155,60,0.6)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </div>
            <div class="patterns-pending-body">
              <p class="patterns-pending-title">Jogue mais partidas para que possamos analisar seus padrões</p>
              <p class="patterns-pending-desc">
                O RiftSync precisa de pelo menos {{ MIN_GAMES_FOR_PATTERNS }} partidas registradas para calcular seu perfil comportamental.
                <span v-if="stats && stats.totalGames > 0">
                  Você tem {{ stats.totalGames }} de {{ MIN_GAMES_FOR_PATTERNS }} partidas registradas.
                </span>
              </p>
              <div class="patterns-progress-bar" v-if="stats">
                <div
                  class="patterns-progress-fill"
                  :style="{ width: `${Math.min((stats.totalGames / MIN_GAMES_FOR_PATTERNS) * 100, 100)}%` }"
                />
              </div>
            </div>
          </div>

          <!-- ── Análise IA ──────────────────────────────────── -->
          <div v-if="pattern" class="section">
            <div class="section-header-row">
              <h2 class="section-title">Análise de Perfil IA</h2>
              <span class="ai-badge">Groq</span>
            </div>
            <p class="section-desc">
              Análise personalizada do seu estilo de jogo com base nos padrões históricos.
            </p>

            <button
              class="ai-btn"
              :disabled="aiLoading"
              @click="runProfileAnalysis"
            >
              <span v-if="aiLoading" class="spin-sm">⟳</span>
              <template v-else>
                <svg width="13" height="13" viewBox="0 0 13 13" fill="none" style="flex-shrink:0">
                  <circle cx="6.5" cy="6.5" r="5.5" stroke="currentColor" stroke-width="1.2"/>
                  <path d="M4 6.5L5.8 8.3L9 5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                {{ insights ? 'Atualizar análise' : 'Analisar perfil' }}
              </template>
            </button>

            <p v-if="aiError" class="ai-error">{{ aiError }}</p>

            <Transition name="fade">
              <div v-if="insights" class="insights-card">
                <!-- Playstyle badge -->
                <div class="playstyle-row">
                  <span class="playstyle-label">Estilo:</span>
                  <span class="playstyle-val">{{ playstyleLabel(insights.playstyle) }}</span>
                </div>

                <!-- Strengths / Weaknesses -->
                <div class="insights-grid">
                  <div class="insights-col">
                    <p class="insights-col-title" style="color:#4ADE80">Pontos Fortes</p>
                    <ul class="insights-list">
                      <li v-for="(s, i) in insights.strengths" :key="i" class="insights-item">
                        <span class="list-dot" style="background:#4ADE80" />{{ s }}
                      </li>
                    </ul>
                  </div>
                  <div class="insights-col">
                    <p class="insights-col-title" style="color:#F59E0B">A Desenvolver</p>
                    <ul class="insights-list">
                      <li v-for="(w, i) in insights.weaknesses" :key="i" class="insights-item">
                        <span class="list-dot" style="background:#F59E0B" />{{ w }}
                      </li>
                    </ul>
                  </div>
                </div>

                <!-- Dica principal -->
                <div class="focus-block">
                  <span class="focus-label">Foco prioritário</span>
                  <p class="focus-text">{{ insights.priorityTip }}</p>
                </div>

                <!-- Nota de progresso -->
                <p class="progress-note">{{ insights.progressNote }}</p>
              </div>
            </Transition>
          </div>

        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pf-root {
  height: 100%;
  background: #010A13;
  display: flex; flex-direction: column;
  position: relative; overflow: hidden; user-select: none;
}
.pf-glow {
  position: absolute; inset: 0; pointer-events: none;
  background: radial-gradient(ellipse 70% 40% at 50% 0%, rgba(10,30,60,.5) 0%, transparent 60%);
}

/* ── Header ─────────────────────────────────────────────── */
.pf-header {
  position: relative; z-index: 1;
  display: flex; align-items: center; justify-content: space-between;
  padding: 14px 20px 12px;
  border-bottom: 1px solid rgba(200,155,60,.1);
  flex-shrink: 0;
}
.back-btn {
  display: flex; align-items: center; gap: 5px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .65rem; font-weight: 600; letter-spacing: .14em; text-transform: uppercase;
  color: rgba(200,155,60,.5); background: none; border: 1px solid rgba(200,155,60,.15);
  padding: 3px 10px; border-radius: 2px; cursor: pointer; transition: all .15s;
}
.back-btn:hover { color: rgba(200,155,60,.9); border-color: rgba(200,155,60,.45); }
.pf-title {
  display: flex; align-items: center; gap: 7px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .78rem; font-weight: 700; letter-spacing: .22em; text-transform: uppercase;
  color: rgba(200,155,60,.6);
}
.pf-diamond { font-size: 7px; color: rgba(200,155,60,.5); }

/* ── Scroll ──────────────────────────────────────────────── */
.pf-scroll { flex: 1; overflow-y: auto; position: relative; z-index: 1; }
.pf-content { max-width: 560px; margin: 0 auto; padding: 20px 24px 40px; display: flex; flex-direction: column; gap: 20px; }

/* ── Center ──────────────────────────────────────────────── */
.pf-center { display: flex; align-items: center; justify-content: center; padding: 80px; }
.spin { font-size: 1.4rem; color: rgba(200,155,60,.5); animation: spin .7s linear infinite; display: inline-block; }
.spin-sm { display: inline-block; animation: spin .7s linear infinite; font-size: .9rem; }

/* ── Player card ─────────────────────────────────────────── */
.player-card {
  border-radius: 6px; background: rgba(28,42,58,.5);
  border: 1px solid rgba(200,155,60,.15); overflow: hidden;
}
.player-inner { display: flex; align-items: center; gap: 14px; padding: 14px 16px; }
.avatar-wrap {
  width: 44px; height: 44px; border-radius: 50%; overflow: hidden;
  flex-shrink: 0; background: rgba(255,255,255,.05);
  border: 2px solid rgba(200,155,60,.3);
}
.avatar-img { width: 100%; height: 100%; object-fit: cover; }
.player-info { flex: 1; min-width: 0; }
.player-name {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .92rem; font-weight: 700; color: rgba(232,224,208,.9);
}
.player-rank { font-size: .62rem; color: rgba(200,155,60,.55); margin-top: 2px; }
.player-winrate { display: flex; flex-direction: column; align-items: flex-end; }
.wr-val {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: 1.1rem; font-weight: 700; color: rgba(200,155,60,.85);
}
.wr-lbl { font-size: .55rem; color: rgba(232,224,208,.3); letter-spacing: .08em; text-transform: uppercase; }

/* ── Section ─────────────────────────────────────────────── */
.section { display: flex; flex-direction: column; gap: 12px; }
.section-header-row { display: flex; align-items: center; justify-content: space-between; }
.section-title {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .62rem; font-weight: 700; letter-spacing: .22em; text-transform: uppercase;
  color: rgba(200,155,60,.45);
}
.section-desc { font-size: .65rem; color: rgba(232,224,208,.3); line-height: 1.5; }
.ai-badge {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .48rem; font-weight: 700; letter-spacing: .16em;
  padding: 2px 7px; border-radius: 2px;
  background: rgba(6,182,212,.1); color: rgba(6,182,212,.75);
  border: 1px solid rgba(6,182,212,.25);
}

/* ── Stats gerais ────────────────────────────────────────── */
.stats-row {
  display: grid; grid-template-columns: repeat(4,1fr); gap: 8px;
}
.stat-block {
  display: flex; flex-direction: column; align-items: center; gap: 3px;
  padding: 12px 8px; border-radius: 4px;
  background: rgba(28,42,58,.4); border: 1px solid rgba(255,255,255,.05);
}
.stat-num {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: 1rem; font-weight: 700; color: rgba(232,224,208,.85);
}
.stat-nm { font-size: .55rem; color: rgba(232,224,208,.3); letter-spacing: .08em; text-transform: uppercase; }

/* ── Champs row ──────────────────────────────────────────── */
.champs-row { display: flex; gap: 8px; flex-wrap: wrap; }
.champ-chip {
  display: flex; align-items: center; gap: 8px;
  padding: 8px 10px; border-radius: 4px;
  background: rgba(28,42,58,.4); border: 1px solid rgba(255,255,255,.05);
  flex: 1; min-width: 120px;
}
.champ-chip-icon { width: 28px; height: 28px; border-radius: 3px; flex-shrink: 0; object-fit: cover; }
.champ-chip-info { display: flex; flex-direction: column; min-width: 0; }
.champ-chip-name { font-size: .72rem; color: rgba(232,224,208,.75); font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.champ-chip-wr   { font-size: .6rem; color: rgba(200,155,60,.5); }

/* ── No data ─────────────────────────────────────────────── */
.no-data-card {
  padding: 20px; border-radius: 4px; text-align: center;
  background: rgba(255,255,255,.02); border: 1px solid rgba(255,255,255,.06);
  display: flex; flex-direction: column; gap: 8px;
}
.no-data-title { font-size: .72rem; font-weight: 600; color: rgba(232,224,208,.5); }
.no-data-desc  { font-size: .65rem; color: rgba(232,224,208,.3); line-height: 1.5; }

/* ── Patterns pending ────────────────────────────────────── */
.patterns-pending-card {
  display: flex; align-items: flex-start; gap: 14px;
  padding: 16px; border-radius: 6px;
  background: rgba(200,155,60,.04);
  border: 1px solid rgba(200,155,60,.18);
}
.patterns-pending-icon { flex-shrink: 0; margin-top: 2px; }
.patterns-pending-body { display: flex; flex-direction: column; gap: 8px; flex: 1; }
.patterns-pending-title {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .78rem; font-weight: 700; letter-spacing: .04em;
  color: rgba(200,155,60,.8); line-height: 1.4;
}
.patterns-pending-desc {
  font-size: .65rem; color: rgba(232,224,208,.4); line-height: 1.55;
}
.patterns-progress-bar {
  height: 3px; border-radius: 2px;
  background: rgba(200,155,60,.12); overflow: hidden; margin-top: 2px;
}
.patterns-progress-fill {
  height: 100%; border-radius: 2px;
  background: rgba(200,155,60,.55);
  transition: width .5s ease;
}

/* ── Metrics ─────────────────────────────────────────────── */
.metrics-list { display: flex; flex-direction: column; gap: 10px; }
.metric-row { display: flex; flex-direction: column; gap: 5px; }
.metric-label-row { display: flex; align-items: center; justify-content: space-between; }
.metric-label {
  font-size: .65rem; color: rgba(232,224,208,.6);
  font-family: 'Rajdhani','Inter',sans-serif; letter-spacing: .06em;
}
.metric-right { display: flex; align-items: center; gap: 8px; }
.metric-level { font-size: .62rem; font-weight: 700; font-family: 'Rajdhani','Inter',sans-serif; letter-spacing: .08em; }
.metric-pct   { font-family: monospace; font-size: .6rem; color: rgba(232,224,208,.3); }
.metric-bar-bg {
  height: 3px; border-radius: 2px; background: rgba(255,255,255,.08);
  overflow: hidden;
}
.metric-bar-fill {
  height: 100%; border-radius: 2px;
  transition: width .4s ease;
}

.deaths-vision-row {
  display: flex; align-items: center; justify-content: space-between;
  padding: 10px 12px; border-radius: 4px;
  background: rgba(28,42,58,.4); border: 1px solid rgba(255,255,255,.05);
}
.dv-label { display: flex; flex-direction: column; gap: 2px; }
.dv-title { font-size: .68rem; color: rgba(232,224,208,.6); }
.dv-desc  { font-size: .58rem; color: rgba(232,224,208,.3); }
.dv-val   { font-family: 'Rajdhani','Inter',sans-serif; font-size: 1.2rem; font-weight: 700; }

/* ── AI button ───────────────────────────────────────────── */
.ai-btn {
  display: flex; align-items: center; justify-content: center; gap: 7px;
  padding: 10px 16px; border-radius: 3px; cursor: pointer;
  font-family: 'Rajdhani','Inter',sans-serif; font-size: .72rem; font-weight: 700;
  letter-spacing: .14em; text-transform: uppercase; transition: all .2s;
  background: rgba(200,155,60,.08); border: 1px solid rgba(200,155,60,.35);
  color: rgba(200,155,60,.8);
}
.ai-btn:hover:not(:disabled) { background: rgba(200,155,60,.15); border-color: rgba(200,155,60,.6); color: #C89B3C; }
.ai-btn:disabled { opacity: .5; cursor: default; }
.ai-error { font-size: .62rem; color: rgba(192,57,43,.7); }

/* ── Insights card ───────────────────────────────────────── */
.insights-card {
  border-radius: 4px; padding: 16px;
  background: rgba(28,42,58,.5); border: 1px solid rgba(200,155,60,.15);
  display: flex; flex-direction: column; gap: 14px;
}
.playstyle-row {
  display: flex; align-items: center; gap: 8px;
  padding-bottom: 10px; border-bottom: 1px solid rgba(255,255,255,.06);
}
.playstyle-label {
  font-size: .62rem; color: rgba(232,224,208,.35);
  font-family: 'Rajdhani','Inter',sans-serif; letter-spacing: .1em; text-transform: uppercase;
}
.playstyle-val {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .78rem; font-weight: 700; color: rgba(200,155,60,.85); letter-spacing: .08em;
}
.insights-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
.insights-col { display: flex; flex-direction: column; gap: 8px; }
.insights-col-title {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .58rem; font-weight: 700; letter-spacing: .18em; text-transform: uppercase;
}
.insights-list { display: flex; flex-direction: column; gap: 6px; list-style: none; }
.insights-item {
  display: flex; align-items: flex-start; gap: 7px;
  font-size: .67rem; color: rgba(232,224,208,.65); line-height: 1.45;
}
.list-dot { width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0; margin-top: 4px; }

.focus-block {
  background: rgba(200,155,60,.06); border: 1px solid rgba(200,155,60,.2);
  border-radius: 3px; padding: 12px;
  display: flex; flex-direction: column; gap: 5px;
}
.focus-label {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .55rem; font-weight: 700; letter-spacing: .18em; text-transform: uppercase;
  color: rgba(200,155,60,.5);
}
.focus-text { font-size: .72rem; color: rgba(232,224,208,.85); line-height: 1.5; }
.progress-note { font-size: .65rem; color: rgba(232,224,208,.4); line-height: 1.5; font-style: italic; }

/* ── Transitions ─────────────────────────────────────────── */
.fade-enter-active { transition: opacity .3s ease, transform .3s ease; }
.fade-enter-from   { opacity: 0; transform: translateY(8px); }

@keyframes spin { to { transform: rotate(360deg); } }
</style>
