import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface RecentMatch {
  gameId:       number
  championId:   number
  championName: string
  championKey:  string
  kills:        number
  deaths:       number
  assists:      number
  win:          boolean
  durationSecs: number
  gameMode:     string
  cs:           number
  visionScore:  number
  kda:          number
}

export interface ChampionStat {
  id:      number
  name:    string
  key:     string
  games:   number
  wins:    number
  winrate: number
  avgKda:  number
}

export interface ChampionMetaStat {
  championId:          number
  patch:               string
  elo:                 string
  role:                string
  gamesPlayed:         number
  winRate:             number
  kda:                 { kills: number; deaths: number; assists: number; ratio: string }
  averageCsPerMin:     number
  averageGoldPerMin:   number
  averageDamagePerMin: number
  averageVisionScore:  number
}

export interface MatchStats {
  totalGames:   number
  wins:         number
  losses:       number
  winrate:      number
  avgKda:       number
  avgCs:        number
  avgVision:    number
  topChampions: ChampionStat[]
}

export interface DashboardAnalysis {
  playstyle:    string
  strengths:    string[]
  weaknesses:   string[]
  priorityTip:  string
  progressNote: string
}

export const useDashboardStore = defineStore('dashboard', () => {
  const matches         = ref<RecentMatch[]>([])
  const stats           = ref<MatchStats | null>(null)
  const analysis        = ref<DashboardAnalysis | null>(null)
  const metaStats       = ref<Map<number, ChampionMetaStat>>(new Map())
  const ddVersion       = ref('14.24.1') // fallback; atualizado após sync
  const loadingMatches  = ref(false)
  const loadingStats    = ref(false)
  const loadingAnalysis = ref(false)
  const loadingSync     = ref(false)
  const loadingMeta     = ref(false)
  const analysisError   = ref<string | null>(null)
  const syncError       = ref<string | null>(null)
  const hasData         = ref(false)

  /** Carrega partidas do banco local. */
  async function fetchCachedMatches() {
    loadingMatches.value = true
    try {
      matches.value = await invoke<RecentMatch[]>('get_cached_matches')
      if (matches.value.length > 0) hasData.value = true
    } catch {
      matches.value = []
    } finally {
      loadingMatches.value = false
    }
  }

  /** Carrega estatísticas agregadas do banco local. */
  async function fetchMatchStats() {
    loadingStats.value = true
    try {
      stats.value = await invoke<MatchStats>('get_match_stats')
    } catch {
      stats.value = null
    } finally {
      loadingStats.value = false
    }
  }

  /**
   * Sync completo:
   *  1. Data Dragon  → champion_cache (nomes dos campeões)
   *  2. LCU API      → matches (histórico de partidas)
   *  3. SQLite local → atualiza estado reativo
   */
  async function syncAll() {
    loadingSync.value = true
    syncError.value   = null
    try {
      // 1. Campeões do Data Dragon (ignora falha de rede — usa cache existente)
      await invoke('sync_champion_cache').catch(() => {})

      // 2. Lê a versão do DD salva no banco
      ddVersion.value = await invoke<string>('get_setting', { key: 'dd_version' })
        .catch(() => ddVersion.value)

      // 3. Histórico do LCU
      await invoke<number>('sync_match_history')

      // 4. Lê do banco atualizado
      await Promise.all([fetchCachedMatches(), fetchMatchStats()])
    } catch (e) {
      syncError.value = e as string
    } finally {
      loadingSync.value = false
    }
  }

  /** Analisa as partidas via Groq e retorna insights. */
  async function fetchAnalysis(role?: string | null) {
    if (matches.value.length === 0) return
    loadingAnalysis.value = true
    analysisError.value   = null
    try {
      const summary = buildSummary(matches.value.slice(0, 5))
      analysis.value = await invoke<DashboardAnalysis>('get_dashboard_analysis', {
        matchesSummary: summary,
        role: role ?? null,
      })
    } catch (e) {
      analysisError.value = e as string
    } finally {
      loadingAnalysis.value = false
    }
  }

  /** Busca meta stats da SpellCoach para os top campeões do jogador. */
  async function fetchMetaStats() {
    const champs = stats.value?.topChampions
    if (!champs?.length) return
    loadingMeta.value = true
    const results = await Promise.allSettled(
      champs.map(c => invoke<ChampionMetaStat>('get_champion_stats', { championId: c.id }))
    )
    results.forEach((r, i) => {
      if (r.status === 'fulfilled') {
        metaStats.value.set(champs[i].id, r.value)
      }
    })
    loadingMeta.value = false
  }

  function buildSummary(ms: RecentMatch[]): string {
    return ms.map((m, i) => {
      const r  = m.win ? 'Vitória' : 'Derrota'
      const mm = Math.floor(m.durationSecs / 60)
      const ss = String(m.durationSecs % 60).padStart(2, '0')
      return `${i + 1}. ${m.championName} — ${m.kills}/${m.deaths}/${m.assists} (KDA ${m.kda.toFixed(2)}), ${r}, ${mm}:${ss}, CS ${m.cs}`
    }).join('\n')
  }

  function formatDuration(secs: number): string {
    return `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, '0')}`
  }

  function reset() {
    matches.value        = []
    stats.value          = null
    analysis.value       = null
    metaStats.value      = new Map()
    analysisError.value  = null
    syncError.value      = null
    hasData.value        = false
  }

  return {
    matches, stats, analysis, metaStats, hasData, ddVersion,
    loadingMatches, loadingStats, loadingAnalysis, loadingSync, loadingMeta,
    analysisError, syncError,
    fetchCachedMatches, fetchMatchStats, syncAll, fetchAnalysis, fetchMetaStats,
    formatDuration, reset,
  }
})
