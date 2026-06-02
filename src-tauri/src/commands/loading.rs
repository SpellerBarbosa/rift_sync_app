// ============================================================
// commands/loading.rs — Dados da tela de carregamento
//
// Busca os 10 jogadores do /lol-gameflow/v1/session, resolve
// PUUID de cada um via /lol-summoner, busca histórico de
// partidas em paralelo e analisa localmente para gerar tags
// de pontos fortes/fracos por jogador.
// ============================================================

use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

use crate::{lcu::client::LcuClient, AppState};

// ── Tipos públicos ─────────────────────────────────────────────

/// Card de um jogador na tela de carregamento.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingPlayer {
    pub summoner_name: String,
    pub champion_id:   i64,
    pub champion_name: String,
    pub champion_key:  String,
    pub position:      String,
    pub team:          i64,
    pub is_local:      bool,
    pub spell1_id:     i64,
    pub spell2_id:     i64,
    pub tags:          Vec<String>,
    pub ad_pct:        u32,
    pub ap_pct:        u32,
    pub true_pct:      u32,
}

// ── Análise de histórico ────────────────────────────────────────

struct PlayerAnalysis {
    kda:            f64,
    win_rate:       f64,
    games_played:   usize,
    champion_games: usize,
    recent_wins:    usize, // das últimas 3 partidas
    avg_deaths:     f64,
}

impl Default for PlayerAnalysis {
    fn default() -> Self {
        Self {
            kda:            0.0,
            win_rate:       0.0,
            games_played:   0,
            champion_games: 0,
            recent_wins:    0,
            avg_deaths:     0.0,
        }
    }
}

/// Extrai stats do jogador do JSON de histórico do LCU.
fn analyze_history(
    history:     &serde_json::Value,
    puuid:       &str,
    champion_id: i64,
) -> PlayerAnalysis {
    let games = match history["games"]["games"].as_array() {
        Some(g) => g,
        None    => return PlayerAnalysis::default(),
    };

    let mut total_kills   = 0i64;
    let mut total_deaths  = 0i64;
    let mut total_assists = 0i64;
    let mut wins          = 0usize;
    let mut champ_games   = 0usize;
    let mut recent_wins   = 0usize;
    let mut count         = 0usize;

    for (i, game) in games.iter().enumerate() {
        let Some(identities)   = game["participantIdentities"].as_array() else { continue };
        let Some(participants) = game["participants"].as_array()           else { continue };

        // Encontra o participantId do jogador nesta partida
        let Some(pid) = identities
            .iter()
            .find(|id| id["player"]["puuid"].as_str() == Some(puuid))
            .and_then(|id| id["participantId"].as_i64())
        else { continue };

        let Some(p) = participants.iter().find(|p| p["participantId"].as_i64() == Some(pid))
        else { continue };

        let stats   = &p["stats"];
        let kills   = stats["kills"].as_i64().unwrap_or(0);
        let deaths  = stats["deaths"].as_i64().unwrap_or(0);
        let assists = stats["assists"].as_i64().unwrap_or(0);
        let win     = stats["win"].as_bool().unwrap_or(false);
        let cid     = p["championId"].as_i64().unwrap_or(0);

        total_kills   += kills;
        total_deaths  += deaths;
        total_assists += assists;
        if win { wins += 1; if i < 3 { recent_wins += 1; } }
        if cid == champion_id { champ_games += 1; }
        count += 1;
    }

    if count == 0 { return PlayerAnalysis::default(); }

    PlayerAnalysis {
        kda:            (total_kills + total_assists) as f64 / total_deaths.max(1) as f64,
        win_rate:       wins as f64 / count as f64,
        games_played:   count,
        champion_games: champ_games,
        recent_wins,
        avg_deaths:     total_deaths as f64 / count as f64,
    }
}

// ── Geração de tags a partir da análise ────────────────────────

fn analysis_tags(analysis: &PlayerAnalysis, archetype: &[String]) -> Vec<String> {
    if analysis.games_played == 0 {
        return archetype.iter().take(3).cloned().collect();
    }

    let mut tags: Vec<String> = Vec::new();

    // Domínio no campeão
    if analysis.champion_games >= 5 {
        tags.push("Main do Campeão".into());
    } else if analysis.champion_games >= 3 {
        tags.push("Joga Bastante".into());
    }

    // Forma recente (últimas 3 partidas) — sinal mais forte
    if analysis.recent_wins == 3 {
        tags.push("Win Streak".into());
    } else if analysis.recent_wins == 0 && analysis.games_played >= 3 {
        tags.push("Tilted".into());
    }

    // KDA
    if analysis.kda >= 5.0 {
        tags.push("KDA Elite".into());
    } else if analysis.kda >= 3.0 {
        tags.push("KDA Sólido".into());
    } else if analysis.kda <= 1.0 {
        tags.push("KDA Fraco".into());
    }

    // Taxa de morte
    if analysis.avg_deaths >= 7.0 {
        tags.push("Morre Muito".into());
    }

    // Win rate notável
    let wr = (analysis.win_rate * 100.0).round() as u32;
    if wr >= 65 || wr <= 35 {
        tags.push(format!("{}% WR", wr));
    }

    // Completa com arquétipo se ainda há espaço
    for t in archetype {
        if tags.len() >= 4 { break; }
        tags.push(t.clone());
    }

    tags.truncate(4);
    tags
}

// ── Tags e split de dano por arquétipo do campeão ─────────────

fn archetype_tags(dd_tags: &[String], position: &str, dd_attack: f64, dd_magic: f64) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let is = |t: &str| dd_tags.iter().any(|x| x.eq_ignore_ascii_case(t));

    if      is("Marksman")                             { tags.push("High AD Damage".into()); }
    else if is("Mage")                                 { tags.push("High AP Damage".into()); }
    else if is("Assassin")                             { tags.push("High Burst".into()); }
    else if is("Tank")                                 { tags.push("Tank".into()); }
    else if is("Fighter") && dd_attack > dd_magic      { tags.push("Physical Fighter".into()); }
    else if is("Support")                              { tags.push("Utility".into()); }

    if is("Marksman") || is("Mage") || is("Assassin") { tags.push("Carry Threat".into()); }

    match position {
        "BOTTOM"  => tags.push("Very Gankable".into()),
        "MIDDLE"  => tags.push("Gankable".into()),
        "TOP"     => tags.push("Semi-Gankable".into()),
        "JUNGLE"  => tags.push("Early Pressure".into()),
        "UTILITY" => tags.push("Great Vision".into()),
        _ => {}
    }

    tags
}

fn damage_split(dd_tags: &[String], dd_attack: f64, dd_magic: f64) -> (u32, u32, u32) {
    let is = |t: &str| dd_tags.iter().any(|x| x.eq_ignore_ascii_case(t));
    if is("Marksman")                              { return (80, 10, 10); }
    if is("Mage")                                  { return (10, 80, 10); }
    if is("Assassin") && dd_attack > dd_magic      { return (65, 25, 10); }
    if is("Assassin")                              { return (25, 65, 10); }
    if is("Support")                               { return (15, 70, 15); }
    if is("Tank")                                  { return (45, 30, 25); }
    let total = (dd_attack + dd_magic).max(1.0);
    let ad    = ((dd_attack / total) * 85.0).round() as u32;
    (ad, 85 - ad, 15)
}

// ── Fetch + análise por jogador (roda em paralelo) ────────────

async fn fetch_analysis(client: LcuClient, summoner_id: i64, champion_id: i64) -> PlayerAnalysis {
    // 1. Resolve PUUID
    let summoner = match client
        .get(&format!("/lol-summoner/v1/summoners/{summoner_id}"))
        .await
    {
        Ok(s)  => s,
        Err(_) => return PlayerAnalysis::default(),
    };

    let puuid = match summoner["puuid"].as_str() {
        Some(p) => p.to_string(),
        None    => return PlayerAnalysis::default(),
    };

    // 2. Histórico (últimas 10 partidas)
    let history = match client.get_match_history(&puuid, 10).await {
        Ok(h)  => h,
        Err(_) => return PlayerAnalysis::default(),
    };

    analyze_history(&history, &puuid, champion_id)
}

// ── Command ────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_loading_screen_players(
    state: State<'_, AppState>,
) -> Result<Vec<LoadingPlayer>, String> {
    // Clona o cliente fora do mutex — permite requisições paralelas sem lock contention
    let client = {
        let guard = state.lcu_client.lock().await;
        guard.as_ref().ok_or("LCU não conectado")?.clone()
    };

    // Sessão do gameflow
    let session = client
        .get("/lol-gameflow/v1/session")
        .await
        .map_err(|e| e.to_string())?;

    let local_id = client
        .get_current_summoner()
        .await
        .map(|s| s["summonerId"].as_i64().unwrap_or(0))
        .unwrap_or(0);

    // Mapa internal_name → (champion_id, spell1, spell2)
    let mut champ_map: HashMap<String, (i64, i64, i64)> = HashMap::new();
    if let Some(sels) = session["gameData"]["playerChampionSelections"].as_array() {
        for sel in sels {
            let name = sel["summonerInternalName"]
                .as_str()
                .unwrap_or("")
                .to_lowercase();
            champ_map.insert(
                name,
                (
                    sel["championId"].as_i64().unwrap_or(0),
                    sel["spell1Id"].as_i64().unwrap_or(4),
                    sel["spell2Id"].as_i64().unwrap_or(14),
                ),
            );
        }
    }

    // Coleta dados brutos de todos os jogadores dos dois times
    struct RawPlayer {
        summoner_name: String,
        summoner_id:   i64,
        position:      String,
        team:          i64,
        is_local:      bool,
        champ_id:      i64,
        sp1:           i64,
        sp2:           i64,
    }

    let mut raw: Vec<RawPlayer> = Vec::new();
    for (team_key, team_num) in [("teamOne", 1i64), ("teamTwo", 2i64)] {
        let Some(team_arr) = session["gameData"][team_key].as_array() else { continue };
        for p in team_arr {
            let summoner_name = p["summonerName"]
                .as_str()
                .or_else(|| p["gameName"].as_str())
                .unwrap_or("Player")
                .to_string();
            let summoner_id = p["summonerId"].as_i64().unwrap_or(0);
            let internal    = p["summonerInternalName"]
                .as_str()
                .unwrap_or("")
                .to_lowercase();
            let position    = p["selectedPosition"]
                .as_str()
                .or_else(|| p["assignedPosition"].as_str())
                .unwrap_or("")
                .to_uppercase();
            let (champ_id, sp1, sp2) = champ_map.get(&internal).copied().unwrap_or((0, 4, 14));
            raw.push(RawPlayer {
                summoner_name,
                summoner_id,
                position,
                team: team_num,
                is_local: summoner_id == local_id,
                champ_id,
                sp1,
                sp2,
            });
        }
    }

    // Lê dados de campeão do DB para todos os jogadores
    struct ChampInfo {
        name:   String,
        key:    String,
        tags:   Vec<String>,
        attack: f64,
        magic:  f64,
    }

    let champ_infos: Vec<ChampInfo> = {
        let db = state.db.lock().await;
        raw.iter().map(|p| {
            if p.champ_id > 0 {
                db.query_row(
                    "SELECT name, key, data_json FROM champion_cache WHERE champion_id = ?1",
                    [p.champ_id],
                    |row| {
                        let n: String = row.get(0)?;
                        let k: String = row.get(1)?;
                        let j: String = row.get(2)?;
                        Ok((n, k, j))
                    },
                )
                .map(|(name, key, json)| {
                    let data: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
                    let tags: Vec<String> = data["tags"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                    ChampInfo {
                        name,
                        key,
                        tags,
                        attack: data["info"]["attack"].as_f64().unwrap_or(5.0),
                        magic:  data["info"]["magic"].as_f64().unwrap_or(5.0),
                    }
                })
                .unwrap_or_else(|_| ChampInfo {
                    name:   format!("Champion {}", p.champ_id),
                    key:    String::new(),
                    tags:   vec![],
                    attack: 5.0,
                    magic:  5.0,
                })
            } else {
                ChampInfo { name: "Unknown".into(), key: String::new(), tags: vec![], attack: 5.0, magic: 5.0 }
            }
        }).collect()
        // DB lock liberado aqui
    };

    // ── Busca histórico + análise em paralelo para todos os 10 jogadores ──
    let analysis_futures: Vec<_> = raw.iter().map(|p| {
        let c   = client.clone();
        let sid = p.summoner_id;
        let cid = p.champ_id;
        async move {
            if sid > 0 { fetch_analysis(c, sid, cid).await }
            else       { PlayerAnalysis::default() }
        }
    }).collect();

    let analyses = join_all(analysis_futures).await;

    // Monta os cards finais
    let players = raw
        .into_iter()
        .zip(champ_infos.into_iter())
        .zip(analyses.into_iter())
        .map(|((p, ci), analysis)| {
            let arch = archetype_tags(&ci.tags, &p.position, ci.attack, ci.magic);
            let tags = analysis_tags(&analysis, &arch);
            let (ad_pct, ap_pct, true_pct) = damage_split(&ci.tags, ci.attack, ci.magic);
            LoadingPlayer {
                summoner_name: p.summoner_name,
                champion_id:   p.champ_id,
                champion_name: ci.name,
                champion_key:  ci.key,
                position:      p.position,
                team:          p.team,
                is_local:      p.is_local,
                spell1_id:     p.sp1,
                spell2_id:     p.sp2,
                tags,
                ad_pct,
                ap_pct,
                true_pct,
            }
        })
        .collect();

    Ok(players)
}
