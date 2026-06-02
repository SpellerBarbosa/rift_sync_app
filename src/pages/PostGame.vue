<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useRouter } from 'vue-router'

const router = useRouter()

// ── Tipos ─────────────────────────────────────────────────────
interface CoachingAlert {
  id: number
  matchId: string
  timestamp: number
  category: string
  severity: string
  message: string
  wasHeard: boolean
}

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

interface PostGameData {
  matchId:      string
  championName: string
  championKey:  string
  role:         string | null
  result:       string
  kills:        number
  deaths:       number
  assists:       number
  csPerMin:     number
  visionScore:  number
  duration:     number
  alerts:       CoachingAlert[]
  pattern:      PlayerPattern | null
}

interface PostGameAnalysis {
  summary:      string
  strengths:    string[]
  improvements: string[]
  focus:        string
}

// ── Estado ────────────────────────────────────────────────────
const loading      = ref(true)
const syncLoading  = ref(false)
const aiLoading    = ref(false)
const error        = ref<string | null>(null)
const data         = ref<PostGameData | null>(null)
const analysis     = ref<PostGameAnalysis | null>(null)
const aiError      = ref<string | null>(null)
const ddVersion    = ref('14.24.1')

// ── Computados ─────────────────────────────────────────────────
const kda = computed(() => {
  if (!data.value) return 0
  const { kills, deaths, assists } = data.value
  return ((kills + assists) / Math.max(1, deaths)).toFixed(2)
})

const durationMin = computed(() => {
  if (!data.value) return '0:00'
  const m = Math.floor(data.value.duration / 60)
  const s = data.value.duration % 60
  return `${m}:${s.toString().padStart(2, '0')}`
})

const isWin = computed(() => data.value?.result === 'WIN')

const championIconUrl = computed(() => {
  if (!data.value?.championKey) return ''
  return `https://ddragon.leagueoflegends.com/cdn/${ddVersion.value}/img/champion/${data.value.championKey}.png`
})

const alertsBySeverity = computed(() => {
  if (!data.value?.alerts) return []
  const order = { CRITICAL: 0, WARNING: 1, INFO: 2 }
  return [...data.value.alerts].sort(
    (a, b) => (order[a.severity as keyof typeof order] ?? 2) - (order[b.severity as keyof typeof order] ?? 2)
  )
})

const severityColor = (sev: string) => ({
  CRITICAL: '#EF4444',
  WARNING:  '#F59E0B',
  INFO:     '#60A5FA',
}[sev] ?? '#60A5FA')

const categoryLabel = (cat: string) => ({
  OBJECTIVE:   'Objetivo',
  VISION:      'Visão',
  MACRO:       'Macro',
  TRADE:       'Trade',
  POSITIONING: 'Posição',
}[cat] ?? cat)

// ── Actions ───────────────────────────────────────────────────
async function loadData() {
  loading.value = true
  error.value   = null
  try {
    // Tenta carregar versão do DataDragon do settings
    const ver = await invoke<string>('get_setting', { key: 'dd_version' }).catch(() => '14.24.1')
    ddVersion.value = ver

    data.value = await invoke<PostGameData>('get_post_game_data')
  } catch (e: any) {
    error.value = typeof e === 'string' ? e : (e?.message ?? 'Erro ao carregar dados da partida')
  } finally {
    loading.value = false
  }
}

async function syncHistory() {
  syncLoading.value = true
  try {
    await invoke('sync_match_history')
    await loadData()
  } catch {
    // ignora — o erro vai aparecer no loadData
  } finally {
    syncLoading.value = false
  }
}

async function runAiAnalysis() {
  if (aiLoading.value) return
  aiLoading.value = true
  aiError.value   = null
  analysis.value  = null
  try {
    analysis.value = await invoke<PostGameAnalysis>('analyze_post_game')
  } catch (e: any) {
    aiError.value = typeof e === 'string' ? e : (e?.message ?? 'Erro na análise IA')
  } finally {
    aiLoading.value = false
  }
}

onMounted(loadData)
</script>

<template>
  <div class="pg-root">
    <div class="pg-glow" />

    <!-- Header -->
    <header class="pg-header">
      <button class="back-btn" @click="router.push('/')">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path d="M9 2L4 7L9 12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        Dashboard
      </button>
      <div class="pg-title">
        <span class="pg-diamond">◆</span>
        Revisão da Partida
      </div>
      <div class="w-20" />
    </header>

    <div class="pg-scroll">
      <div class="pg-content">

        <!-- Loading -->
        <div v-if="loading" class="pg-center">
          <span class="spin">⟳</span>
          <p class="loading-text">Carregando dados da partida...</p>
        </div>

        <!-- Erro + botão de sync -->
        <div v-else-if="error" class="pg-center">
          <p class="error-text">{{ error }}</p>
          <p class="hint-text">Sincronize o histórico para ver a análise da última partida.</p>
          <button class="action-btn" :disabled="syncLoading" @click="syncHistory">
            <span v-if="syncLoading" class="spin-sm">⟳</span>
            <span v-else>Sincronizar histórico</span>
          </button>
        </div>

        <!-- Conteúdo -->
        <template v-else-if="data">

          <!-- ── Card da partida ─────────────────────────────── -->
          <div class="match-card" :class="isWin ? 'match-win' : 'match-loss'">
            <div class="match-result-bar" :class="isWin ? 'bar-win' : 'bar-loss'" />

            <div class="match-inner">
              <!-- Ícone do campeão -->
              <div class="champ-wrap">
                <img
                  v-if="championIconUrl"
                  :src="championIconUrl"
                  :alt="data.championName"
                  class="champ-icon"
                  @error="($event.target as HTMLImageElement).style.display='none'"
                />
                <div class="champ-fallback">{{ data.championName[0] }}</div>
              </div>

              <!-- Infos principais -->
              <div class="match-info">
                <div class="champ-name">{{ data.championName }}</div>
                <div class="match-meta">
                  <span class="role-badge">{{ data.role ?? '?' }}</span>
                  <span class="duration-badge">{{ durationMin }}</span>
                </div>
              </div>

              <!-- Resultado -->
              <div class="result-block">
                <span class="result-text" :class="isWin ? 'result-win' : 'result-loss'">
                  {{ isWin ? 'Vitória' : 'Derrota' }}
                </span>
              </div>
            </div>

            <!-- Stats grid -->
            <div class="stats-grid">
              <div class="stat-cell">
                <span class="stat-val">{{ data.kills }}/{{ data.deaths }}/{{ data.assists }}</span>
                <span class="stat-lbl">KDA</span>
              </div>
              <div class="stat-cell">
                <span class="stat-val">{{ kda }}</span>
                <span class="stat-lbl">Ratio</span>
              </div>
              <div class="stat-cell">
                <span class="stat-val">{{ data.csPerMin.toFixed(1) }}</span>
                <span class="stat-lbl">CS/min</span>
              </div>
              <div class="stat-cell">
                <span class="stat-val">{{ data.visionScore }}</span>
                <span class="stat-lbl">Visão</span>
              </div>
            </div>
          </div>

          <!-- ── Botão de sync se não há alertas ───────────────── -->
          <div v-if="data.alerts.length === 0" class="sync-hint">
            <p class="hint-text">Nenhum alerta de coaching registrado para esta partida.</p>
            <button class="action-btn-sm" :disabled="syncLoading" @click="syncHistory">
              <span v-if="syncLoading" class="spin-sm">⟳</span>
              <span v-else>Sincronizar histórico</span>
            </button>
          </div>

          <!-- ── Alertas de coaching ────────────────────────────── -->
          <div v-else class="section">
            <h2 class="section-title">Alertas desta Partida</h2>
            <div class="alerts-list">
              <div
                v-for="alert in alertsBySeverity"
                :key="alert.id"
                class="alert-item"
              >
                <div class="alert-dot" :style="{ background: severityColor(alert.severity) }" />
                <div class="alert-body">
                  <div class="alert-meta">
                    <span class="alert-cat" :style="{ color: severityColor(alert.severity) }">
                      {{ categoryLabel(alert.category) }}
                    </span>
                    <span class="alert-time">{{ Math.floor(alert.timestamp / 60) }}:{{ String(alert.timestamp % 60).padStart(2,'0') }}</span>
                  </div>
                  <p class="alert-msg">{{ alert.message }}</p>
                </div>
              </div>
            </div>
          </div>

          <!-- ── Análise IA ──────────────────────────────────────── -->
          <div class="section">
            <div class="section-header-row">
              <h2 class="section-title">Análise com IA</h2>
              <span class="ai-badge">Groq</span>
            </div>
            <p class="section-desc">
              Análise personalizada baseada nos dados desta partida e no seu histórico de jogadas.
            </p>

            <button
              class="ai-btn"
              :disabled="aiLoading"
              @click="runAiAnalysis"
            >
              <span v-if="aiLoading" class="spin-sm">⟳</span>
              <template v-else>
                <svg width="13" height="13" viewBox="0 0 13 13" fill="none" style="flex-shrink:0">
                  <circle cx="6.5" cy="6.5" r="5.5" stroke="currentColor" stroke-width="1.2"/>
                  <path d="M4 6.5L5.8 8.3L9 5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                {{ analysis ? 'Atualizar análise' : 'Analisar partida' }}
              </template>
            </button>

            <p v-if="aiError" class="ai-error">{{ aiError }}</p>

            <!-- Resultado da análise -->
            <Transition name="fade">
              <div v-if="analysis" class="analysis-card">
                <!-- Resumo -->
                <p class="analysis-summary">{{ analysis.summary }}</p>

                <div class="analysis-grid">
                  <!-- Pontos positivos -->
                  <div class="analysis-col">
                    <p class="analysis-col-title" style="color:#4ADE80">Pontos positivos</p>
                    <ul class="analysis-list">
                      <li v-for="(s, i) in analysis.strengths" :key="i" class="analysis-list-item">
                        <span class="list-dot" style="background:#4ADE80" />
                        {{ s }}
                      </li>
                    </ul>
                  </div>

                  <!-- Melhorias -->
                  <div class="analysis-col">
                    <p class="analysis-col-title" style="color:#F59E0B">A melhorar</p>
                    <ul class="analysis-list">
                      <li v-for="(imp, i) in analysis.improvements" :key="i" class="analysis-list-item">
                        <span class="list-dot" style="background:#F59E0B" />
                        {{ imp }}
                      </li>
                    </ul>
                  </div>
                </div>

                <!-- Foco -->
                <div class="focus-block">
                  <span class="focus-label">Foco para a próxima</span>
                  <p class="focus-text">{{ analysis.focus }}</p>
                </div>
              </div>
            </Transition>
          </div>

        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pg-root {
  height: 100%;
  background: #010A13;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
  user-select: none;
}
.pg-glow {
  position: absolute; inset: 0; pointer-events: none;
  background: radial-gradient(ellipse 70% 40% at 50% 0%, rgba(10,30,60,.5) 0%, transparent 60%);
}

/* ── Header ───────────────────────────────────────────── */
.pg-header {
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
.pg-title {
  display: flex; align-items: center; gap: 7px;
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .78rem; font-weight: 700; letter-spacing: .22em; text-transform: uppercase;
  color: rgba(200,155,60,.6);
}
.pg-diamond { font-size: 7px; color: rgba(200,155,60,.5); }

/* ── Scroll ───────────────────────────────────────────── */
.pg-scroll { flex: 1; overflow-y: auto; position: relative; z-index: 1; }
.pg-content { max-width: 560px; margin: 0 auto; padding: 20px 24px 40px; display: flex; flex-direction: column; gap: 20px; }

/* ── Estados centrais ─────────────────────────────────── */
.pg-center {
  display: flex; flex-direction: column; align-items: center;
  justify-content: center; gap: 12px; padding: 60px 20px; text-align: center;
}
.spin       { font-size: 1.4rem; color: rgba(200,155,60,.5); animation: spin .7s linear infinite; display: inline-block; }
.spin-sm    { display: inline-block; animation: spin .7s linear infinite; font-size: .9rem; }
.loading-text { font-size: .7rem; color: rgba(232,224,208,.3); letter-spacing: .08em; }
.error-text   { font-size: .72rem; color: rgba(192,57,43,.7); }
.hint-text    { font-size: .65rem; color: rgba(232,224,208,.3); }

/* ── Match card ───────────────────────────────────────── */
.match-card {
  border-radius: 6px; overflow: hidden;
  background: rgba(28,42,58,.5);
  border: 1px solid rgba(200,155,60,.15);
  position: relative;
}
.match-result-bar { height: 2px; }
.bar-win  { background: linear-gradient(to right, #4ADE80, rgba(74,222,128,.2)); }
.bar-loss { background: linear-gradient(to right, #EF4444, rgba(239,68,68,.2)); }
.match-win  { border-color: rgba(74,222,128,.2); }
.match-loss { border-color: rgba(239,68,68,.2); }

.match-inner {
  display: flex; align-items: center; gap: 14px;
  padding: 14px 16px 10px;
}
.champ-wrap {
  width: 48px; height: 48px; border-radius: 4px;
  overflow: hidden; flex-shrink: 0; position: relative;
  background: rgba(255,255,255,.05);
}
.champ-icon { width: 100%; height: 100%; object-fit: cover; position: absolute; inset: 0; }
.champ-fallback {
  position: absolute; inset: 0; display: flex; align-items: center; justify-content: center;
  font-size: 1.2rem; font-weight: 700; color: rgba(200,155,60,.5);
}
.match-info { flex: 1; min-width: 0; }
.champ-name {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .95rem; font-weight: 700; letter-spacing: .04em;
  color: rgba(232,224,208,.9);
}
.match-meta { display: flex; gap: 6px; margin-top: 4px; }
.role-badge, .duration-badge {
  font-size: .58rem; font-family: 'Rajdhani','Inter',sans-serif;
  letter-spacing: .12em; text-transform: uppercase;
  padding: 2px 7px; border-radius: 2px;
  background: rgba(255,255,255,.05); color: rgba(232,224,208,.4);
  border: 1px solid rgba(255,255,255,.08);
}
.result-text {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .78rem; font-weight: 700; letter-spacing: .14em; text-transform: uppercase;
}
.result-win  { color: #4ADE80; }
.result-loss { color: #EF4444; }

.stats-grid {
  display: grid; grid-template-columns: repeat(4,1fr);
  border-top: 1px solid rgba(255,255,255,.06);
}
.stat-cell {
  display: flex; flex-direction: column; align-items: center;
  padding: 10px 8px; gap: 3px;
  border-right: 1px solid rgba(255,255,255,.06);
}
.stat-cell:last-child { border-right: none; }
.stat-val {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .88rem; font-weight: 700; color: rgba(232,224,208,.85);
}
.stat-lbl {
  font-size: .55rem; letter-spacing: .1em; text-transform: uppercase;
  color: rgba(232,224,208,.3);
}

/* ── Sync hint ─────────────────────────────────────────── */
.sync-hint {
  display: flex; flex-direction: column; align-items: center; gap: 10px;
  padding: 16px; border-radius: 4px;
  background: rgba(255,255,255,.02); border: 1px solid rgba(255,255,255,.06);
}

/* ── Section ───────────────────────────────────────────── */
.section { display: flex; flex-direction: column; gap: 12px; }
.section-header-row { display: flex; align-items: center; justify-content: space-between; }
.section-title {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .62rem; font-weight: 700; letter-spacing: .22em; text-transform: uppercase;
  color: rgba(200,155,60,.45);
}
.section-desc {
  font-size: .65rem; color: rgba(232,224,208,.3); line-height: 1.5;
}
.ai-badge {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .48rem; font-weight: 700; letter-spacing: .16em;
  padding: 2px 7px; border-radius: 2px;
  background: rgba(6,182,212,.1); color: rgba(6,182,212,.75);
  border: 1px solid rgba(6,182,212,.25);
}

/* ── Alerts ────────────────────────────────────────────── */
.alerts-list { display: flex; flex-direction: column; gap: 6px; }
.alert-item {
  display: flex; gap: 10px; align-items: flex-start;
  padding: 10px 12px; border-radius: 4px;
  background: rgba(28,42,58,.4); border: 1px solid rgba(255,255,255,.05);
}
.alert-dot {
  width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; margin-top: 4px;
}
.alert-body { flex: 1; min-width: 0; }
.alert-meta { display: flex; align-items: center; justify-content: space-between; margin-bottom: 3px; }
.alert-cat {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .55rem; font-weight: 700; letter-spacing: .14em; text-transform: uppercase;
}
.alert-time {
  font-family: monospace; font-size: .58rem; color: rgba(232,224,208,.25);
}
.alert-msg { font-size: .68rem; color: rgba(232,224,208,.75); line-height: 1.45; }

/* ── AI button ─────────────────────────────────────────── */
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

/* ── Action buttons ───────────────────────────────────── */
.action-btn {
  display: flex; align-items: center; gap: 6px;
  padding: 8px 20px; border-radius: 3px; cursor: pointer;
  font-family: 'Rajdhani','Inter',sans-serif; font-size: .7rem; font-weight: 700;
  letter-spacing: .14em; text-transform: uppercase; transition: all .2s;
  background: rgba(200,155,60,.08); border: 1px solid rgba(200,155,60,.3);
  color: rgba(200,155,60,.7);
}
.action-btn:hover:not(:disabled) { background: rgba(200,155,60,.15); color: #C89B3C; }
.action-btn:disabled { opacity: .5; cursor: default; }
.action-btn-sm {
  display: flex; align-items: center; gap: 6px;
  padding: 5px 14px; border-radius: 2px; cursor: pointer;
  font-family: 'Rajdhani','Inter',sans-serif; font-size: .62rem; font-weight: 600;
  letter-spacing: .1em; text-transform: uppercase; transition: all .2s;
  background: rgba(200,155,60,.06); border: 1px solid rgba(200,155,60,.2);
  color: rgba(200,155,60,.6);
}
.action-btn-sm:hover:not(:disabled) { background: rgba(200,155,60,.12); color: rgba(200,155,60,.9); }
.action-btn-sm:disabled { opacity: .4; cursor: default; }

/* ── Analysis card ─────────────────────────────────────── */
.analysis-card {
  border-radius: 4px; padding: 16px;
  background: rgba(28,42,58,.5); border: 1px solid rgba(200,155,60,.15);
  display: flex; flex-direction: column; gap: 14px;
}
.analysis-summary {
  font-size: .72rem; line-height: 1.6; color: rgba(232,224,208,.75);
  border-bottom: 1px solid rgba(255,255,255,.06); padding-bottom: 12px;
}
.analysis-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
.analysis-col { display: flex; flex-direction: column; gap: 8px; }
.analysis-col-title {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .58rem; font-weight: 700; letter-spacing: .18em; text-transform: uppercase;
}
.analysis-list { display: flex; flex-direction: column; gap: 6px; list-style: none; }
.analysis-list-item {
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

/* ── Transitions ───────────────────────────────────────── */
.fade-enter-active { transition: opacity .3s ease, transform .3s ease; }
.fade-enter-from   { opacity: 0; transform: translateY(8px); }

@keyframes spin { to { transform: rotate(360deg); } }
</style>
