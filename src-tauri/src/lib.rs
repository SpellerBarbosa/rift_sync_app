// Suprime dead_code de módulos stub que serão implementados nas fases 5-9
#![allow(dead_code)]
// ============================================================
// lib.rs — Entry point da biblioteca Tauri (Rust)
//
// Responsável por:
//   1. Inicializar o banco de dados SQLite
//   2. Criar e gerenciar o AppState compartilhado
//   3. Iniciar as tarefas em background (LCU polling, game state, coach)
//   4. Registrar todos os Tauri commands
// ============================================================

use std::sync::{atomic::AtomicBool, Arc};
use tauri::Manager;
use tokio::sync::Mutex;

// ── Módulos internos ─────────────────────────────────────────
mod coach;
mod commands;
pub mod db;
mod game_state;
pub mod groq;
mod lcu;
pub mod ocr;
pub mod spellcoach;
mod tts;

// ── Tipos públicos compartilhados ─────────────────────────────

/// Handle do cliente LCU — protegido por Mutex assíncrono.
/// `None` quando o LoL não está conectado.
pub type LcuState = Arc<Mutex<Option<lcu::client::LcuClient>>>;

/// Estado global do aplicativo — injetado em todos os commands via Tauri.
pub struct AppState {
    /// Conexão SQLite — WAL mode, thread-safe via tokio::sync::Mutex
    pub db: Arc<Mutex<rusqlite::Connection>>,

    /// Cliente LCU atual (None se LoL estiver fechado)
    pub lcu_client: LcuState,

    /// Fase atual do jogo — fonte da verdade no backend
    pub game_phase: Arc<Mutex<game_state::states::GamePhase>>,

    /// Controle de mudo sem overhead de lock (AtomicBool)
    pub is_muted: Arc<AtomicBool>,

    /// Cliente SpellCoach — None se SPELLCOACH_API_KEY não estiver configurada
    pub spellcoach_client: Option<Arc<spellcoach::SpellCoachClient>>,

    /// Cache LRU de respostas TTS — evita chamadas repetidas à API
    pub tts_cache: Arc<Mutex<tts::cache::TtsCache>>,

    /// Cliente Groq — para análise pós-game (requer GROQ_API_KEY)
    pub groq_client: Option<Arc<groq::client::GroqClient>>,
}

// ── Entry point ───────────────────────────────────────────────

/// Inicializa e executa a aplicação Tauri.
/// `#[cfg_attr(mobile, tauri::mobile_entry_point)]` habilita suporte
/// a iOS/Android no futuro (padrão Tauri 2).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Inicializa o logger estruturado (RUST_LOG controla o nível)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "riftsync_ai_lib=debug,tauri_plugin_updater=off,warn".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // ── Variáveis de ambiente (.env em dev) ───────────
            dotenvy::dotenv().ok();

            // ── Banco de dados ────────────────────────────────
            let db_conn = db::connection::init_db(app.handle())
                .map_err(|e| {
                    tracing::error!("Falha ao inicializar banco: {e}");
                    e
                })?;
            let db = Arc::new(Mutex::new(db_conn));

            // ── Cliente SpellCoach (opcional — requer SPELLCOACH_API_KEY) ─
            let spellcoach_client = match std::env::var("SPELLCOACH_API_KEY") {
                Ok(key) if !key.is_empty() => {
                    match spellcoach::SpellCoachClient::new(key) {
                        Ok(client) => {
                            tracing::info!("SpellCoach API conectada — stats de campeões ativo.");
                            Some(Arc::new(client))
                        }
                        Err(e) => {
                            tracing::warn!("Falha ao criar cliente SpellCoach: {e}");
                            None
                        }
                    }
                }
                _ => {
                    tracing::warn!("SPELLCOACH_API_KEY não definida — stats de meta desativados.");
                    None
                }
            };

            // ── Cliente Groq (análise pós-game) ──────────────
            let groq_client = match std::env::var("GROQ_API_KEY") {
                Ok(key) if !key.is_empty() => {
                    match groq::client::GroqClient::new(key) {
                        Ok(c) => {
                            tracing::info!("Groq API conectada — análise pós-game ativa.");
                            Some(Arc::new(c))
                        }
                        Err(e) => {
                            tracing::warn!("Falha ao criar cliente Groq: {e}");
                            None
                        }
                    }
                }
                _ => {
                    tracing::warn!("GROQ_API_KEY não definida — análise pós-game desativada.");
                    None
                }
            };

            // ── Cache TTS ─────────────────────────────────────
            let tts_cache = match tts::cache::TtsCache::new() {
                Ok(c)  => Arc::new(Mutex::new(c)),
                Err(e) => {
                    tracing::error!("Falha ao criar TTS cache: {e}");
                    return Err(e.into());
                }
            };

            // ── AppState ──────────────────────────────────────
            let lcu_client: LcuState = Arc::new(Mutex::new(None));
            let game_phase = Arc::new(Mutex::new(game_state::states::GamePhase::default()));

            app.manage(AppState {
                db:                Arc::clone(&db),
                lcu_client:        Arc::clone(&lcu_client),
                game_phase:        Arc::clone(&game_phase),
                is_muted:          Arc::new(AtomicBool::new(false)),
                spellcoach_client: spellcoach_client,
                tts_cache:         Arc::clone(&tts_cache),
                groq_client:       groq_client,
            });

            // ── Sync automático da SpellCoach API ─────────────
            // Sincroniza meta stats no startup e repete a cada 6h.
            // O delay inicial de 8s evita contention com o setup do LCU.
            if let Some(sc_client) = &app.state::<AppState>().spellcoach_client {
                let db_sync     = Arc::clone(&db);
                let client_sync = Arc::clone(sc_client);
                let app_sync    = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    const SIX_HOURS: tokio::time::Duration =
                        tokio::time::Duration::from_secs(6 * 60 * 60);

                    tokio::time::sleep(tokio::time::Duration::from_secs(8)).await;
                    loop {
                        tracing::info!("SpellCoach: iniciando sync automático (meta + builds)…");
                        let (ok, err) = spellcoach::sync::full_sync_with_progress(
                            Arc::clone(&db_sync),
                            Arc::clone(&client_sync),
                            &app_sync,
                        ).await;
                        tracing::info!("SpellCoach auto-sync: {ok} ok, {err} erros");
                        tokio::time::sleep(SIX_HOURS).await;
                    }
                });
            }

            // ── Tarefa GC do cache TTS (a cada 10 min) ────────
            let cache_for_gc = Arc::clone(&tts_cache);
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(tts::cache::GC_INTERVAL).await;
                    cache_for_gc.lock().await.gc();
                }
            });

            // ── Tarefa de polling do LCU (background) ─────────
            let app_handle = app.handle().clone();
            let lcu_for_polling = Arc::clone(&lcu_client);
            tauri::async_runtime::spawn(async move {
                lcu::client::start_lcu_polling(app_handle, lcu_for_polling).await;
            });

            // ── Tarefa de polling do game state (background) ──
            let app_handle2 = app.handle().clone();
            let lcu_for_state = Arc::clone(&lcu_client);
            let phase_for_state = Arc::clone(&game_phase);
            tauri::async_runtime::spawn(async move {
                game_state::manager::run_game_state_loop(
                    app_handle2,
                    lcu_for_state,
                    phase_for_state,
                )
                .await;
            });

            // ── Canal OCR → Coach (eventos de tela em tempo real) ─
            let (ocr_tx, ocr_rx) = tokio::sync::mpsc::channel::<String>(64);

            // Lado do time — compartilhado entre OCR e Coach
            let is_blue_side = Arc::new(AtomicBool::new(true));

            // Templates dos campeões inimigos — populado pelo Coach, usado pelo OCR
            let enemy_templates: Arc<tokio::sync::Mutex<Vec<image::GrayImage>>> =
                Arc::new(tokio::sync::Mutex::new(Vec::new()));

            // ── Loop de OCR (background) ──────────────────────
            let phase_for_ocr      = Arc::clone(&game_phase);
            let blue_side_for_ocr  = Arc::clone(&is_blue_side);
            let app_for_ocr        = app.handle().clone();
            let templates_for_ocr  = Arc::clone(&enemy_templates);
            tauri::async_runtime::spawn(async move {
                ocr::run_ocr_loop(
                    phase_for_ocr,
                    ocr_tx,
                    blue_side_for_ocr,
                    app_for_ocr,
                    templates_for_ocr,
                ).await;
            });

            // ── Loop de coaching procedural (background) ──────
            {
                let app_handle3          = app.handle().clone();
                let lcu_for_coach        = Arc::clone(&lcu_client);
                let phase_for_coach      = Arc::clone(&game_phase);
                let db_for_coach         = Arc::clone(&db);
                let blue_side_for_coach  = Arc::clone(&is_blue_side);
                let templates_for_coach  = Arc::clone(&enemy_templates);
                tauri::async_runtime::spawn(async move {
                    coach::engine::run_coach_loop(
                        app_handle3,
                        lcu_for_coach,
                        phase_for_coach,
                        ocr_rx,
                        db_for_coach,
                        blue_side_for_coach,
                        templates_for_coach,
                    )
                    .await;
                });
            }

            tracing::info!("RiftSync AI inicializado com sucesso.");
            Ok(())
        })
        // ── Registro de commands ──────────────────────────────
        .invoke_handler(tauri::generate_handler![
            // loading
            commands::loading::get_loading_screen_players,
            // game_state
            commands::game_state::get_game_phase,
            commands::game_state::get_lcu_connection_status,
            commands::game_state::get_gameflow_phase,
            commands::game_state::get_champ_select_session,
            commands::game_state::get_local_player_pick,
            commands::game_state::force_lcu_connect,
            // player
            commands::player::get_current_player,
            commands::player::get_current_summoner,
            commands::player::get_recent_matches,
            commands::player::get_last_player,
            commands::player::get_cached_matches,
            commands::player::get_match_stats,
            commands::player::sync_match_history,
            commands::player::sync_champion_cache,
            commands::player::get_setting,
            commands::player::set_setting,
            commands::player::get_all_settings,
            // builds / wards (SpellCoach)
            commands::builds::sync_champion_assets,
            commands::builds::get_champion_build,
            commands::builds::get_champion_wards,
            // champion / spellcoach
            commands::champion::sync_spellcoach_data,
            commands::champion::get_champion_data,
            commands::champion::get_all_champions,
            commands::champion::get_champion_matchup,
            commands::champion::get_champion_stats,
            commands::champion::get_matchup_stats,
            commands::champion::get_pick_recommendation,
            // coach
            commands::coach::set_coach_muted,
            commands::coach::get_post_game_data,
            commands::coach::analyze_post_game,
            commands::coach::analyze_player_profile_command,
            commands::coach::get_dashboard_analysis,
            commands::coach::debug_fire_replay,
            commands::coach::debug_fire_ward,
            commands::coach::speak_tts,
            commands::coach::warm_up_tts,
            commands::coach::list_tts_voices,
            // player (extended)
            commands::player::get_player_patterns,
            commands::player::compute_player_patterns,
            // window
            commands::window::minimize_main,
            commands::window::show_rune_overlay,
            commands::window::hide_rune_overlay,
            commands::window::enter_game_mode,
            commands::window::exit_game_mode,
            commands::window::show_flashcard_window,
            commands::window::hide_flashcard_window,
        ])
        .run(tauri::generate_context!())
        .expect("Erro ao iniciar aplicação Tauri");
}
