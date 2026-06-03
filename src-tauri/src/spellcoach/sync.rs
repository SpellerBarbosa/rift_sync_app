// ============================================================
// spellcoach/sync.rs — Sincronização com progresso
//
// A API SpellCoach não suporta filtros role/tier como query params.
// Estratégia correta: buscar TODOS os registros de uma vez (~6 requests
// com limit=1000) e inserir diretamente no banco.
//
// Eventos Tauri emitidos: "sync_progress"
//   { phase: "meta_stats"|"builds"|"done", current, total, label }
// ============================================================

use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;
use tauri::Emitter;
use tokio::sync::Mutex;

use super::SpellCoachClient;

pub const ROLES: &[&str] = &["ADC", "TOP", "MID", "JUNGLE", "SUPPORT"];
pub const TIERS: &[&str] = &[
    "CHALLENGER", "GRANDMASTER", "MASTER",
    "DIAMOND", "EMERALD", "GOLD", "SILVER", "BRONZE", "IRON",
];

// ── Evento de progresso ───────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub phase:   String,
    pub current: usize,
    pub total:   usize,
    pub label:   String,
}

fn emit_progress(app: &tauri::AppHandle, p: SyncProgress) {
    let _ = app.emit("sync_progress", p);
}

// ── SQL helper ────────────────────────────────────────────────

const UPSERT_META_SQL: &str = "
    INSERT INTO champion_meta_stats (
        champion_id, role, tier, win_rate, wins, losses, total_games,
        avg_kills, avg_deaths, avg_assists, avg_kda,
        avg_damage_taken, avg_damage_to_champions, avg_damage_to_objectives,
        avg_vision_score, avg_wards_placed, avg_wards_killed,
        avg_control_wards_bought, pick_rate,
        power_phase_early, power_phase_mid, power_phase_late,
        synced_at
    ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,CURRENT_TIMESTAMP)
    ON CONFLICT(champion_id, role, tier) DO UPDATE SET
        win_rate = excluded.win_rate, wins = excluded.wins, losses = excluded.losses,
        total_games = excluded.total_games, avg_kills = excluded.avg_kills,
        avg_deaths = excluded.avg_deaths, avg_assists = excluded.avg_assists,
        avg_kda = excluded.avg_kda, avg_damage_taken = excluded.avg_damage_taken,
        avg_damage_to_champions = excluded.avg_damage_to_champions,
        avg_damage_to_objectives = excluded.avg_damage_to_objectives,
        avg_vision_score = excluded.avg_vision_score,
        avg_wards_placed = excluded.avg_wards_placed,
        avg_wards_killed = excluded.avg_wards_killed,
        avg_control_wards_bought = excluded.avg_control_wards_bought,
        pick_rate = excluded.pick_rate,
        power_phase_early = excluded.power_phase_early,
        power_phase_mid = excluded.power_phase_mid,
        power_phase_late = excluded.power_phase_late,
        synced_at = CURRENT_TIMESTAMP";

// ── Sincronização completa ────────────────────────────────────

/// Sincroniza meta stats (fase 1) e builds (fase 2).
/// A API não suporta filtros — busca tudo de uma vez e insere diretamente.
pub async fn full_sync_with_progress(
    db:     Arc<Mutex<Connection>>,
    client: Arc<SpellCoachClient>,
    app:    &tauri::AppHandle,
) -> (i64, i64) {
    let mut ok:  i64 = 0;
    let mut err: i64 = 0;

    // ── Fase 1: busca todos os ~5500 registros em ~6 requests ──
    emit_progress(app, SyncProgress {
        phase:   "meta_stats".into(),
        current: 0,
        total:   0,
        label:   "Baixando dados da API...".into(),
    });

    let all_champs = match client.get_all_champions().await {
        Ok(v)  => {
            tracing::info!("SpellCoach: {} registros recebidos da API", v.len());
            v
        }
        Err(e) => {
            tracing::warn!("Falha ao buscar dados da API: {e}");
            err += 1;
            Vec::new()
        }
    };

    let total_records = all_champs.len();
    let mut unique_names: std::collections::HashSet<String> = Default::default();

    // Insere em batch, emitindo progresso a cada 200 registros
    {
        let db = db.lock().await;
        for (i, c) in all_champs.iter().enumerate() {
            if !c.champion_name.is_empty() {
                unique_names.insert(c.champion_name.clone());
            }

            if i % 200 == 0 {
                emit_progress(app, SyncProgress {
                    phase:   "meta_stats".into(),
                    current: i + 1,
                    total:   total_records,
                    label:   format!("{} {}", c.champion_name, c.role),
                });
            }

            let res = db.execute(
                UPSERT_META_SQL,
                rusqlite::params![
                    c.champion_id, c.role, c.tier,
                    c.win_rate, c.wins, c.losses, c.total_games,
                    c.avg_kills, c.avg_deaths, c.avg_assists, c.avg_kda,
                    c.avg_damage_taken, c.avg_damage_to_champions,
                    c.avg_damage_to_objectives, c.avg_vision_score,
                    c.avg_wards_placed, c.avg_wards_killed,
                    c.avg_control_wards_bought, c.pick_rate,
                    c.power_phases.early, c.power_phases.mid, c.power_phases.late,
                ],
            );
            if res.is_ok() { ok += 1 } else { err += 1 }
        }
    }

    tracing::info!("Meta stats: {} registros inseridos", ok);

    // Fallback: se a API não retornou nomes, usa champion_cache local
    if unique_names.is_empty() {
        if let Ok(db) = db.try_lock() {
            if let Ok(mut stmt) = db.prepare(
                "SELECT name FROM champion_cache WHERE name != '' ORDER BY name"
            ) {
                let names: Vec<String> = stmt
                    .query_map([], |row| row.get::<_, String>(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default();
                for n in names { unique_names.insert(n); }
            }
        }
    }

    // ── Fase 2: builds por campeão (paralelo) ─────────────────
    let champ_names: Vec<String> = unique_names.into_iter().collect();
    let total_champs = champ_names.len();
    let counter = Arc::new(AtomicUsize::new(0));
    let sem = Arc::new(tokio::sync::Semaphore::new(8));
    let mut join_set = tokio::task::JoinSet::new();

    for name in champ_names {
        let db_arc    = Arc::clone(&db);
        let cli_arc   = Arc::clone(&client);
        let sem_arc   = Arc::clone(&sem);
        let app_clone = app.clone();
        let cnt_arc   = Arc::clone(&counter);

        join_set.spawn(async move {
            let _permit = sem_arc.acquire().await.unwrap();
            let current = cnt_arc.fetch_add(1, Ordering::Relaxed) + 1;
            emit_progress(&app_clone, SyncProgress {
                phase:   "builds".into(),
                current,
                total:   total_champs,
                label:   name.clone(),
            });
            sync_champion_assets(db_arc, cli_arc, &name).await
        });
    }

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(())) => ok  += 1,
            _           => err += 1,
        }
    }

    emit_progress(app, SyncProgress {
        phase:   "done".into(),
        current: 0, total: 0, label: String::new(),
    });

    tracing::info!("SpellCoach full sync: {ok} ok, {err} erros");
    (ok, err)
}

// ── Compat: sync apenas meta stats sem eventos ────────────────

pub async fn sync_all_meta_stats(
    db:     Arc<Mutex<Connection>>,
    client: Arc<SpellCoachClient>,
) -> Result<(i64, i64)> {
    let all = client.get_all_champions().await?;
    let mut ok:  i64 = 0;
    let mut err: i64 = 0;

    let db = db.lock().await;
    for c in &all {
        let res = db.execute(UPSERT_META_SQL, rusqlite::params![
            c.champion_id, c.role, c.tier,
            c.win_rate, c.wins, c.losses, c.total_games,
            c.avg_kills, c.avg_deaths, c.avg_assists, c.avg_kda,
            c.avg_damage_taken, c.avg_damage_to_champions,
            c.avg_damage_to_objectives, c.avg_vision_score,
            c.avg_wards_placed, c.avg_wards_killed,
            c.avg_control_wards_bought, c.pick_rate,
            c.power_phases.early, c.power_phases.mid, c.power_phases.late,
        ]);
        if res.is_ok() { ok += 1 } else { err += 1 }
    }

    Ok((ok, err))
}

// ── Sync de build + wards de um campeão ──────────────────────

pub async fn sync_champion_assets(
    db:            Arc<Mutex<Connection>>,
    client:        Arc<SpellCoachClient>,
    champion_name: &str,
) -> Result<()> {
    let (runes_r, items_r, skills_r, spikes_r) = tokio::join!(
        client.get_champion_runes(champion_name),
        client.get_champion_items(champion_name),
        client.get_champion_skills(champion_name),
        client.get_champion_spikes(champion_name),
    );

    let runes  = runes_r.ok().and_then(|v| v.get("data").cloned());
    let items  = items_r.ok().and_then(|v| v.get("data").cloned());
    let skills = skills_r.ok().and_then(|v| v.get("data").cloned());
    let spikes = spikes_r.ok().and_then(|v| v.get("data").cloned());

    let role = runes.as_ref().and_then(|v| v.get("role").and_then(|r| r.as_str())).unwrap_or("").to_string();
    let tier = runes.as_ref().and_then(|v| v.get("tier").and_then(|t| t.as_str())).unwrap_or("").to_string();

    {
        let db = db.lock().await;
        db.execute(
            "INSERT INTO champion_builds
                (champion_name, role, tier, runes_json, items_json, skills_json, spikes_json, synced_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,CURRENT_TIMESTAMP)
             ON CONFLICT(champion_name) DO UPDATE SET
                role = excluded.role, tier = excluded.tier,
                runes_json = excluded.runes_json, items_json = excluded.items_json,
                skills_json = excluded.skills_json, spikes_json = excluded.spikes_json,
                synced_at = CURRENT_TIMESTAMP",
            rusqlite::params![
                champion_name, role, tier,
                runes.as_ref().map(|v| v.to_string()),
                items.as_ref().map(|v| v.to_string()),
                skills.as_ref().map(|v| v.to_string()),
                spikes.as_ref().map(|v| v.to_string()),
            ],
        )?;
    }

    let (wards_r, heatmap_r) = tokio::join!(
        client.get_champion_wards(champion_name),
        client.get_champion_ward_heatmap(champion_name),
    );

    if let Ok(wards_resp) = wards_r {
        let heatmap_pts = heatmap_r.ok()
            .and_then(|v| v.get("data").cloned())
            .and_then(|d| d.get("points").cloned());

        if let Some(list) = wards_resp.get("data").and_then(|d| d.as_array()) {
            let db = db.lock().await;
            for entry in list {
                let side        = entry.get("side").and_then(|s| s.as_str()).unwrap_or("UNKNOWN");
                let w_role      = entry.get("role").and_then(|r| r.as_str()).unwrap_or("");
                let w_tier      = entry.get("tier").and_then(|t| t.as_str()).unwrap_or("");
                let total_games = entry.get("totalGames").and_then(|g| g.as_i64()).unwrap_or(0);
                let placements  = entry.get("wardPlacements").cloned();

                let side_heatmap = heatmap_pts.as_ref().and_then(|pts| pts.as_array()).map(|arr| {
                    serde_json::Value::Array(
                        arr.iter()
                            .filter(|p| p.get("side").and_then(|s| s.as_str()) == Some(side))
                            .cloned()
                            .collect(),
                    )
                });

                let _ = db.execute(
                    "INSERT INTO champion_wards
                        (champion_name, side, role, tier, total_games, placements_json, heatmap_json, synced_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,CURRENT_TIMESTAMP)
                     ON CONFLICT(champion_name, side, role, tier) DO UPDATE SET
                        total_games = excluded.total_games,
                        placements_json = excluded.placements_json,
                        heatmap_json = excluded.heatmap_json,
                        synced_at = CURRENT_TIMESTAMP",
                    rusqlite::params![
                        champion_name, side, w_role, w_tier, total_games,
                        placements.map(|v| v.to_string()),
                        side_heatmap.map(|v| v.to_string()),
                    ],
                );
            }
        }
    }

    Ok(())
}
