// ============================================================
// commands/builds.rs — Builds, runas, itens e wards via SpellCoach
// ============================================================

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

// ── Structs de resposta ───────────────────────────────────────

/// Build completa de um campeão lida do banco local.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionBuildResponse {
    pub champion_name: String,
    pub role:          String,
    pub tier:          String,
    pub runes:         Option<serde_json::Value>,
    pub items:         Option<serde_json::Value>,
    pub skills:        Option<serde_json::Value>,
    pub spikes:        Option<serde_json::Value>,
    pub synced_at:     Option<String>,
}

/// Dados de ward de um campeão (um lado) lidos do banco local.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionWardsResponse {
    pub champion_name: String,
    pub side:          String,
    pub role:          String,
    pub tier:          String,
    pub total_games:   i64,
    pub placements:    Option<serde_json::Value>,
    pub heatmap:       Option<serde_json::Value>,
    pub synced_at:     Option<String>,
}

/// Resultado do sync de build + wards de um campeão.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionAssetSyncResult {
    pub champion_name: String,
    pub build_synced:  bool,
    pub wards_synced:  bool,
    pub error:         Option<String>,
}

// ── Helpers de banco ──────────────────────────────────────────

fn parse_json_col(raw: Option<String>) -> Option<serde_json::Value> {
    raw.and_then(|s| serde_json::from_str(&s).ok())
}

// ── Commands ──────────────────────────────────────────────────

/// Sincroniza build completa (runas + itens + habilidades + spikes) e wards
/// de um campeão específico para o banco local.
/// Chamado automaticamente ao lock-in no champ select ou manualmente.
#[tauri::command]
pub async fn sync_champion_assets(
    champion_name: String,
    state: State<'_, AppState>,
) -> Result<ChampionAssetSyncResult, String> {
    let client = state
        .spellcoach_client
        .as_ref()
        .ok_or_else(|| "SPELLCOACH_API_KEY não configurada".to_string())?
        .clone();

    let mut build_synced = false;
    let mut wards_synced = false;
    let mut error_msg: Option<String> = None;

    // ── Sync de builds ────────────────────────────────────────
    let (runes_res, items_res, skills_res, spikes_res) = tokio::join!(
        client.get_champion_runes(&champion_name),
        client.get_champion_items(&champion_name),
        client.get_champion_skills(&champion_name),
        client.get_champion_spikes(&champion_name),
    );

    let runes_json  = runes_res.ok().and_then(|v| v.get("data").cloned());
    let items_json  = items_res.ok().and_then(|v| v.get("data").cloned());
    let skills_json = skills_res.ok().and_then(|v| v.get("data").cloned());
    let spikes_json = spikes_res.ok().and_then(|v| v.get("data").cloned());

    // Deriva role e tier dos dados disponíveis
    let role = runes_json.as_ref()
        .and_then(|v| v.get("role").and_then(|r| r.as_str()))
        .or_else(|| items_json.as_ref().and_then(|v| v.get("role").and_then(|r| r.as_str())))
        .unwrap_or("")
        .to_string();

    let tier = runes_json.as_ref()
        .and_then(|v| v.get("tier").and_then(|t| t.as_str()))
        .or_else(|| items_json.as_ref().and_then(|v| v.get("tier").and_then(|t| t.as_str())))
        .unwrap_or("")
        .to_string();

    {
        let db = state.db.lock().await;
        let result = db.execute(
            "INSERT INTO champion_builds (champion_name, role, tier, runes_json, items_json, skills_json, spikes_json, synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
             ON CONFLICT(champion_name) DO UPDATE SET
                role        = excluded.role,
                tier        = excluded.tier,
                runes_json  = excluded.runes_json,
                items_json  = excluded.items_json,
                skills_json = excluded.skills_json,
                spikes_json = excluded.spikes_json,
                synced_at   = CURRENT_TIMESTAMP",
            rusqlite::params![
                champion_name,
                role,
                tier,
                runes_json.as_ref().map(|v| v.to_string()),
                items_json.as_ref().map(|v| v.to_string()),
                skills_json.as_ref().map(|v| v.to_string()),
                spikes_json.as_ref().map(|v| v.to_string()),
            ],
        );
        match result {
            Ok(_) => build_synced = true,
            Err(e) => error_msg = Some(format!("Erro ao salvar build: {e}")),
        }
    }

    // ── Sync de wards ─────────────────────────────────────────
    match client.get_champion_wards(&champion_name).await {
        Ok(wards_resp) => {
            if let Some(ward_list) = wards_resp.get("data").and_then(|d| d.as_array()) {
                let heatmap_resp = client.get_champion_ward_heatmap(&champion_name).await.ok();
                let heatmap_points = heatmap_resp
                    .as_ref()
                    .and_then(|v| v.get("data"))
                    .and_then(|d| d.get("points"))
                    .cloned();

                let db = state.db.lock().await;
                let mut all_ok = true;

                for ward_entry in ward_list {
                    let side = ward_entry.get("side")
                        .and_then(|s| s.as_str())
                        .unwrap_or("UNKNOWN");
                    let w_role = ward_entry.get("role")
                        .and_then(|r| r.as_str())
                        .unwrap_or("");
                    let w_tier = ward_entry.get("tier")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    let total_games = ward_entry.get("totalGames")
                        .and_then(|g| g.as_i64())
                        .unwrap_or(0);
                    let placements = ward_entry.get("wardPlacements").cloned();

                    // Heatmap filtrado pelo lado
                    let side_heatmap = heatmap_points.as_ref().map(|pts| {
                        if let Some(arr) = pts.as_array() {
                            let filtered: Vec<&serde_json::Value> = arr
                                .iter()
                                .filter(|p| {
                                    p.get("side")
                                        .and_then(|s| s.as_str())
                                        .map(|s| s == side)
                                        .unwrap_or(false)
                                })
                                .collect();
                            serde_json::Value::Array(
                                filtered.into_iter().cloned().collect(),
                            )
                        } else {
                            serde_json::Value::Array(vec![])
                        }
                    });

                    let res = db.execute(
                        "INSERT INTO champion_wards
                            (champion_name, side, role, tier, total_games, placements_json, heatmap_json, synced_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
                         ON CONFLICT(champion_name, side, role, tier) DO UPDATE SET
                            total_games     = excluded.total_games,
                            placements_json = excluded.placements_json,
                            heatmap_json    = excluded.heatmap_json,
                            synced_at       = CURRENT_TIMESTAMP",
                        rusqlite::params![
                            champion_name,
                            side,
                            w_role,
                            w_tier,
                            total_games,
                            placements.map(|v| v.to_string()),
                            side_heatmap.map(|v| v.to_string()),
                        ],
                    );

                    if res.is_err() {
                        all_ok = false;
                    }
                }

                wards_synced = all_ok;
            }
        }
        Err(e) => {
            let msg = format!("Erro ao buscar wards: {e}");
            tracing::warn!("{msg}");
            if error_msg.is_none() {
                error_msg = Some(msg);
            }
        }
    }

    tracing::info!(
        "sync_champion_assets({champion_name}): build={build_synced} wards={wards_synced}"
    );

    Ok(ChampionAssetSyncResult { champion_name, build_synced, wards_synced, error: error_msg })
}

/// Retorna a build completa de um campeão do banco local.
/// Fallback automático para a API se o banco estiver vazio.
#[tauri::command]
pub async fn get_champion_build(
    champion_name: String,
    state: State<'_, AppState>,
) -> Result<ChampionBuildResponse, String> {
    {
        let db = state.db.lock().await;
        let result = db.query_row(
            "SELECT champion_name, role, tier, runes_json, items_json, skills_json, spikes_json, synced_at
             FROM champion_builds WHERE champion_name = ?1",
            [&champion_name],
            |row| {
                Ok(ChampionBuildResponse {
                    champion_name: row.get(0)?,
                    role:          row.get(1)?,
                    tier:          row.get(2)?,
                    runes:         parse_json_col(row.get(3)?),
                    items:         parse_json_col(row.get(4)?),
                    skills:        parse_json_col(row.get(5)?),
                    spikes:        parse_json_col(row.get(6)?),
                    synced_at:     row.get(7)?,
                })
            },
        );
        if let Ok(build) = result {
            return Ok(build);
        }
    }

    // Fallback: busca da API e salva no banco
    let client = state
        .spellcoach_client
        .as_ref()
        .ok_or_else(|| format!("Build de '{champion_name}' não encontrada no banco e SPELLCOACH_API_KEY não configurada"))?
        .clone();

    // Dispara sync em background e retorna os dados da API diretamente
    let (runes_res, items_res, skills_res, spikes_res) = tokio::join!(
        client.get_champion_runes(&champion_name),
        client.get_champion_items(&champion_name),
        client.get_champion_skills(&champion_name),
        client.get_champion_spikes(&champion_name),
    );

    let runes  = runes_res.ok().and_then(|v| v.get("data").cloned());
    let items  = items_res.ok().and_then(|v| v.get("data").cloned());
    let skills = skills_res.ok().and_then(|v| v.get("data").cloned());
    let spikes = spikes_res.ok().and_then(|v| v.get("data").cloned());

    let role = runes.as_ref()
        .and_then(|v| v.get("role").and_then(|r| r.as_str()))
        .unwrap_or("")
        .to_string();
    let tier = runes.as_ref()
        .and_then(|v| v.get("tier").and_then(|t| t.as_str()))
        .unwrap_or("")
        .to_string();

    // Persiste no banco para próximas consultas
    {
        let db = state.db.lock().await;
        let _ = db.execute(
            "INSERT OR REPLACE INTO champion_builds
                (champion_name, role, tier, runes_json, items_json, skills_json, spikes_json, synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)",
            rusqlite::params![
                champion_name,
                role,
                tier,
                runes.as_ref().map(|v| v.to_string()),
                items.as_ref().map(|v| v.to_string()),
                skills.as_ref().map(|v| v.to_string()),
                spikes.as_ref().map(|v| v.to_string()),
            ],
        );
    }

    Ok(ChampionBuildResponse {
        champion_name,
        role,
        tier,
        runes,
        items,
        skills,
        spikes,
        synced_at: None,
    })
}

/// Retorna dados de ward de um campeão do banco local.
/// Se `side` for None, retorna ambos os lados (BLUE e RED).
#[tauri::command]
pub async fn get_champion_wards(
    champion_name: String,
    side: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ChampionWardsResponse>, String> {
    {
        let db = state.db.lock().await;
        let (query, params_vec): (&str, Vec<String>) = if let Some(ref s) = side {
            (
                "SELECT champion_name, side, role, tier, total_games, placements_json, heatmap_json, synced_at
                 FROM champion_wards WHERE champion_name = ?1 AND side = ?2",
                vec![champion_name.clone(), s.to_uppercase()],
            )
        } else {
            (
                "SELECT champion_name, side, role, tier, total_games, placements_json, heatmap_json, synced_at
                 FROM champion_wards WHERE champion_name = ?1",
                vec![champion_name.clone()],
            )
        };

        let mut stmt = db.prepare(query).map_err(|e| e.to_string())?;

        let rows: Vec<ChampionWardsResponse> = stmt
            .query_map(rusqlite::params_from_iter(params_vec.iter()), |row| {
                Ok(ChampionWardsResponse {
                    champion_name: row.get(0)?,
                    side:          row.get(1)?,
                    role:          row.get(2)?,
                    tier:          row.get(3)?,
                    total_games:   row.get(4)?,
                    placements:    parse_json_col(row.get(5)?),
                    heatmap:       parse_json_col(row.get(6)?),
                    synced_at:     row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        if !rows.is_empty() {
            return Ok(rows);
        }
    }

    // Fallback: busca da API
    let client = state
        .spellcoach_client
        .as_ref()
        .ok_or_else(|| format!("Wards de '{champion_name}' não encontrados — execute sync_champion_assets primeiro"))?
        .clone();

    let wards_resp = client
        .get_champion_wards(&champion_name)
        .await
        .map_err(|e| e.to_string())?;

    let heatmap_resp = client.get_champion_ward_heatmap(&champion_name).await.ok();
    let heatmap_points = heatmap_resp
        .as_ref()
        .and_then(|v| v.get("data"))
        .and_then(|d| d.get("points"))
        .cloned();

    let ward_list = wards_resp
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();

    let side_filter = side.as_deref().map(str::to_uppercase);

    let results: Vec<ChampionWardsResponse> = ward_list
        .iter()
        .filter(|entry| {
            if let Some(ref sf) = side_filter {
                entry.get("side").and_then(|s| s.as_str()) == Some(sf.as_str())
            } else {
                true
            }
        })
        .map(|entry| {
            let entry_side = entry.get("side").and_then(|s| s.as_str()).unwrap_or("").to_string();

            let side_heatmap = heatmap_points.as_ref().map(|pts| {
                if let Some(arr) = pts.as_array() {
                    let filtered: Vec<_> = arr
                        .iter()
                        .filter(|p| {
                            p.get("side").and_then(|s| s.as_str()) == Some(entry_side.as_str())
                        })
                        .cloned()
                        .collect();
                    serde_json::Value::Array(filtered)
                } else {
                    serde_json::Value::Array(vec![])
                }
            });

            ChampionWardsResponse {
                champion_name: champion_name.clone(),
                side:          entry_side,
                role:          entry.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string(),
                tier:          entry.get("tier").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                total_games:   entry.get("totalGames").and_then(|g| g.as_i64()).unwrap_or(0),
                placements:    entry.get("wardPlacements").cloned(),
                heatmap:       side_heatmap,
                synced_at:     None,
            }
        })
        .collect();

    Ok(results)
}
