// ============================================================
// ocr/ — Motor de OCR para leitura do HUD do LoL
//
// Tecnologia:
//   - xcap         → captura de tela (GDI, sem deps extras)
//   - image        → pré-processamento (escala 2x, threshold)
//   - Windows OCR  → reconhecimento de texto (built-in Win 10+)
//   - spell_cd     → detecção de cooldown por análise de cor
//
// Produz eventos de texto que o CoachEngine consome via canal mpsc.
// ============================================================

pub mod capture;
pub mod engine;
pub mod minimap;
pub mod parser;
pub mod preprocess;
pub mod regions;
pub mod spell_cd;
pub mod template;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::mpsc;

use crate::game_state::states::GamePhase;
use tokio::sync::Mutex;

use regions::regions_1080p;
use spell_cd::{SpellState, detect_spell_state, spell_change_event, enemy_spell_event};

/// Intervalo entre ciclos OCR completos.
/// 2s é suficiente — o timer muda a cada segundo mas o coach age a cada 30s.
const OCR_INTERVAL_SECS: u64 = 2;

/// Ciclos consecutivos necessários para confirmar uma mudança de estado.
/// 4 ciclos × 2s = 8s — filtra qualquer flicker de saturação abaixo da
/// duração mínima real de cooldown (~105s para Exhaust).
const CONFIRM_CYCLES: u8 = 4;

/// Rastreia o estado de um spell com debounce para filtrar ruído do OCR.
///
/// Um spell precisa ser detectado no mesmo estado novo por CONFIRM_CYCLES
/// consecutivos antes de o evento ser emitido. Isso elimina falsos positivos
/// causados por quedas breves de saturação (efeitos visuais, artefatos de captura).
struct SpellTracker {
    confirmed: SpellState,
    pending:   SpellState,
    streak:    u8,
}

impl SpellTracker {
    fn new() -> Self {
        Self { confirmed: SpellState::Unknown, pending: SpellState::Unknown, streak: 0 }
    }

    /// Alimenta uma nova leitura. Retorna `Some((anterior, novo))` se a mudança
    /// foi confirmada após CONFIRM_CYCLES ciclos consecutivos.
    fn update(&mut self, detected: SpellState) -> Option<(SpellState, SpellState)> {
        if detected == self.confirmed {
            self.pending = detected;
            self.streak  = 0;
            return None;
        }
        if detected == self.pending {
            self.streak += 1;
        } else {
            self.pending = detected;
            self.streak  = 1;
        }
        if self.streak >= CONFIRM_CYCLES {
            let prev       = std::mem::replace(&mut self.confirmed, self.pending.clone());
            self.streak    = 0;
            return Some((prev, self.confirmed.clone()));
        }
        None
    }

    fn reset(&mut self) {
        self.confirmed = SpellState::Unknown;
        self.pending   = SpellState::Unknown;
        self.streak    = 0;
    }
}

/// Loop OCR em background.
///
/// O OCR agora trata APENAS o que a LCU/Live API não fornece:
///   - Cooldown de summoner spells (análise de cor — sem endpoint equivalente)
///   - Imagem do minimapa (para o WardOverlay)
///   - Contagem de aliados/inimigos no minimapa por zona
///
/// Timer e kill score foram removidos — a LCU fornece via
/// `/liveclientdata/gamestats` e `/liveclientdata/eventdata` (polled a 5s).
pub async fn run_ocr_loop(
    game_phase:       Arc<Mutex<GamePhase>>,
    tx:               mpsc::Sender<String>,
    is_blue_side:     Arc<AtomicBool>,
    app:              AppHandle,
    enemy_templates:  Arc<Mutex<Vec<image::GrayImage>>>,
) {
    let (screen_w, screen_h) = capture::game_monitor_size()
        .unwrap_or((1920, 1080));

    // Trackers com debounce para spells DO JOGADOR
    let mut tracker_spell_d = SpellTracker::new();
    let mut tracker_spell_f = SpellTracker::new();

    // Trackers com debounce para spells dos 5 INIMIGOS (índice 0 = slot 1)
    let mut tracker_enemy_d: Vec<SpellTracker> = (0..5).map(|_| SpellTracker::new()).collect();
    let mut tracker_enemy_f: Vec<SpellTracker> = (0..5).map(|_| SpellTracker::new()).collect();

    tracing::info!("[OCR] loop iniciado — resolução: {screen_w}x{screen_h} | modo: spells + minimapa");

    loop {
        tokio::time::sleep(Duration::from_secs(OCR_INTERVAL_SECS)).await;

        let phase = game_phase.lock().await.clone();
        if !phase.is_in_game() {
            tracker_spell_d.reset();
            tracker_spell_f.reset();
            for t in tracker_enemy_d.iter_mut() { t.reset(); }
            for t in tracker_enemy_f.iter_mut() { t.reset(); }
            continue;
        }

        // ── Minimapa — aliados e inimigos ─────────────────────
        // A imagem capturada NÃO é mais enviada ao frontend (bordas pretas
        // do frame do HUD apareciam sobre o minimapa real do jogo).
        // O WardOverlay é transparente: o minimapa do LoL aparece por baixo,
        // e os dots de ward ficam sobrepostos sem substituir a imagem.
        let minimap_region = regions_1080p::MINIMAP.scale_to(screen_w, screen_h);
        if let Ok(img) = capture::capture_region(minimap_region) {
            let blue_side = is_blue_side.load(Ordering::Relaxed);

            let ally_count = minimap::count_allies_in_bot(&img, blue_side);
            tracing::debug!("[OCR] minimapa bot allies={ally_count} (blue_side={blue_side})");
            let _ = tx.try_send(format!("minimap_ally_bot:{ally_count}"));

            let enemies = minimap::detect_enemy_zones(&img);
            tracing::debug!(
                "[OCR] minimapa inimigos top={} bot={}",
                enemies.top_zone, enemies.bot_zone
            );
            let _ = tx.try_send(format!(
                "minimap_enemy:{}-{}",
                enemies.top_zone, enemies.bot_zone
            ));

            // Posições individuais de dots inimigos
            let enemy_pos = minimap::detect_enemy_positions(&img);
            let pos_str: String = enemy_pos
                .iter()
                .map(|(x, y)| format!("{x:.1},{y:.1}"))
                .collect::<Vec<_>>()
                .join(";");
            let _ = tx.try_send(format!("minimap_enemy_pos:{pos_str}"));

            // ── Template matching de campeões inimigos ─────────────
            let templates_snap = enemy_templates.lock().await.clone();
            if !templates_snap.is_empty() {
                let minimap_gray = image::DynamicImage::ImageRgba8(img).to_luma8();
                for (idx, tpl) in templates_snap.iter().enumerate() {
                    if let Some(m) = template::find_best_match(&minimap_gray, tpl) {
                        if m.score <= template::MATCH_THRESHOLD {
                            tracing::debug!(
                                "[OCR] campeão {} detectado ({:.1}%, {:.1}%) score={}",
                                idx, m.x_pct, m.y_pct, m.score
                            );
                            let _ = tx.try_send(
                                format!("enemy_template:{:.1},{:.1},{idx}", m.x_pct, m.y_pct)
                            );
                        }
                    }
                }
            }
        }

        // ── Cooldown de summoner spells (análise de cor) ───────
        let spell_d_region = regions_1080p::SPELL_D.scale_to(screen_w, screen_h);
        if let Ok(icon) = capture::capture_region(spell_d_region) {
            let state = detect_spell_state(&icon);
            if let Some((prev, cur)) = tracker_spell_d.update(state) {
                tracing::debug!("[OCR] spell D confirmado: {:?} → {:?}", prev, cur);
                if let Some(event) = spell_change_event("Flash_D", &cur, &prev) {
                    let _ = tx.try_send(event);
                }
            }
        }

        let spell_f_region = regions_1080p::SPELL_F.scale_to(screen_w, screen_h);
        if let Ok(icon) = capture::capture_region(spell_f_region) {
            let state = detect_spell_state(&icon);
            if let Some((prev, cur)) = tracker_spell_f.update(state) {
                tracing::debug!("[OCR] spell F confirmado: {:?} → {:?}", prev, cur);
                if let Some(event) = spell_change_event("Flash_F", &cur, &prev) {
                    let _ = tx.try_send(event);
                }
            }
        }

        // ── Summoner spells dos inimigos (análise de cor, painel superior) ──
        for i in 0usize..5 {
            let d_region = regions_1080p::ENEMY_SPELL_D[i].scale_to(screen_w, screen_h);
            if let Ok(icon) = capture::capture_region(d_region) {
                let state = detect_spell_state(&icon);
                if let Some((prev, cur)) = tracker_enemy_d[i].update(state) {
                    tracing::debug!("[OCR] inimigo {} D confirmado: {:?} → {:?}", i + 1, prev, cur);
                    if let Some(event) = enemy_spell_event(i + 1, "D", &cur, &prev) {
                        let _ = tx.try_send(event);
                    }
                }
            }

            let f_region = regions_1080p::ENEMY_SPELL_F[i].scale_to(screen_w, screen_h);
            if let Ok(icon) = capture::capture_region(f_region) {
                let state = detect_spell_state(&icon);
                if let Some((prev, cur)) = tracker_enemy_f[i].update(state) {
                    tracing::debug!("[OCR] inimigo {} F confirmado: {:?} → {:?}", i + 1, prev, cur);
                    if let Some(event) = enemy_spell_event(i + 1, "F", &cur, &prev) {
                        let _ = tx.try_send(event);
                    }
                }
            }
        }
    }
}

