// ============================================================
// coach/ward_advisor.rs — Recomendações de ward baseadas em dados reais
//
// Carrega placements do SpellCoach (via banco local) filtrando por:
//   · tier = CHALLENGER (maior tier disponível)
//   · role do jogador (posicionamentos específicos da lane)
//   · side (BLUE/RED — espelha coordenadas no red side)
//
// Conversão de coordenadas do mapa SR (0-14450):
//   xPct = x / 14450 * 100
//   yPct = (1 - y / 14450) * 100   ← eixo Y invertido
// ============================================================

use std::sync::Arc;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

const MAP_SIZE: f64 = 14450.0;

// ── Structs públicos ──────────────────────────────────────────

/// Spot de ward com coordenadas absolutas prontas para o overlay.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartWardSpot {
    pub x_pct:      f64,
    pub y_pct:      f64,
    pub label:      String,
    pub sublabel:   String,
    pub color:      String,
    pub ward_type:  String,
}

/// Resultado de `get_spots_with_alerts`: spots visuais + mensagem de coaching.
pub struct WardAdvice {
    pub spots:   Vec<SmartWardSpot>,
    /// Alerta de coaching textual, ou None se não há spots suficientes.
    pub alert:   Option<String>,
}

// ── Structs internos (desserialização do JSON do banco) ───────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlacement {
    x:              f64,
    y:              f64,
    ward_type:      String,
    game_phase:     String,
    timestamp_sec:  i64,
    count:          i64,
    wins:           i64,
}

// ── WardAdvisor ───────────────────────────────────────────────

pub struct WardAdvisor {
    placements:   Vec<RawPlacement>,
    champion:     String,
    role:         String,
    side:         String,
    tier_used:    String,  // tier efetivamente carregado (Challenger ou fallback)
}

impl WardAdvisor {
    pub fn new() -> Self {
        Self {
            placements: Vec::new(),
            champion:   String::new(),
            role:       String::new(),
            side:       String::new(),
            tier_used:  String::new(),
        }
    }

    /// Normaliza o role do formato Live API para o formato SpellCoach API.
    /// Ex: Live API "BOTTOM" → SpellCoach "ADC", "UTILITY" → "SUPPORT", "MIDDLE" → "MID".
    fn normalize_role(role: &str) -> &'static str {
        match role.to_uppercase().as_str() {
            "BOTTOM" | "BOT" | "ADC" => "ADC",
            "UTILITY" | "SUPPORT" | "SUP" => "SUPPORT",
            "MIDDLE" | "MID" => "MID",
            "JUNGLE" | "JGL" => "JUNGLE",
            "TOP" => "TOP",
            _ => "TOP",
        }
    }

    /// Carrega placements do banco para (champion, side, role), preferindo CHALLENGER.
    /// Fallback hierárquico: CHALLENGER → GRANDMASTER → MASTER → qualquer tier.
    pub async fn load(
        &mut self,
        db:            &Arc<Mutex<Connection>>,
        champion_name: &str,
        side:          &str,
        role:          &str,
    ) {
        let side_up = side.to_uppercase();
        // Normaliza role para o formato da SpellCoach API antes de consultar o banco
        let role_up = Self::normalize_role(role).to_string();

        // Não recarrega se contexto igual
        if self.champion == champion_name
            && self.side == side_up
            && self.role == role_up
        {
            return;
        }

        let db = db.lock().await;

        // Tenta carregar em ordem de prioridade de tier
        let tier_priority = ["CHALLENGER", "GRANDMASTER", "MASTER", ""];

        for tier in &tier_priority {
            let result: rusqlite::Result<(String, String)> = if tier.is_empty() {
                // Qualquer tier disponível com mais partidas
                db.query_row(
                    "SELECT placements_json, tier FROM champion_wards
                     WHERE champion_name = ?1 AND side = ?2 AND role = ?3
                     ORDER BY total_games DESC LIMIT 1",
                    rusqlite::params![champion_name, &side_up, &role_up],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            } else {
                db.query_row(
                    "SELECT placements_json, tier FROM champion_wards
                     WHERE champion_name = ?1 AND side = ?2 AND role = ?3
                       AND tier = ?4
                     ORDER BY total_games DESC LIMIT 1",
                    rusqlite::params![champion_name, &side_up, &role_up, tier],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            };

            match result {
                Ok((json, found_tier)) => {
                    self.placements = serde_json::from_str::<Vec<RawPlacement>>(&json)
                        .unwrap_or_default();
                    self.champion  = champion_name.to_string();
                    self.side      = side_up;
                    self.role      = role_up;
                    self.tier_used = found_tier.clone();
                    tracing::info!(
                        "[WardAdvisor] {} placements para {} / {} / {} (tier: {})",
                        self.placements.len(), champion_name, side, role, found_tier
                    );
                    return;
                }
                Err(_) => continue,
            }
        }

        // Nenhum dado encontrado para esse role — tenta sem role (dados globais)
        let fallback: rusqlite::Result<(String, String)> = db.query_row(
            "SELECT placements_json, tier FROM champion_wards
             WHERE champion_name = ?1 AND side = ?2
             ORDER BY CASE tier
                 WHEN 'CHALLENGER'  THEN 1
                 WHEN 'GRANDMASTER' THEN 2
                 WHEN 'MASTER'      THEN 3
                 ELSE 4
             END, total_games DESC LIMIT 1",
            rusqlite::params![champion_name, &side_up],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        if let Ok((json, found_tier)) = fallback {
            self.placements = serde_json::from_str::<Vec<RawPlacement>>(&json)
                .unwrap_or_default();
            self.champion  = champion_name.to_string();
            self.side      = side_up;
            self.role      = role_up;
            self.tier_used = found_tier.clone();
            tracing::debug!(
                "[WardAdvisor] fallback sem role: {} placements para {} (tier: {})",
                self.placements.len(), champion_name, found_tier
            );
        } else {
            self.placements.clear();
            self.champion = champion_name.to_string();
            self.side     = side_up;
            self.role     = role_up;
            self.tier_used = String::new();
            tracing::debug!("[WardAdvisor] sem dados para {} / {} / {}", champion_name, side, role);
        }
    }

    pub fn has_data(&self) -> bool {
        !self.placements.is_empty()
    }

    /// Retorna spots visuais E um alerta de coaching textual contextualizado.
    ///
    /// O alerta menciona o champion, role, tier usado, tipo de ward e timing —
    /// ex: "Ward o dragon pit agora — Caitlyn ADC Challenger usa Trinket aqui às 4:20"
    pub fn get_advice(
        &self,
        game_time_sec: u32,
        is_red_side:   bool,
        max_spots:     usize,
    ) -> WardAdvice {
        let spots = self.get_spots(game_time_sec, is_red_side, max_spots);

        if spots.is_empty() {
            return WardAdvice { spots, alert: None };
        }

        // Gera alerta apenas se temos contexto suficiente
        let alert = if !self.champion.is_empty() {
            let best = &spots[0];
            let role_label = match self.role.as_str() {
                "TOP"     => "Top",
                "JUNGLE"  => "Jungle",
                "MID" | "MIDDLE"   => "Mid",
                "BOTTOM" | "ADC"   => "ADC",
                "SUPPORT" | "UTILITY" => "Suporte",
                _         => self.role.as_str(),
            };
            let tier_label = if self.tier_used.is_empty() { "alto elo" } else { &self.tier_used };
            let ward_label = match best.ward_type.as_str() {
                "CONTROL_WARD" => "Ward de Controle",
                "BLUE_TRINKET" => "Sentinela",
                _              => "Trinket",
            };
            let zone = zone_from_label(&best.label, &best.sublabel);

            Some(format!(
                "Ward {} agora — {} {} {} usa {} aqui ({})",
                zone, self.champion, role_label, tier_label, ward_label, best.sublabel
            ))
        } else {
            None
        };

        WardAdvice { spots, alert }
    }

    /// Retorna apenas os spots visuais (sem alerta textual).
    pub fn get_spots(
        &self,
        game_time_sec: u32,
        is_red_side:   bool,
        max_spots:     usize,
    ) -> Vec<SmartWardSpot> {
        if self.placements.is_empty() {
            return Vec::new();
        }

        let phase = current_phase(game_time_sec);
        let mut candidates = filter_by_phase(&self.placements, phase, game_time_sec);

        // Amplia para fase anterior se dados insuficientes
        if candidates.len() < 3 && phase != "EARLY" {
            let prev = if phase == "LATE" { "MID" } else { "EARLY" };
            for p in filter_by_phase(&self.placements, prev, game_time_sec) {
                if !candidates.iter().any(|c: &&RawPlacement| {
                    (c.x - p.x).abs() < 600.0 && (c.y - p.y).abs() < 600.0
                }) {
                    candidates.push(p);
                }
            }
        }

        // Ranking: win_rate × ln(count+1)
        let mut ranked: Vec<(&RawPlacement, f64)> = candidates
            .iter()
            .filter(|p| p.count >= 2)
            .map(|p| {
                let wr    = if p.count > 0 { p.wins as f64 / p.count as f64 } else { 0.5 };
                let score = wr * (p.count as f64 + 1.0).ln();
                (*p, score)
            })
            .collect();

        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Agrupa por proximidade — mantém o melhor de cada cluster
        let mut selected: Vec<&RawPlacement> = Vec::new();
        for (p, _) in &ranked {
            if !selected.iter().any(|s| {
                (s.x - p.x).abs() < 500.0 && (s.y - p.y).abs() < 500.0
            }) {
                selected.push(p);
            }
            if selected.len() >= max_spots { break; }
        }

        selected.iter().map(|p| to_spot(p, is_red_side, &self.tier_used)).collect()
    }
}

// ── Helpers privados ──────────────────────────────────────────

fn current_phase(game_time_sec: u32) -> &'static str {
    match game_time_sec {
        0..=479    => "EARLY",
        480..=1199 => "MID",
        _          => "LATE",
    }
}

fn filter_by_phase<'a>(
    placements:    &'a [RawPlacement],
    phase:         &str,
    game_time_sec: u32,
) -> Vec<&'a RawPlacement> {
    let window_sec: f64 = match phase {
        "EARLY" => 60.0,
        "MID"   => 120.0,
        _       => 180.0,
    };
    placements
        .iter()
        .filter(|p| {
            p.game_phase == phase
                && (p.timestamp_sec as f64 - game_time_sec as f64).abs() <= window_sec
        })
        .collect()
}

fn to_spot(p: &RawPlacement, is_red_side: bool, tier: &str) -> SmartWardSpot {
    let (gx, gy) = if is_red_side {
        (MAP_SIZE - p.x, MAP_SIZE - p.y)
    } else {
        (p.x, p.y)
    };

    let x_pct = (gx / MAP_SIZE * 100.0).clamp(0.0, 100.0);
    let y_pct = ((1.0 - gy / MAP_SIZE) * 100.0).clamp(0.0, 100.0);

    let (label, color) = match p.ward_type.as_str() {
        "CONTROL_WARD"  => ("Controle",  "#F97316"),
        "BLUE_TRINKET"  => ("Sentinela", "#60A5FA"),
        _               => ("Trinket",   "#C8AA6E"),
    };

    let mins = p.timestamp_sec / 60;
    let secs = p.timestamp_sec % 60;
    let phase_label = match p.game_phase.as_str() {
        "EARLY" => "fase inicial",
        "MID"   => "fase média",
        _       => "fase final",
    };
    let tier_short = match tier {
        "CHALLENGER"  => "Chall",
        "GRANDMASTER" => "GM",
        "MASTER"      => "Master",
        t if !t.is_empty() => t,
        _             => "alto elo",
    };
    let sublabel = format!("{phase_label} · {mins}:{secs:02} · {tier_short}");

    SmartWardSpot {
        x_pct,
        y_pct,
        label:     label.to_string(),
        sublabel,
        color:     color.to_string(),
        ward_type: p.ward_type.clone(),
    }
}

/// Converte label + sublabel de um SmartWardSpot numa descrição legível para o alerta.
fn zone_from_label(label: &str, sublabel: &str) -> String {
    // Usa o label do tipo de ward + a fase como contexto mínimo
    let phase = if sublabel.contains("inicial") { "agora (fase inicial)"  }
                else if sublabel.contains("média") { "agora (fase média)" }
                else { "agora" };
    format!("{} {}", label.to_lowercase(), phase)
}
