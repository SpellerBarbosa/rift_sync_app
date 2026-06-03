// ============================================================
// commands/coach.rs — Commands de controle do sistema de coaching
// ============================================================

use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};

use crate::db::models::{CoachingSession, PlayerPattern};
use crate::AppState;

/// Endpoint Piper TTS — stream de áudio
const TTS_API_HF: &str = "https://spell2014-riftsyncai.hf.space/tts/stream";

// ── Structs do pós-game ───────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostGameData {
    pub match_id:      String,
    pub champion_name: String,
    pub champion_key:  String,
    pub role:          Option<String>,
    pub result:        String,
    pub kills:         i64,
    pub deaths:        i64,
    pub assists:       i64,
    pub cs_per_min:    f64,
    pub vision_score:  i64,
    pub duration:      i64,
    pub alerts:        Vec<CoachingSession>,
    pub pattern:       Option<PlayerPattern>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostGameAnalysis {
    pub summary:      String,
    pub strengths:    Vec<String>,
    pub improvements: Vec<String>,
    pub focus:        String,
}

// ── Helpers internos ──────────────────────────────────────────

/// Roteia a síntese de voz:
///   • Vozes Piper TTS (pt_BR-*, en_US-*) → HuggingFace API
///   • Demais → Windows SpeechSynthesizer via PowerShell
async fn synthesize_tts(text: &str, voice: &str, speed: f32, state: &State<'_, AppState>) -> Result<String, String> {
    if voice.starts_with("pt_BR-") || voice.starts_with("en_US-")
    || voice.starts_with("pf_") || voice.starts_with("pm_")
    || voice.starts_with("af_") || voice.starts_with("am_") {
        fetch_tts_hf(text, voice, speed, state).await
    } else {
        let t = text.to_string();
        let v = voice.to_string();
        tokio::task::spawn_blocking(move || crate::tts::winrt::synthesize_blocking(t, v))
            .await
            .map_err(|e| format!("Thread TTS gerou panic: {e}"))?
            .map_err(|e| format!("TTS falhou: {e}"))
    }
}

/// Chama a API Piper TTS com retry automático em 408 (cold start).
async fn fetch_tts_hf(text: &str, voice: &str, speed: f32, state: &State<'_, AppState>) -> Result<String, String> {
    let client = { state.tts_cache.lock().await.client.clone() };
    let delays  = [0u64, 20, 35];

    for (attempt, &delay) in delays.iter().enumerate() {
        if delay > 0 {
            tracing::info!("[TTS HF] Space acordando — aguardando {delay}s (tentativa {})", attempt + 1);
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }

        let resp = match client
            .post(TTS_API_HF)
            .json(&serde_json::json!({ "text": text, "voice": voice, "speed": speed }))
            .send()
            .await
        {
            Ok(r)  => r,
            Err(e) => return Err(format!("TTS API offline: {e}")),
        };

        if resp.status() == reqwest::StatusCode::REQUEST_TIMEOUT {
            tracing::warn!("[TTS HF] 408 — Space dormindo, tentativa {}/{}", attempt + 1, delays.len());
            if attempt + 1 < delays.len() { continue; }
            return Err("TTS indisponível — Space demorando para acordar, tente em segundos".to_string());
        }

        if !resp.status().is_success() {
            return Err(format!("TTS retornou {}", resp.status()));
        }

        let ct = resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("audio/wav")
            .split(';').next().unwrap_or("audio/wav")
            .trim()
            .to_string();

        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        return Ok(format!("data:{};base64,{}", ct, B64.encode(&bytes)));
    }

    Err("TTS indisponível após múltiplas tentativas".to_string())
}

/// Retorna lista de vozes instaladas no Windows (para debug/seleção no frontend).
#[tauri::command]
pub async fn list_tts_voices() -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(crate::tts::winrt::list_system_voices)
        .await
        .map_err(|e| format!("Thread falhou: {e}"))?
        .map_err(|e| format!("Falha ao listar vozes: {e}"))
        .map(|voices| {
            voices.into_iter().map(|(id, name, locale)| {
                serde_json::json!({ "id": id, "name": name, "locale": locale })
            }).collect()
        })
}

// ── Commands públicos ─────────────────────────────────────────

/// Retorna o data URL de áudio para um texto — verifica o cache antes
/// de sintetizar. Cache hit = resposta instantânea sem re-síntese.
///
/// Fluxo:
///   1. lock(cache) → get(text, voice) → Some(url) → retorna imediatamente
///   2. None → synthesize_tts (WinRT) → lock(cache).insert → retorna url
#[tauri::command]
pub async fn speak_tts(
    text:  String,
    voice: String,
    speed: Option<f32>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let speed = speed.unwrap_or(1.0).clamp(0.5, 2.0);

    // Cache key inclui speed para que mudanças de velocidade não sirvam áudio errado
    let cache_key = format!("{voice}|{speed:.2}|{text}");

    // 1. Cache hit
    {
        let mut cache = state.tts_cache.lock().await;
        if let Some(url) = cache.get(&cache_key, "") {
            return Ok(url);
        }
    }

    // 2. Cache miss — roteia para HF Piper ou Windows TTS
    let is_hf = voice.starts_with("pt_BR-") || voice.starts_with("en_US-")
             || voice.starts_with("pf_")    || voice.starts_with("pm_")
             || voice.starts_with("af_")    || voice.starts_with("am_");
    tracing::debug!("[TTS] MISS → {} speed={:.2} '{:.60}'", if is_hf { "HF Piper" } else { "Win" }, speed, text);
    let data_url = synthesize_tts(&text, &voice, speed, &state).await?;

    // 3. Armazena no cache
    state.tts_cache.lock().await.insert(&cache_key, "", data_url.clone());

    Ok(data_url)
}

/// WinRT TTS é local — não precisa de warm-up.
/// Mantido por compatibilidade com o frontend que o chama.
#[tauri::command]
pub async fn warm_up_tts(_voice: String, _state: State<'_, AppState>) -> Result<(), String> {
    Ok(())
}

/// Silencia ou ativa o sistema de voz TTS.
#[tauri::command]
pub async fn set_coach_muted(
    muted: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.is_muted.store(muted, Ordering::Relaxed);
    tracing::info!("Coach {}", if muted { "mutado" } else { "com voz" });
    Ok(())
}

/// Retorna dados da última partida + alertas gerados + padrões do jogador.
/// Fonte primária: tabela `matches` + `coaching_sessions` do banco local.
#[tauri::command]
pub async fn get_post_game_data(
    state: State<'_, AppState>,
) -> Result<PostGameData, String> {
    let db = state.db.lock().await;

    // Última partida do banco
    let row = db.query_row(
        "SELECT m.match_id, m.champion_name, COALESCE(cc.key,''), m.role, m.result,
                COALESCE(m.kills,0), COALESCE(m.deaths,0), COALESCE(m.assists,0),
                COALESCE(m.cs_per_min,0.0), COALESCE(m.vision_score,0), COALESCE(m.duration,0)
         FROM matches m
         LEFT JOIN champion_cache cc ON cc.champion_id = m.champion_id
         WHERE m.player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)
         ORDER BY m.id DESC LIMIT 1",
        [],
        |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, f64>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, i64>(10)?,
        )),
    ).map_err(|e| format!("Nenhuma partida no banco — sincronize o histórico: {e}"))?;

    let (match_id, champion_name, champion_key, role, result,
         kills, deaths, assists, cs_per_min, vision_score, duration) = row;

    // Alertas da sessão mais recente (prioridade: CRITICAL > WARNING > INFO)
    let mut stmt = db.prepare(
        "SELECT id, match_id, timestamp, category, severity, message, was_heard
         FROM coaching_sessions
         WHERE match_id = ?1
         ORDER BY CASE severity WHEN 'CRITICAL' THEN 0 WHEN 'WARNING' THEN 1 ELSE 2 END,
                  timestamp ASC
         LIMIT 10",
    ).map_err(|e| e.to_string())?;

    // Fallback: se não houver alertas para esse match_id, pega os mais recentes
    let alerts: Vec<CoachingSession> = {
        let by_match: Vec<CoachingSession> = stmt
            .query_map([&match_id], |row| {
                Ok(CoachingSession {
                    id:        row.get(0)?,
                    match_id:  row.get(1)?,
                    timestamp: row.get(2)?,
                    category:  row.get(3)?,
                    severity:  row.get(4)?,
                    message:   row.get(5)?,
                    was_heard: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();

        if by_match.is_empty() {
            let mut recent_stmt = db.prepare(
                "SELECT id, match_id, timestamp, category, severity, message, was_heard
                 FROM coaching_sessions
                 ORDER BY id DESC LIMIT 10",
            ).map_err(|e| e.to_string())?;
            let fallback: Vec<CoachingSession> = recent_stmt
                .query_map([], |row| {
                    Ok(CoachingSession {
                        id:        row.get(0)?,
                        match_id:  row.get(1)?,
                        timestamp: row.get(2)?,
                        category:  row.get(3)?,
                        severity:  row.get(4)?,
                        message:   row.get(5)?,
                        was_heard: row.get(6)?,
                    })
                })
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .collect();
            fallback
        } else {
            by_match
        }
    };

    // Padrões comportamentais do jogador
    let pattern = db.query_row(
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
    ).ok();

    Ok(PostGameData {
        match_id, champion_name, champion_key, role, result,
        kills, deaths, assists, cs_per_min, vision_score, duration,
        alerts, pattern,
    })
}

/// Chama o Groq para análise pós-game personalizada.
/// Combina estatísticas da última partida, alertas gerados e padrões históricos.
#[tauri::command]
pub async fn analyze_post_game(
    state: State<'_, AppState>,
) -> Result<PostGameAnalysis, String> {
    let groq = state.groq_client.as_ref()
        .ok_or("Análise IA não configurada")?
        .clone();

    // Lê idioma configurado pelo usuário
    let language = {
        let db = state.db.lock().await;
        db.query_row(
            "SELECT value FROM settings WHERE key = 'app_language'",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap_or_else(|_| "pt-BR".to_string())
    };

    // Busca dados direto do banco (sem reusar o command para evitar State aninhado)
    let (match_id, champion_name, role, result, kills, deaths, assists, cs_per_min, vision_score, duration) = {
        let db = state.db.lock().await;
        db.query_row(
            "SELECT m.match_id, m.champion_name, m.role, m.result,
                    COALESCE(m.kills,0), COALESCE(m.deaths,0), COALESCE(m.assists,0),
                    COALESCE(m.cs_per_min,0.0), COALESCE(m.vision_score,0), COALESCE(m.duration,0)
             FROM matches m
             WHERE m.player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)
             ORDER BY m.id DESC LIMIT 1",
            [],
            |row| Ok((
                row.get::<_,String>(0)?,
                row.get::<_,String>(1)?,
                row.get::<_,Option<String>>(2)?,
                row.get::<_,String>(3)?,
                row.get::<_,i64>(4)?,
                row.get::<_,i64>(5)?,
                row.get::<_,i64>(6)?,
                row.get::<_,f64>(7)?,
                row.get::<_,i64>(8)?,
                row.get::<_,i64>(9)?,
            )),
        ).map_err(|e| format!("Nenhuma partida no banco — sincronize o histórico: {e}"))?
    };

    let kda = (kills + assists) as f64 / deaths.max(1) as f64;

    // Alertas da sessão
    let alerts_text = {
        let db = state.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT category, severity, message FROM coaching_sessions
             WHERE match_id = ?1
             ORDER BY CASE severity WHEN 'CRITICAL' THEN 0 WHEN 'WARNING' THEN 1 ELSE 2 END
             LIMIT 8",
        ).map_err(|e| e.to_string())?;

        let rows: Vec<String> = stmt
            .query_map([&match_id], |row| {
                Ok(format!("[{}] {}", row.get::<_,String>(0)?, row.get::<_,String>(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();

        if rows.is_empty() { "nenhum alerta registrado".to_string() } else { rows.join("; ") }
    };

    // Padrões comportamentais
    let pattern_text = {
        let db = state.db.lock().await;
        db.query_row(
            "SELECT aggression_score, deaths_without_vision, avg_deaths_10_15,
                    lane_dominance, objective_control, roam_frequency, tp_efficiency, ward_score
             FROM player_patterns
             WHERE player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)
             LIMIT 1",
            [],
            |row| {
                Ok(format!(
                    "agressividade={:.1} ward_score={:.1} controle_obj={:.1} \
                     mortes_sem_visão={} avg_deaths_10-15={:.1} \
                     domínio_lane={:.1} roam={:.1} TP_eff={:.1}",
                    row.get::<_,f64>(0)?, row.get::<_,f64>(7)?, row.get::<_,f64>(4)?,
                    row.get::<_,i64>(1)?, row.get::<_,f64>(2)?,
                    row.get::<_,f64>(3)?, row.get::<_,f64>(5)?, row.get::<_,f64>(6)?,
                ))
            },
        ).unwrap_or_else(|_| "sem dados históricos".to_string())
    };

    // Últimas 5 partidas
    let recent_matches = {
        let db = state.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT champion_name, COALESCE(role,'?'), result,
                    COALESCE(kills,0), COALESCE(deaths,0), COALESCE(assists,0),
                    COALESCE(cs_per_min,0.0)
             FROM matches
             WHERE player_id = (SELECT id FROM players ORDER BY updated_at DESC LIMIT 1)
             ORDER BY id DESC LIMIT 5",
        ).map_err(|e| e.to_string())?;

        let rows: Vec<String> = stmt.query_map([], |row| {
            Ok(format!(
                "{} ({}) {} {}/{}/{} {:.1}cs/min",
                row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?,
                row.get::<_,i64>(3)?, row.get::<_,i64>(4)?, row.get::<_,i64>(5)?,
                row.get::<_,f64>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();
        rows.join("\n")
    };

    let body = groq.analyze_post_game(
        &champion_name,
        role.as_deref().unwrap_or("?"),
        &result,
        kda,
        kills, deaths, assists,
        cs_per_min,
        vision_score,
        duration / 60,
        &alerts_text,
        &pattern_text,
        &recent_matches,
        &language,
    ).await.map_err(|e| format!("Erro ao chamar IA: {e}"))?;

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("Resposta IA vazia")?;

    serde_json::from_str::<PostGameAnalysis>(content)
        .map_err(|e| format!("JSON de análise inválido: {e}"))
}

/// Chama o Groq para análise do perfil comportamental do jogador.
/// Requer player_patterns no banco — retorna erro se não houver dados.
#[tauri::command]
pub async fn analyze_player_profile_command(
    summary: String,
    state: State<'_, AppState>,
) -> Result<crate::groq::client::PlayerInsights, String> {
    let groq = state.groq_client.as_ref()
        .ok_or("Análise IA não configurada")?
        .clone();

    let language = {
        let db = state.db.lock().await;
        db.query_row(
            "SELECT value FROM settings WHERE key = 'app_language'",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap_or_else(|_| "pt-BR".to_string())
    };

    let pattern = {
        let db = state.db.lock().await;
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
        ).map_err(|_| "Sem dados comportamentais — jogue mais partidas com o RiftSync ativo")?
    };

    groq.analyze_player_profile(&pattern, &summary, &language).await
        .map_err(|e| format!("Erro na análise de perfil: {e}"))
}

/// Análise do dashboard: recebe resumo das últimas partidas + role e retorna insights.
/// Usa player_patterns se disponível; caso contrário usa valores neutros.
/// Retorna o mesmo tipo PlayerInsights (camelCase → DashboardAnalysis no frontend).
#[tauri::command]
pub async fn get_dashboard_analysis(
    matches_summary: String,
    role:            Option<String>,
    state: State<'_, AppState>,
) -> Result<crate::groq::client::PlayerInsights, String> {
    let groq = state.groq_client.as_ref()
        .ok_or("GROQ_API_KEY não configurada — adicione ao .env para análises com IA.")?
        .clone();

    let language = {
        let db = state.db.lock().await;
        db.query_row(
            "SELECT value FROM settings WHERE key = 'app_language'",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap_or_else(|_| "pt-BR".to_string())
    };

    // Tenta ler padrões comportamentais; usa zeros se ainda não houver dados suficientes
    let pattern = {
        let db = state.db.lock().await;
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
        ).unwrap_or_else(|_| PlayerPattern {
            id: 0, player_id: 0,
            aggression_score: 0.5, deaths_without_vision: 0,
            avg_deaths_10_15: 0.0, lane_dominance: 0.5,
            objective_control: 0.5, roam_frequency: 0.5,
            tp_efficiency: 0.5, ward_score: 0.5,
        })
    };

    let role_str = role.as_deref().unwrap_or("não informado");
    let summary  = format!("Role: {role_str}\n{matches_summary}");

    groq.analyze_player_profile(&pattern, &summary, &language).await
        .map_err(|e| format!("Erro na análise do dashboard: {e}"))
}

/// Dispara uma sequência de alertas de teste para verificar flashcards + TTS + WardOverlay.
/// Alertas VISION também emitem `ward_spots_forced` para testar a barra de ward.
#[tauri::command]
pub async fn debug_fire_replay(app: AppHandle) -> Result<(), String> {
    use tokio::time::{sleep, Duration};

    // (category, severity, message, delay_ms, ward_spots_to_show)
    // ward_spots_to_show: lista de IDs enviada via ward_spots_forced quando category == VISION
    let sequence: Vec<(&'static str, &'static str, &'static str, u64, &'static [&'static str])> = vec![
        ("OBJECTIVE",  "INFO",     "Herald em 25s — posicione na boca do rio topo",              0,     &[]),
        ("VISION",     "WARNING",  "Ward score baixo (22%) — cubra o rio antes do dragão",    2_500,  &["dragon_pit", "pixel_ward", "river_bot_brush"]),
        ("OBJECTIVE",  "WARNING",  "Dragão em 30s — reúna o time agora",                      3_000,  &[]),
        ("MACRO",      "INFO",     "Você está à frente 8-3 — converta em torres e objetivos", 2_000,  &[]),
        ("TRADE",      "WARNING",  "Inimigo fraco após troca — jogue agressivo agora",        3_500,  &[]),
        ("POSITIONING","INFO",     "Segure posição — baron spawn em 40s",                     3_000,  &[]),
        ("OBJECTIVE",  "CRITICAL", "INIMIGO A 1 DRAKE DA ALMA — conteste a todo custo!",      1_500,  &[]),
        ("VISION",     "INFO",     "Ward profundo necessário — jungle inimigo longe",         13_500,  &["baron_pit", "baron_river", "deep_top_enemy"]),
        ("MACRO",      "WARNING",  "Mid tower desprotegida — recue antes de objetivos",        4_000,  &[]),
        ("TRADE",      "INFO",     "Cooldown inimigo quase zerado — não engaje agora",         6_500,  &[]),
    ];

    tracing::info!("debug_fire_replay — {} alertas", sequence.len());

    tauri::async_runtime::spawn(async move {
        for (category, severity, message, delay_ms, ward_spots) in sequence {
            if delay_ms > 0 {
                sleep(Duration::from_millis(delay_ms)).await;
            }

            let payload = serde_json::json!({
                "id":        Uuid::new_v4().to_string(),
                "category":  category,
                "severity":  severity,
                "message":   message,
                "timestamp": std::time::SystemTime::now()
                                 .duration_since(std::time::UNIX_EPOCH)
                                 .unwrap_or_default()
                                 .as_millis() as u64,
            });

            if let Err(e) = app.emit("coach_alert", payload) {
                tracing::warn!("debug_fire_replay: falha ao emitir alerta — {e}");
            }

            // VISION + spots definidos → emite ward_spots_forced para testar a barra de ward
            if category == "VISION" && !ward_spots.is_empty() {
                if let Err(e) = app.emit("ward_spots_forced", ward_spots) {
                    tracing::warn!("debug_fire_replay: falha ao emitir ward_spots_forced — {e}");
                }
            }
        }
        tracing::info!("debug_fire_replay — concluído.");
    });

    Ok(())
}

/// Dispara o WardOverlay com spots de teste.
/// Garante que a janela overlay está visível antes de emitir — necessário quando
/// o teste é feito fora de uma partida (a janela pode estar hidden).
/// Emite 3 rodadas de spots diferentes para testar posicionamento e blue/red side.
#[tauri::command]
pub async fn debug_fire_ward(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use tauri::{PhysicalPosition, PhysicalSize};
    use tokio::time::{sleep, Duration};

    // Garante que a janela overlay está visível e cobre a tela inteira.
    // pointer-events: none no CSS — não captura input do usuário.
    if let Some(fc) = app.get_webview_window("flashcard") {
        let (sw, sh) = app.primary_monitor()
            .ok().flatten()
            .map(|m| { let s = m.size(); (s.width, s.height) })
            .unwrap_or((1920, 1080));

        let _ = fc.set_size(tauri::Size::Physical(PhysicalSize { width: sw, height: sh }));
        let _ = fc.set_position(tauri::Position::Physical(PhysicalPosition { x: 0, y: 0 }));
        let _ = fc.set_ignore_cursor_events(true);
        let _ = fc.show();
        tracing::info!("debug_fire_ward — overlay visível {sw}x{sh}");
    }

    // Três rodadas de spots diferentes — testa blue/red side e transições
    let rounds: &[(&[&str], u64)] = &[
        (&["dragon_pit", "pixel_ward", "river_bot_brush"],   100),  // lado drake
        (&["baron_pit",  "baron_river", "deep_top_enemy"],  13_000), // lado baron
        (&["top_river",  "tribush_top", "enemy_red_buff"],  13_000), // top lane + jungle
    ];

    tracing::info!("debug_fire_ward — {} rodadas", rounds.len());

    tauri::async_runtime::spawn(async move {
        for (spots, delay_ms) in rounds {
            if *delay_ms > 0 {
                sleep(Duration::from_millis(*delay_ms)).await;
            }
            if let Err(e) = app.emit("ward_spots_forced", spots) {
                tracing::warn!("debug_fire_ward: falha — {e}");
            } else {
                tracing::info!("debug_fire_ward → {:?}", spots);
            }
        }
        tracing::info!("debug_fire_ward — concluído.");
    });

    Ok(())
}
