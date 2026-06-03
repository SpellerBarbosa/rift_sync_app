// ============================================================
// db/models.rs — Structs que espelham o schema do banco
//
// Todos os campos são serializáveis (Serde) para que possam
// ser retornados diretamente por Tauri commands ao frontend.
// ============================================================

use serde::{Deserialize, Serialize};

/// Jogador monitorado — tabela `players`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id:              i64,
    pub puuid:           String,
    pub riot_name:       String,
    pub tag:             String,
    pub region:          String,
    pub role:            Option<String>,
    pub rank:            Option<String>,    // pt-BR formatado ex: "Ouro II"
    pub lp:              Option<i64>,
    pub winrate:         Option<f64>,
    pub profile_icon_id: i64,               // ID do ícone de perfil (DDragon)
    pub summoner_level:  i64,               // Nível do invocador (para borda)
    pub tier_en:         Option<String>,    // Tier em inglês ex: "GOLD" (para emblem URL)
}

/// Partida registrada — tabela `matches`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub id: i64,
    pub player_id: i64,
    pub match_id: String,
    pub champion_id: i64,
    pub champion_name: String,
    pub role: Option<String>,
    pub result: String,
    pub kills: Option<i64>,
    pub deaths: Option<i64>,
    pub assists: Option<i64>,
    pub cs: Option<i64>,
    pub cs_per_min: Option<f64>,
    pub vision_score: Option<i64>,
    pub damage_dealt: Option<i64>,
    pub gold_earned: Option<i64>,
    pub duration: Option<i64>,
    pub played_at: Option<String>,
}

/// Padrões comportamentais — tabela `player_patterns`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPattern {
    pub id: i64,
    pub player_id: i64,
    pub aggression_score: f64,
    pub deaths_without_vision: i64,
    pub avg_deaths_10_15: f64,
    pub lane_dominance: f64,
    pub objective_control: f64,
    pub roam_frequency: f64,
    pub tp_efficiency: f64,
    pub ward_score: f64,
}

/// Alerta de coaching — tabela `coaching_sessions`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachingSession {
    pub id: i64,
    pub match_id: String,
    pub timestamp: i64,
    pub category: String,
    pub severity: String,
    pub message: String,
    pub was_heard: bool,
}

/// Dado de campeão em cache — tabela `champion_cache`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionCache {
    pub champion_id: i64,
    pub name: String,
    pub key: String,
    pub data_json: String,
}

// ── Structs de uso no frontend (não necessariamente no banco) ──

/// Campeão simplificado para exibição na UI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Champion {
    pub id: i64,
    pub name: String,
    pub key: String,
    pub title: String,
}

/// Análise de matchup retornada pela SpellCoach API (Fase 4)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionMatchup {
    pub difficulty_score: u8,  // 1–10
    pub is_favorable: bool,
    pub main_tip: String,
    pub power_spikes: Vec<String>,
}

/// Estatística de meta de campeão — tabela `champion_meta_stats`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionMetaStat {
    pub champion_id:              i64,
    pub role:                     String,
    pub tier:                     String,
    pub win_rate:                 f64,
    pub wins:                     i64,
    pub losses:                   i64,
    pub total_games:              i64,
    pub avg_kills:                f64,
    pub avg_deaths:               f64,
    pub avg_assists:              f64,
    pub avg_kda:                  f64,
    pub avg_damage_taken:         f64,
    pub avg_damage_to_champions:  f64,
    pub avg_damage_to_objectives: f64,
    pub avg_vision_score:         f64,
    pub avg_wards_placed:         f64,
    pub avg_wards_killed:         f64,
    pub avg_control_wards_bought: f64,
    pub pick_rate:                f64,
    pub power_phase_early:        f64,
    pub power_phase_mid:          f64,
    pub power_phase_late:         f64,
}

/// Alerta de coaching emitido em tempo real via evento Tauri
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachAlert {
    pub id: String,            // UUID v4
    pub tip_id: String,        // ID estático da regra (ex: "kill_ally") — usado para calibração
    pub category: String,      // AlertCategory
    pub severity: String,      // AlertSeverity
    pub message: String,
    pub timestamp: i64,        // segundo da partida (0 se fora de jogo)
}
