// ============================================================
// spellcoach/sync.rs — Lógica de sincronização standalone
//
// Funções que recebem Arc<DB> + Arc<Client> diretamente,
// sem depender de tauri::State. Usadas tanto pelos Tauri commands
// quanto pelas tarefas de background em lib.rs.
// ============================================================

use std::sync::Arc;

use anyhow::Result;
use rusqlite::Connection;
use tokio::sync::Mutex;

use super::SpellCoachClient;

const ROLES: &[&str] = &["ADC", "TOP", "MID", "JUNGLE", "SUPPORT"];
const TIERS: &[&str] = &[
    "CHALLENGER", "GRANDMASTER", "MASTER",
    "DIAMOND", "EMERALD", "GOLD", "SILVER", "BRONZE", "IRON",
];

// ── Meta stats ────────────────────────────────────────────────

/// Sincroniza estatísticas de meta (champion_meta_stats) para todas as
/// combinações de role × tier. Retorna (synced, errors).
pub async fn sync_all_meta_stats(
    db:     Arc<Mutex<Connection>>,
    client: Arc<SpellCoachClient>,
) -> Result<(i64, i64)> {
    let mut synced: i64 = 0;
    let mut errors: i64 = 0;

    for role in ROLES {
        for tier in TIERS {
            match client.get_champions(role, tier).await {
                Ok(champions) => {
                    let db = db.lock().await;
                    for c in &champions {
                        let res = db.execute(
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
                        if res.is_ok() { synced += 1 } else { errors += 1 }
                    }
                    tracing::debug!("SpellCoach meta sync: {role}/{tier} — {} registros", champions.len());
                }
                Err(e) => {
                    tracing::warn!("SpellCoach meta sync falhou para {role}/{tier}: {e}");
                    errors += 1;
                }
            }
        }
    }

    tracing::info!("SpellCoach meta sync concluído: {synced} ok, {errors} erros");
    Ok((synced, errors))
}

// ── Build + wards de um campeão ───────────────────────────────

/// Sincroniza build completa e wards de um campeão para o banco local.
/// Chamada automaticamente no lock-in do champ select.
pub async fn sync_champion_assets(
    db:            Arc<Mutex<Connection>>,
    client:        Arc<SpellCoachClient>,
    champion_name: &str,
) -> Result<()> {
    // Busca runas, itens, habilidades e spikes em paralelo
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

    let role = runes.as_ref()
        .and_then(|v| v.get("role").and_then(|r| r.as_str()))
        .unwrap_or("")
        .to_string();
    let tier = runes.as_ref()
        .and_then(|v| v.get("tier").and_then(|t| t.as_str()))
        .unwrap_or("")
        .to_string();

    {
        let db = db.lock().await;
        db.execute(
            "INSERT INTO champion_builds
                (champion_name, role, tier, runes_json, items_json, skills_json, spikes_json, synced_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7, CURRENT_TIMESTAMP)
             ON CONFLICT(champion_name) DO UPDATE SET
                role        = excluded.role,
                tier        = excluded.tier,
                runes_json  = excluded.runes_json,
                items_json  = excluded.items_json,
                skills_json = excluded.skills_json,
                spikes_json = excluded.spikes_json,
                synced_at   = CURRENT_TIMESTAMP",
            rusqlite::params![
                champion_name, role, tier,
                runes.as_ref().map(|v| v.to_string()),
                items.as_ref().map(|v| v.to_string()),
                skills.as_ref().map(|v| v.to_string()),
                spikes.as_ref().map(|v| v.to_string()),
            ],
        )?;
    }

    // Wards + heatmap em paralelo
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
                let side = entry.get("side").and_then(|s| s.as_str()).unwrap_or("UNKNOWN");
                let w_role = entry.get("role").and_then(|r| r.as_str()).unwrap_or("");
                let w_tier = entry.get("tier").and_then(|t| t.as_str()).unwrap_or("");
                let total_games = entry.get("totalGames").and_then(|g| g.as_i64()).unwrap_or(0);
                let placements = entry.get("wardPlacements").cloned();

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
                     VALUES (?1,?2,?3,?4,?5,?6,?7, CURRENT_TIMESTAMP)
                     ON CONFLICT(champion_name, side, role, tier) DO UPDATE SET
                        total_games     = excluded.total_games,
                        placements_json = excluded.placements_json,
                        heatmap_json    = excluded.heatmap_json,
                        synced_at       = CURRENT_TIMESTAMP",
                    rusqlite::params![
                        champion_name, side, w_role, w_tier, total_games,
                        placements.map(|v| v.to_string()),
                        side_heatmap.map(|v| v.to_string()),
                    ],
                );
            }
        }
    }

    tracing::info!("SpellCoach asset sync concluído: {champion_name}");
    Ok(())
}
