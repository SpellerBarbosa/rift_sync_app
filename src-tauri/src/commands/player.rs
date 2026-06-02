// ============================================================
// commands/player.rs — Commands de dados do jogador
// ============================================================

use serde::Serialize;
use tauri::State;

use crate::db::models::Player;
use crate::AppState;

// reqwest já é dependência do projeto (usado por lcu/client.rs e groq/client.rs)

/// Resumo de uma partida recente extraído da LCU API.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentMatch {
    pub game_id:       i64,
    pub champion_id:   i64,
    pub champion_name: String,
    pub champion_key:  String,   // chave para URL do Data Dragon (ex: "Garen")
    pub kills:         i64,
    pub deaths:        i64,
    pub assists:       i64,
    pub win:           bool,
    pub duration_secs: i64,
    pub game_mode:     String,
    pub cs:            i64,
    pub vision_score:  i64,
    pub kda:           f64,
}

/// Retorna o summoner atualmente logado no LoL.
/// Busca da LCU API e persiste/atualiza no banco local (incluindo elo).
#[tauri::command]
pub async fn get_current_player(state: State<'_, AppState>) -> Result<Player, String> {
    // Busca summoner + ranked em um único lock
    let (puuid, game_name, tag_line, profile_icon_id, summoner_level, rank, lp, winrate, tier_en) = {
        let guard = state.lcu_client.lock().await;
        let client = guard.as_ref().ok_or("LoL não está aberto")?;

        let summoner = client.get_current_summoner().await.map_err(|e| e.to_string())?;
        let puuid = summoner["puuid"].as_str().unwrap_or("").to_string();

        // A LCU pode retornar o Riot ID completo em `gameName` ("Nome#TAG")
        // ou separado em `gameName` + `tagLine`. Normaliza os dois casos.
        let raw_name = summoner["gameName"]
            .as_str()
            .or_else(|| summoner["name"].as_str())
            .unwrap_or("");
        let (game_name, tag_line) = if let Some(idx) = raw_name.find('#') {
            (raw_name[..idx].to_string(), raw_name[idx + 1..].to_string())
        } else {
            (
                raw_name.to_string(),
                summoner["tagLine"].as_str().unwrap_or("BR1").to_string(),
            )
        };
        let profile_icon_id = summoner["profileIconId"].as_i64().unwrap_or(0);
        let summoner_level  = summoner["summonerLevel"].as_i64().unwrap_or(0);

        // Ranked stats — falha silenciosa, elo fica como null
        let (rank, lp, winrate, tier_en) = match client.get_ranked_stats().await {
            Ok(ranked) => parse_ranked(&ranked),
            Err(_)     => (None, None, None, None),
        };

        (puuid, game_name, tag_line, profile_icon_id, summoner_level, rank, lp, winrate, tier_en)
    };

    let db = state.db.lock().await;
    db.execute(
        "INSERT INTO players (puuid, riot_name, tag, rank, lp, winrate, profile_icon_id, summoner_level, tier_en)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(puuid) DO UPDATE SET
           riot_name       = excluded.riot_name,
           tag             = excluded.tag,
           rank            = COALESCE(excluded.rank,    players.rank),
           lp              = COALESCE(excluded.lp,      players.lp),
           winrate         = COALESCE(excluded.winrate, players.winrate),
           profile_icon_id = excluded.profile_icon_id,
           summoner_level  = excluded.summoner_level,
           tier_en         = COALESCE(excluded.tier_en, players.tier_en),
           updated_at      = CURRENT_TIMESTAMP",
        rusqlite::params![puuid, game_name, tag_line, rank, lp, winrate,
                          profile_icon_id, summoner_level, tier_en],
    )
    .map_err(|e| e.to_string())?;

    db.query_row(
        "SELECT id, puuid, riot_name, tag, region, role, rank, lp, winrate,
                profile_icon_id, summoner_level, tier_en
         FROM players WHERE puuid = ?1",
        [&puuid],
        |row| Ok(Player {
            id:              row.get(0)?,
            puuid:           row.get(1)?,
            riot_name:       row.get(2)?,
            tag:             row.get(3)?,
            region:          row.get(4)?,
            role:            row.get(5)?,
            rank:            row.get(6)?,
            lp:              row.get(7)?,
            winrate:         row.get(8)?,
            profile_icon_id: row.get(9)?,
            summoner_level:  row.get(10)?,
            tier_en:         row.get(11)?,
        }),
    )
    .map_err(|e| e.to_string())
}

/// Parseia a fila RANKED_SOLO_5x5 da resposta de `/lol-ranked/v1/current-ranked-stats`.
/// Retorna (rank_pt, lp, winrate, tier_en).
fn parse_ranked(data: &serde_json::Value) -> (Option<String>, Option<i64>, Option<f64>, Option<String>) {
    let queues = match data["queues"].as_array() {
        Some(q) => q,
        None    => return (None, None, None, None),
    };

    let queue = queues.iter()
        .find(|q| q["queueType"].as_str() == Some("RANKED_SOLO_5x5"))
        .or_else(|| queues.iter().find(|q| q["queueType"].as_str() == Some("RANKED_FLEX_SR")));

    let q = match queue { Some(q) => q, None => return (None, None, None, None) };

    let tier = q["tier"].as_str().unwrap_or("");
    let div  = q["division"].as_str().unwrap_or("");

    if tier.is_empty() || tier == "NONE" {
        return (None, None, None, None);
    }

    // Formata "GOLD II"
    let tier_pt = match tier {
        "IRON"        => "Ferro",
        "BRONZE"      => "Bronze",
        "SILVER"      => "Prata",
        "GOLD"        => "Ouro",
        "PLATINUM"    => "Platina",
        "EMERALD"     => "Esmeralda",
        "DIAMOND"     => "Diamante",
        "MASTER"      => "Mestre",
        "GRANDMASTER" => "GrãoMestre",
        "CHALLENGER"  => "Desafiante",
        other         => other,
    };

    let rank_str = if div.is_empty() || div == "NA" {
        tier_pt.to_string()
    } else {
        format!("{tier_pt} {div}")
    };

    let lp = q["leaguePoints"].as_i64();
    let wins: f64   = q["wins"].as_i64().unwrap_or(0) as f64;
    let losses: f64 = q["losses"].as_i64().unwrap_or(0) as f64;
    let winrate = if wins + losses > 0.0 {
        Some((wins / (wins + losses) * 100.0 * 10.0).round() / 10.0)
    } else {
        None
    };

    (Some(rank_str), lp, winrate, Some(tier.to_uppercase()))
}

/// Retorna as últimas 5 partidas do jogador conectado via LCU API.
#[tauri::command]
pub async fn get_recent_matches(
    state: State<'_, AppState>,
) -> Result<Vec<RecentMatch>, String> {
    // Summoner + histórico em uma única aquisição de lock
    let (puuid, account_id, history) = {
        let guard = state.lcu_client.lock().await;
        let client = guard.as_ref().ok_or("LCU não conectado")?;

        let summoner = client
            .get_current_summoner()
            .await
            .map_err(|e| e.to_string())?;

        let puuid      = summoner["puuid"].as_str().unwrap_or("").to_string();
        let account_id = summoner["accountId"].as_str().unwrap_or("").to_string();

        if puuid.is_empty() {
            return Err("PUUID do summoner não disponível".to_string());
        }

        let history = client
            .get_match_history(&puuid, 5)
            .await
            .map_err(|e| e.to_string())?;

        (puuid, account_id, history)
    };

    let games: Vec<serde_json::Value> = if let Some(arr) = history["games"].as_array() {
        arr.clone()
    } else if let Some(arr) = history["games"]["games"].as_array() {
        arr.clone()
    } else if let Some(arr) = history.as_array() {
        arr.clone()
    } else {
        return Ok(Vec::new());
    };

    let mut recent: Vec<RecentMatch> = Vec::new();

    for game in games.iter().take(5) {
        let participants = match game["participants"].as_array() {
            Some(ps) => ps,
            None     => continue,
        };

        // Tenta formato novo (puuid direto) → formato antigo (via identities)
        let p = {
            let by_puuid = participants.iter().find(|p| p["puuid"].as_str() == Some(puuid.as_str()));
            if let Some(p) = by_puuid {
                p
            } else {
                let pid = game["participantIdentities"]
                    .as_array()
                    .and_then(|ids| ids.iter().find(|id| {
                        let pl = &id["player"];
                        pl["puuid"].as_str()     == Some(puuid.as_str())
                            || pl["accountId"].as_str() == Some(account_id.as_str())
                    }))
                    .and_then(|id| id["participantId"].as_i64());
                match pid.and_then(|pid| participants.iter().find(|p| p["participantId"].as_i64() == Some(pid))) {
                    Some(p) => p,
                    None    => continue,
                }
            }
        };

        let stats      = if p["stats"].is_object() { &p["stats"] } else { p };
        let champion_id = p["championId"].as_i64().unwrap_or(0);
        let kills       = stats["kills"].as_i64().unwrap_or(0);
        let deaths      = stats["deaths"].as_i64().unwrap_or(0);
        let assists     = stats["assists"].as_i64().unwrap_or(0);
        let win         = stats["win"].as_bool().unwrap_or(false);
        let cs          = stats["totalMinionsKilled"].as_i64().unwrap_or(0)
                        + stats["neutralMinionsKilled"].as_i64().unwrap_or(0);
        let vision      = stats["visionScore"].as_i64().unwrap_or(0);
        let duration    = game["gameDuration"].as_i64().or_else(|| game["gameLength"].as_i64()).unwrap_or(0);
        let mode        = game["gameMode"].as_str().unwrap_or("CLASSIC").to_string();
        let kda         = (kills + assists) as f64 / deaths.max(1) as f64;

        // Nome e key do campeão via cache local
        let (champion_name, champion_key) = {
            let db = state.db.lock().await;
            db.query_row(
                "SELECT name, COALESCE(key, '') FROM champion_cache WHERE champion_id = ?1",
                [champion_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap_or_else(|_| (format!("#{champion_id}"), String::new()))
        };

        recent.push(RecentMatch {
            game_id: game["gameId"].as_i64().unwrap_or(0),
            champion_id,
            champion_name,
            champion_key,
            kills, deaths, assists, win,
            duration_secs: duration,
            game_mode: mode,
            cs, vision_score: vision, kda,
        });
    }

    Ok(recent)
}

// ── Structs para o dashboard ──────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionStat {
    pub id:      i64,      // ID numérico (para SpellCoach API)
    pub name:    String,
    pub key:     String,   // chave Data Dragon para o ícone
    pub games:   i64,
    pub wins:    i64,
    pub winrate: f64,
    pub avg_kda: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchStats {
    pub total_games:   i64,
    pub wins:          i64,
    pub losses:        i64,
    pub winrate:       f64,
    pub avg_kda:       f64,
    pub avg_cs:        f64,
    pub avg_vision:    f64,
    pub top_champions: Vec<ChampionStat>,
}

// ── Commands do dashboard (cache-first, sem LCU necessário) ───

/// Retorna o último jogador conhecido do banco local (não requer LCU).
#[tauri::command]
pub async fn get_last_player(state: State<'_, AppState>) -> Result<Player, String> {
    let db = state.db.lock().await;
    db.query_row(
        "SELECT id, puuid, riot_name, tag, region, role, rank, lp, winrate,
                profile_icon_id, summoner_level, tier_en
         FROM players ORDER BY updated_at DESC LIMIT 1",
        [],
        |row| Ok(Player {
            id:              row.get(0)?,
            puuid:           row.get(1)?,
            riot_name:       row.get(2)?,
            tag:             row.get(3)?,
            region:          row.get(4)?,
            role:            row.get(5)?,
            rank:            row.get(6)?,
            lp:              row.get(7)?,
            winrate:         row.get(8)?,
            profile_icon_id: row.get(9)?,
            summoner_level:  row.get(10)?,
            tier_en:         row.get(11)?,
        }),
    )
    .map_err(|e| format!("Nenhum jogador no banco: {e}"))
}

/// Lê as últimas 10 partidas do banco local (não requer LCU).
#[tauri::command]
pub async fn get_cached_matches(
    state: State<'_, AppState>,
) -> Result<Vec<RecentMatch>, String> {
    let db = state.db.lock().await;

    let mut stmt = db
        .prepare(
            "SELECT m.id,
                    COALESCE(m.champion_id, 0),
                    COALESCE(m.champion_name, '?'),
                    COALESCE(cc.key, ''),
                    COALESCE(m.kills, 0),
                    COALESCE(m.deaths, 0),
                    COALESCE(m.assists, 0),
                    CASE WHEN m.result = 'WIN' THEN 1 ELSE 0 END,
                    COALESCE(m.duration, 0),
                    COALESCE(m.cs, 0),
                    COALESCE(m.vision_score, 0)
             FROM matches m
             LEFT JOIN champion_cache cc ON cc.champion_id = m.champion_id
             WHERE m.player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)
             ORDER BY CAST(m.match_id AS INTEGER) DESC
             LIMIT 10",
        )
        .map_err(|e| e.to_string())?;

    let matches = stmt
        .query_map([], |row| {
            let kills: i64   = row.get(4)?;
            let deaths: i64  = row.get(5)?;
            let assists: i64 = row.get(6)?;
            let kda = (kills + assists) as f64 / deaths.max(1) as f64;
            Ok(RecentMatch {
                game_id:       row.get(0)?,
                champion_id:   row.get(1)?,
                champion_name: row.get(2)?,
                champion_key:  row.get(3)?,
                kills, deaths, assists,
                win:           row.get::<_, i64>(7)? == 1,
                duration_secs: row.get(8)?,
                game_mode:     "CLASSIC".to_string(),
                cs:            row.get(9)?,
                vision_score:  row.get(10)?,
                kda,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();

    Ok(matches)
}

/// Retorna estatísticas agregadas e top campeões do banco local.
#[tauri::command]
pub async fn get_match_stats(
    state: State<'_, AppState>,
) -> Result<MatchStats, String> {
    let db = state.db.lock().await;

    let (total_games, wins, avg_kda, avg_cs, avg_vision) = db
        .query_row(
            "SELECT
                COUNT(*),
                SUM(CASE WHEN result = 'WIN' THEN 1 ELSE 0 END),
                AVG(CAST(COALESCE(kills,0)+COALESCE(assists,0) AS REAL) / MAX(1,COALESCE(deaths,1))),
                AVG(CAST(COALESCE(cs,0) AS REAL)),
                AVG(CAST(COALESCE(vision_score,0) AS REAL))
             FROM matches
             WHERE player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)",
            [],
            |row| Ok((
                row.get::<_, i64>(0).unwrap_or(0),
                row.get::<_, i64>(1).unwrap_or(0),
                row.get::<_, f64>(2).unwrap_or(0.0),
                row.get::<_, f64>(3).unwrap_or(0.0),
                row.get::<_, f64>(4).unwrap_or(0.0),
            )),
        )
        .unwrap_or((0, 0, 0.0, 0.0, 0.0));

    let mut champ_stmt = db
        .prepare(
            "SELECT
                COALESCE(m.champion_id, 0),
                COALESCE(m.champion_name, '?'),
                COALESCE(cc.key, ''),
                COUNT(*) as games,
                SUM(CASE WHEN m.result = 'WIN' THEN 1 ELSE 0 END) as wins,
                AVG(CAST(COALESCE(m.kills,0)+COALESCE(m.assists,0) AS REAL) / MAX(1,COALESCE(m.deaths,1)))
             FROM matches m
             LEFT JOIN champion_cache cc ON cc.champion_id = m.champion_id
             WHERE m.player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)
             GROUP BY m.champion_id, m.champion_name
             ORDER BY games DESC, wins DESC
             LIMIT 3",
        )
        .map_err(|e| e.to_string())?;

    let top_champions = champ_stmt
        .query_map([], |row| {
            let games: i64 = row.get(3)?;
            let wins: i64  = row.get(4).unwrap_or(0);
            let kda: f64   = row.get(5).unwrap_or(0.0);
            Ok(ChampionStat {
                id:      row.get(0)?,
                name:    row.get(1)?,
                key:     row.get(2)?,
                games,
                wins,
                winrate: if games > 0 { (wins as f64 / games as f64 * 100.0 * 10.0).round() / 10.0 } else { 0.0 },
                avg_kda: (kda * 100.0).round() / 100.0,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();

    let winrate = if total_games > 0 {
        (wins as f64 / total_games as f64 * 100.0 * 10.0).round() / 10.0
    } else { 0.0 };

    Ok(MatchStats {
        total_games,
        wins,
        losses: total_games - wins,
        winrate,
        avg_kda:    (avg_kda * 100.0).round() / 100.0,
        avg_cs:     avg_cs.round(),
        avg_vision: avg_vision.round(),
        top_champions,
    })
}

/// Sincroniza histórico da LCU para o banco (até 10 partidas).
/// Retorna o número de novas partidas salvas.
#[tauri::command]
pub async fn sync_match_history(
    state: State<'_, AppState>,
) -> Result<u32, String> {
    // ── Fase 1: buscar da LCU (uma única aquisição de lock) ──
    let (puuid, account_id, history) = {
        let guard = state.lcu_client.lock().await;
        let client = guard.as_ref().ok_or("LCU não conectado — abra o League of Legends")?;

        let summoner = client.get_current_summoner().await
            .map_err(|e| format!("Falha ao buscar summoner: {e}"))?;
        let puuid      = summoner["puuid"].as_str().unwrap_or("").to_string();
        let account_id = summoner["accountId"]
            .as_str()
            .or_else(|| summoner["accountId"].as_u64().map(|_| ""))
            .unwrap_or("").to_string();

        if puuid.is_empty() { return Err("PUUID não disponível no summoner".to_string()); }

        let history = client.get_match_history(&puuid, 10).await
            .map_err(|e| format!("Erro no histórico de partidas: {e}"))?;
        (puuid, account_id, history)
    };

    // A LCU pode retornar { games: [...] } ou { games: { games: [...] } }
    let games: Vec<serde_json::Value> = if let Some(arr) = history["games"].as_array() {
        arr.clone()
    } else if let Some(arr) = history["games"]["games"].as_array() {
        arr.clone()
    } else if let Some(arr) = history.as_array() {
        arr.clone()
    } else {
        tracing::warn!("Formato inesperado no histórico: {}", history);
        return Err(format!("Formato de histórico desconhecido — keys: {:?}",
            history.as_object().map(|o| o.keys().collect::<Vec<_>>())));
    };

    // ── Fase 2: parsear partidas em memória (sem locks) ──────
    struct ParsedGame {
        game_id: i64, champion_id: i64, role: String, result: String,
        kills: i64, deaths: i64, assists: i64, cs: i64,
        cs_per_min: f64, vision: i64, damage: i64, gold: i64, duration: i64,
    }

    let mut parsed: Vec<ParsedGame> = Vec::new();

    for game in games.iter().take(10) {
        // ── Encontra o participant do jogador ─────────────────
        // Formato novo: puuid direto no participant
        // Formato antigo: via participantIdentities → participantId
        let p = {
            let participants = match game["participants"].as_array() {
                Some(ps) => ps,
                None     => continue,
            };

            // Tenta formato novo: puuid direto
            let by_puuid = participants.iter().find(|p| {
                p["puuid"].as_str() == Some(puuid.as_str())
            });

            if let Some(p) = by_puuid {
                p
            } else {
                // Formato antigo: encontra o participantId via identities
                let pid = game["participantIdentities"]
                    .as_array()
                    .and_then(|ids| ids.iter().find(|id| {
                        let pl = &id["player"];
                        pl["puuid"].as_str()     == Some(puuid.as_str())
                            || pl["accountId"].as_str() == Some(account_id.as_str())
                    }))
                    .and_then(|id| id["participantId"].as_i64());

                match pid {
                    Some(pid) => match participants.iter().find(|p| p["participantId"].as_i64() == Some(pid)) {
                        Some(p) => p,
                        None    => continue,
                    },
                    None => continue,
                }
            }
        };

        // ── Extrai estatísticas (campo "stats" ou direto no participant) ──
        let stats = if p["stats"].is_object() { &p["stats"] } else { p };

        let duration = game["gameDuration"].as_i64()
            .or_else(|| game["gameLength"].as_i64())
            .unwrap_or(0);

        let cs = stats["totalMinionsKilled"].as_i64()
            .or_else(|| stats["minionsKilled"].as_i64())
            .unwrap_or(0)
            + stats["neutralMinionsKilled"].as_i64().unwrap_or(0);

        let win = stats["win"].as_bool()
            .or_else(|| stats["gameEndedInEarlySurrender"].as_bool().map(|_| false))
            .unwrap_or(false);

        let role = p["timeline"]["lane"].as_str()
            .or_else(|| p["individualPosition"].as_str())
            .or_else(|| p["teamPosition"].as_str())
            .unwrap_or("NONE")
            .to_string();

        parsed.push(ParsedGame {
            game_id:     game["gameId"].as_i64().unwrap_or(0),
            champion_id: p["championId"].as_i64().unwrap_or(0),
            role,
            result:  if win { "WIN".into() } else { "LOSS".into() },
            kills:   stats["kills"].as_i64().unwrap_or(0),
            deaths:  stats["deaths"].as_i64().unwrap_or(0),
            assists: stats["assists"].as_i64().unwrap_or(0),
            cs,
            cs_per_min: if duration > 0 { cs as f64 / (duration as f64 / 60.0) } else { 0.0 },
            vision:  stats["visionScore"].as_i64().unwrap_or(0),
            damage:  stats["totalDamageDealtToChampions"].as_i64().unwrap_or(0),
            gold:    stats["goldEarned"].as_i64().unwrap_or(0),
            duration,
        });
    }

    if parsed.is_empty() { return Ok(0); }

    // ── Fase 3: gravar no banco em lock único ─────────────────
    let db = state.db.lock().await;

    db.execute(
        "INSERT INTO players (puuid, riot_name, tag) VALUES (?1,'','')
         ON CONFLICT(puuid) DO NOTHING",
        [&puuid],
    ).map_err(|e| e.to_string())?;

    let player_id: i64 = db
        .query_row("SELECT id FROM players WHERE puuid = ?1", [&puuid], |r| r.get(0))
        .map_err(|e| e.to_string())?;

    let mut saved = 0u32;

    for m in &parsed {
        let champ_name: String = db
            .query_row(
                "SELECT name FROM champion_cache WHERE champion_id = ?1",
                [m.champion_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| format!("#{}", m.champion_id));

        let rows = db.execute(
            "INSERT INTO matches
                (player_id, match_id, champion_id, champion_name, role, result,
                 kills, deaths, assists, cs, cs_per_min, vision_score,
                 damage_dealt, gold_earned, duration)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
             ON CONFLICT(match_id) DO NOTHING",
            rusqlite::params![
                player_id, m.game_id.to_string(), m.champion_id, champ_name,
                m.role, m.result, m.kills, m.deaths, m.assists, m.cs,
                m.cs_per_min, m.vision, m.damage, m.gold, m.duration,
            ],
        ).map_err(|e| e.to_string())?;

        if rows > 0 { saved += 1; }
    }

    Ok(saved)
}

/// Baixa e armazena os dados de campeões do Data Dragon.
/// Deve ser chamado uma vez após o sync, ou quando a versão do jogo muda.
#[tauri::command]
pub async fn sync_champion_cache(
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    // Última versão do jogo
    let versions: Vec<String> = http
        .get("https://ddragon.leagueoflegends.com/api/versions.json")
        .send()
        .await
        .map_err(|e| format!("Falha ao buscar versões do Data Dragon: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Versões inválidas: {e}"))?;

    let version = versions.first().ok_or("Lista de versões vazia")?;

    let url = format!(
        "https://ddragon.leagueoflegends.com/cdn/{version}/data/en_US/champion.json"
    );

    let resp: serde_json::Value = http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Falha ao buscar campeões: {e}"))?
        .json()
        .await
        .map_err(|e| format!("JSON de campeões inválido: {e}"))?;

    let champs = resp["data"]
        .as_object()
        .ok_or("Campo 'data' ausente na resposta do Data Dragon")?;

    let db = state.db.lock().await;

    // Persiste a versão usada para montar URLs de ícones no frontend
    db.execute(
        "INSERT INTO settings (key, value) VALUES ('dd_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [version],
    ).map_err(|e| e.to_string())?;

    let mut saved = 0u32;

    for (_, champ) in champs {
        let id: i64 = champ["key"]
            .as_str()
            .and_then(|k| k.parse().ok())
            .unwrap_or(0);
        if id == 0 { continue; }

        let name = champ["name"].as_str().unwrap_or("?");
        let key  = champ["id"].as_str().unwrap_or("?");

        let rows = db.execute(
            "INSERT INTO champion_cache (champion_id, name, key, data_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(champion_id) DO UPDATE SET
               name      = excluded.name,
               key       = excluded.key,
               data_json = excluded.data_json,
               cached_at = CURRENT_TIMESTAMP",
            rusqlite::params![id, name, key, champ.to_string()],
        ).map_err(|e| e.to_string())?;

        if rows > 0 { saved += 1; }
    }

    // Corrige partidas já salvas que têm o nome como #id
    db.execute(
        "UPDATE matches
         SET champion_name = (
             SELECT name FROM champion_cache
             WHERE champion_cache.champion_id = matches.champion_id
         )
         WHERE champion_name LIKE '#%'",
        [],
    ).map_err(|e| e.to_string())?;

    tracing::info!("Champion cache: {saved} campeões salvos (versão {version}).");
    Ok(saved)
}

/// Lê um valor da tabela settings pelo key.
#[tauri::command]
pub async fn get_setting(
    key: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let db = state.db.lock().await;
    db.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [&key],
        |row| row.get(0),
    )
    .map_err(|_| format!("Setting '{key}' não encontrada"))
}

/// Grava ou atualiza um valor na tabela settings.
#[tauri::command]
pub async fn set_setting(
    key:   String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().await;
    db.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [&key, &value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Retorna todas as configurações como objeto JSON {key: value}.
#[tauri::command]
pub async fn get_all_settings(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let db = state.db.lock().await;
    let mut stmt = db
        .prepare("SELECT key, value FROM settings")
        .map_err(|e| e.to_string())?;

    let mut map = serde_json::Map::new();
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;

    for row in rows.flatten() {
        map.insert(row.0, serde_json::Value::String(row.1));
    }

    Ok(serde_json::Value::Object(map))
}

/// Deriva padrões comportamentais a partir das partidas salvas e persiste em player_patterns.
///
/// Requer mínimo de 5 partidas no banco. Retorna o número de partidas usadas ou
/// 0 se não houver dados suficientes (sem erro — o frontend mostra a mensagem adequada).
///
/// Métricas derivadas das colunas disponíveis em `matches`:
///   ward_score        → vision_score normalizado (50 pts = 1.0)
///   aggression_score  → taxa de kills no KDA
///   lane_dominance    → cs/min normalizado (7 cs/min = 1.0)
///   objective_control → win rate (proxy de impacto em objetivos)
///   roam_frequency    → taxa de assists no KDA (proxy de presença em outras lanes)
///   tp_efficiency     → proxy combinado de win rate + vision
///   avg_deaths_10_15  → estimativa: 30% das mortes ocorrem no intervalo 10-15min
///   deaths_without_vision → partidas com vision_score < 15 × deaths acumuladas
#[tauri::command]
pub async fn compute_player_patterns(
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let db = state.db.lock().await;

    // Busca o player mais recente
    let player_id: i64 = db.query_row(
        "SELECT id FROM players ORDER BY updated_at DESC LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|_| "Nenhum jogador no banco".to_string())?;

    // Agrega todas as partidas do jogador
    let (total_games, wins, avg_vision, avg_cs_min, avg_kills, avg_deaths, avg_assists, deaths_no_vision) = db
        .query_row(
            "SELECT
                COUNT(*),
                SUM(CASE WHEN result = 'WIN' THEN 1 ELSE 0 END),
                AVG(CAST(COALESCE(vision_score,0) AS REAL)),
                AVG(CAST(COALESCE(cs_per_min,0)   AS REAL)),
                AVG(CAST(COALESCE(kills,0)         AS REAL)),
                AVG(CAST(COALESCE(deaths,0)        AS REAL)),
                AVG(CAST(COALESCE(assists,0)       AS REAL)),
                SUM(CASE WHEN COALESCE(vision_score,0) < 15 THEN COALESCE(deaths,0) ELSE 0 END)
             FROM matches
             WHERE player_id = ?1",
            [player_id],
            |row| Ok((
                row.get::<_,i64>(0).unwrap_or(0),
                row.get::<_,i64>(1).unwrap_or(0),
                row.get::<_,f64>(2).unwrap_or(0.0),
                row.get::<_,f64>(3).unwrap_or(0.0),
                row.get::<_,f64>(4).unwrap_or(0.0),
                row.get::<_,f64>(5).unwrap_or(0.0),
                row.get::<_,f64>(6).unwrap_or(0.0),
                row.get::<_,i64>(7).unwrap_or(0),
            )),
        )
        .map_err(|e| e.to_string())?;

    // Mínimo de 5 partidas para cálculo significativo
    if total_games < 5 {
        return Ok(0);
    }

    let clamp = |v: f64| v.clamp(0.0, 1.0);
    let kda_sum = avg_kills + avg_deaths + avg_assists;

    let ward_score       = clamp(avg_vision / 50.0);
    let aggression_score = clamp(avg_kills / kda_sum.max(1.0));
    let lane_dominance   = clamp(avg_cs_min / 7.0);
    let win_rate         = wins as f64 / total_games as f64;
    let objective_ctrl   = clamp(win_rate);
    let roam_frequency   = clamp(avg_assists / kda_sum.max(1.0));
    // TP efficiency: combina win rate + contribuição de assists (proxy de jogadas de mapa)
    let tp_efficiency    = clamp((win_rate + roam_frequency) / 2.0);
    let avg_deaths_10_15 = avg_deaths * 0.30;

    db.execute(
        "INSERT INTO player_patterns
             (player_id, aggression_score, deaths_without_vision, avg_deaths_10_15,
              lane_dominance, objective_control, roam_frequency, tp_efficiency, ward_score)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(player_id) DO UPDATE SET
             aggression_score      = excluded.aggression_score,
             deaths_without_vision = excluded.deaths_without_vision,
             avg_deaths_10_15      = excluded.avg_deaths_10_15,
             lane_dominance        = excluded.lane_dominance,
             objective_control     = excluded.objective_control,
             roam_frequency        = excluded.roam_frequency,
             tp_efficiency         = excluded.tp_efficiency,
             ward_score            = excluded.ward_score,
             updated_at            = CURRENT_TIMESTAMP",
        rusqlite::params![
            player_id,
            aggression_score,
            deaths_no_vision,
            avg_deaths_10_15,
            lane_dominance,
            objective_ctrl,
            roam_frequency,
            tp_efficiency,
            ward_score,
        ],
    ).map_err(|e| e.to_string())?;

    tracing::info!(
        "player_patterns atualizado — {total_games} partidas | \
         ward={ward_score:.2} agg={aggression_score:.2} lane={lane_dominance:.2} \
         obj={objective_ctrl:.2} roam={roam_frequency:.2}"
    );

    Ok(total_games as u32)
}

/// Retorna os padrões comportamentais do jogador mais recente (tabela player_patterns).
/// Retorna None se ainda não houver dados suficientes para calcular o perfil.
#[tauri::command]
pub async fn get_player_patterns(
    state: State<'_, AppState>,
) -> Result<Option<crate::db::models::PlayerPattern>, String> {
    let db = state.db.lock().await;
    let result = db.query_row(
        "SELECT id, player_id, aggression_score, deaths_without_vision, avg_deaths_10_15,
                lane_dominance, objective_control, roam_frequency, tp_efficiency, ward_score
         FROM player_patterns
         WHERE player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)
         LIMIT 1",
        [],
        |row| Ok(crate::db::models::PlayerPattern {
            id:                    row.get(0)?,
            player_id:             row.get(1)?,
            aggression_score:      row.get(2)?,
            deaths_without_vision: row.get(3)?,
            avg_deaths_10_15:      row.get(4)?,
            lane_dominance:        row.get(5)?,
            objective_control:     row.get(6)?,
            roam_frequency:        row.get(7)?,
            tp_efficiency:         row.get(8)?,
            ward_score:            row.get(9)?,
        }),
    );
    match result {
        Ok(p)  => Ok(Some(p)),
        Err(_) => Ok(None),
    }
}

/// Retorna o summoner logado sem persistir no banco (rápido, para debug).
#[tauri::command]
pub async fn get_current_summoner(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let client_guard = state.lcu_client.lock().await;
    let client = client_guard
        .as_ref()
        .ok_or("LCU não conectado")?;

    client
        .get_current_summoner()
        .await
        .map_err(|e| e.to_string())
}
