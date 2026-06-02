// ============================================================
// groq/client.rs — Cliente da API Groq para análise pós-game
//
// Tipos usados pelos testes de integração (tests/groq_integration.rs)
// e pelas fases 8-9 do blueprint (PostGame + Perfil Comportamental).
//
// Implementação real planejada para a Fase 8.
// ============================================================

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::db::models::PlayerPattern;

// ── Contexto de jogo enviado ao Groq ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DragonCount {
    pub ally:  u8,
    pub enemy: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternSummary {
    pub aggression:       f64,
    pub ward_score:       f64,
    pub objective_ctrl:   f64,
    pub avg_deaths_10_15: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameContext {
    pub game_time_secs:  u32,
    pub player_role:     String,
    pub champion_name:   Option<String>,
    pub game_phase:      String,
    pub is_ahead:        bool,
    pub ally_score:      u8,
    pub enemy_score:     u8,
    pub dragon_count:    DragonCount,
    pub recent_events:   Vec<String>,
    pub player_patterns: Option<PatternSummary>,
}

// ── Respostas do Groq ─────────────────────────────────────────

/// Alerta de coaching gerado pelo Groq para situações complexas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroqAlert {
    pub category: String,  // OBJECTIVE | VISION | MACRO | TRADE | POSITIONING
    pub severity: String,  // INFO | WARNING | CRITICAL
    pub message:  String,  // Máximo 15 palavras, acionável
    pub reason:   String,  // Justificativa interna do modelo
}

/// Análise comportamental do jogador gerada pelo Groq.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInsights {
    pub playstyle:     String,       // "aggressive" | "passive" | "balanced"
    pub strengths:     Vec<String>,  // 1-3 pontos fortes detectados
    pub weaknesses:    Vec<String>,  // 1-3 padrões negativos
    pub priority_tip:  String,       // Foco principal para a próxima sessão
    pub progress_note: String,       // Comentário sobre evolução recente
}

// ── Cliente ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GroqClient {
    http:    Client,
    api_key: String,
}

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const MODEL:        &str = "llama-3.3-70b-versatile";

impl GroqClient {
    pub fn new(api_key: String) -> Result<Self> {
        if api_key.is_empty() {
            bail!("GROQ_API_KEY não pode ser vazia");
        }
        let http = Client::builder()
            .build()
            .context("Falha ao criar HTTP client para Groq")?;
        Ok(Self { http, api_key })
    }

    /// Gera alertas de coaching situacionais com base no contexto da partida.
    /// Usado no pós-game para identificar momentos-chave perdidos (Fase 8).
    pub async fn get_coaching_alerts(&self, ctx: &GameContext) -> Result<Vec<GroqAlert>> {
        let system = "Você é um coach de League of Legends. \
            Analise o contexto e retorne APENAS um JSON com chave \"alerts\": array de objetos com \
            category (OBJECTIVE|VISION|MACRO|TRADE|POSITIONING), severity (INFO|WARNING|CRITICAL), \
            message (máx 15 palavras, português, acionável), reason (justificativa). \
            Máximo 2 alertas. Retorne [] se não há urgência real.";

        let user = format!(
            "Role: {} | Campeão: {} | Tempo: {}s | Placar: {}-{} | \
             Drakes: aliado {} inimigo {} | Na frente: {} | \
             Eventos recentes: {} | \
             Padrões: {}",
            ctx.player_role,
            ctx.champion_name.as_deref().unwrap_or("Desconhecido"),
            ctx.game_time_secs,
            ctx.ally_score, ctx.enemy_score,
            ctx.dragon_count.ally, ctx.dragon_count.enemy,
            ctx.is_ahead,
            ctx.recent_events.join("; "),
            ctx.player_patterns.as_ref().map_or(
                "sem dados".to_string(),
                |p| format!(
                    "agressividade={:.1} ward={:.1} obj={:.1} mortes10-15={:.1}",
                    p.aggression, p.ward_score, p.objective_ctrl, p.avg_deaths_10_15
                )
            ),
        );

        let body = self.chat_request(system, &user).await?;
        let alerts_val = body["choices"][0]["message"]["content"]
            .as_str()
            .context("Groq retornou resposta vazia")?;

        let parsed: serde_json::Value = serde_json::from_str(alerts_val)
            .context("Groq retornou JSON inválido")?;

        let alerts: Vec<GroqAlert> = serde_json::from_value(
            parsed["alerts"].clone()
        ).context("Campo 'alerts' ausente ou malformado")?;

        Ok(alerts)
    }

    /// Analisa o perfil comportamental do jogador com base em partidas históricas.
    /// Usado na tela de Perfil e no onboarding personalizado (Fase 9).
    pub async fn analyze_player_profile(
        &self,
        pattern:          &PlayerPattern,
        matches_summary:  &str,
    ) -> Result<PlayerInsights> {
        let system = "Você é um coach de League of Legends especializado em perfil comportamental. \
            Analise os dados e retorne APENAS um JSON com: \
            playstyle (aggressive|passive|balanced), \
            strengths (array 1-3 strings), \
            weaknesses (array 1-3 strings), \
            priority_tip (string), \
            progress_note (string). \
            Todas as strings em português.";

        let user = format!(
            "Dados de padrão comportamental:\n\
             - Agressividade: {:.2}\n\
             - Ward score: {:.2}\n\
             - Controle de objetivos: {:.2}\n\
             - Mortes sem visão (total): {}\n\
             - Média mortes min 10-15: {:.1}\n\
             - Domínio de lane: {:.2}\n\
             - Frequência de roam: {:.2}\n\
             - Eficiência de TP: {:.2}\n\n\
             Resumo das últimas partidas:\n{}",
            pattern.aggression_score,
            pattern.ward_score,
            pattern.objective_control,
            pattern.deaths_without_vision,
            pattern.avg_deaths_10_15,
            pattern.lane_dominance,
            pattern.roam_frequency,
            pattern.tp_efficiency,
            matches_summary,
        );

        let body = self.chat_request(system, &user).await?;
        let content = body["choices"][0]["message"]["content"]
            .as_str()
            .context("Groq retornou resposta vazia")?;

        let insights: PlayerInsights = serde_json::from_str(content)
            .context("Groq retornou JSON de perfil inválido")?;

        Ok(insights)
    }

    /// Análise pós-game: resume a última partida + padrões históricos.
    /// Retorna JSON com summary, strengths, improvements e focus.
    pub async fn analyze_post_game(
        &self,
        champion:       &str,
        role:           &str,
        result:         &str,
        kda:            f64,
        kills:          i64,
        deaths:         i64,
        assists:        i64,
        cs_per_min:     f64,
        vision_score:   i64,
        duration_min:   i64,
        alerts_text:    &str,
        pattern_text:   &str,
        recent_matches: &str,
    ) -> Result<serde_json::Value> {
        let system = "Você é um coach de League of Legends especializado em análise pós-game. \
            Analise os dados da partida e retorne APENAS um JSON com: \
            summary (string, 1-2 frases diretas sobre o desempenho), \
            strengths (array de 1-2 strings com pontos positivos concretos), \
            improvements (array de 1-2 strings com áreas prioritárias), \
            focus (string, UMA instrução concreta e acionável para a próxima partida). \
            Use português, seja direto e específico.";

        let user = format!(
            "Partida: {champion} ({role}) | {result} | KDA {kda:.1} ({kills}/{deaths}/{assists}) \
             | {cs_per_min:.1} cs/min | Visão: {vision_score} | {duration_min}min\n\
             Alertas gerados no jogo: {alerts_text}\n\
             Padrões históricos: {pattern_text}\n\
             Últimas partidas:\n{recent_matches}",
        );

        self.chat_request(system, &user).await
    }

    // ── Helpers ───────────────────────────────────────────────

    async fn chat_request(&self, system: &str, user: &str) -> Result<serde_json::Value> {
        let payload = serde_json::json!({
            "model": MODEL,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user",   "content": user   },
            ],
            "temperature": 0.3,
            "max_tokens":  512,
            "response_format": { "type": "json_object" },
        });

        let resp = self.http
            .post(GROQ_API_URL)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .context("Falha ao contactar a API Groq")?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await
            .context("Resposta Groq não é JSON válido")?;

        if !status.is_success() {
            let msg = body["error"]["message"]
                .as_str()
                .unwrap_or("erro desconhecido");
            bail!("Groq API retornou {status}: {msg}");
        }

        Ok(body)
    }
}
