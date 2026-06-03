use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://spellcoachapiv2.vercel.app";

/// Normaliza o nome do campeão para a API SpellCoach (remove espaços).
/// "Lee Sin" → "LeeSin", "Twisted Fate" → "TwistedFate", "Master Yi" → "MasterYi".
fn encode_name(name: &str) -> String {
    name.replace(' ', "")
}

#[derive(Debug, Clone)]
pub struct SpellCoachClient {
    http:    Client,
    api_key: String,
}

// ── Response structs (nova API) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PowerPhases {
    pub early: f64,
    pub mid:   f64,
    pub late:  f64,
}

/// Dados de um campeão retornados pela nova API /api/champions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionData {
    pub champion_id:               i64,
    #[serde(default)]
    pub champion_name:             String,
    pub role:                      String,
    pub tier:                      String,
    pub win_rate:                  f64,
    #[serde(default)]
    pub wins:                      i64,
    #[serde(default)]
    pub losses:                    i64,
    pub total_games:               i64,
    #[serde(default)]
    pub avg_kills:                 f64,
    #[serde(default)]
    pub avg_deaths:                f64,
    #[serde(default)]
    pub avg_assists:               f64,
    #[serde(default)]
    pub avg_kda:                   f64,
    #[serde(default)]
    pub avg_damage_taken:          f64,
    #[serde(default)]
    pub avg_damage_to_champions:   f64,
    #[serde(default)]
    pub avg_damage_to_objectives:  f64,
    #[serde(default)]
    pub avg_vision_score:          f64,
    #[serde(default)]
    pub avg_wards_placed:          f64,
    #[serde(default)]
    pub avg_wards_killed:          f64,
    #[serde(default)]
    pub avg_control_wards_bought:  f64,
    #[serde(default)]
    pub pick_rate:                 f64,
    #[serde(default)]
    pub power_phases:              PowerPhases,
}

/// Wrapper de paginação da resposta de /api/champions.
/// Captura campos de paginação opcionais — a API pode ou não retorná-los.
#[derive(Debug, Deserialize)]
struct ChampionsPage {
    data:        Vec<ChampionData>,
    /// Total de registros disponíveis na API (opcional).
    #[serde(default)]
    total:       Option<u64>,
    /// Indica se há mais páginas (opcional).
    #[serde(default, rename = "hasMore")]
    has_more:    Option<bool>,
    /// Página atual (opcional).
    #[serde(default)]
    page:        Option<u64>,
    /// Registros por página retornados pela API (opcional).
    #[serde(default)]
    limit:       Option<u64>,
}

// ── Struct de matchup (mantida para compatibilidade com o frontend) ──

/// Estatísticas de matchup entre dois campeões.
/// Como a nova API não tem endpoint de matchup, os win rates são
/// os win rates individuais de cada campeão no seu tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchupStats {
    pub champion_a_id: i64,
    pub champion_b_id: i64,
    pub patch:         String,
    pub elo:           String,
    pub games_played:  i64,
    /// Win rate do campeão A × 100 (escala 0-100 para o frontend).
    pub win_rate_a:    f64,
    /// Win rate do campeão B × 100 (escala 0-100 para o frontend).
    pub win_rate_b:    f64,
}

// ── Implementação ─────────────────────────────────────────────

impl SpellCoachClient {
    pub fn new(api_key: String) -> Result<Self> {
        let http = Client::builder()
            .build()
            .context("Falha ao criar HTTP client para SpellCoach")?;
        Ok(Self { http, api_key })
    }

    /// Busca TODOS os campeões sem filtro de role/tier (a API não suporta esses filtros).
    /// Usa paginação limit=1000 — tipicamente 6 requests para ~5500 registros.
    pub async fn get_all_champions(&self) -> Result<Vec<ChampionData>> {
        self.fetch_champions_all(&format!("{BASE_URL}/api/champions")).await
    }

    /// Busca campeões de um role e tier específicos.
    /// Busca todos os dados de uma vez e filtra no cliente (API não suporta filtros).
    pub async fn get_champions(&self, role: &str, tier: &str) -> Result<Vec<ChampionData>> {
        let all = self.get_all_champions().await?;
        Ok(all.into_iter()
            .filter(|c| c.role.eq_ignore_ascii_case(role) && c.tier.eq_ignore_ascii_case(tier))
            .collect())
    }

    /// Compat alias — usa get_all_champions internamente.
    async fn get_all_champions_page(&self) -> Result<Vec<ChampionData>> {
        self.get_all_champions().await
    }

    /// Busca stats de um campeão específico pelo ID.
    /// Pesquisa na lista completa retornada pela API.
    pub async fn get_champion_stats(&self, champion_id: i64) -> Result<ChampionData> {
        let champions = self.get_all_champions_page().await?;
        champions
            .into_iter()
            .find(|c| c.champion_id == champion_id)
            .ok_or_else(|| anyhow::anyhow!("Campeão {champion_id} não encontrado na SpellCoach API"))
    }

    /// Busca stats de um campeão filtrando por role e tier.
    /// Útil para recomendações contextualizadas ao pick do jogador.
    pub async fn get_champion_stats_by_role(
        &self,
        champion_id: i64,
        role: &str,
        tier: &str,
    ) -> Result<Option<ChampionData>> {
        let champions = self.get_champions(role, tier).await?;
        Ok(champions.into_iter().find(|c| c.champion_id == champion_id))
    }

    /// Retorna estatísticas de matchup aproximadas.
    /// Usa os win rates individuais de cada campeão (no mesmo tier).
    /// Win rates retornados em escala 0-100 para consumo direto pelo frontend.
    pub async fn get_matchup(&self, champion_a_id: i64, champion_b_id: i64) -> Result<MatchupStats> {
        let champions = self.get_all_champions_page().await?;

        let champ_a = champions.iter().find(|c| c.champion_id == champion_a_id);
        let champ_b = champions.iter().find(|c| c.champion_id == champion_b_id);

        if champ_a.is_none() && champ_b.is_none() {
            bail!(
                "Nenhum dos campeões ({champion_a_id}, {champion_b_id}) encontrado na SpellCoach API"
            );
        }

        Ok(MatchupStats {
            champion_a_id,
            champion_b_id,
            patch:        "current".to_string(),
            elo:          champ_a.map(|c| c.tier.clone()).unwrap_or_default(),
            games_played: champ_a.map(|c| c.total_games).unwrap_or(0),
            win_rate_a:   champ_a.map(|c| c.win_rate * 100.0).unwrap_or(50.0),
            win_rate_b:   champ_b.map(|c| c.win_rate * 100.0).unwrap_or(50.0),
        })
    }

    // ── Builds ────────────────────────────────────────────────

    /// Retorna a build completa de um campeão (runes + items + skills + spikes).
    pub async fn get_champion_build(&self, champion_name: &str) -> Result<serde_json::Value> {
        let url = format!("{BASE_URL}/api/builds/{champion_name}");
        self.fetch_json(&url).await
    }

    /// Retorna apenas as runas de um campeão.
    pub async fn get_champion_runes(&self, champion_name: &str) -> Result<serde_json::Value> {
        let name = encode_name(champion_name);
        let url = format!("{BASE_URL}/api/builds/{name}/runes");
        self.fetch_json(&url).await
    }

    /// Retorna itens e core builds de um campeão.
    pub async fn get_champion_items(&self, champion_name: &str) -> Result<serde_json::Value> {
        let name = encode_name(champion_name);
        let url = format!("{BASE_URL}/api/builds/{name}/items");
        self.fetch_json(&url).await
    }

    /// Retorna a ordem de habilidades de um campeão.
    pub async fn get_champion_skills(&self, champion_name: &str) -> Result<serde_json::Value> {
        let name = encode_name(champion_name);
        let url = format!("{BASE_URL}/api/builds/{name}/skills");
        self.fetch_json(&url).await
    }

    /// Retorna os power spikes de um campeão.
    pub async fn get_champion_spikes(&self, champion_name: &str) -> Result<serde_json::Value> {
        let name = encode_name(champion_name);
        let url = format!("{BASE_URL}/api/builds/{name}/spikes");
        self.fetch_json(&url).await
    }

    // ── Wards ────────────────────────────────────────────────

    /// Retorna posicionamentos de ward de um campeão filtrando por tier.
    /// Usa CHALLENGER por padrão — maior tier = melhor posicionamento.
    /// Fallback automático se a API não suportar o parâmetro.
    pub async fn get_champion_wards(&self, champion_name: &str) -> Result<serde_json::Value> {
        let name = encode_name(champion_name);
        let url = format!("{BASE_URL}/api/wards/{name}?tier=CHALLENGER");
        match self.fetch_json(&url).await {
            Ok(v) => Ok(v),
            // Se a API não aceitar ?tier=, tenta sem o parâmetro
            Err(_) => self.fetch_json(&format!("{BASE_URL}/api/wards/{name}")).await,
        }
    }

    /// Retorna o heatmap de wards de um campeão (Challenger tier preferido).
    pub async fn get_champion_ward_heatmap(&self, champion_name: &str) -> Result<serde_json::Value> {
        let url = format!("{BASE_URL}/api/wards/{champion_name}/heatmap?tier=CHALLENGER");
        match self.fetch_json(&url).await {
            Ok(v) => Ok(v),
            Err(_) => self.fetch_json(&format!("{BASE_URL}/api/wards/{champion_name}/heatmap")).await,
        }
    }

    /// Retorna dados de ward por role (todos os campeões daquela role, Challenger tier).
    pub async fn get_wards_by_role(&self, role: &str) -> Result<serde_json::Value> {
        let url = format!("{BASE_URL}/api/wards/role/{role}?tier=CHALLENGER");
        match self.fetch_json(&url).await {
            Ok(v) => Ok(v),
            Err(_) => self.fetch_json(&format!("{BASE_URL}/api/wards/role/{role}")).await,
        }
    }

    // ── Helpers privados ──────────────────────────────────────

    /// Faz GET em qualquer URL e retorna o JSON completo da resposta.
    async fn fetch_json(&self, url: &str) -> Result<serde_json::Value> {
        let resp = self
            .http
            .get(url)
            .header("x-api-key", &self.api_key)
            .send()
            .await
            .context("Falha ao acessar SpellCoach API")?;

        let status = resp.status();
        if !status.is_success() {
            bail!("SpellCoach retornou {status} para {url}");
        }

        resp.json::<serde_json::Value>()
            .await
            .context("Resposta inválida da SpellCoach API")
    }

    /// Busca uma única página de campeões pela URL exata.
    async fn fetch_champions_page(&self, url: &str) -> Result<ChampionsPage> {
        let resp = self
            .http
            .get(url)
            .header("x-api-key", &self.api_key)
            .send()
            .await
            .context("Falha ao acessar SpellCoach API")?;

        let status = resp.status();
        if !status.is_success() {
            bail!("SpellCoach retornou {status} para {url}");
        }

        resp.json::<ChampionsPage>()
            .await
            .context("Resposta inválida da SpellCoach API")
    }

    /// Percorre todas as páginas de campeões e retorna a lista completa.
    ///
    /// Estratégia de paginação:
    ///   1. Solicita `?limit=1000&page=N` — busca 1000 por página
    ///   2. Para quando `hasMore = false`, `total` atingido, ou página vazia
    ///   3. Se a API não suportar paginação, retorna o que a primeira página der
    async fn fetch_champions_all(&self, base_url: &str) -> Result<Vec<ChampionData>> {
        const PAGE_SIZE: u64 = 1000;
        const MAX_PAGES: u64 = 100; // proteção contra loop infinito

        let mut all: Vec<ChampionData> = Vec::new();
        let mut page_num: u64 = 1;

        // Detecta se a URL já tem parâmetros
        let sep = if base_url.contains('?') { '&' } else { '?' };

        loop {
            let url = format!(
                "{base_url}{sep}limit={PAGE_SIZE}&page={page_num}"
            );

            let page = match self.fetch_champions_page(&url).await {
                Ok(p)  => p,
                Err(e) => {
                    if page_num == 1 {
                        // Primeira página falhou — tenta URL sem parâmetros de paginação
                        tracing::warn!("[SpellCoach] paginação não suportada, buscando sem parâmetros: {e}");
                        let fallback = self.fetch_champions_page(base_url).await?;
                        return Ok(fallback.data);
                    }
                    // Páginas seguintes: para silenciosamente
                    tracing::debug!("[SpellCoach] fim de páginas na página {page_num}: {e}");
                    break;
                }
            };

            let count = page.data.len() as u64;
            let has_more = page.has_more.unwrap_or(count >= PAGE_SIZE);
            let total    = page.total;

            tracing::debug!(
                "[SpellCoach] página {page_num}: {} registros (total={:?}, hasMore={:?})",
                count, total, page.has_more
            );

            all.extend(page.data);

            // Condições de parada
            if !has_more || count == 0 || count < PAGE_SIZE {
                break;
            }
            if let Some(t) = total {
                if all.len() as u64 >= t {
                    break;
                }
            }
            if page_num >= MAX_PAGES {
                tracing::warn!("[SpellCoach] limite de {} páginas atingido", MAX_PAGES);
                break;
            }

            page_num += 1;
        }

        tracing::info!("[SpellCoach] total carregado: {} registros (em {} páginas)", all.len(), page_num);
        Ok(all)
    }
}
