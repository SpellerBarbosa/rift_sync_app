// ============================================================
// coach/engine.rs — Loop de coaching procedural em tempo real
//
// Sistema determinístico baseado em eventos do jogo:
//   1. OCR lê HUD a cada 2s (timer, placar, cooldowns)
//   2. LCU Live API atualiza estado de objetivos a cada 30s
//   3. ProcEngine avalia regras sem chamadas externas (< 1ms)
//   4. Alertas emitidos via Tauri para o overlay
// ============================================================

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::db::models::{CoachAlert, PlayerPattern};
use crate::game_state::states::GamePhase;
use crate::lcu::client::LcuClient;

// ── Contagem de drakes por time ───────────────────────────────

#[derive(Debug, Default)]
pub struct DragonCount {
    pub ally:  u8,
    pub enemy: u8,
}

// ── Estado do jogo (alimenta o ProcEngine) ────────────────────

struct GameInfo {
    champion_name:     Option<String>,
    is_ahead:          bool,
    ally_score:        u8,
    enemy_score:       u8,
    dragon_count:      DragonCount,
    next_dragon_spawn: u32,
    next_baron_spawn:  Option<u32>,
    herald_killed:     bool,
    /// Nomes dos campeões inimigos (CamelCase DDragon) para download de ícones
    enemy_champions:   Vec<String>,
    /// "ORDER" ou "CHAOS" — usado para atribuir kills de eventos LCU
    ally_team:         String,
    /// true = ORDER (time azul), false = CHAOS (time vermelho)
    is_blue_side:      bool,
    /// true se o jogador local tem trinket de ward (3340/3363) ou Control Ward (2055) no inventário
    has_ward_available: bool,
}

impl Default for GameInfo {
    fn default() -> Self {
        Self {
            champion_name:     None,
            is_ahead:          false,
            ally_score:        0,
            enemy_score:       0,
            dragon_count:      DragonCount::default(),
            next_dragon_spawn: 300, // primeiro drake às 5:00
            next_baron_spawn:  None,
            herald_killed:     false,
            enemy_champions:   Vec::new(),
            ally_team:         "ORDER".to_string(),
            is_blue_side:      true,
            has_ward_available: false,
        }
    }
}

// ── Fetch de estado do jogo via LCU ──────────────────────────

async fn fetch_player_role(client: &LcuClient) -> String {
    // Busca a sessão de champ select e o summonerId do jogador local
    let session = match client.get_champ_select_session().await.ok() {
        Some(s) => s,
        None    => return "UNKNOWN".to_string(),
    };

    let local_id = session["localPlayerCellId"].as_i64().unwrap_or(-1);

    let raw = session["myTeam"].as_array()
        .and_then(|team| {
            team.iter()
                .find(|p| p["cellId"].as_i64().unwrap_or(-2) == local_id)
                .and_then(|p| p["assignedPosition"].as_str())
        });

    match raw {
        Some(pos) => match pos.to_uppercase().as_str() {
            "MIDDLE" => "MID".to_string(),
            p        => p.to_string(),
        },
        None => "UNKNOWN".to_string(),
    }
}

async fn fetch_game_info(client: &LcuClient) -> GameInfo {
    let Ok(data) = client.get_live_game_stats().await else {
        return GameInfo::default();
    };

    let players = match data["allPlayers"].as_array() {
        Some(p) => p.clone(),
        None    => return GameInfo::default(),
    };

    let active_player_name = {
        let sn = data["activePlayer"]["summonerName"].as_str().unwrap_or("");
        if sn.is_empty() {
            data["activePlayer"]["riotIdGameName"].as_str().unwrap_or("")
        } else {
            sn
        }
    }.to_string();

    let mut champion_name = None;
    let mut ally_score: u8  = 0;
    let mut enemy_score: u8 = 0;
    let mut ally_team: Option<&str> = None;

    // IDs de trinkets que permitem colocar wards (Stealth Ward, Farsight Alteration)
    const WARD_TRINKET_IDS: &[u64] = &[3340, 3363];
    const CONTROL_WARD_ID:   u64   = 2055;

    let mut has_ward_available = false;

    for p in &players {
        let sn = p["summonerName"].as_str().unwrap_or("");
        let rn = p["riotIdGameName"].as_str().unwrap_or("");
        if !active_player_name.is_empty()
            && (sn == active_player_name || rn == active_player_name)
        {
            ally_team     = p["team"].as_str();
            champion_name = p["championName"].as_str().map(str::to_string);

            if let Some(items) = p["items"].as_array() {
                has_ward_available = items.iter().any(|item| {
                    let id = item["itemID"].as_u64().unwrap_or(0);
                    if id == CONTROL_WARD_ID {
                        item["count"].as_u64().unwrap_or(0) > 0
                    } else {
                        WARD_TRINKET_IDS.contains(&id)
                    }
                });
            }
        }
    }

    for p in &players {
        let kills = p["scores"]["kills"].as_u64().unwrap_or(0) as u8;
        if p["team"].as_str() == ally_team {
            ally_score  += kills;
        } else {
            enemy_score += kills;
        }
    }

    let mut dragon_count       = DragonCount::default();
    let mut last_dragon_kill_t: u32         = 0;
    let mut last_baron_kill_t:  Option<u32> = None;
    let mut herald_killed                   = false;

    if let Some(events) = data["events"]["Events"].as_array() {
        for ev in events {
            let event_time = ev["EventTime"].as_f64().map(|t| t as u32).unwrap_or(0);
            match ev["EventName"].as_str() {
                Some("DragonKill") => {
                    let killer = ev["KillerName"].as_str().unwrap_or("");
                    let killer_team = players
                        .iter()
                        .find(|p| {
                            let sn = p["summonerName"].as_str().unwrap_or("");
                            let rn = p["riotIdGameName"].as_str().unwrap_or("");
                            !killer.is_empty() && (sn == killer || rn == killer)
                        })
                        .and_then(|p| p["team"].as_str());
                    match killer_team == ally_team {
                        true  => dragon_count.ally  = dragon_count.ally.saturating_add(1),
                        false => dragon_count.enemy = dragon_count.enemy.saturating_add(1),
                    }
                    last_dragon_kill_t = last_dragon_kill_t.max(event_time);
                }
                Some("BaronKill") => {
                    last_baron_kill_t = Some(last_baron_kill_t.unwrap_or(0).max(event_time));
                }
                Some("RiftHeraldKill") => { herald_killed = true; }
                _ => {}
            }
        }
    }

    // Campeões inimigos para download de ícones de template matching
    let ally_team_str = ally_team.unwrap_or("ORDER");
    let enemy_champions: Vec<String> = players
        .iter()
        .filter(|p| p["team"].as_str() != Some(ally_team_str))
        .filter_map(|p| p["championName"].as_str().map(str::to_string))
        .collect();

    GameInfo {
        champion_name,
        is_ahead:           ally_score > enemy_score,
        ally_score,
        enemy_score,
        dragon_count,
        next_dragon_spawn:  if last_dragon_kill_t > 0 { last_dragon_kill_t + 300 } else { 300 },
        next_baron_spawn:   last_baron_kill_t.map(|t| t + 360),
        herald_killed,
        enemy_champions,
        ally_team:          ally_team_str.to_string(),
        is_blue_side:       ally_team_str == "ORDER",
        has_ward_available,
    }
}

// ── Fetch de idioma do banco ──────────────────────────────────

async fn fetch_app_language(db: &Arc<Mutex<rusqlite::Connection>>) -> String {
    let db = db.lock().await;
    db.query_row(
        "SELECT value FROM settings WHERE key = 'app_language'",
        [],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "pt-BR".to_string())
}

// ── Fetch de padrão comportamental do banco ───────────────────

async fn fetch_player_pattern(
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> Option<PlayerPattern> {
    let db = db.lock().await;
    db.query_row(
        "SELECT id, player_id, aggression_score, deaths_without_vision, avg_deaths_10_15,
                lane_dominance, objective_control, roam_frequency, tp_efficiency, ward_score
         FROM player_patterns
         WHERE player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)
         LIMIT 1",
        [],
        |row| Ok(PlayerPattern {
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
    )
    .ok()
}

// ── Helpers para emitir alertas ───────────────────────────────

fn emit_alert(app: &AppHandle, alert: &CoachAlert) {
    tracing::info!("[Coach] [{}/{}]: {}", alert.severity, alert.category, alert.message);
    // A visibilidade da janela flashcard é controlada pelo Vue (watch em activeCard).
    // Rust só emite o evento — o frontend decide quando mostrar/esconder.
    app.emit("coach_alert", alert).ok();
}

// ── Loop principal ────────────────────────────────────────────

pub async fn run_coach_loop(
    app:           AppHandle,
    lcu_client:    Arc<Mutex<Option<LcuClient>>>,
    game_phase:    Arc<Mutex<GamePhase>>,
    mut ocr_rx:    tokio::sync::mpsc::Receiver<String>,
    db:            Arc<Mutex<rusqlite::Connection>>,
    is_blue_side:  Arc<AtomicBool>,
    // Templates dos ícones dos campeões inimigos compartilhados com o OCR loop.
    // Populado pelo coach quando proc_info fica disponível pela primeira vez.
    enemy_templates: Arc<Mutex<Vec<image::GrayImage>>>,
) {
    let mut proc_engine  = super::procedural::ProcEngine::new();
    let mut ward_advisor = super::ward_advisor::WardAdvisor::new();
    let mut game_time: u32 = 0;

    // Idioma lido do banco; atualizado junto com os dados do LCU (a cada 30s).
    let mut app_lang: String = fetch_app_language(&db).await;

    // Pesos de calibração por tip_id — atualizados ao fim de cada partida.
    let mut tip_weights = super::calibration::load_weights(&db).await;

    let mut proc_role:    String           = "UNKNOWN".to_string();
    // Inicializa com defaults para que evaluate() rode mesmo sem LCU conectado.
    // A primeira atualização do LCU (need_refresh=true) substitui com dados reais.
    let mut proc_info:    Option<GameInfo> = Some(GameInfo::default());
    let mut last_info_at: Option<Instant>  = None;

    let mut proc_pattern: Option<PlayerPattern> = None;

    // Placar via LCU events (substitui ocr_scores — mais confiável, sem ruído de OCR)
    let mut lcu_ally_score:  u8  = 0;
    let mut lcu_enemy_score: u8  = 0;
    let mut last_event_time: f64 = 0.0;  // EventTime do último evento processado
    let mut last_events_at:  Option<Instant> = None; // controla o poll de 5s

    // Último count de aliados na zona bot lane do minimapa (None = ainda sem leitura)
    let mut ocr_ally_bot:    Option<u8> = None;
    // Dots inimigos detectados por zona do minimapa (top = topo do mapa, bot = bot lane)
    let mut ocr_enemy_top:   Option<u8> = None;
    let mut ocr_enemy_bot:   Option<u8> = None;

    // ── Estado de spells do jogador (para smite e ambas disponíveis) ──
    let mut smite_slot:    Option<char> = None; // 'D' ou 'F' se é jungle
    let mut spell_d_on_cd: bool         = false;
    let mut spell_f_on_cd: bool         = false;

    // ── Oponente de lane para o MatchupOverlay ────────────────────
    // Identificado uma vez; emite matchup_init no frontend.
    let mut lane_opponent_name:  Option<String> = None;
    let mut lane_opponent_id:    i64            = -1;
    let mut my_champion_id:      i64            = -1;
    let mut prev_opponent_level: u8             = 0;
    let mut opp_spell1:          String         = String::new();
    let mut opp_spell2:          String         = String::new();

    // ── Nível e CS para coaching de lane ─────────────────────────
    let mut prev_player_level:   u8  = 0;
    let mut prev_player_cs_lane: u32 = 0;
    let mut prev_opponent_cs:    u32 = 0;

    // Rastreamento de ward-reveals:
    //   pending  = (x, y, frames_consecutivos) — dot precisa de 3 frames para confirmar
    //   notified = posições já notificadas (cooldown 120s por área de 20%)
    //   last_enemy_notif = rate limiter global (máx 1 notif a cada 20s)
    let mut pending_enemy_pos: Vec<(f32, f32, u8)>      = Vec::new();
    let mut notified_enemy:    Vec<(f32, f32, Instant)> = Vec::new();
    let mut last_enemy_notif:  Option<Instant>          = None;

    // Fallback: se o OCR não entregar game_time por mais de 5s, o motor
    // avalia mesmo assim usando o contador interno — garante que dicas
    // apareçam mesmo que a leitura OCR falhe temporariamente.
    let mut last_ocr_at:      Option<Instant> = None;
    let mut last_fallback_at: Option<Instant> = None;

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let phase = game_phase.lock().await.clone();

        if phase.is_in_game() {
            game_time += 1;
        } else {
            if game_time > 0 {
                tracing::info!("[Coach] sessão encerrada — {game_time}s de jogo");
                game_time        = 0;
                proc_info        = Some(GameInfo::default());
                proc_pattern     = None;
                last_info_at     = None;
                last_ocr_at      = None;
                last_fallback_at = None;
                lcu_ally_score   = 0;
                lcu_enemy_score  = 0;
                last_event_time  = 0.0;
                last_events_at   = None;
                ocr_ally_bot     = None;
                ocr_enemy_top    = None;
                ocr_enemy_bot    = None;
                pending_enemy_pos.clear();
                notified_enemy.clear();
                last_enemy_notif   = None;
                ward_advisor       = super::ward_advisor::WardAdvisor::new();
                smite_slot         = None;
                spell_d_on_cd      = false;
                spell_f_on_cd      = false;
                lane_opponent_name  = None;
                lane_opponent_id    = -1;
                my_champion_id      = -1;
                prev_opponent_level = 0;
                opp_spell1          = String::new();
                opp_spell2          = String::new();
                prev_player_level  = 0;
                prev_player_cs_lane = 0;
                prev_opponent_cs   = 0;
                enemy_templates.lock().await.clear();
                proc_engine.reset();
                // Recalibra pesos ao fim da partida se houver dados suficientes
                if let Some(new_weights) = super::calibration::recalibrate_if_ready(&db).await {
                    tip_weights = new_weights;
                }
            }
            continue;
        }

        // ── Drena eventos OCR ─────────────────────────────────
        // ocr_game_time → atualiza game_time e sinaliza avaliação das regras
        // ocr_score     → atualiza placar para detecção de kill
        // outros        → reação imediata de spell/posicionamento
        let mut should_evaluate = false;

        while let Ok(event) = ocr_rx.try_recv() {
            tracing::debug!("[Coach] OCR: {event}");

            if let Some(rest) = event.strip_prefix("ocr_game_time:") {
                if let Ok(t) = rest.parse::<u32>() {
                    // Reconcilia com contador interno — OCR tem precedência se divergiu > 3s
                    if t.abs_diff(game_time) > 3 {
                        game_time = t;
                    }
                    should_evaluate = true;
                    last_ocr_at = Some(Instant::now());
                }
            } else if let Some(rest) = event.strip_prefix("minimap_ally_bot:") {
                if let Ok(count) = rest.parse::<u8>() {
                    ocr_ally_bot = Some(count);
                }
            } else if let Some(rest) = event.strip_prefix("minimap_enemy:") {
                if let Some((top, bot)) = parse_minimap_enemy(rest) {
                    ocr_enemy_top = Some(top);
                    ocr_enemy_bot = Some(bot);
                }
            } else if let Some(rest) = event.strip_prefix("minimap_enemy_pos:") {
                // Detecção de dots por cor — usada apenas para contagem por zona
                // (ocr_enemy_top / ocr_enemy_bot). Ward reveal usa template matching.
                let _ = parse_enemy_positions(rest);
            } else if let Some(rest) = event.strip_prefix("enemy_template:") {
                if let Some((x, y, idx)) = parse_template_match(rest) {
                    // Resolve nome do campeão pelo índice no vetor de inimigos
                    let champ_name = proc_info.as_ref()
                        .and_then(|i| i.enemy_champions.get(idx))
                        .map(|s| s.as_str())
                        .unwrap_or("");

                    // ── Ward reveal via template matching (confiável) ─
                    detect_ward_reveals(
                        &app,
                        &[(x, y)],
                        champ_name,
                        &mut pending_enemy_pos,
                        &mut notified_enemy,
                        &mut last_enemy_notif,
                        game_time,
                        &app_lang,
                    );

                    // ── Brecha de gank (inimigo além do rio) ──────────
                    let is_blue = proc_info.as_ref()
                        .map(|i| i.is_blue_side)
                        .unwrap_or(true);
                    for alert in proc_engine.handle_gank_opportunity(
                        x, y, is_blue, game_time, &proc_role, &app_lang,
                    ) {
                        emit_alert(&app, &alert);
                    }
                }
            } else {
                // Rastreia estado das spells D e F para smite e "ambas disponíveis"
                if event.contains("Flash_D aliado em cooldown") || event.contains("_D aliado em cooldown") {
                    spell_d_on_cd = true;
                } else if event.contains("Flash_D aliado disponível") || event.contains("_D aliado disponível") {
                    spell_d_on_cd = false;
                }
                if event.contains("Flash_F aliado em cooldown") || event.contains("_F aliado em cooldown") {
                    spell_f_on_cd = true;
                } else if event.contains("Flash_F aliado disponível") || event.contains("_F aliado disponível") {
                    spell_f_on_cd = false;
                }

                // Smite em CD = o slot que tem smite está em cooldown
                let smite_on_cd = smite_slot.map(|s| match s {
                    'D' => spell_d_on_cd,
                    'F' => spell_f_on_cd,
                    _   => false,
                }).unwrap_or(false);

                // Objetivo próximo = drake ou baron disponível ou em < 60s
                let objective_near = proc_info.as_ref().map(|info| {
                    let dragon_secs = info.next_dragon_spawn.saturating_sub(game_time);
                    let baron_secs  = info.next_baron_spawn
                        .map(|t| t.saturating_sub(game_time))
                        .unwrap_or(u32::MAX);
                    dragon_secs <= 60 || baron_secs <= 60
                }).unwrap_or(false);

                // O "outro" spell está disponível?
                let other_spell_available = if event.contains("_D aliado") {
                    !spell_f_on_cd
                } else {
                    !spell_d_on_cd
                };

                for alert in proc_engine.react_to_ocr_event(
                    &event, game_time, &proc_role,
                    smite_on_cd, objective_near, other_spell_available,
                    &app_lang,
                ) {
                    emit_alert(&app, &alert);
                }
            }
        }

        // ── Fast poll LCU: game_time + kill events (a cada 5s) ──────
        // Substitui o OCR de timer e placar com dados diretos da Live API.
        let events_due = last_events_at
            .map(|t| t.elapsed() >= Duration::from_secs(5))
            .unwrap_or(true);

        if events_due && phase.is_in_game() {
            let client_guard = lcu_client.lock().await;
            if let Some(ref client) = *client_guard {
                // Sincroniza game_time
                if let Ok(t) = client.get_live_game_time().await {
                    let lcu_t = t as u32;
                    if lcu_t.abs_diff(game_time) > 3 {
                        game_time = lcu_t;
                    }
                    should_evaluate = true;
                    last_ocr_at = Some(Instant::now()); // suprime o fallback
                }

                // Placar preciso via allgamedata — mesma fonte do proc_info
                // mas a cada 5s para detectar kills em tempo real sem OCR.
                // A chamada é local (port 2999) e leva < 10ms.
                if let Ok(data) = client.get_live_game_stats().await {
                    let scores = extract_team_scores(&data);
                    if let Some((new_ally, new_enemy)) = scores {
                        let (prev_ally, prev_enemy) = (lcu_ally_score, lcu_enemy_score);
                        lcu_ally_score  = new_ally;
                        lcu_enemy_score = new_enemy;

                        if new_ally != prev_ally || new_enemy != prev_enemy {
                            for alert in proc_engine.handle_score_change(
                                new_ally, new_enemy,
                                prev_ally, prev_enemy,
                                game_time, proc_pattern.as_ref(),
                                &proc_role, &app_lang,
                            ) {
                                emit_alert(&app, &alert);
                            }
                        }

                        // ── Eventos de objetivos e estruturas ─────────────────
                        if let Some(events) = data["events"]["Events"].as_array() {
                            let ally_team = proc_info.as_ref()
                                .map(|i| i.ally_team.as_str())
                                .unwrap_or("ORDER");
                            let players = data["allPlayers"].as_array();

                            // dragon counts atuais para contexto de Elder
                            let (d_ally, d_enemy) = proc_info.as_ref()
                                .map(|i| (i.dragon_count.ally, i.dragon_count.enemy))
                                .unwrap_or((0, 0));

                            // Pre-scan: coleta kills novas para detecção de teamfight e counter-kill
                            let new_kills: Vec<(f64, &str, &str)> = events.iter()
                                .filter_map(|ev| {
                                    if ev["EventName"].as_str() != Some("ChampionKill") { return None; }
                                    let t = ev["EventTime"].as_f64()?;
                                    if t <= last_event_time { return None; }
                                    Some((t,
                                        ev["KillerName"].as_str().unwrap_or(""),
                                        ev["VictimName"].as_str().unwrap_or("")))
                                })
                                .collect();

                            for ev in events {
                                let ev_time = ev["EventTime"].as_f64().unwrap_or(0.0);
                                if ev_time <= last_event_time { continue; }
                                last_event_time = ev_time;

                                let killer = ev["KillerName"].as_str().unwrap_or("");
                                let killer_team = players
                                    .and_then(|ps| ps.iter().find(|p| {
                                        let sn = p["summonerName"].as_str().unwrap_or("");
                                        let rn = p["riotIdGameName"].as_str().unwrap_or("");
                                        !killer.is_empty() && (sn == killer || rn == killer)
                                    }))
                                    .and_then(|p| p["team"].as_str());
                                let ally_got_it = killer_team == Some(ally_team);

                                // Torre derrubada: TeamId indica qual time perdeu a torre
                                // (o time que NÃO derrubou é quem a possuía)
                                let tower_team  = ev["TeamId"].as_str();
                                let tower_is_ally_loss = tower_team == Some(ally_team);

                                // ── ChampionKill ─────────────────────────────────────
                                if ev["EventName"].as_str() == Some("ChampionKill") {
                                    let killer_name = ev["KillerName"].as_str().unwrap_or("");
                                    let victim_name = ev["VictimName"].as_str().unwrap_or("");

                                    let find_player = |name: &str| -> Option<&serde_json::Value> {
                                        if name.is_empty() { return None; }
                                        players?.iter().find(|p| {
                                            let sn = p["summonerName"].as_str().unwrap_or("");
                                            let rn = p["riotIdGameName"].as_str().unwrap_or("");
                                            sn == name || rn == name
                                        })
                                    };

                                    let killer_champ    = find_player(killer_name).and_then(|p| p["championName"].as_str()).unwrap_or("").to_string();
                                    let killer_team_str = find_player(killer_name).and_then(|p| p["team"].as_str()).unwrap_or("").to_string();
                                    let victim_champ    = find_player(victim_name).and_then(|p| p["championName"].as_str()).unwrap_or("").to_string();
                                    let victim_role     = find_player(victim_name).and_then(|p| p["position"].as_str()).unwrap_or("").to_string();
                                    let kill_is_ally    = killer_team_str == ally_team;

                                    // Teamfight: 3+ kills em janela de 10s
                                    let is_teamfight = new_kills.iter()
                                        .filter(|(t, _, _)| (t - ev_time).abs() <= 10.0)
                                        .count() >= 3;

                                    // Counter-kill: killer foi morto nos 10s seguintes
                                    let killer_counter_killed = new_kills.iter()
                                        .any(|(t, _, v)| *v == killer_name && *t > ev_time && *t <= ev_time + 10.0);

                                    // TP threat: outro inimigo vivo tem Teleport
                                    let enemy_tp_threat = kill_is_ally && players.map(|ps| {
                                        ps.iter().any(|p| {
                                            p["team"].as_str() != Some(ally_team)
                                                && p["summonerName"].as_str().unwrap_or("") != victim_name
                                                && p["riotIdGameName"].as_str().unwrap_or("") != victim_name
                                                && (p["summonerSpells"]["summonerSpellOne"]["displayName"]
                                                        .as_str().unwrap_or("").contains("Teleport")
                                                    || p["summonerSpells"]["summonerSpellTwo"]["displayName"]
                                                        .as_str().unwrap_or("").contains("Teleport"))
                                        })
                                    }).unwrap_or(false);

                                    let dragon_secs = proc_info.as_ref()
                                        .map(|i| i.next_dragon_spawn.saturating_sub(game_time))
                                        .unwrap_or(u32::MAX);
                                    let baron_secs = proc_info.as_ref()
                                        .and_then(|i| i.next_baron_spawn)
                                        .map(|t| t.saturating_sub(game_time));

                                    let kill_ctx = super::procedural::KillCtx {
                                        killer_champ,
                                        victim_champ,
                                        victim_role,
                                        is_ally_kill:          kill_is_ally,
                                        game_time,
                                        player_role:           proc_role.clone(),
                                        dragon_secs,
                                        baron_secs,
                                        enemy_tp_threat,
                                        is_teamfight,
                                        killer_counter_killed,
                                        has_ward_available:    proc_info.as_ref()
                                            .map(|i| i.has_ward_available)
                                            .unwrap_or(false),
                                    };

                                    for alert in proc_engine.handle_champion_kill(&kill_ctx, proc_pattern.as_ref(), &app_lang) {
                                        if !super::calibration::should_emit(&alert.tip_id, &tip_weights) {
                                            continue;
                                        }
                                        emit_alert(&app, &alert);
                                        let predicted = match alert.tip_id.as_str() {
                                            "kill_ally"  => Some("ally_objective"),
                                            "kill_enemy" => Some("no_ally_death"),
                                            _            => None,
                                        };
                                        if let Some(pred) = predicted {
                                            super::calibration::log_tip(
                                                &db, &alert.tip_id, &alert.category,
                                                game_time, pred,
                                            ).await;
                                        }
                                    }
                                }

                                let objective = match ev["EventName"].as_str() {
                                    Some("DragonKill")      => Some(("dragon",    ally_got_it)),
                                    Some("BaronKill")       => Some(("baron",     ally_got_it)),
                                    Some("RiftHeraldKill")  => Some(("herald",    ally_got_it)),
                                    Some("TurretKilled")    => Some(("tower",     !tower_is_ally_loss)),
                                    Some("InhibKilled")     => Some(("inhibitor", !tower_is_ally_loss)),
                                    _                       => None,
                                };

                                if let Some((obj, ally)) = objective {
                                    for alert in proc_engine.handle_objective_event(
                                        obj, ally, game_time, &proc_role,
                                        d_ally, d_enemy,
                                        proc_pattern.as_ref(), &app_lang,
                                    ) {
                                        emit_alert(&app, &alert);
                                    }

                                    // Coaching específico pelo tipo do drake
                                    if obj == "dragon" {
                                        let drake_type = ev["DragonType"].as_str().unwrap_or("Unknown");
                                        for alert in proc_engine.handle_drake_type(drake_type, ally, game_time, &app_lang) {
                                            emit_alert(&app, &alert);
                                        }
                                    }
                                }
                            }
                            // Resolve outcomes de tips pendentes cujo window de 30s fechou
                            let players_slice: Vec<serde_json::Value> = players
                                .map(|ps| ps.to_vec())
                                .unwrap_or_default();
                            super::calibration::resolve_outcomes(
                                &db, game_time, ally_team,
                                events, &players_slice,
                            ).await;
                        }

                        // ── CS, nível e smite — extraídos do allPlayers ───────
                        let active_name = {
                            let sn = data["activePlayer"]["summonerName"].as_str().unwrap_or("");
                            if sn.is_empty() {
                                data["activePlayer"]["riotIdGameName"].as_str().unwrap_or("")
                            } else {
                                sn
                            }
                        }.to_string();

                        // Sincroniza proc_role com a posição real do jogador no jogo.
                        // Garante role correto mesmo quando champ select já encerrou.
                        if let Some(ps) = data["allPlayers"].as_array() {
                            if let Some(me) = ps.iter().find(|p| {
                                let sn = p["summonerName"].as_str().unwrap_or("");
                                let rn = p["riotIdGameName"].as_str().unwrap_or("");
                                !active_name.is_empty() && (sn == active_name || rn == active_name)
                            }) {
                                let api_pos = me["position"].as_str().unwrap_or("");
                                let normalized = match api_pos {
                                    "MIDDLE"                        => "MID",
                                    p if !p.is_empty() && p != "NONE" => p,
                                    _                               => "",
                                };
                                if !normalized.is_empty() {
                                    proc_role = normalized.to_string();
                                }
                            }
                        }

                        // Detecta slot do smite (atualiza uma vez ao iniciar partida)
                        if smite_slot.is_none() && proc_role == "JUNGLE" {
                            let spell1 = data["activePlayer"]["summonerSpells"]["summonerSpellOne"]["displayName"]
                                .as_str().unwrap_or("");
                            let spell2 = data["activePlayer"]["summonerSpells"]["summonerSpellTwo"]["displayName"]
                                .as_str().unwrap_or("");
                            if spell1.eq_ignore_ascii_case("smite") {
                                smite_slot = Some('D');
                            } else if spell2.eq_ignore_ascii_case("smite") {
                                smite_slot = Some('F');
                            }
                        }

                        if let Some(players) = data["allPlayers"].as_array() {
                            // Posição do oponente de lane baseado no proc_role
                            let opp_pos = match proc_role.as_str() {
                                "MID"                         => "MIDDLE",
                                "SUPPORT" | "UTILITY"         => "UTILITY",
                                "ADC" | "BOTTOM"              => "BOTTOM",
                                pos                           => pos,
                            };
                            let ally_team = proc_info.as_ref()
                                .map(|i| i.ally_team.as_str())
                                .unwrap_or("ORDER");

                            // Jogador ativo
                            if let Some(me) = players.iter().find(|p| {
                                let sn = p["summonerName"].as_str().unwrap_or("");
                                let rn = p["riotIdGameName"].as_str().unwrap_or("");
                                !active_name.is_empty() && (sn == active_name || rn == active_name)
                            }) {
                                let player_cs  = me["scores"]["creepScore"].as_u64().unwrap_or(0) as u32;
                                let player_lvl = me["level"].as_u64().unwrap_or(0) as u8;

                                // CS vs benchmark
                                for alert in proc_engine.handle_cs_update(player_cs, game_time, &proc_role, &app_lang) {
                                    emit_alert(&app, &alert);
                                }

                                // Oponente de lane (mesmo role, time inimigo)
                                if let Some(opp) = players.iter().find(|p| {
                                    p["team"].as_str() != Some(ally_team) &&
                                    p["position"].as_str().map(|pos| {
                                        pos.eq_ignore_ascii_case(opp_pos)
                                    }).unwrap_or(false)
                                }) {
                                    let opp_cs  = opp["scores"]["creepScore"].as_u64().unwrap_or(0) as u32;
                                    let opp_lvl = opp["level"].as_u64().unwrap_or(0) as u8;

                                    // CS vs oponente
                                    if player_cs != prev_player_cs_lane || opp_cs != prev_opponent_cs {
                                        for alert in proc_engine.handle_cs_lane_gap(
                                            player_cs, opp_cs, &proc_role, game_time, &app_lang,
                                        ) { emit_alert(&app, &alert); }
                                        prev_player_cs_lane = player_cs;
                                        prev_opponent_cs    = opp_cs;
                                    }

                                    // Level vs oponente
                                    if player_lvl != prev_player_level {
                                        for alert in proc_engine.handle_level_change(
                                            player_lvl, prev_player_level, opp_lvl, &proc_role, game_time, &app_lang,
                                        ) { emit_alert(&app, &alert); }
                                        prev_player_level = player_lvl;
                                    }

                                    // ── Matchup overlay: identificação + spikes do oponente ──
                                    let opp_champ_name = opp["championName"]
                                        .as_str().unwrap_or("").to_string();

                                    // Captura summoner spells uma vez (imutáveis durante a partida)
                                    if opp_spell1.is_empty() && !opp_champ_name.is_empty() {
                                        opp_spell1 = opp["summonerSpells"]["summonerSpellOne"]["displayName"]
                                            .as_str().unwrap_or("").to_string();
                                        opp_spell2 = opp["summonerSpells"]["summonerSpellTwo"]["displayName"]
                                            .as_str().unwrap_or("").to_string();
                                    }

                                    // Emite matchup_init na primeira identificação do oponente
                                    if lane_opponent_name.is_none() && !opp_champ_name.is_empty() {
                                        let opp_champ_id = opp["championId"].as_i64().unwrap_or(-1);
                                        let me_champ_id  = me["championId"].as_i64().unwrap_or(-1);
                                        lane_opponent_name = Some(opp_champ_name.clone());
                                        lane_opponent_id   = opp_champ_id;
                                        my_champion_id     = me_champ_id;
                                        app.emit("matchup_init", serde_json::json!({
                                            "myChampionId": me_champ_id,
                                            "enemyId":      opp_champ_id,
                                            "enemyName":    opp_champ_name,
                                            "enemyLevel":   opp_lvl,
                                            "enemySpell1":  opp_spell1,
                                            "enemySpell2":  opp_spell2,
                                        })).ok();
                                    }

                                    // Power spikes do oponente → alerta + atualização do overlay
                                    if opp_lvl != prev_opponent_level && opp_lvl > 0 {
                                        for alert in proc_engine.handle_enemy_level_spike(
                                            opp_lvl, prev_opponent_level, game_time, &proc_role, &app_lang,
                                        ) {
                                            emit_alert(&app, &alert);
                                        }
                                        app.emit("matchup_level_update", serde_json::json!({
                                            "level":   opp_lvl,
                                            "enemyId": lane_opponent_id,
                                            "myId":    my_champion_id,
                                        })).ok();
                                        prev_opponent_level = opp_lvl;
                                    }

                                } else if prev_player_level != me["level"].as_u64().unwrap_or(0) as u8 {
                                    // Sem oponente detectado — apenas power spike
                                    let lvl = me["level"].as_u64().unwrap_or(0) as u8;
                                    for alert in proc_engine.handle_level_change(
                                        lvl, prev_player_level, lvl, &proc_role, game_time, &app_lang,
                                    ) { emit_alert(&app, &alert); }
                                    prev_player_level = lvl;
                                }
                            }
                        }
                    }
                }
            }
            last_events_at = Some(Instant::now());
        }

        // ── Fallback wall-clock: avalia mesmo sem LCU ────────────────
        if !should_evaluate && game_time > 0 {
            let lcu_stale = last_ocr_at
                .map(|t| t.elapsed() > Duration::from_secs(10))
                .unwrap_or(true);
            let fallback_due = last_fallback_at
                .map(|t| t.elapsed() >= Duration::from_secs(5))
                .unwrap_or(true);
            if lcu_stale && fallback_due {
                should_evaluate  = true;
                last_fallback_at = Some(Instant::now());
                tracing::debug!("[Coach] LCU inativo — avaliando via contador interno (t={game_time}s)");
            }
        }

        // ── Motor procedural ──────────────────────────────────────────
        if should_evaluate {
            if let Some(ref info) = proc_info {
                let ally_s  = lcu_ally_score.max(info.ally_score);
                let enemy_s = lcu_enemy_score.max(info.enemy_score);

                // support_in_lane = true quando há 2+ aliados na zona bot.
                // Default true para não gerar alertas falsos enquanto o minimapa
                // ainda não entregou leitura ou quando o duo está junto.
                let support_in_lane = ocr_ally_bot.map(|c| c >= 2).unwrap_or(true);

                // Determina se o jungle inimigo está no lado oposto ao do jogador.
                // TOP/JUNGLE → perigoso se inimigo no top_zone; seguro se no bot_zone.
                // BOTTOM/ADC/SUPPORT → perigoso se no bot_zone; seguro se no top_zone.
                // Default false (não assume que é seguro sem leitura).
                let enemy_in_opp_jungle = {
                    let role = proc_role.as_str();
                    let top  = ocr_enemy_top.unwrap_or(0);
                    let bot  = ocr_enemy_bot.unwrap_or(0);
                    match role {
                        "TOP" | "JUNGLE"           => bot >= 1 && top == 0,
                        "BOTTOM" | "ADC" | "SUPPORT" | "UTILITY" => top >= 1 && bot == 0,
                        "MID"                      => (top >= 1 || bot >= 1) && !(top >= 1 && bot >= 1),
                        _                          => false,
                    }
                };

                let proc_ctx = super::procedural::ProcContext {
                    game_time:         game_time,
                    player_role:       proc_role.clone(),
                    champion:          info.champion_name.clone(),
                    is_ahead:          ally_s > enemy_s,
                    ally_score:        ally_s,
                    enemy_score:       enemy_s,
                    dragon_ally:       info.dragon_count.ally,
                    dragon_enemy:      info.dragon_count.enemy,
                    next_dragon_spawn: info.next_dragon_spawn,
                    next_baron_spawn:  info.next_baron_spawn,
                    herald_killed:     info.herald_killed,
                    pattern:           proc_pattern.clone(),
                    support_in_lane,
                    enemy_in_opp_jungle,
                };

                let eval_alerts = proc_engine.evaluate(&proc_ctx, &app_lang);

                // Detecta se algum alerta de VISÃO foi gerado nesta avaliação.
                // Se sim, os spots de ward são emitidos como `ward_spots_forced`
                // — o frontend ignora o cooldown de 60s e exibe imediatamente,
                // sincronizando a barra de ward com a dica de visão.
                let vision_fired = eval_alerts.iter().any(|a| a.category == "VISION");

                for alert in eval_alerts {
                    emit_alert(&app, &alert);
                }

                let spots = proc_engine.suggest_ward_spots(&proc_ctx);

                // Spots da SpellCoach API — só emite se o jogador tem ward no inventário
                let is_red       = proc_info.as_ref().map(|i| !i.is_blue_side).unwrap_or(false);
                let has_ward     = proc_info.as_ref().map(|i| i.has_ward_available).unwrap_or(false);
                let advice = if has_ward {
                    ward_advisor.get_advice(game_time, is_red, 5)
                } else {
                    super::ward_advisor::WardAdvice { spots: vec![], alert: None }
                };

                // Momentos chave para mostrar ward spots no WardOverlay:
                //   1. Alerta de visão disparado → emissão forçada imediata
                //   2. Pré-objetivo (drake/baron em ≤ 90s) → proativo sem esperar visão
                //   3. Fluxo normal → respeita cooldown de 60s do frontend
                let dragon_secs = proc_ctx.next_dragon_spawn.saturating_sub(game_time);
                let baron_secs  = proc_ctx.next_baron_spawn
                    .map(|t| t.saturating_sub(game_time))
                    .unwrap_or(u32::MAX);
                let pre_objective = dragon_secs <= 90 || baron_secs <= 90;

                if vision_fired {
                    // Alerta de visão → WardOverlay aparece imediatamente
                    if !spots.is_empty() {
                        app.emit("ward_spots_forced", &spots).ok();
                    }
                    if !advice.spots.is_empty() {
                        app.emit("ward_spots_smart", &advice.spots).ok();
                    }
                    // Alerta textual mencionando o spot Challenger
                    if let Some(ref msg) = advice.alert {
                        emit_alert(&app, &CoachAlert {
                            id:        uuid::Uuid::new_v4().to_string(),
                            tip_id:    "ward_smart".to_string(),
                            category:  "VISION".to_string(),
                            severity:  "INFO".to_string(),
                            message:   msg.clone(),
                            timestamp: game_time as i64,
                        });
                    }
                } else if pre_objective && !advice.spots.is_empty() {
                    // Pré-objetivo: mostra spots Challenger no WardOverlay sem alert sonoro
                    app.emit("ward_spots_smart", &advice.spots).ok();
                    // Spots estáticos também se existirem
                    if !spots.is_empty() {
                        app.emit("ward_spots_forced", &spots).ok();
                    }
                } else {
                    // Fluxo normal: respeita cooldown de 60s do frontend
                    if !spots.is_empty() {
                        app.emit("ward_spots", &spots).ok();
                    }
                    if !advice.spots.is_empty() {
                        app.emit("ward_spots_smart", &advice.spots).ok();
                    }
                }
            }
        }

        // ── Refresh LCU a cada 30s — só fonte de dados, não gatilho ─
        let client_guard = lcu_client.lock().await;
        let Some(client) = client_guard.as_ref() else { continue };

        let need_refresh = last_info_at
            .map(|t| t.elapsed() >= Duration::from_secs(30))
            .unwrap_or(true);

        if need_refresh {
            // Atualiza idioma a cada ciclo de 30s para refletir mudanças em runtime.
            app_lang = fetch_app_language(&db).await;

            let fetched_role = fetch_player_role(client).await;
            if fetched_role != "UNKNOWN" {
                proc_role = fetched_role;
            }
            let info  = fetch_game_info(client).await;
            is_blue_side.store(info.is_blue_side, Ordering::Relaxed);

            // Carrega templates na primeira vez que temos os campeões inimigos
            let templates_empty = enemy_templates.lock().await.is_empty();
            if templates_empty && !info.enemy_champions.is_empty() {
                let version = client.get_game_version().await.unwrap_or_else(|_| "14.14.1".into());
                load_enemy_templates(client, &info.enemy_champions, &version, &enemy_templates).await;
            }

            // Notifica o frontend sobre o lado do time para o WardOverlay
            // ajustar os spots de ward profundo dinamicamente.
            app.emit("team_side", info.is_blue_side).ok();

            // Carrega dados de ward do SpellCoach quando o campeão + role forem identificados
            if !ward_advisor.has_data() {
                if let Some(ref champ) = info.champion_name {
                    let side = if info.is_blue_side { "BLUE" } else { "RED" };
                    ward_advisor.load(&db, champ, side, &proc_role).await;
                }
            }

            proc_info    = Some(info);
            proc_pattern = fetch_player_pattern(&db).await;
            last_info_at = Some(Instant::now());
        }
    }
}

/// Extrai (ally_score, enemy_score) de um snapshot allgamedata.
/// Reutiliza a mesma lógica de fetch_game_info mas sem alocar GameInfo completo.
fn extract_team_scores(data: &serde_json::Value) -> Option<(u8, u8)> {
    let players = data["allPlayers"].as_array()?;

    // Suporte a Riot ID (riotIdGameName) além do legado summonerName
    let sn = data["activePlayer"]["summonerName"].as_str().unwrap_or("");
    let rn = data["activePlayer"]["riotIdGameName"].as_str().unwrap_or("");
    let active_name = if !sn.is_empty() { sn } else { rn };

    if active_name.is_empty() { return None; }

    let ally_team = players.iter()
        .find(|p| {
            let psn = p["summonerName"].as_str().unwrap_or("");
            let prn = p["riotIdGameName"].as_str().unwrap_or("");
            psn == active_name || prn == active_name
        })
        .and_then(|p| p["team"].as_str());

    let mut ally:  u8 = 0;
    let mut enemy: u8 = 0;
    for p in players {
        let kills = p["scores"]["kills"].as_u64().unwrap_or(0) as u8;
        if p["team"].as_str() == ally_team { ally  += kills; }
        else                               { enemy += kills; }
    }
    Some((ally, enemy))
}

fn parse_template_match(s: &str) -> Option<(f32, f32, usize)> {
    let mut it = s.splitn(3, ',');
    let x:   f32   = it.next()?.parse().ok()?;
    let y:   f32   = it.next()?.parse().ok()?;
    let idx: usize = it.next()?.parse().ok()?;
    Some((x, y, idx))
}

fn parse_minimap_enemy(s: &str) -> Option<(u8, u8)> {
    let mut parts = s.splitn(2, '-');
    let top: u8 = parts.next()?.parse().ok()?;
    let bot: u8 = parts.next()?.parse().ok()?;
    Some((top, bot))
}

/// Parseia "x1,y1;x2,y2;..." em lista de posições percentuais.
fn parse_enemy_positions(s: &str) -> Vec<(f32, f32)> {
    if s.is_empty() { return Vec::new(); }
    s.split(';')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, ',');
            let x: f32 = it.next()?.parse().ok()?;
            let y: f32 = it.next()?.parse().ok()?;
            Some((x, y))
        })
        .collect()
}

/// Detecta ward-reveals via template matching com dupla proteção contra spam:
///   1. Confirmação em 2 frames consecutivos (~4s) — template já é confiável
///   2. Cooldown de 120s por área (raio 20%) — evita repetir mesma posição
///   3. Rate limiter global — máx 1 alerta a cada 25s
///
/// Fonte: apenas template matching de ícone de campeão (não dots de cor).
fn detect_ward_reveals(
    app:         &AppHandle,
    current:     &[(f32, f32)],
    champ_name:  &str,
    pending:     &mut Vec<(f32, f32, u8)>,
    notified:    &mut Vec<(f32, f32, Instant)>,
    last_notif:  &mut Option<Instant>,
    game_time:   u32,
    lang:        &str,
) {
    const CONFIRM_FRAMES:  u8  = 2;    // 2 × ~2s OCR = ~4s visível antes de alertar
    const NOTIFIED_SECS:   u64 = 120;
    const NOTIFIED_RADIUS: f32 = 20.0;
    const MATCH_RADIUS:    f32 = 8.0;
    const GLOBAL_COOLDOWN: u64 = 25;
    const MIN_GAME_TIME:   u32 = 90;   // ignora primeiros 90s (loading/spawn)

    if game_time < MIN_GAME_TIME {
        pending.clear();
        return;
    }

    notified.retain(|(_, _, t)| t.elapsed().as_secs() < NOTIFIED_SECS);

    let mut new_pending: Vec<(f32, f32, u8)> = Vec::new();

    for &(x, y) in current {
        // Ignora bases (template pode matchear no spawn pool)
        if (x < 12.0 && y > 85.0) || (x > 88.0 && y < 15.0) { continue; }

        if let Some(i) = pending.iter().position(|&(px, py, _)| {
            (x - px).abs() < MATCH_RADIUS && (y - py).abs() < MATCH_RADIUS
        }) {
            let (px, py, count) = pending[i];
            let new_count = count + 1;

            if new_count >= CONFIRM_FRAMES {
                let area_clear = !notified.iter().any(|&(nx, ny, _)| {
                    (x - nx).abs() < NOTIFIED_RADIUS && (y - ny).abs() < NOTIFIED_RADIUS
                });
                let global_clear = last_notif
                    .map(|t| t.elapsed().as_secs() >= GLOBAL_COOLDOWN)
                    .unwrap_or(true);

                if area_clear && global_clear {
                    let zone = classify_zone(x, y);
                    let zone_label = zone_label(zone, lang);

                    let message = if lang == "en-US" {
                        if champ_name.is_empty() {
                            format!("Enemy spotted in {} — ward here to maintain vision", zone_label)
                        } else {
                            format!("{} spotted in {} — ward here to maintain vision", champ_name, zone_label)
                        }
                    } else if champ_name.is_empty() {
                        format!("Inimigo avistado na {} — ward aqui para manter a visão", zone_label)
                    } else {
                        format!("{} avistado na {} — ward aqui para manter a visão", champ_name, zone_label)
                    };

                    emit_alert(app, &CoachAlert {
                        id:        uuid::Uuid::new_v4().to_string(),
                        tip_id:    "vision_enemy".to_string(),
                        category:  "VISION".to_string(),
                        severity:  "WARNING".to_string(),
                        message,
                        timestamp: game_time as i64,
                    });

                    // Ward spots relevantes para a zona onde o inimigo foi avistado
                    let spots: &[&str] = match zone {
                        "baron"    => &["baron_pit", "baron_river"],
                        "dragon"   => &["dragon_pit", "pixel_ward", "river_bot_brush"],
                        "top_lane" => &["top_river", "tribush_top"],
                        "bot_lane" => &["river_bot_brush", "pixel_ward"],
                        "jg_top"   => &["deep_top_enemy", "baron_river"],
                        "jg_bot"   => &["deep_bot_enemy", "river_bot_brush"],
                        "river_mid" => &["mid_river_drake", "mid_river_baron"],
                        _                 => &[],
                    };
                    if !spots.is_empty() {
                        app.emit("ward_spots_forced", spots).ok();
                    }
                    app.emit("enemy_spotted", serde_json::json!({
                        "x_pct": x, "y_pct": y, "zone": zone, "champ": champ_name,
                    })).ok();

                    notified.push((x, y, Instant::now()));
                    *last_notif = Some(Instant::now());
                }
                new_pending.push((px, py, 0));
            } else {
                new_pending.push((px, py, new_count));
            }
        } else {
            new_pending.push((x, y, 1));
        }
    }

    *pending = new_pending;
}

/// Classifica a posição (x%, y%) em uma zona nomeada do mapa SR.
/// Coordenadas: x 0%=esquerda, 100%=direita; y 0%=topo, 100%=baixo.
/// Carrega templates dos ícones dos campeões inimigos:
///   1. LCU local → mapa display name → DDragon key (sem chamada externa)
///   2. Data Dragon CDN → ícone base PNG (confiável: minimap sempre usa ícone base)
async fn load_enemy_templates(
    client:    &LcuClient,
    champions: &[String],
    version:   &str,
    templates: &Arc<Mutex<Vec<image::GrayImage>>>,
) {
    let name_to_key = match client.get_champion_name_to_ddragon_key().await {
        Ok(m)  => m,
        Err(e) => {
            tracing::warn!("[Coach] falha ao obter mapa DDragon: {e}");
            return;
        }
    };

    let mut loaded = Vec::new();

    for name in champions {
        let ddragon_key = name_to_key.get(name.as_str())
            .cloned()
            .unwrap_or_else(|| name.replace(' ', "")); // fallback: remove espaços

        match client.download_champion_icon(&ddragon_key, version).await {
            Ok(bytes) => {
                if let Some(tpl) = crate::ocr::template::decode_and_prepare(&bytes) {
                    tracing::info!("[Coach] template DDragon carregado: {name} → {ddragon_key}");
                    loaded.push(tpl);
                } else {
                    tracing::warn!("[Coach] falha ao decodificar ícone de {name}");
                }
            }
            Err(e) => {
                tracing::warn!("[Coach] falha ao baixar ícone de {name} ({ddragon_key}): {e}");
            }
        }
    }

    if !loaded.is_empty() {
        tracing::info!("[Coach] {} templates prontos para matching", loaded.len());
        *templates.lock().await = loaded;
    }
}

/// Classifica a posição em uma zona interna (chave estável, independente de idioma).
fn classify_zone(x: f32, y: f32) -> &'static str {
    if (28.0..=44.0).contains(&x) && (18.0..=36.0).contains(&y) { return "baron"; }
    if (60.0..=80.0).contains(&x) && (60.0..=80.0).contains(&y) { return "dragon"; }
    if x < 15.0 && y > 82.0 { return "ally_base"; }
    if x > 85.0 && y < 18.0 { return "enemy_base"; }
    if x < 18.0 || (x < 28.0 && y < 45.0) { return "top_lane"; }
    if y > 82.0 || (x > 72.0 && y > 55.0)  { return "bot_lane"; }
    if x < 48.0 && y < 52.0 { return "jg_top"; }
    if x > 52.0 && y > 48.0 { return "jg_bot"; }
    if (35.0..=65.0).contains(&x) && (35.0..=65.0).contains(&y) { return "river_mid"; }
    "map"
}

/// Traduz a zona interna para o rótulo exibido ao jogador.
fn zone_label(zone: &str, lang: &str) -> &'static str {
    if lang == "en-US" {
        match zone {
            "baron"      => "Baron area",
            "dragon"     => "Dragon area",
            "ally_base"  => "allied base",
            "enemy_base" => "enemy base",
            "top_lane"   => "top lane",
            "bot_lane"   => "bot lane",
            "jg_top"     => "top jungle",
            "jg_bot"     => "bot jungle",
            "river_mid"  => "river / mid",
            _            => "the map",
        }
    } else {
        match zone {
            "baron"      => "área do Baron",
            "dragon"     => "área do Drake",
            "ally_base"  => "base aliada",
            "enemy_base" => "base inimiga",
            "top_lane"   => "top lane",
            "bot_lane"   => "bot lane",
            "jg_top"     => "jungle top",
            "jg_bot"     => "jungle bot",
            "river_mid"  => "rio / mid",
            _            => "mapa",
        }
    }
}
