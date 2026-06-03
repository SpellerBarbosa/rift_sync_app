use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

fn screen_size(app: &AppHandle) -> (u32, u32) {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|m| { let s = m.size(); (s.width, s.height) })
        .unwrap_or((1920, 1080))
}

/// Posiciona e configura uma janela overlay sem exibi-la.
fn prepare_win(app: &AppHandle, label: &str, x: i32, y: i32) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_position(tauri::Position::Physical(PhysicalPosition { x, y }));
        let _ = w.set_ignore_cursor_events(true);
    }
}

/// Minimiza a janela principal sem abrir overlay.
#[tauri::command]
pub async fn minimize_main(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        main.minimize().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Ativa o modo in-game:
/// - Minimiza janela principal
/// - Posiciona TODAS as janelas overlay nos locais corretos
/// - Mostra APENAS o flashcard (alertas do coach — sempre ativo)
/// - As demais janelas gerenciam a própria visibilidade baseada em conteúdo
#[tauri::command]
pub async fn enter_game_mode(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        main.minimize().map_err(|e| e.to_string())?;
    }

    let (sw, sh) = screen_size(&app);
    let sw = sw as i32;
    let sh = sh as i32;

    // ── Flashcard — pré-posiciona no canto superior direito ────────
    // Não exibe aqui: FlashCard.vue gerencia a visibilidade via
    // show_flashcard_window / hide_flashcard_window conforme o card ativo.
    if let Some(fc) = app.get_webview_window("flashcard") {
        let _ = fc.set_size(tauri::Size::Physical(PhysicalSize { width: 360, height: 152 }));
        let _ = fc.set_position(tauri::Position::Physical(PhysicalPosition { x: sw - 372, y: 12 }));
        let _ = fc.set_ignore_cursor_events(true);
    }

    // ── Posiciona demais janelas sem exibir (cada uma exibe quando tiver conteúdo) ──
    let ward_y = sh - (sh * 13 / 100) - 215;
    prepare_win(&app, "ward_win",     (sw - 560) / 2, ward_y);
    prepare_win(&app, "matchup_win",  sw - 282,        172);
    prepare_win(&app, "ingame_build", 10,              sh / 2 - 170);
    prepare_win(&app, "loading_win",  (sw - 980) / 2, (sh - 570) / 2);

    tracing::info!("[Window] Modo in-game: janelas overlay posicionadas.");
    Ok(())
}

/// Desativa o modo in-game — esconde todas as janelas e restaura a principal.
#[tauri::command]
pub async fn exit_game_mode(app: AppHandle) -> Result<(), String> {
    for label in ["flashcard", "ward_win", "matchup_win", "ingame_build", "loading_win"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.set_ignore_cursor_events(false);
            let _ = w.hide();
        }
    }
    if let Some(main) = app.get_webview_window("main") {
        main.unminimize().map_err(|e| e.to_string())?;
        main.set_focus().map_err(|e| e.to_string())?;
    }
    tracing::info!("[Window] Modo in-game desativado.");
    Ok(())
}

/// Exibe o flashcard (chamado por FlashCard.vue quando o primeiro card fica ativo).
#[tauri::command]
pub async fn show_flashcard_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("flashcard") {
        w.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Esconde o flashcard (chamado por FlashCard.vue quando a fila esvazia).
#[tauri::command]
pub async fn hide_flashcard_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("flashcard") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Exibe as janelinhas de champ-select: rune_panel + build_panel + champ_pick_win.
#[tauri::command]
pub async fn show_rune_overlay(app: AppHandle) -> Result<(), String> {
    let (sw, sh) = screen_size(&app);
    let sh = sh as i32;
    let sw = sw as i32;

    if let Some(rp) = app.get_webview_window("rune_panel") {
        rp.set_position(tauri::Position::Physical(PhysicalPosition { x: sw - 330, y: sh / 2 - 210 }))
          .map_err(|e| e.to_string())?;
        rp.show().map_err(|e| e.to_string())?;
    }
    if let Some(bp) = app.get_webview_window("build_panel") {
        bp.set_position(tauri::Position::Physical(PhysicalPosition { x: 10, y: sh / 2 - 195 }))
          .map_err(|e| e.to_string())?;
        bp.show().map_err(|e| e.to_string())?;
    }
    // champ_pick_win removido: o overlay de pick não tem conteúdo visível nessa fase
    Ok(())
}

/// Esconde as janelinhas de champ-select.
#[tauri::command]
pub async fn hide_rune_overlay(app: AppHandle) -> Result<(), String> {
    for label in ["rune_panel", "build_panel", "champ_pick_win"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
        }
    }
    Ok(())
}
