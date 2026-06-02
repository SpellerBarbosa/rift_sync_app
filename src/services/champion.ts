// ============================================================
// Service: champion — Wrapper dos Tauri commands de campeão
// Usado pelos stores e pages para buscar dados de campeões
// ============================================================
import { invoke } from '@tauri-apps/api/core'
import type { Champion, ChampionMatchup } from '../stores/champion'

export function getChampionData(id: number): Promise<Champion> {
  return invoke<Champion>('get_champion_data', { id })
}

export function getAllChampions(): Promise<Champion[]> {
  return invoke<Champion[]>('get_all_champions')
}

export function getChampionMatchup(
  championId: number,
  enemyId: number
): Promise<ChampionMatchup> {
  return invoke<ChampionMatchup>('get_champion_matchup', { championId, enemyId })
}
