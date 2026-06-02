// ============================================================
// Store: player — Dados do jogador atual
// Busca e persiste informações do summoner via LCU API
// ============================================================
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Player {
  id:             number
  puuid:          string
  riotName:       string
  tag:            string
  region:         string
  role:           string | null
  rank:           string | null   // pt-BR ex: "Ouro II"
  lp:             number | null
  winrate:        number | null
  profileIconId:  number          // ID do ícone DDragon
  summonerLevel:  number          // Nível do invocador
  tierEn:         string | null   // Tier inglês ex: "GOLD"
}


export const usePlayerStore = defineStore('player', () => {
  // ── Estado ─────────────────────────────────────────────────
  const currentPlayer = ref<Player | null>(null)
  const isLoading     = ref(false)
  const error         = ref<string | null>(null)

  // ── Getters ────────────────────────────────────────────────
  /** Nome exibido — só o nome do invocador, sem a tag.
   *  Remove também qualquer "#tag" que tenha sido salvo no cache antigo. */
  const displayName = computed(() => {
    const name = currentPlayer.value?.riotName ?? 'Desconhecido'
    return name.includes('#') ? name.split('#')[0] : name
  })

  /** Riot ID completo para contextos onde a tag é necessária (ex: card do invocador). */
  const fullRiotId = computed(() => {
    if (!currentPlayer.value) return ''
    const name = displayName.value
    const tag  = currentPlayer.value.tag
    return tag ? `${name}#${tag}` : name
  })

  const rankDisplay = computed(() => {
    if (!currentPlayer.value?.rank) return 'Sem ranking'
    const lp = currentPlayer.value.lp != null ? ` — ${currentPlayer.value.lp} LP` : ''
    return `${currentPlayer.value.rank}${lp}`
  })

  /** URL do ícone de perfil no Data Dragon.
   *  Requer a versão do patch — passada como parâmetro para não
   *  criar dependência circular com o dashboardStore. */
  /** URL do ícone de perfil no Data Dragon.
   *  Usa o ícone 29 (genérico) como fallback quando o ID ainda não foi sincronizado. */
  function profileIconUrl(ddVersion: string): string {
    const id = currentPlayer.value?.profileIconId || 29
    const v  = ddVersion || '14.24.1'
    return `https://ddragon.leagueoflegends.com/cdn/${v}/img/profileicon/${id}.png`
  }

  // Mapeamento do rank em pt-BR para o tier em inglês (fallback para cache antigo)
  const PT_TO_TIER: Record<string, string> = {
    'Ferro': 'iron', 'Bronze': 'bronze', 'Prata': 'silver', 'Ouro': 'gold',
    'Platina': 'platinum', 'Esmeralda': 'emerald', 'Diamante': 'diamond',
    'Mestre': 'master', 'GrãoMestre': 'grandmaster', 'Desafiante': 'challenger',
  }

  /** URL do emblema de elo via CommunityDragon.
   *  Usa tierEn quando disponível; caso contrário deriva do rank em pt-BR. */
  const rankEmblemUrl = computed(() => {
    let tier = currentPlayer.value?.tierEn?.toLowerCase()

    if (!tier && currentPlayer.value?.rank) {
      const firstWord = currentPlayer.value.rank.split(' ')[0]
      tier = PT_TO_TIER[firstWord]
    }

    if (!tier) return ''
    return `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/ranked-emblem/emblem-${tier}.png`
  })

  /** Classe CSS da borda do avatar baseada no nível do invocador. */
  const levelBorderClass = computed(() => {
    const lvl = currentPlayer.value?.summonerLevel ?? 0
    if (lvl >= 300) return 'avatar-border--prestige'
    if (lvl >= 200) return 'avatar-border--diamond'
    if (lvl >= 100) return 'avatar-border--gold'
    if (lvl >= 30)  return 'avatar-border--silver'
    return                  'avatar-border--default'
  })

  // ── Actions ────────────────────────────────────────────────

  async function fetchLastPlayer() {
    if (currentPlayer.value) return
    isLoading.value = true
    try {
      currentPlayer.value = await invoke<Player>('get_last_player')
    } catch {
      // Nenhum jogador no banco ainda
    } finally {
      isLoading.value = false
    }
  }

  async function fetchCurrentPlayer() {
    isLoading.value = true
    error.value = null
    try {
      currentPlayer.value = await invoke<Player>('get_current_player')
    } catch (e) {
      error.value = e as string
      currentPlayer.value = null
    } finally {
      isLoading.value = false
    }
  }

  return {
    currentPlayer,
    isLoading,
    error,
    displayName,
    fullRiotId,
    rankDisplay,
    profileIconUrl,
    rankEmblemUrl,
    levelBorderClass,
    fetchCurrentPlayer,
    fetchLastPlayer,
  }
})
