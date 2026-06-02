// ============================================================
// Service: coach — Wrapper dos Tauri commands de coaching
// Análise pós-game, perfil do jogador e controle de voz
// ============================================================
import { invoke } from '@tauri-apps/api/core'

/** Retorna a análise da última partida (pós-game). */
export function getPostGameAnalysis(matchId: string): Promise<unknown> {
  return invoke('get_post_game_analysis', { matchId })
}

/** Retorna o perfil comportamental do jogador atual. */
export function getPlayerProfile(): Promise<unknown> {
  return invoke('get_player_profile')
}

/** Define se o sistema de voz está silenciado. */
export function setCoachMuted(muted: boolean): Promise<void> {
  return invoke('set_coach_muted', { muted })
}

/** Dispara sequência de alertas de teste (todas as categorias e severidades). */
export function debugFireReplay(): Promise<void> {
  return invoke('debug_fire_replay')
}

/** Dispara a barra de ward com spots de teste (3 rodadas de 14s cada). */
export function debugFireWard(): Promise<void> {
  return invoke('debug_fire_ward')
}

