// ============================================================
// commands/game_state.rs — Commands relacionados ao estado do jogo
// ============================================================

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

/// Estado atual do pick do jogador local no champ select.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPlayerPick {
    pub champion_id:       i64,
    pub assigned_position: String,
    pub spell1_id:         i64,
    pub spell2_id:         i64,
    pub is_locked_in:      bool,
    pub pick_intent:       i64,
}

/// Retorna a fase atual do jogo como string (ex: "INGAME", "IDLE").
/// Usado pelo frontend para sincronizar o estado ao iniciar o app.
#[tauri::command]
pub async fn get_game_phase(state: State<'_, AppState>) -> Result<String, String> {
    let phase = state.game_phase.lock().await;
    Ok(phase.as_str().to_string())
}

/// Retorna `true` se o cliente League of Legends está conectado.
#[tauri::command]
pub async fn get_lcu_connection_status(state: State<'_, AppState>) -> Result<bool, String> {
    let client = state.lcu_client.lock().await;
    Ok(client.is_some())
}

/// Retorna a fase bruta do gameflow direto da LCU API.
/// Útil para debug ou sincronização forçada.
#[tauri::command]
pub async fn get_gameflow_phase(state: State<'_, AppState>) -> Result<String, String> {
    let client_guard = state.lcu_client.lock().await;
    let client = client_guard
        .as_ref()
        .ok_or("LCU não conectado")?;

    client
        .get_gameflow_phase()
        .await
        .map_err(|e| e.to_string())
}

/// Força a conexão com o LCU lendo o lockfile e testando a API.
/// Retorna a fase atual ou um erro detalhado que mostre onde falhou.
#[tauri::command]
pub async fn force_lcu_connect(state: State<'_, AppState>) -> Result<String, String> {
    use crate::lcu::{lockfile, client::LcuClient};

    let lockfile_data = lockfile::read_lockfile()
        .map_err(|e| format!("Lockfile não encontrado: {e}"))?;

    let client = LcuClient::from_lockfile(&lockfile_data)
        .map_err(|e| format!("Falha ao criar cliente HTTP: {e}"))?;

    let phase = client
        .get_gameflow_phase()
        .await
        .map_err(|e| format!("API LCU inacessível (porta {}): {e}", lockfile_data.port))?;

    let mut guard = state.lcu_client.lock().await;
    *guard = Some(client);

    Ok(phase)
}

/// Retorna dados brutos da sessão de champ select (para a Fase 4).
#[tauri::command]
pub async fn get_champ_select_session(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let client_guard = state.lcu_client.lock().await;
    let client = client_guard
        .as_ref()
        .ok_or("LCU não conectado")?;

    client
        .get_champ_select_session()
        .await
        .map_err(|e| e.to_string())
}

/// Retorna o estado atual do pick do jogador local no champ select.
/// Lê a sessão LCU e extrai a entrada correspondente ao `localPlayerCellId`.
#[tauri::command]
pub async fn get_local_player_pick(
    state: State<'_, AppState>,
) -> Result<LocalPlayerPick, String> {
    let client_guard = state.lcu_client.lock().await;
    let client = client_guard
        .as_ref()
        .ok_or("LCU não conectado")?;

    let session = client
        .get_champ_select_session()
        .await
        .map_err(|e| e.to_string())?;

    let local_cell_id = session["localPlayerCellId"].as_i64().unwrap_or(0);

    let my_team = session["myTeam"]
        .as_array()
        .ok_or("myTeam ausente na sessão de champ select")?;

    let player = my_team
        .iter()
        .find(|p| p["cellId"].as_i64() == Some(local_cell_id))
        .ok_or("Jogador local não encontrado na sessão")?;

    let champion_id = player["championId"].as_i64().unwrap_or(0);

    Ok(LocalPlayerPick {
        champion_id,
        assigned_position: player["assignedPosition"]
            .as_str()
            .unwrap_or("MIDDLE")
            .to_uppercase(),
        spell1_id:    player["spell1Id"].as_i64().unwrap_or(4),
        spell2_id:    player["spell2Id"].as_i64().unwrap_or(14),
        is_locked_in: champion_id > 0,
        pick_intent:  player["championPickIntent"].as_i64().unwrap_or(0),
    })
}
