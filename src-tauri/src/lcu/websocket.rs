// ============================================================
// lcu/websocket.rs — WebSocket de eventos em tempo real do LCU
//
// O LCU expõe um WebSocket que emite eventos de mudança de
// estado sem necessidade de polling HTTP.
//
// Protocolo: wss://127.0.0.1:{port}
// Subscribe: [5, "OnJsonApiEvent_{endpoint}"]
// ============================================================

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use futures_util::{SinkExt, StreamExt};
use native_tls::TlsConnector;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio_tungstenite::{
    connect_async_tls_with_config,
    tungstenite::{http::Request, protocol::Message},
    Connector,
};

use super::lockfile::LockfileData;
use crate::game_state::states::GamePhase;

/// Inicia o listener de eventos WebSocket do LCU.
/// Emite `game_state_changed` para o frontend quando a phase muda.
///
/// Deve ser chamado em `tauri::async_runtime::spawn()`.
pub async fn start_event_listener(app: AppHandle, lockfile: LockfileData) -> Result<()> {
    // TLS connector com bypass de certificado — igual ao cliente HTTP
    let tls = TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .context("Falha ao criar TLS connector para WebSocket")?;

    // Monta request com header de autenticação Basic
    let auth = B64.encode(format!("riot:{}", lockfile.token));
    let ws_url = format!("wss://127.0.0.1:{}/", lockfile.port);

    let request = Request::builder()
        .uri(&ws_url)
        .header("Authorization", format!("Basic {auth}"))
        .body(())
        .context("Falha ao montar request WebSocket")?;

    let (mut ws, _) = connect_async_tls_with_config(
        request,
        None,
        false,
        Some(Connector::NativeTls(tls)),
    )
    .await
    .context("Falha ao conectar WebSocket ao LCU")?;

    tracing::info!("WebSocket LCU conectado em {ws_url}");

    // Subscreve ao evento de mudança de phase do gameflow
    let subscribe = serde_json::json!([5, "OnJsonApiEvent_lol-gameflow_v1_gameflow-phase"])
        .to_string();
    ws.send(Message::Text(subscribe))
        .await
        .context("Falha ao enviar subscribe")?;

    // Loop de processamento de mensagens
    while let Some(result) = ws.next().await {
        match result {
            Ok(Message::Text(text)) => {
                handle_event(&app, &text);
            }
            Ok(Message::Close(_)) => {
                tracing::info!("WebSocket LCU encerrado pelo servidor.");
                break;
            }
            Err(e) => {
                tracing::warn!("Erro no WebSocket LCU: {e}");
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Parseia uma mensagem de evento do LCU e emite o evento Tauri correspondente.
/// Formato Riot: [opcode, "event_name", { data: "value", ... }]
fn handle_event(app: &AppHandle, text: &str) {
    let Ok(json) = serde_json::from_str::<Value>(text) else {
        return;
    };

    let Some(arr) = json.as_array() else { return };
    if arr.len() < 3 { return; }

    // O campo "data" contém a fase como string (ex: "InProgress")
    if let Some(phase_str) = arr[2]["data"].as_str() {
        if let Some(phase) = GamePhase::from_lcu_str(phase_str) {
            tracing::debug!("WS LCU → game phase: {}", phase.as_str());
            app.emit("game_state_changed", phase.as_str()).ok();
        }
    }
}
