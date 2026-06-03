<script setup lang="ts">
import { onMounted, onUnmounted, ref, computed, watch } from 'vue'
import { listen, type UnlistenFn }  from '@tauri-apps/api/event'
import { usePlayerStore }           from '../stores/player'
import { useGameStateStore }        from '../stores/gameState'
import { useDashboardStore }        from '../stores/dashboard'

const playerStore    = usePlayerStore()
const gameStateStore = useGameStateStore()
const dash           = useDashboardStore()

// ── Progresso de sincronização SpellCoach ─────────────────────
interface SyncProgress { phase: string; current: number; total: number; label: string }

const syncProgress = ref<SyncProgress | null>(null)
let   unlistenSync: UnlistenFn | null = null

onMounted(async () => {
  unlistenSync = await listen<SyncProgress>('sync_progress', (e) => {
    syncProgress.value = e.payload.phase === 'done' ? null : e.payload
  })
})

onUnmounted(() => { unlistenSync?.() })

const syncLabel = computed(() => {
  const p = syncProgress.value
  if (!p) return ''
  if (p.phase === 'meta_stats') return `Sincronizando meta... ${p.current}/${p.total}`
  if (p.phase === 'builds')     return `Builds: ${p.label} (${p.current}/${p.total})`
  return p.label
})

const syncPct = computed(() => {
  const p = syncProgress.value
  if (!p || p.total === 0) return 0
  return Math.round((p.current / p.total) * 100)
})

function championIcon(key: string): string {
  if (!key) return ''
  return `https://ddragon.leagueoflegends.com/cdn/${dash.ddVersion}/img/champion/${key}.png`
}

function onImgError(e: Event) {
  const img = e.target as HTMLImageElement
  img.style.display = 'none'
  const fallback = img.nextElementSibling as HTMLElement | null
  if (fallback) fallback.style.display = 'flex'
}

async function syncFromLcu() {
  await playerStore.fetchCurrentPlayer()
  await dash.syncAll()
  if (dash.stats?.topChampions.length) dash.fetchMetaStats()
  if (dash.matches.length > 0) dash.fetchAnalysis(playerStore.currentPlayer?.role)
}

onMounted(async () => {
  // 1. Carrega cache local imediatamente (resposta rápida sem LCU)
  await Promise.all([
    playerStore.fetchLastPlayer(),
    dash.fetchCachedMatches(),
    dash.fetchMatchStats(),
  ])
  if (dash.stats?.topChampions.length) dash.fetchMetaStats()
  if (dash.matches.length > 0) dash.fetchAnalysis(playerStore.currentPlayer?.role)

  // 2. Se o LoL já estava online quando o dashboard abriu, sincroniza na hora
  //    (o watch só captura a transição false→true, não o estado já ativo)
  if (gameStateStore.isLolRunning) {
    syncFromLcu()
  }
})

watch(() => gameStateStore.isLolRunning, (running) => {
  if (running) syncFromLcu()
})


function winrateColor(wr: number) {
  if (wr >= 55) return 'text-rs-success'
  if (wr < 45)  return 'text-rs-danger'
  return 'text-rs-white/70'
}
</script>

<template>
  <div class="dash-root">

    <div class="dash-glow" />
    <div class="corner tl" /><div class="corner tr" />
    <div class="corner bl" /><div class="corner br" />
    <div class="edge top-0" /><div class="edge bottom-0" />

    <div class="dash-layout">

      <!-- ── Header ─────────────────────────────────────── -->
      <header class="dash-header">

        <div class="flex items-center gap-2 shrink-0">
          <div class="logo-diamond" />
          <span class="logo-text">RiftSync AI</span>
        </div>

        <div class="flex-1 mx-5 h-px" style="background:linear-gradient(to right,rgba(200,155,60,.2),transparent)" />

        <!-- Card do invocador -->
        <div v-if="playerStore.currentPlayer" class="player-card-frame">
        <div class="player-card">

          <!-- Quadrado esquerdo: ícone de perfil preenche todo o espaço -->
          <div class="avatar-square" :class="playerStore.levelBorderClass">
            <img
              :src="playerStore.profileIconUrl(dash.ddVersion)"
              :alt="playerStore.displayName"
              class="avatar-img"
              @error="($event.target as HTMLImageElement).style.display='none'"
            />
            <div class="avatar-fallback">
              {{ playerStore.displayName.charAt(0).toUpperCase() }}
            </div>
          </div>

          <!-- Informações -->
          <div class="player-info">
            <p class="player-name">{{ playerStore.displayName }}</p>

            <div class="player-elo-row">
              <img
                v-if="playerStore.rankEmblemUrl"
                :src="playerStore.rankEmblemUrl"
                class="elo-emblem"
                :alt="playerStore.currentPlayer.tierEn ?? ''"
                @error="($event.target as HTMLImageElement).style.display='none'"
              />
              <span class="player-rank">{{ playerStore.rankDisplay }}</span>
            </div>

            <p class="player-tag">{{ playerStore.fullRiotId }}</p>
          </div>

        </div></div>
        <div v-else-if="playerStore.isLoading" class="player-card-skeleton" />
        <div v-else class="player-card-frame"><div class="player-card player-card--empty">
          <p class="text-rs-white/20 text-xs tracking-widest">{{ $t('dashboard.noPlayerData') }}</p>
        </div></div>

        <div class="lcu-badge" :class="gameStateStore.isLolRunning ? 'badge-on' : 'badge-off'">
          <div class="badge-dot" :class="gameStateStore.isLolRunning ? 'dot-on' : 'dot-off'" />
          {{ gameStateStore.isLolRunning ? $t('dashboard.online') : $t('dashboard.offline') }}
        </div>

      </header>

      <!-- ── Sync progress ─────────────────────────────────── -->
      <Transition name="sync-fade">
        <div v-if="syncProgress" class="sync-bar">
          <div class="sync-spinner" />
          <span class="sync-label">{{ syncLabel }}</span>
          <div class="sync-track">
            <div class="sync-fill" :style="{ width: syncPct + '%' }" />
          </div>
          <span class="sync-pct">{{ syncPct }}%</span>
        </div>
      </Transition>

      <!-- ── Stats row ──────────────────────────────────── -->
      <div class="stats-row">
        <div class="stat-card" v-for="stat in [
          { label: $t('dashboard.stats.matches'), value: dash.stats?.totalGames ?? '—',               color: '' },
          { label: $t('dashboard.stats.winrate'), value: dash.stats ? dash.stats.winrate + '%' : '—', color: dash.stats ? winrateColor(dash.stats.winrate) : '' },
          { label: $t('dashboard.stats.avgKda'),  value: dash.stats?.avgKda ?? '—',                   color: '' },
          { label: $t('dashboard.stats.avgCs'),   value: dash.stats?.avgCs  ?? '—',                   color: '' },
        ]" :key="stat.label">
          <p class="stat-value" :class="stat.color || 'text-rs-gold/80'">{{ stat.value }}</p>
          <p class="stat-label">{{ stat.label }}</p>
        </div>
      </div>

      <!-- ── Main grid ──────────────────────────────────── -->
      <div class="main-grid">

        <!-- ── Últimas Partidas ────────────────────────── -->
        <section class="grid-col">
          <h2 class="section-title">
            <span class="title-diamond" />{{ $t('dashboard.recentMatches') }}
          </h2>

          <div v-if="dash.loadingMatches" class="flex flex-col gap-1.5">
            <div v-for="i in 5" :key="i" class="match-skeleton" />
          </div>

          <div v-else-if="dash.matches.length" class="flex flex-col gap-1">
            <div
              v-for="m in dash.matches" :key="m.gameId"
              class="match-row"
              :class="m.win ? 'row-win' : 'row-loss'"
            >
              <!-- Win/loss bar -->
              <div class="match-bar" :class="m.win ? 'bar-win' : 'bar-loss'" />

              <!-- Champion portrait -->
              <div class="champ-portrait-wrap">
                <img
                  v-if="m.championKey"
                  :src="championIcon(m.championKey)"
                  :alt="m.championName"
                  class="champ-portrait"
                  @error="onImgError"
                />
                <div class="champ-portrait-fallback" :style="{display: m.championKey ? 'none' : 'flex'}">
                  {{ m.championName.charAt(0) }}
                </div>
              </div>

              <!-- Name + KDA -->
              <div class="flex-1 min-w-0">
                <p class="match-champ">{{ m.championName }}</p>
                <p class="match-kda-line">
                  <span class="text-rs-white/75">{{ m.kills }}</span>
                  <span class="kda-sep">/</span>
                  <span class="text-rs-danger/80">{{ m.deaths }}</span>
                  <span class="kda-sep">/</span>
                  <span class="text-rs-white/75">{{ m.assists }}</span>
                  <span class="kda-ratio">{{ m.kda.toFixed(1) }} KDA</span>
                </p>
              </div>

              <!-- Result badge -->
              <span class="match-badge" :class="m.win ? 'badge-win' : 'badge-loss'">
                {{ m.win ? $t('dashboard.win') : $t('dashboard.loss') }}
              </span>

              <!-- Meta -->
              <span class="match-meta">
                {{ dash.formatDuration(m.durationSecs) }}
                <span class="opacity-25 mx-1">·</span>
                {{ m.cs }}cs
              </span>
            </div>
          </div>

          <div v-else class="empty-col">
            <i18n-t keypath="dashboard.emptyMatches" tag="p" class="text-rs-white/25 text-xs leading-relaxed text-center">
              <template #br><br /></template>
            </i18n-t>
          </div>
        </section>

        <div class="vert-divider" />

        <!-- ── Campeões + Análise ──────────────────────── -->
        <section class="grid-col">

          <h2 class="section-title"><span class="title-diamond" />{{ $t('dashboard.topChampions') }}</h2>

          <div v-if="dash.stats?.topChampions.length" class="flex flex-col gap-2 mb-4">
            <div v-for="c in dash.stats.topChampions" :key="c.name" class="champ-row">

              <!-- Champion icon -->
              <div class="champ-icon-wrap">
                <img
                  v-if="c.key"
                  :src="championIcon(c.key)"
                  :alt="c.name"
                  class="champ-icon-img"
                  @error="onImgError"
                />
                <div class="champ-icon-fallback" :style="{display: c.key ? 'none' : 'flex'}">
                  {{ c.name.charAt(0) }}
                </div>
              </div>

              <div class="flex-1 min-w-0">
                <div class="flex items-baseline justify-between gap-2">
                  <span class="champ-name">{{ c.name }}</span>
                  <span class="text-[10px] font-mono text-rs-white/30">{{ c.games }}j</span>
                </div>
                <!-- Player WR + winrate bar -->
                <div class="flex items-center gap-2 mt-0.5">
                  <span class="text-[11px]" :class="winrateColor(c.winrate)">{{ c.winrate.toFixed(0) }}% WR</span>
                  <span class="text-[10px] text-rs-white/30">KDA {{ c.avgKda }}</span>
                  <div class="wr-bar-bg ml-auto">
                    <div class="wr-bar-fill" :class="c.winrate >= 50 ? 'fill-win' : 'fill-loss'"
                         :style="{ width: Math.min(c.winrate, 100) + '%' }" />
                  </div>
                </div>
                <!-- SpellCoach meta comparison -->
                <div v-if="dash.metaStats.get(c.id)" class="meta-row">
                  <span class="meta-tag">META</span>
                  <span class="meta-val">{{ dash.metaStats.get(c.id)!.winRate.toFixed(1) }}% WR</span>
                  <span class="meta-sep">·</span>
                  <span class="meta-val">{{ dash.metaStats.get(c.id)!.averageCsPerMin.toFixed(1) }} CS/m</span>
                  <span class="meta-sep">·</span>
                  <span class="meta-val">{{ dash.metaStats.get(c.id)!.kda.ratio }} KDA</span>
                  <span class="meta-patch ml-auto">{{ dash.metaStats.get(c.id)!.patch }}</span>
                </div>
                <div v-else-if="dash.loadingMeta" class="meta-row">
                  <span class="meta-loading">{{ $t('dashboard.loadingMeta') }}</span>
                </div>
              </div>
            </div>
          </div>

          <div v-else-if="!dash.loadingStats" class="mb-4">
            <p class="text-rs-white/20 text-xs">{{ $t('dashboard.noChampionData') }}</p>
          </div>

          <!-- Separator -->
          <div class="ornament-sep mb-4">
            <div class="sep-line" /><div class="sep-diamond" /><div class="sep-line" />
          </div>

          <!-- Análise IA -->
          <h2 class="section-title mb-3"><span class="title-diamond" />{{ $t('dashboard.performanceAnalysis') }}</h2>

          <div v-if="dash.loadingAnalysis" class="flex items-center gap-2 py-2">
            <div class="w-1.5 h-1.5 rounded-full bg-rs-gold animate-pulse" />
            <span class="text-rs-gold/45 text-xs tracking-wider">{{ $t('dashboard.analyzingAi') }}</span>
          </div>

          <div v-else-if="dash.analysis" class="flex flex-col gap-4">
            <div>
              <p class="insight-header text-rs-success/65">{{ $t('dashboard.strengths') }}</p>
              <ul class="mt-1.5 flex flex-col gap-1">
                <li v-for="s in dash.analysis.strengths" :key="s" class="insight-item">
                  <span class="bullet bg-rs-success/60" />
                  <span class="text-rs-white/70 text-sm">{{ s }}</span>
                </li>
              </ul>
            </div>
            <div>
              <p class="insight-header text-rs-gold/65">{{ $t('dashboard.weaknesses') }}</p>
              <ul class="mt-1.5 flex flex-col gap-1">
                <li v-for="w in dash.analysis.weaknesses" :key="w" class="insight-item">
                  <span class="bullet bg-rs-gold/60" />
                  <span class="text-rs-white/60 text-sm">{{ w }}</span>
                </li>
              </ul>
            </div>
            <div class="priority-box">
              <p class="priority-label">{{ $t('dashboard.priorityFocus') }}</p>
              <p class="text-rs-white/80 text-sm leading-relaxed">{{ dash.analysis.priorityTip }}</p>
            </div>
          </div>

          <div v-else-if="!dash.loadingAnalysis" class="py-2">
            <i18n-t v-if="dash.analysisError?.includes('GROQ_API_KEY')" keypath="dashboard.noGroqKey" tag="p" class="text-rs-white/20 text-xs leading-relaxed">
              <template #key><span class="font-mono text-rs-gold/40">GROQ_API_KEY</span></template>
            </i18n-t>
            <p v-else-if="dash.matches.length === 0" class="text-rs-white/20 text-xs">
              {{ $t('dashboard.playMoreMatches') }}
            </p>
            <p v-else-if="dash.analysisError" class="text-rs-white/20 text-xs">{{ dash.analysisError }}</p>
          </div>

        </section>
      </div>

    </div>

    <span class="version">v0.1.0</span>
  </div>
</template>

<style scoped>
/* ── Root ─────────────────────────────────────────────── */
.dash-root {
  height: 100%;
  background: #010A13;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
  user-select: none;
}
.dash-glow {
  position: absolute; inset: 0; pointer-events: none;
  background:
    radial-gradient(ellipse 70% 50% at 50% 0%, rgba(10,30,60,.6) 0%, transparent 60%),
    radial-gradient(ellipse 40% 30% at 50% 100%, rgba(200,155,60,.04) 0%, transparent 50%);
}
.edge { position:absolute; left:0; right:0; height:1px; pointer-events:none; background:linear-gradient(to right,transparent,rgba(200,155,60,.3),transparent); }
.corner { position:absolute; width:16px; height:16px; pointer-events:none; }
.tl { top:16px; left:16px;  border-top:1px solid rgba(200,155,60,.4); border-left:1px solid rgba(200,155,60,.4); }
.tr { top:16px; right:16px; border-top:1px solid rgba(200,155,60,.4); border-right:1px solid rgba(200,155,60,.4); }
.bl { bottom:16px; left:16px;  border-bottom:1px solid rgba(200,155,60,.4); border-left:1px solid rgba(200,155,60,.4); }
.br { bottom:16px; right:16px; border-bottom:1px solid rgba(200,155,60,.4); border-right:1px solid rgba(200,155,60,.4); }

/* ── Layout ───────────────────────────────────────────── */
.dash-layout {
  position: relative; z-index: 10;
  display: flex; flex-direction: column;
  height: 100%; padding: 14px 24px 10px;
}

/* ── Header ───────────────────────────────────────────── */
.dash-header {
  display: flex; align-items: center; gap: 12px;
  padding-bottom: 10px;
  border-bottom: 1px solid rgba(200,155,60,.1);
  flex-shrink: 0;
}
.logo-diamond { width:7px; height:7px; background:#C89B3C; transform:rotate(45deg); opacity:.6; flex-shrink:0; }
.logo-text { font-family:'Rajdhani','Inter',sans-serif; font-size:.82rem; font-weight:700; letter-spacing:.22em; text-transform:uppercase; color:rgba(200,155,60,.55); }

/* ── Frame com cantoneiras (borda personalizada) ──────── */
.player-card-frame {
  position: relative;
  flex-shrink: 0;
  padding: 1px;       /* espaço para as cantoneiras não sobreporem o conteúdo */
}

/* Borda fina de base */
.player-card-frame::before {
  content: '';
  position: absolute; inset: 0;
  border: 1px solid rgba(200,155,60,.18);
  border-radius: 3px;
  pointer-events: none;
}

/* 4 cantoneiras em L — 8 linhas de gradiente nos cantos */
.player-card-frame::after {
  content: '';
  position: absolute; inset: 0;
  border-radius: 3px;
  pointer-events: none;
  background:
    /* canto superior-esquerdo — horizontal */
    linear-gradient(rgba(200,155,60,.85), rgba(200,155,60,.85))
      top    left  / 12px 1.5px no-repeat,
    /* canto superior-esquerdo — vertical */
    linear-gradient(rgba(200,155,60,.85), rgba(200,155,60,.85))
      top    left  / 1.5px 12px no-repeat,
    /* canto superior-direito — horizontal */
    linear-gradient(rgba(200,155,60,.85), rgba(200,155,60,.85))
      top    right / 12px 1.5px no-repeat,
    /* canto superior-direito — vertical */
    linear-gradient(rgba(200,155,60,.85), rgba(200,155,60,.85))
      top    right / 1.5px 12px no-repeat,
    /* canto inferior-esquerdo — horizontal */
    linear-gradient(rgba(200,155,60,.85), rgba(200,155,60,.85))
      bottom left  / 12px 1.5px no-repeat,
    /* canto inferior-esquerdo — vertical */
    linear-gradient(rgba(200,155,60,.85), rgba(200,155,60,.85))
      bottom left  / 1.5px 12px no-repeat,
    /* canto inferior-direito — horizontal */
    linear-gradient(rgba(200,155,60,.85), rgba(200,155,60,.85))
      bottom right / 12px 1.5px no-repeat,
    /* canto inferior-direito — vertical */
    linear-gradient(rgba(200,155,60,.85), rgba(200,155,60,.85))
      bottom right / 1.5px 12px no-repeat;
}

/* ── Card do invocador ────────────────────────────────── */
.player-card {
  display: flex; align-items: stretch; gap: 0;
  background: linear-gradient(160deg, rgba(200,155,60,.06), rgba(10,14,20,.8));
  overflow: hidden;
  border-radius: 2px;
}
.player-card--empty {
  align-items: center; justify-content: center;
  padding: 8px 16px;
}
.player-card-skeleton {
  width: 212px; height: 58px; border-radius: 3px;
  background: rgba(200,155,60,.04); animation: pulse 1.5s ease infinite;
}

/* ── Quadrado esquerdo (ícone preenche tudo) ──────────── */
.avatar-square {
  width: 56px; flex-shrink: 0;
  position: relative; overflow: hidden;
  border-right: 2px solid transparent; /* a borda colorida vem do levelBorderClass */
}
.avatar-img {
  position: absolute; inset: 0;
  width: 100%; height: 100%;
  object-fit: cover;
  z-index: 1;
}
.avatar-fallback {
  position: absolute; inset: 0;
  background: rgba(28,42,58,.95);
  display: flex; align-items: center; justify-content: center;
  font-weight: 700; font-size: 1.1rem; color: #C89B3C;
  z-index: 0;
}
/* Borda direita colorida por nível — funciona como separador temático */
.avatar-border--default  { border-right-color: rgba(255,255,255,.15); }
.avatar-border--silver   { border-right-color: #c8d6de; }
.avatar-border--gold     { border-right-color: #C89B3C; }
.avatar-border--diamond  { border-right-color: #aaddff; }
.avatar-border--prestige { border-right-color: #fff0a0; }

/* ── Informações à direita ────────────────────────────── */
.player-info {
  min-width: 0; flex: 1;
  padding: 7px 12px;
  display: flex; flex-direction: column; justify-content: center; gap: 2px;
}
.player-name {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size: .86rem; font-weight: 700; letter-spacing: .04em;
  color: rgba(200,170,110,.95);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  line-height: 1;
}
.player-elo-row {
  display: flex; align-items: center; gap: 5px; margin-top: 1px;
}
.elo-emblem {
  width: 24px; height: 24px; object-fit: contain; flex-shrink: 0;
  filter: brightness(1.15) drop-shadow(0 0 4px rgba(200,155,60,.4));
}
.player-rank { font-size: .63rem; color: rgba(200,155,60,.7); white-space: nowrap; }
.player-tag  {
  font-size: .56rem; font-family: monospace;
  color: rgba(232,224,208,.18); letter-spacing: .02em;
  margin-top: 1px;
}

.lcu-badge { display:flex; align-items:center; gap:5px; font-size:.6rem; letter-spacing:.12em; text-transform:uppercase; padding:3px 8px; border-radius:2px; flex-shrink:0; font-family:'Rajdhani','Inter',sans-serif; }
.badge-on  { color:rgba(39,174,96,.8); border:1px solid rgba(39,174,96,.25); background:rgba(39,174,96,.06); }
.badge-off { color:rgba(200,155,60,.3); border:1px solid rgba(200,155,60,.1); }
.badge-dot { width:5px; height:5px; border-radius:50%; }
.dot-on  { background:#27AE60; box-shadow:0 0 5px #27AE60; animation:ping .8s ease infinite; }
.dot-off { background:rgba(200,155,60,.2); }
@keyframes ping { 0%,100%{opacity:1;} 50%{opacity:.5;} }

/* ── Sync progress bar ───────────────────────────────── */
.sync-bar {
  display:       flex;
  align-items:   center;
  gap:           10px;
  padding:       7px 12px;
  margin:        2px 0 4px;
  flex-shrink:   0;
  background:    rgba(200,155,60,.06);
  border:        1px solid rgba(200,155,60,.22);
  border-radius: 3px;
  /* clip hextech no canto superior-direito */
  clip-path: polygon(0 0, calc(100% - 10px) 0, 100% 10px, 100% 100%, 0 100%);
}

.sync-spinner {
  width:        11px;
  height:       11px;
  border-radius: 50%;
  border:       2px solid rgba(200,155,60,.2);
  border-top-color: #C89B3C;
  animation:    spin .7s linear infinite;
  flex-shrink:  0;
}

.sync-label {
  font-family:    'Rajdhani','Inter',sans-serif;
  font-size:      .7rem;
  font-weight:    600;
  letter-spacing: .14em;
  text-transform: uppercase;
  color:          rgba(200,155,60,.9);
  min-width:      0;
  overflow:       hidden;
  text-overflow:  ellipsis;
  white-space:    nowrap;
  flex:           1;
}

.sync-track {
  width:         120px;
  height:        4px;
  background:    rgba(200,155,60,.12);
  border-radius: 2px;
  overflow:      hidden;
  flex-shrink:   0;
}

.sync-fill {
  height:     100%;
  background: linear-gradient(to right, rgba(200,155,60,.7), #C89B3C);
  transition: width .3s ease;
  border-radius: 2px;
}

.sync-pct {
  font-family: 'Rajdhani','Inter',sans-serif;
  font-size:   .72rem;
  font-weight: 700;
  color:       rgba(200,155,60,.75);
  flex-shrink: 0;
  min-width:   36px;
  text-align:  right;
}

.sync-fade-enter-active { transition: opacity .2s ease; }
.sync-fade-leave-active { transition: opacity .3s ease; }
.sync-fade-enter-from,
.sync-fade-leave-to     { opacity: 0; }

/* ── Stats row ────────────────────────────────────────── */
.stats-row {
  display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px;
  padding: 8px 0; flex-shrink: 0;
}
.stat-card {
  display:flex; flex-direction:column; align-items:center; justify-content:center;
  padding: 8px 4px;
  border: 1px solid rgba(200,155,60,.1);
  background: rgba(200,155,60,.03);
  position: relative; overflow: hidden;
}
.stat-card::before {
  content:''; position:absolute; bottom:0; left:0; right:0; height:1px;
  background:linear-gradient(to right, transparent, rgba(200,155,60,.3), transparent);
}
.stat-value { font-family:'Rajdhani','Inter',sans-serif; font-size:1.3rem; font-weight:700; line-height:1; }
.stat-label { font-size:.58rem; letter-spacing:.15em; text-transform:uppercase; color:rgba(200,155,60,.4); margin-top:3px; }

/* ── Main grid ────────────────────────────────────────── */
.main-grid {
  display: grid; grid-template-columns: 1fr 1px 1fr;
  gap: 0 20px; flex: 1; min-height: 0; padding-top: 10px;
}
.grid-col     { display:flex; flex-direction:column; overflow-y:auto; min-height:0; }
.vert-divider { background:linear-gradient(to bottom, transparent, rgba(200,155,60,.18), transparent); }

/* ── Section titles ───────────────────────────────────── */
.section-title {
  display:flex; align-items:center; gap:7px;
  font-family:'Rajdhani','Inter',sans-serif; font-size:.6rem; font-weight:600;
  letter-spacing:.25em; text-transform:uppercase; color:rgba(200,155,60,.45);
  padding-bottom:8px; border-bottom:1px solid rgba(200,155,60,.08);
  margin-bottom:8px; flex-shrink:0;
}
.title-diamond { width:4px; height:4px; background:#C89B3C; transform:rotate(45deg); flex-shrink:0; opacity:.6; }

/* ── Match rows ───────────────────────────────────────── */
.match-skeleton { height:48px; border-radius:2px; background:rgba(200,155,60,.04); animation:pulse 1.5s ease infinite; }

.match-row {
  display:flex; align-items:center; gap:10px;
  padding: 6px 10px 6px 12px;
  position:relative; overflow:hidden;
  border-radius:0 3px 3px 0;
  border: 1px solid rgba(200,155,60,.06); border-left:none;
  transition: background .15s ease;
}
.row-win  { background:rgba(39,174,96,.04); }
.row-win:hover  { background:rgba(39,174,96,.07); }
.row-loss { background:rgba(192,57,43,.04); }
.row-loss:hover { background:rgba(192,57,43,.07); }

.match-bar { position:absolute; left:0; top:0; bottom:0; width:3px; }
.bar-win   { background:rgba(39,174,96,.7); }
.bar-loss  { background:rgba(192,57,43,.55); }

/* Champion portrait in match row */
.champ-portrait-wrap {
  width: 38px; height: 38px; flex-shrink:0; position:relative;
  border-radius: 3px; overflow: hidden;
  border: 1px solid rgba(200,155,60,.25);
}
.champ-portrait {
  width:100%; height:100%; object-fit:cover; object-position: center top;
  display: block;
}
.champ-portrait-fallback {
  width:100%; height:100%;
  background:rgba(28,42,58,.9);
  align-items:center; justify-content:center;
  font-weight:700; font-size:.8rem; color:#C89B3C;
}

.match-champ    { font-family:'Rajdhani','Inter',sans-serif; font-weight:600; font-size:.78rem; color:#C8AA6E; line-height:1.2; }
.match-kda-line { font-family:monospace; font-size:.7rem; display:flex; align-items:center; gap:1px; }
.kda-sep        { color:rgba(232,224,208,.2); margin:0 2px; }
.kda-ratio      { margin-left:6px; color:rgba(200,155,60,.45); font-size:.65rem; }

.match-badge { font-family:'Rajdhani','Inter',sans-serif; font-size:.6rem; font-weight:700; letter-spacing:.08em; padding:2px 6px; border-radius:2px; flex-shrink:0; }
.badge-win   { color:rgba(39,174,96,.9); background:rgba(39,174,96,.12); border:1px solid rgba(39,174,96,.25); }
.badge-loss  { color:rgba(192,57,43,.8); background:rgba(192,57,43,.1); border:1px solid rgba(192,57,43,.2); }
.match-meta  { margin-left:auto; font-family:monospace; font-size:.65rem; color:rgba(232,224,208,.28); white-space:nowrap; }

/* ── Champion rows ────────────────────────────────────── */
.champ-row {
  display:flex; align-items:center; gap:10px;
  padding: 5px 0; border-bottom:1px solid rgba(200,155,60,.06);
}
.champ-icon-wrap {
  width:36px; height:36px; flex-shrink:0; border-radius:3px; overflow:hidden;
  border:1px solid rgba(200,155,60,.3);
}
.champ-icon-img {
  width:100%; height:100%; object-fit:cover; object-position: center top; display:block;
}
.champ-icon-fallback {
  width:100%; height:100%; background:rgba(28,42,58,.8);
  align-items:center; justify-content:center;
  font-weight:700; font-size:.7rem; color:#C89B3C;
}
.champ-name  { font-family:'Rajdhani','Inter',sans-serif; font-weight:600; font-size:.78rem; color:rgba(200,170,110,.85); }
.wr-bar-bg   { width:44px; height:3px; background:rgba(200,155,60,.1); border-radius:2px; flex-shrink:0; }
.wr-bar-fill { height:100%; border-radius:2px; transition:width .4s ease; }
.fill-win    { background:rgba(39,174,96,.65); }
.fill-loss   { background:rgba(192,57,43,.5); }

/* ── Ornament separator ───────────────────────────────── */
.ornament-sep { display:flex; align-items:center; }
.sep-line     { flex:1; height:1px; background:linear-gradient(to right,transparent,rgba(200,155,60,.25),transparent); }
.sep-diamond  { width:5px; height:5px; background:rgba(200,155,60,.4); transform:rotate(45deg); margin:0 8px; flex-shrink:0; }

/* ── Analysis ─────────────────────────────────────────── */
.insight-header { font-family:'Rajdhani','Inter',sans-serif; font-size:.58rem; font-weight:600; letter-spacing:.2em; text-transform:uppercase; }
.insight-item   { display:flex; align-items:flex-start; gap:7px; }
.bullet         { width:4px; height:4px; border-radius:50%; flex-shrink:0; margin-top:6px; }
.priority-box   { padding:10px 12px; border:1px solid rgba(200,155,60,.18); background:rgba(200,155,60,.04); clip-path:polygon(8px 0%,100% 0%,calc(100% - 8px) 100%,0% 100%); }
.priority-label { font-family:'Rajdhani','Inter',sans-serif; font-size:.55rem; letter-spacing:.2em; text-transform:uppercase; color:rgba(200,155,60,.45); margin-bottom:4px; }

/* ── Empty state ──────────────────────────────────────── */
.empty-col { flex:1; display:flex; align-items:center; justify-content:center; padding:20px 0; }

@keyframes spin { to { transform: rotate(360deg); } }
.spin { display:inline-block; animation: spin .7s linear infinite; }

/* ── SpellCoach meta row ──────────────────────────────────── */
.meta-row {
  display:flex; align-items:center; gap:4px;
  margin-top: 3px; font-family:monospace; font-size:.6rem;
  padding: 2px 5px;
  background: rgba(10,30,60,.5);
  border-left: 2px solid rgba(100,150,220,.3);
}
.meta-tag    { font-family:'Rajdhani','Inter',sans-serif; font-size:.52rem; font-weight:700; letter-spacing:.14em; color:rgba(100,160,255,.5); background:rgba(100,150,220,.1); padding:0 4px; border-radius:1px; }
.meta-val    { color:rgba(200,210,255,.45); }
.meta-sep    { color:rgba(200,155,60,.2); }
.meta-patch  { color:rgba(200,155,60,.25); font-size:.55rem; }
.meta-loading { color:rgba(200,155,60,.2); font-size:.58rem; letter-spacing:.05em; }

/* ── Version ──────────────────────────────────────────── */
.version { position:absolute; bottom:5px; right:20px; font-size:8px; font-family:monospace; letter-spacing:.3em; color:rgba(200,155,60,.15); }
</style>
