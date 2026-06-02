// ============================================================
// commands/champion.rs — Commands de dados de campeões
// ============================================================

use serde::{Deserialize, Serialize};
use tauri::State;

use std::sync::Arc;

use crate::db::models::{Champion, ChampionMatchup, ChampionMetaStat};
use crate::AppState;

// ── Structs de resposta ───────────────────────────────────────

/// Recomendação de runas e spells para o pick do jogador.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickRecommendation {
    pub role:               String,
    pub keystone:           String,
    pub primary_path:       String,
    pub secondary_path:     String,
    pub spell1_name:        String,
    pub spell2_name:        String,
    pub spell1_id:          i64,
    pub spell2_id:          i64,
    pub primary_path_id:    i64,
    pub primary_rune_ids:   Vec<i64>,
    pub secondary_path_id:  i64,
    pub secondary_rune_ids: Vec<i64>,
    pub stat_shard_ids:     Vec<i64>,
    pub meta_win_rate:      Option<f64>,
    pub meta_games:         Option<i64>,
    pub patch:              Option<String>,
}

/// Formato compatível com ChampionMetaStat no frontend (dashboard.ts).
/// Win rate em escala 0-100.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionMetaStatResponse {
    pub champion_id:            i64,
    pub patch:                  String,
    pub elo:                    String,
    pub role:                   String,
    pub games_played:           i64,
    pub win_rate:               f64,
    pub kda:                    KdaDetail,
    pub average_cs_per_min:     f64,
    pub average_gold_per_min:   f64,
    pub average_damage_per_min: f64,
    pub average_vision_score:   f64,
}

#[derive(Debug, Serialize)]
pub struct KdaDetail {
    pub kills:   f64,
    pub deaths:  f64,
    pub assists: f64,
    pub ratio:   String,
}

/// Resultado do sync da SpellCoach API para o banco local.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellCoachSyncResult {
    pub synced:          i64,
    pub errors:          i64,
    pub combinations_ok: Vec<String>,
}

/// Estatísticas de matchup retornadas ao frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchupStatsResponse {
    pub champion_a_id: i64,
    pub champion_b_id: i64,
    pub patch:         String,
    pub elo:           String,
    pub games_played:  i64,
    /// Win rate do campeão A em escala 0-100.
    pub win_rate_a:    f64,
    /// Win rate do campeão B em escala 0-100.
    pub win_rate_b:    f64,
}

// ── Roles e tiers sincronizados ───────────────────────────────

const ROLES: &[&str] = &["ADC", "TOP", "MID", "JUNGLE", "SUPPORT"];
const TIERS: &[&str] = &[
    "CHALLENGER", "GRANDMASTER", "MASTER",
    "DIAMOND", "EMERALD", "GOLD", "SILVER", "BRONZE", "IRON",
];

// ── Defaults de runas por role ────────────────────────────────

struct RoleDefaults {
    keystone:           &'static str,
    primary_path:       &'static str,
    secondary_path:     &'static str,
    spell1_id:          i64,
    spell2_id:          i64,
    spell1_name:        &'static str,
    spell2_name:        &'static str,
    primary_path_id:    i64,
    primary_rune_ids:   [i64; 4],
    secondary_path_id:  i64,
    secondary_rune_ids: [i64; 2],
    stat_shard_ids:     [i64; 3],
}

fn role_defaults(role: &str) -> RoleDefaults {
    match role {
        "TOP" => RoleDefaults {
            keystone: "Conqueror", primary_path: "Precision", secondary_path: "Resolve",
            spell1_id: 4, spell2_id: 12, spell1_name: "Flash", spell2_name: "Teleport",
            primary_path_id: 8000,
            primary_rune_ids: [8010, 9111, 9104, 8014],
            secondary_path_id: 8400,
            secondary_rune_ids: [8446, 8429],
            stat_shard_ids: [5008, 5008, 5001],
        },
        "JUNGLE" => RoleDefaults {
            keystone: "Conqueror", primary_path: "Precision", secondary_path: "Domination",
            spell1_id: 11, spell2_id: 4, spell1_name: "Smite", spell2_name: "Flash",
            primary_path_id: 8000,
            primary_rune_ids: [8010, 9111, 9103, 8014],
            secondary_path_id: 8100,
            secondary_rune_ids: [8139, 8105],
            stat_shard_ids: [5008, 5008, 5001],
        },
        "MIDDLE" => RoleDefaults {
            keystone: "Electrocute", primary_path: "Domination", secondary_path: "Sorcery",
            spell1_id: 4, spell2_id: 14, spell1_name: "Flash", spell2_name: "Ignite",
            primary_path_id: 8100,
            // 8140 = Grisly Mementos (slot 2) — substituiu Eyeball Collection (8138) no patch 14.x
            primary_rune_ids: [8112, 8143, 8140, 8106],
            secondary_path_id: 8200,
            secondary_rune_ids: [8226, 8237],
            stat_shard_ids: [5008, 5008, 5001],
        },
        "BOTTOM" => RoleDefaults {
            keystone: "Lethal Tempo", primary_path: "Precision", secondary_path: "Domination",
            spell1_id: 4, spell2_id: 7, spell1_name: "Flash", spell2_name: "Heal",
            primary_path_id: 8000,
            primary_rune_ids: [8008, 9111, 9104, 8014],
            secondary_path_id: 8100,
            secondary_rune_ids: [8139, 8106],
            // 5005 (Attack Speed) só existe em STAT_ROWS[0]; row 1 (flex) precisa de 5008
            stat_shard_ids: [5005, 5008, 5001],
        },
        _ => RoleDefaults { // UTILITY / Support
            keystone: "Aftershock", primary_path: "Resolve", secondary_path: "Inspiration",
            spell1_id: 4, spell2_id: 3, spell1_name: "Flash", spell2_name: "Exhaust",
            primary_path_id: 8400,
            primary_rune_ids: [8439, 8463, 8473, 8451],
            secondary_path_id: 8300,
            secondary_rune_ids: [8347, 8410],
            stat_shard_ids: [5008, 5002, 5001],
        },
    }
}

// ── Helpers de banco ──────────────────────────────────────────

/// Lê uma linha de `champion_meta_stats` pelo ID do campeão.
/// Usa o registro com maior número de partidas quando há múltiplos roles.
fn query_meta_stat(
    db: &rusqlite::Connection,
    champion_id: i64,
) -> Option<ChampionMetaStat> {
    db.query_row(
        "SELECT champion_id, role, tier, win_rate, wins, losses, total_games,
                avg_kills, avg_deaths, avg_assists, avg_kda,
                avg_damage_taken, avg_damage_to_champions, avg_damage_to_objectives,
                avg_vision_score, avg_wards_placed, avg_wards_killed, avg_control_wards_bought,
                pick_rate, power_phase_early, power_phase_mid, power_phase_late
         FROM champion_meta_stats
         WHERE champion_id = ?1
         ORDER BY total_games DESC
         LIMIT 1",
        [champion_id],
        row_to_meta_stat,
    ).ok()
}

/// Lê uma linha de `champion_meta_stats` filtrando por role.
fn query_meta_stat_by_role(
    db: &rusqlite::Connection,
    champion_id: i64,
    role: &str,
) -> Option<ChampionMetaStat> {
    db.query_row(
        "SELECT champion_id, role, tier, win_rate, wins, losses, total_games,
                avg_kills, avg_deaths, avg_assists, avg_kda,
                avg_damage_taken, avg_damage_to_champions, avg_damage_to_objectives,
                avg_vision_score, avg_wards_placed, avg_wards_killed, avg_control_wards_bought,
                pick_rate, power_phase_early, power_phase_mid, power_phase_late
         FROM champion_meta_stats
         WHERE champion_id = ?1 AND role = ?2
         ORDER BY total_games DESC
         LIMIT 1",
        rusqlite::params![champion_id, role],
        row_to_meta_stat,
    ).ok()
}

fn row_to_meta_stat(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChampionMetaStat> {
    Ok(ChampionMetaStat {
        champion_id:              row.get(0)?,
        role:                     row.get(1)?,
        tier:                     row.get(2)?,
        win_rate:                 row.get(3)?,
        wins:                     row.get(4)?,
        losses:                   row.get(5)?,
        total_games:              row.get(6)?,
        avg_kills:                row.get(7)?,
        avg_deaths:               row.get(8)?,
        avg_assists:              row.get(9)?,
        avg_kda:                  row.get(10)?,
        avg_damage_taken:         row.get(11)?,
        avg_damage_to_champions:  row.get(12)?,
        avg_damage_to_objectives: row.get(13)?,
        avg_vision_score:         row.get(14)?,
        avg_wards_placed:         row.get(15)?,
        avg_wards_killed:         row.get(16)?,
        avg_control_wards_bought: row.get(17)?,
        pick_rate:                row.get(18)?,
        power_phase_early:        row.get(19)?,
        power_phase_mid:          row.get(20)?,
        power_phase_late:         row.get(21)?,
    })
}

fn meta_stat_to_response(stat: &ChampionMetaStat) -> ChampionMetaStatResponse {
    ChampionMetaStatResponse {
        champion_id:            stat.champion_id,
        patch:                  "current".to_string(),
        elo:                    stat.tier.clone(),
        role:                   stat.role.clone(),
        games_played:           stat.total_games,
        win_rate:               stat.win_rate * 100.0,
        kda:                    KdaDetail {
            kills:   stat.avg_kills,
            deaths:  stat.avg_deaths,
            assists: stat.avg_assists,
            ratio:   format!("{:.2}", stat.avg_kda),
        },
        average_cs_per_min:    0.0,
        average_gold_per_min:  0.0,
        average_damage_per_min: stat.avg_damage_to_champions,
        average_vision_score:  stat.avg_vision_score,
    }
}

// ── Commands ──────────────────────────────────────────────────

/// Sincroniza dados de meta de campeões da SpellCoach API para o banco local.
/// Itera sobre todas as combinações de role × tier e faz upsert no SQLite.
/// Deve ser chamado manualmente pelo usuário ou na inicialização do app.
#[tauri::command]
pub async fn sync_spellcoach_data(
    state: State<'_, AppState>,
) -> Result<SpellCoachSyncResult, String> {
    let client = state
        .spellcoach_client
        .as_ref()
        .ok_or_else(|| "SPELLCOACH_API_KEY não configurada".to_string())?
        .clone();

    let mut synced:          i64 = 0;
    let mut errors:          i64 = 0;
    let mut combinations_ok: Vec<String> = Vec::new();

    for role in ROLES {
        for tier in TIERS {
            match client.get_champions(role, tier).await {
                Ok(champions) => {
                    let db = state.db.lock().await;
                    for c in &champions {
                        let result = db.execute(
                            "INSERT INTO champion_meta_stats (
                                champion_id, role, tier, win_rate, wins, losses, total_games,
                                avg_kills, avg_deaths, avg_assists, avg_kda,
                                avg_damage_taken, avg_damage_to_champions, avg_damage_to_objectives,
                                avg_vision_score, avg_wards_placed, avg_wards_killed, avg_control_wards_bought,
                                pick_rate, power_phase_early, power_phase_mid, power_phase_late,
                                synced_at
                             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22, CURRENT_TIMESTAMP)
                             ON CONFLICT(champion_id, role, tier) DO UPDATE SET
                                win_rate                 = excluded.win_rate,
                                wins                     = excluded.wins,
                                losses                   = excluded.losses,
                                total_games              = excluded.total_games,
                                avg_kills                = excluded.avg_kills,
                                avg_deaths               = excluded.avg_deaths,
                                avg_assists              = excluded.avg_assists,
                                avg_kda                  = excluded.avg_kda,
                                avg_damage_taken         = excluded.avg_damage_taken,
                                avg_damage_to_champions  = excluded.avg_damage_to_champions,
                                avg_damage_to_objectives = excluded.avg_damage_to_objectives,
                                avg_vision_score         = excluded.avg_vision_score,
                                avg_wards_placed         = excluded.avg_wards_placed,
                                avg_wards_killed         = excluded.avg_wards_killed,
                                avg_control_wards_bought = excluded.avg_control_wards_bought,
                                pick_rate                = excluded.pick_rate,
                                power_phase_early        = excluded.power_phase_early,
                                power_phase_mid          = excluded.power_phase_mid,
                                power_phase_late         = excluded.power_phase_late,
                                synced_at                = CURRENT_TIMESTAMP",
                            rusqlite::params![
                                c.champion_id,
                                c.role,
                                c.tier,
                                c.win_rate,
                                c.wins,
                                c.losses,
                                c.total_games,
                                c.avg_kills,
                                c.avg_deaths,
                                c.avg_assists,
                                c.avg_kda,
                                c.avg_damage_taken,
                                c.avg_damage_to_champions,
                                c.avg_damage_to_objectives,
                                c.avg_vision_score,
                                c.avg_wards_placed,
                                c.avg_wards_killed,
                                c.avg_control_wards_bought,
                                c.pick_rate,
                                c.power_phases.early,
                                c.power_phases.mid,
                                c.power_phases.late,
                            ],
                        );
                        match result {
                            Ok(_) => synced += 1,
                            Err(e) => {
                                tracing::warn!("Falha ao inserir campeão {}: {e}", c.champion_id);
                                errors += 1;
                            }
                        }
                    }
                    combinations_ok.push(format!("{role}/{tier}"));
                    tracing::debug!(
                        "SpellCoach sync: {role}/{tier} — {} campeões",
                        champions.len()
                    );
                }
                Err(e) => {
                    tracing::warn!("SpellCoach sync falhou para {role}/{tier}: {e}");
                    errors += 1;
                }
            }
        }
    }

    tracing::info!(
        "SpellCoach sync concluído: {synced} registros, {errors} erros"
    );
    Ok(SpellCoachSyncResult { synced, errors, combinations_ok })
}

/// Retorna dados de um campeão pelo ID numérico.
#[tauri::command]
pub async fn get_champion_data(
    id: i64,
    state: State<'_, AppState>,
) -> Result<Champion, String> {
    let db = state.db.lock().await;

    db.query_row(
        "SELECT champion_id, name, key, data_json FROM champion_cache WHERE champion_id = ?1",
        [id],
        |row| {
            let data_json: String = row.get(3)?;
            let data: serde_json::Value =
                serde_json::from_str(&data_json).unwrap_or_default();
            Ok(Champion {
                id:    row.get(0)?,
                name:  row.get(1)?,
                key:   row.get(2)?,
                title: data["title"].as_str().unwrap_or("").to_string(),
            })
        },
    ).map_err(|_| format!("Campeão {id} não encontrado no cache"))
}

/// Retorna todos os campeões do cache local.
#[tauri::command]
pub async fn get_all_champions(state: State<'_, AppState>) -> Result<Vec<Champion>, String> {
    let db = state.db.lock().await;

    let mut stmt = db
        .prepare("SELECT champion_id, name, key, data_json FROM champion_cache ORDER BY name")
        .map_err(|e| e.to_string())?;

    let champions = stmt
        .query_map([], |row| {
            let data_json: String = row.get(3)?;
            let data: serde_json::Value =
                serde_json::from_str(&data_json).unwrap_or_default();
            Ok(Champion {
                id:    row.get(0)?,
                name:  row.get(1)?,
                key:   row.get(2)?,
                title: data["title"].as_str().unwrap_or("").to_string(),
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(champions)
}

/// Retorna análise de matchup entre dois campeões (stub legado).
#[tauri::command]
pub async fn get_champion_matchup(
    champion_id: i64,
    enemy_id: i64,
    _state: State<'_, AppState>,
) -> Result<ChampionMatchup, String> {
    tracing::debug!("get_champion_matchup({champion_id} vs {enemy_id}) — stub");
    Ok(ChampionMatchup {
        difficulty_score: 5,
        is_favorable:     false,
        main_tip:         "Use get_matchup_stats para dados reais.".to_string(),
        power_spikes:     vec![],
    })
}

/// Retorna estatísticas de meta de um campeão.
/// Lê do banco local (populado via sync_spellcoach_data).
/// Fallback para a API em tempo real se o banco estiver vazio.
#[tauri::command]
pub async fn get_champion_stats(
    champion_id: i64,
    state: State<'_, AppState>,
) -> Result<ChampionMetaStatResponse, String> {
    // Tenta o banco local primeiro
    {
        let db = state.db.lock().await;
        if let Some(stat) = query_meta_stat(&db, champion_id) {
            return Ok(meta_stat_to_response(&stat));
        }
    }

    // Fallback: consulta a API em tempo real
    let client = state
        .spellcoach_client
        .as_ref()
        .ok_or_else(|| format!("Campeão {champion_id} não encontrado no banco e SPELLCOACH_API_KEY não configurada"))?;

    let data = client
        .get_champion_stats(champion_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ChampionMetaStatResponse {
        champion_id:            data.champion_id,
        patch:                  "current".to_string(),
        elo:                    data.tier.clone(),
        role:                   data.role.clone(),
        games_played:           data.total_games,
        win_rate:               data.win_rate * 100.0,
        kda:                    KdaDetail {
            kills:   data.avg_kills,
            deaths:  data.avg_deaths,
            assists: data.avg_assists,
            ratio:   format!("{:.2}", data.avg_kda),
        },
        average_cs_per_min:    0.0,
        average_gold_per_min:  0.0,
        average_damage_per_min: data.avg_damage_to_champions,
        average_vision_score:  data.avg_vision_score,
    })
}

/// Retorna recomendação de runas e spells para o campeão e role selecionados.
/// Usa win rate do banco local se disponível.
#[tauri::command]
pub async fn get_pick_recommendation(
    champion_id: i64,
    role: String,
    state: State<'_, AppState>,
) -> Result<PickRecommendation, String> {
    let role_upper = role.to_uppercase();
    let d = role_defaults(&role_upper);

    // Mapeamento entre roles da LCU e roles da SpellCoach API
    let api_role = match role_upper.as_str() {
        "MIDDLE"  => "MID",
        "BOTTOM"  => "ADC",
        "UTILITY" => "SUPPORT",
        other     => other,
    };

    let (meta_win_rate, meta_games) = {
        let db = state.db.lock().await;
        if let Some(stat) = query_meta_stat_by_role(&db, champion_id, api_role)
            .or_else(|| query_meta_stat(&db, champion_id))
        {
            (Some(stat.win_rate * 100.0), Some(stat.total_games))
        } else {
            // Fallback para a API se o banco estiver vazio
            drop(db);
            if let Some(client) = &state.spellcoach_client {
                match client.get_champion_stats(champion_id).await {
                    Ok(s) => (Some(s.win_rate * 100.0), Some(s.total_games)),
                    Err(_) => (None, None),
                }
            } else {
                (None, None)
            }
        }
    };

    // ── Sync automático de assets no lock-in ─────────────────
    // Fire-and-forget: não bloqueia a resposta ao frontend.
    // Throttle de 12h: só sincroniza se o dado não existir ou estiver desatualizado.
    if let Some(client) = state.spellcoach_client.as_ref() {
        let (champ_name_opt, needs_sync): (Option<String>, bool) = {
            let db = state.db.lock().await;
            let champ_name: Option<String> = db.query_row(
                "SELECT name FROM champion_cache WHERE champion_id = ?1",
                [champion_id],
                |row| row.get(0),
            ).ok();

            let stale = champ_name.as_deref().map_or(false, |name| {
                db.query_row(
                    "SELECT COUNT(*) = 0 OR MAX(synced_at < datetime('now', '-12 hours'))
                     FROM champion_builds WHERE champion_name = ?1",
                    [name],
                    |row| row.get::<_, bool>(0),
                ).unwrap_or(true)
            });

            (champ_name, stale)
        };

        if needs_sync {
            if let Some(champ_name) = champ_name_opt {
                let db_bg     = Arc::clone(&state.db);
                let client_bg = Arc::clone(client);
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::spellcoach::sync::sync_champion_assets(
                        db_bg, client_bg, &champ_name,
                    ).await {
                        tracing::warn!("Asset sync falhou para {champ_name}: {e}");
                    }
                });
            }
        } else {
            tracing::debug!("Asset sync ignorado — dados recentes (< 12h).");
        }
    }

    Ok(PickRecommendation {
        role:               role_upper,
        keystone:           d.keystone.to_string(),
        primary_path:       d.primary_path.to_string(),
        secondary_path:     d.secondary_path.to_string(),
        spell1_name:        d.spell1_name.to_string(),
        spell2_name:        d.spell2_name.to_string(),
        spell1_id:          d.spell1_id,
        spell2_id:          d.spell2_id,
        primary_path_id:    d.primary_path_id,
        primary_rune_ids:   d.primary_rune_ids.to_vec(),
        secondary_path_id:  d.secondary_path_id,
        secondary_rune_ids: d.secondary_rune_ids.to_vec(),
        stat_shard_ids:     d.stat_shard_ids.to_vec(),
        meta_win_rate,
        meta_games,
        patch:              None,
    })
}

/// Retorna estatísticas de matchup entre dois campeões.
/// Usa win rates individuais do banco local em escala 0-100.
#[tauri::command]
pub async fn get_matchup_stats(
    champion_a_id: i64,
    champion_b_id: i64,
    state: State<'_, AppState>,
) -> Result<MatchupStatsResponse, String> {
    let db = state.db.lock().await;

    let stat_a = query_meta_stat(&db, champion_a_id);
    let stat_b = query_meta_stat(&db, champion_b_id);

    if stat_a.is_none() && stat_b.is_none() {
        // Fallback para a API se o banco estiver vazio
        drop(db);
        let client = state
            .spellcoach_client
            .as_ref()
            .ok_or_else(|| "Dados de matchup não disponíveis — execute sync_spellcoach_data primeiro".to_string())?;

        let ms = client
            .get_matchup(champion_a_id, champion_b_id)
            .await
            .map_err(|e| e.to_string())?;

        return Ok(MatchupStatsResponse {
            champion_a_id: ms.champion_a_id,
            champion_b_id: ms.champion_b_id,
            patch:         ms.patch,
            elo:           ms.elo,
            games_played:  ms.games_played,
            win_rate_a:    ms.win_rate_a,
            win_rate_b:    ms.win_rate_b,
        });
    }

    Ok(MatchupStatsResponse {
        champion_a_id,
        champion_b_id,
        patch:        "current".to_string(),
        elo:          stat_a.as_ref().map(|s| s.tier.clone()).unwrap_or_default(),
        games_played: stat_a.as_ref().map(|s| s.total_games).unwrap_or(0),
        win_rate_a:   stat_a.as_ref().map(|s| s.win_rate * 100.0).unwrap_or(50.0),
        win_rate_b:   stat_b.as_ref().map(|s| s.win_rate * 100.0).unwrap_or(50.0),
    })
}
