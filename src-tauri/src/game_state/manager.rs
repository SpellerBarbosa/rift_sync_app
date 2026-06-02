// ============================================================
// game_state/manager.rs — Loop de detecção de mudança de fase
//
// Fonte primária: LCU /lol-gameflow/v1/gameflow-phase (a cada 2s).
// Fonte secundária: porta 2999 (Live Client Data API) — detecta
//   partidas e replays mesmo quando o LCU não está conectado ou
//   retorna uma fase desconhecida.
//
// Emite `game_state_changed` ao frontend somente em transições reais.
// ============================================================

use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::game_state::states::GamePhase;
use crate::lcu::client::LcuClient;

/// Tenta um GET rápido na porta 2999 (Live Client Data API).
/// Retorna `true` se a partida (ou replay) está ativa.
/// Ignora certificado TLS — mesmo self-signed da Riot.
async fn probe_port_2999() -> bool {
    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(400))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    client
        .get("https://127.0.0.1:2999/liveclientdata/activeplayername")
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Loop de polling do estado do jogo.
/// Executa continuamente em background.
///
/// Deve ser chamado em `tauri::async_runtime::spawn()`.
pub async fn run_game_state_loop(
    app: AppHandle,
    lcu_client: Arc<Mutex<Option<LcuClient>>>,
    current_phase: Arc<Mutex<GamePhase>>,
) {
    let mut poll_interval = Duration::from_secs(2);

    loop {
        sleep(poll_interval).await;

        // ── Fonte primária: LCU ───────────────────────────────
        let lcu_phase: Option<GamePhase> = {
            let client_guard = lcu_client.lock().await;
            if let Some(client) = client_guard.as_ref() {
                match client.get_gameflow_phase().await {
                    Ok(phase_str) => {
                        let mapped = GamePhase::from_lcu_str(&phase_str);
                        if mapped.is_none() {
                            tracing::warn!("[GameState] fase LCU desconhecida: '{phase_str}' — verificando porta 2999");
                        }
                        mapped
                    }
                    Err(e) => {
                        tracing::debug!("[GameState] falha ao consultar LCU: {e}");
                        None
                    }
                }
            } else {
                None
            }
        };

        // ── Fonte secundária: porta 2999 ──────────────────────
        // Usada quando o LCU não está conectado ou retornou fase
        // desconhecida — cobre replays e casos de arranque tardio.
        let effective_phase: Option<GamePhase> = match &lcu_phase {
            Some(p) if p.is_in_game() => Some(p.clone()),
            _ => {
                // LCU disse "não está em jogo" ou está desconectado;
                // confirma via porta 2999 antes de aceitar.
                if probe_port_2999().await {
                    tracing::info!("[GameState] porta 2999 respondeu — forçando INGAME (replay ou live sem LCU)");
                    Some(GamePhase::InGame)
                } else {
                    lcu_phase
                }
            }
        };

        let Some(new_phase) = effective_phase else { continue };

        // ── Emite apenas em mudanças reais ────────────────────
        let mut phase_guard = current_phase.lock().await;
        if new_phase != *phase_guard {
            tracing::info!("[GameState] {} → {}", phase_guard.as_str(), new_phase.as_str());
            *phase_guard = new_phase.clone();
            drop(phase_guard);

            app.emit("game_state_changed", new_phase.as_str()).ok();

            poll_interval = if new_phase.is_in_game() || new_phase.is_in_champ_select() {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(2)
            };
        }
    }
}
