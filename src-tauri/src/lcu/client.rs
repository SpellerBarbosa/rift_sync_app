// ============================================================
// lcu/client.rs — HTTP client para a LCU API local
//
// A Riot usa um certificado TLS auto-assinado no endpoint
// localhost — por isso `danger_accept_invalid_certs(true)`.
// Esta configuração é intencionalmente localizada aqui para
// não vazar para outros clientes HTTP da aplicação.
//
// Autenticação: Basic Auth com `riot:{token}` em Base64.
// ============================================================

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::Client;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use super::lockfile::{self, LockfileData};

/// Wrapper do cliente HTTP da LCU API.
/// Criado a partir do lockfile quando o LoL é detectado.
#[derive(Debug, Clone)]
pub struct LcuClient {
    http:     Client,
    base_url: String,
    auth:     String,  // "Basic <base64(riot:token)>"
}

impl LcuClient {
    /// Constrói o cliente a partir dos dados do lockfile.
    pub fn from_lockfile(data: &LockfileData) -> Result<Self> {
        let http = Client::builder()
            // Bypass intencional: certificado self-signed da Riot em localhost
            .danger_accept_invalid_certs(true)
            // Timeout para evitar que chamadas à port 2999 travem o loop do coach
            // durante a tela de carregamento (API ainda não respondendo).
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("Falha ao criar HTTP client")?;

        let base_url = format!("{}://127.0.0.1:{}", data.protocol, data.port);
        let credentials = B64.encode(format!("riot:{}", data.token));
        let auth = format!("Basic {credentials}");

        Ok(Self { http, base_url, auth })
    }

    // ── Métodos de requisição ──────────────────────────────────

    /// GET genérico para qualquer endpoint da LCU.
    pub async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);

        let response = self
            .http
            .get(&url)
            .header("Authorization", &self.auth)
            .send()
            .await
            .with_context(|| format!("LCU GET falhou: {path}"))?;

        let status = response.status();
        if !status.is_success() {
            bail!("LCU retornou {status} para GET {path}");
        }

        response
            .json::<Value>()
            .await
            .with_context(|| format!("Resposta inválida de GET {path}"))
    }

    // ── Endpoints específicos ──────────────────────────────────

    /// Dados do summoner atualmente logado no cliente LoL.
    pub async fn get_current_summoner(&self) -> Result<Value> {
        self.get("/lol-summoner/v1/current-summoner").await
    }

    /// Fase atual do gameflow (string como "ChampSelect", "InProgress", etc.).
    pub async fn get_gameflow_phase(&self) -> Result<String> {
        let json = self.get("/lol-gameflow/v1/gameflow-phase").await?;

        // A Riot retorna a fase como JSON string entre aspas
        json.as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("Formato inesperado na resposta de gameflow-phase"))
    }

    /// Sessão atual de seleção de campeões.
    pub async fn get_champ_select_session(&self) -> Result<Value> {
        self.get("/lol-champ-select/v1/session").await
    }

    /// Lobby atual (dados de sala e tipo de jogo).
    pub async fn get_lobby(&self) -> Result<Value> {
        self.get("/lol-lobby/v2/lobby").await
    }

    /// Estatísticas de ranked do jogador logado.
    pub async fn get_ranked_stats(&self) -> Result<Value> {
        self.get("/lol-ranked/v1/current-ranked-stats").await
    }

    /// Últimas `count` partidas do jogador (histórico local da LCU).
    pub async fn get_match_history(&self, puuid: &str, count: u8) -> Result<Value> {
        self.get(&format!(
            "/lol-match-history/v1/products/lol/{puuid}/matches?begIndex=0&endIndex={count}"
        ))
        .await
    }

    /// Dados ao vivo da partida: jogadores, scores e eventos (drakes, baron, etc.).
    /// Usa a Live Game API do cliente (porta 2999), não a LCU padrão.
    pub async fn get_live_game_stats(&self) -> Result<Value> {
        // A Live Client Data API roda sempre em localhost:2999 sem autenticação
        let url = "https://127.0.0.1:2999/liveclientdata/allgamedata";
        let response = self
            .http
            .get(url)
            .send()
            .await
            .context("Falha ao acessar Live Client Data API")?;

        response
            .json::<Value>()
            .await
            .context("Resposta inválida da Live Client Data API")
    }

    /// Versão do patch atual (ex: "14.10.1") — lida do LCU, sem chamada externa.
    pub async fn get_game_version(&self) -> Result<String> {
        let raw = self.get("/lol-patch/v1/game-version").await?;
        let s = raw.as_str().unwrap_or("14.10.1");
        let trimmed = s.splitn(4, '.').take(3).collect::<Vec<_>>().join(".");
        Ok(trimmed)
    }

    /// Mapa: nome de exibição → chave Data Dragon (alias).
    /// Ex: "Miss Fortune" → "MissFortune", "Wukong" → "MonkeyKing".
    /// Fonte: LCU local (champion-summary.json) — sem chamada externa.
    pub async fn get_champion_name_to_ddragon_key(&self) -> Result<std::collections::HashMap<String, String>> {
        let data = self.get("/lol-game-data/assets/v1/champion-summary.json").await?;
        let mut map = std::collections::HashMap::new();
        if let Some(arr) = data.as_array() {
            for entry in arr {
                if let (Some(name), Some(alias)) = (
                    entry["name"].as_str(),
                    entry["alias"].as_str(),
                ) {
                    if !alias.is_empty() {
                        map.insert(name.to_string(), alias.to_string());
                    }
                }
            }
        }
        Ok(map)
    }

    /// Ícone PNG do campeão via Data Dragon CDN.
    /// Usa a chave DDragon (alias) — ex: "MissFortune", "MonkeyKing".
    /// O minimapa sempre exibe o ícone base independente de skin,
    /// por isso o ícone do Data Dragon é um template confiável.
    pub async fn download_champion_icon(&self, ddragon_key: &str, version: &str) -> Result<Vec<u8>> {
        let url = format!(
            "https://ddragon.leagueoflegends.com/cdn/{version}/img/champion/{ddragon_key}.png"
        );
        let bytes = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Falha ao baixar ícone de {ddragon_key}"))?
            .bytes()
            .await
            .context("Falha ao ler bytes do ícone")?;
        Ok(bytes.to_vec())
    }

    /// Tempo de jogo em segundos — endpoint leve, polled a cada 5s.
    /// Substitui a leitura OCR do timer do HUD.
    pub async fn get_live_game_time(&self) -> Result<f64> {
        let url = "https://127.0.0.1:2999/liveclientdata/gamestats";
        let data: Value = self
            .http
            .get(url)
            .send()
            .await
            .context("Falha ao acessar gamestats")?
            .json()
            .await
            .context("Resposta inválida de gamestats")?;

        data["gameTime"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("gameTime ausente na resposta de gamestats"))
    }

    /// Lista de eventos do jogo (kills, drakes, baron, etc.).
    /// Endpoint leve polled a cada 5s para detectar kills sem OCR.
    pub async fn get_live_events(&self) -> Result<Value> {
        let url = "https://127.0.0.1:2999/liveclientdata/eventdata";
        self.http
            .get(url)
            .send()
            .await
            .context("Falha ao acessar eventdata")?
            .json::<Value>()
            .await
            .context("Resposta inválida de eventdata")
    }
}

// ── Tarefa em background: polling do lockfile ─────────────────

/// Loop assíncrono que detecta abertura/fechamento do LoL,
/// conecta ao LCU e emite eventos Tauri para o frontend.
///
/// Deve ser iniciado em `tauri::async_runtime::spawn()` no setup.
pub async fn start_lcu_polling(app: AppHandle, lcu_state: crate::LcuState) {
    let mut was_connected = false;
    let poll_interval = tokio::time::Duration::from_secs(2);
    // Tracks how long we've been failing to connect after detecting the lockfile.
    let mut connect_attempts: u32 = 0;
    // After this many failed attempts (~30 s) the LCU is unlikely to be just "starting up".
    const WARN_AFTER_ATTEMPTS: u32 = 15;

    loop {
        let is_running = lockfile::is_lol_running();

        if is_running && !was_connected {
            // LoL acabou de abrir — tenta conectar
            match try_connect(&lcu_state).await {
                Ok(_) => {
                    connect_attempts = 0;
                    was_connected = true;
                    app.emit("lcu_connected", true).ok();
                    tracing::info!("LCU conectado.");
                }
                Err(e) => {
                    connect_attempts += 1;
                    // Lockfile appears before the HTTP server is ready; keep quiet
                    // during normal startup and only warn if it takes unusually long.
                    if connect_attempts >= WARN_AFTER_ATTEMPTS {
                        tracing::warn!("Falha ao conectar ao LCU (tentativa {connect_attempts}): {e}");
                    } else {
                        tracing::debug!("LCU ainda não disponível (tentativa {connect_attempts}): {e}");
                    }
                }
            }
        } else if !is_running && was_connected {
            connect_attempts = 0;
            // LoL foi fechado — limpa estado
            was_connected = false;
            let mut client_guard = lcu_state.lock().await;
            *client_guard = None;
            drop(client_guard);

            app.emit("lcu_connected", false).ok();
            app.emit("game_state_changed", "IDLE").ok();
            tracing::info!("LCU desconectado — LoL fechado.");
        } else if !is_running {
            // LoL fechou antes de conectar — zera contador para a próxima abertura
            connect_attempts = 0;
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Lê o lockfile e inicializa o LcuClient no estado compartilhado.
async fn try_connect(lcu_state: &crate::LcuState) -> Result<()> {
    let lockfile_data = lockfile::read_lockfile()?;
    let client = LcuClient::from_lockfile(&lockfile_data)?;

    // Valida a conexão com uma requisição simples
    client.get_gameflow_phase().await?;

    let mut guard = lcu_state.lock().await;
    *guard = Some(client);

    Ok(())
}
