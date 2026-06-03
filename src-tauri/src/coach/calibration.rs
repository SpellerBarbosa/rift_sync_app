// ============================================================
// coach/calibration.rs — Calibração de precisão das dicas
//
// Fluxo:
//   1. log_tip()         → registra dica emitida com outcome esperado
//   2. resolve_outcomes() → 30s depois, verifica se o outcome aconteceu
//   3. recalibrate_if_ready() → a cada 50 outcomes, recalcula pesos por tip_id
//   4. load_weights()    → carrega pesos no início do engine para filtragem
//
// Outcomes rastreados:
//   "ally_objective"  → kill_ally:  aliado capturou torre/objetivo em 30s?
//   "no_ally_death"   → kill_enemy: nenhum aliado morreu em 30s?
// ============================================================

use std::collections::HashMap;
use std::sync::Arc;
use rusqlite::Connection;
use tokio::sync::Mutex;

pub type TipWeights = HashMap<String, f64>;

/// Threshold abaixo do qual uma dica é suprimida (acerto < 25%).
pub const SUPPRESS_THRESHOLD: f64 = 0.25;

/// Carrega pesos do banco. Retorna HashMap vazio se banco ainda não tem dados.
pub async fn load_weights(db: &Arc<Mutex<Connection>>) -> TipWeights {
    let db = db.lock().await;
    let mut weights = TipWeights::new();

    let mut stmt = match db.prepare(
        "SELECT tip_id, accuracy FROM coach_tip_weights"
    ) {
        Ok(s)  => s,
        Err(_) => return weights,
    };

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    });

    if let Ok(iter) = rows {
        for row in iter.flatten() {
            weights.insert(row.0, row.1);
        }
    }

    if !weights.is_empty() {
        tracing::info!("[Calibration] {} pesos carregados do banco", weights.len());
    }

    weights
}

/// Retorna true se a dica tem peso suficiente para ser emitida.
/// Tips sem histórico (não presentes no mapa) passam por padrão.
pub fn should_emit(tip_id: &str, weights: &TipWeights) -> bool {
    weights.get(tip_id).copied().unwrap_or(1.0) >= SUPPRESS_THRESHOLD
}

/// Registra uma dica emitida para rastreamento de outcome.
/// Só loga tips com outcomes verificáveis (kill_ally, kill_enemy).
pub async fn log_tip(
    db:        &Arc<Mutex<Connection>>,
    tip_id:    &str,
    category:  &str,
    game_time: u32,
    predicted: &str,
) {
    let db = db.lock().await;
    if let Err(e) = db.execute(
        "INSERT INTO coach_tip_log (tip_id, category, game_time, predicted)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![tip_id, category, game_time as i64, predicted],
    ) {
        tracing::warn!("[Calibration] Falha ao logar tip {}: {}", tip_id, e);
    }
}

/// Verifica outcomes pendentes cujo window de 30s já fechou.
/// Chamado a cada ciclo de eventos (5s) enquanto há tips pendentes.
pub async fn resolve_outcomes(
    db:           &Arc<Mutex<Connection>>,
    current_time: u32,
    ally_team:    &str,
    all_events:   &[serde_json::Value],
    players:      &[serde_json::Value],
) {
    let db = db.lock().await;

    // Coleta tips pendentes com window fechada
    let mut stmt = match db.prepare(
        "SELECT id, predicted, game_time FROM coach_tip_log
         WHERE outcome IS NULL AND game_time + 30 <= ?1
         LIMIT 50"
    ) {
        Ok(s)  => s,
        Err(_) => return,
    };
    let pending: Vec<(i64, String, i64)> = stmt
        .query_map(rusqlite::params![current_time as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();

    if pending.is_empty() { return; }

    for (id, predicted, tip_game_time) in pending {
        let window_start = tip_game_time as f64;
        let window_end   = (tip_game_time + 30) as f64;

        let hit = match predicted.as_str() {
            // Aliado capturou torre/objetivo na janela de 30s
            "ally_objective" => {
                all_events.iter().any(|ev| {
                    let t = ev["EventTime"].as_f64().unwrap_or(0.0);
                    if t <= window_start || t > window_end { return false; }

                    let is_objective = matches!(
                        ev["EventName"].as_str(),
                        Some("TurretKilled") | Some("DragonKill") |
                        Some("BaronKill")    | Some("RiftHeraldKill")
                    );
                    if !is_objective { return false; }

                    // Verifica se foi o time aliado que capturou
                    let actor = ev["KillerName"].as_str()
                        .or_else(|| ev["TeamId"].as_str())
                        .unwrap_or("");

                    // Para TurretKilled: TeamId indica quem PERDEU a torre
                    if ev["EventName"].as_str() == Some("TurretKilled") {
                        return ev["TeamId"].as_str() != Some(ally_team);
                    }

                    players.iter().any(|p| {
                        let sn = p["summonerName"].as_str().unwrap_or("");
                        let rn = p["riotIdGameName"].as_str().unwrap_or("");
                        (sn == actor || rn == actor) && p["team"].as_str() == Some(ally_team)
                    })
                })
            }

            // Nenhum aliado morreu na janela de 30s
            "no_ally_death" => {
                !all_events.iter().any(|ev| {
                    let t = ev["EventTime"].as_f64().unwrap_or(0.0);
                    if t <= window_start || t > window_end { return false; }
                    if ev["EventName"].as_str() != Some("ChampionKill") { return false; }

                    let victim = ev["VictimName"].as_str().unwrap_or("");
                    players.iter().any(|p| {
                        let sn = p["summonerName"].as_str().unwrap_or("");
                        let rn = p["riotIdGameName"].as_str().unwrap_or("");
                        (sn == victim || rn == victim) && p["team"].as_str() == Some(ally_team)
                    })
                })
            }

            _ => false,
        };

        let outcome = if hit { "hit" } else { "miss" };
        let _ = db.execute(
            "UPDATE coach_tip_log SET outcome = ?1 WHERE id = ?2",
            rusqlite::params![outcome, id],
        );
    }
}

/// Recalibra pesos se houver >= 50 outcomes resolvidos no total.
/// Retorna os novos pesos se recalibrou, None caso contrário.
pub async fn recalibrate_if_ready(
    db: &Arc<Mutex<Connection>>,
) -> Option<TipWeights> {
    let db = db.lock().await;

    let total: i64 = db.query_row(
        "SELECT COUNT(*) FROM coach_tip_log WHERE outcome IS NOT NULL",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    if total < 50 {
        tracing::debug!("[Calibration] {} outcomes — aguardando 50 para recalibrar", total);
        return None;
    }

    // Calcula accuracy por tip_id com >= 5 amostras
    let mut stmt = match db.prepare(
        "SELECT tip_id,
                CAST(SUM(CASE WHEN outcome = 'hit' THEN 1 ELSE 0 END) AS REAL) / COUNT(*),
                COUNT(*)
         FROM coach_tip_log
         WHERE outcome IS NOT NULL
         GROUP BY tip_id
         HAVING COUNT(*) >= 5"
    ) {
        Ok(s)  => s,
        Err(_) => return None,
    };
    let rows: Vec<(String, f64, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?, row.get::<_, i64>(2)?))
        })
        .map(|iter| iter.flatten().collect())
        .unwrap_or_default();

    if rows.is_empty() { return None; }

    let mut new_weights = TipWeights::new();
    for (tip_id, accuracy, samples) in &rows {
        let _ = db.execute(
            "INSERT INTO coach_tip_weights (tip_id, accuracy, sample_count, updated_at)
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(tip_id) DO UPDATE SET
                 accuracy     = excluded.accuracy,
                 sample_count = excluded.sample_count,
                 updated_at   = CURRENT_TIMESTAMP",
            rusqlite::params![tip_id, accuracy, samples],
        );
        new_weights.insert(tip_id.clone(), *accuracy);
    }

    tracing::info!(
        "[Calibration] Recalibrado: {} tip_ids | {} outcomes totais",
        rows.len(), total
    );
    Some(new_weights)
}
