// ============================================================
// Service: lcu — Wrapper dos Tauri commands do módulo LCU
// Camada entre os stores/pages e o invoke() do Tauri
// ============================================================
import { invoke } from '@tauri-apps/api/core'

/** Retorna se o cliente LCU está conectado. */
export function getLcuConnectionStatus(): Promise<boolean> {
  return invoke<boolean>('get_lcu_connection_status')
}

/** Retorna a fase atual do gameflow (string bruta do LCU). */
export function getGameflowPhase(): Promise<string> {
  return invoke<string>('get_gameflow_phase')
}

/** Retorna os dados do summoner atualmente logado. */
export function getCurrentSummoner(): Promise<unknown> {
  return invoke('get_current_summoner')
}

/** Retorna a sessão atual de champ select (dados da seleção). */
export function getChampSelectSession(): Promise<unknown> {
  return invoke('get_champ_select_session')
}
