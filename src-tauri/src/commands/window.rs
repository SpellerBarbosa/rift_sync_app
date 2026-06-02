use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

fn screen_size(app: &AppHandle) -> (u32, u32) {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let s = m.size();
            (s.width, s.height)
        })
        .unwrap_or((1920, 1080))
}

/// Minimiza a janela principal sem abrir overlay.
/// Chamado ao entrar em BAN_PHASE ou PICK_PHASE.
#[tauri::command]
pub async fn minimize_main(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        main.minimize().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Ativa o modo in-game: minimiza a janela principal e posiciona
/// a janela flashcard no centro-direito da tela (ainda oculta).
/// A janela flashcard só aparece quando o primeiro alerta é emitido.
#[tauri::command]
pub async fn enter_game_mode(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        main.minimize().map_err(|e| e.to_string())?;
    }

    // Modo jogo: overlay full-screen transparente sobre o jogo.
    // Cobre todo o monitor onde o LoL está rodando.
    // pointer-events: none no CSS — não captura mouse nem teclado.
    if let Some(fc) = app.get_webview_window("flashcard") {
        let (sw, sh) = screen_size(&app);

        fc.set_size(tauri::Size::Physical(PhysicalSize { width: sw, height: sh }))
          .map_err(|e| e.to_string())?;
        fc.set_position(tauri::Position::Physical(PhysicalPosition { x: 0, y: 0 }))
          .map_err(|e| e.to_string())?;
        fc.set_ignore_cursor_events(true).map_err(|e| e.to_string())?;
        fc.show().map_err(|e| e.to_string())?;

        tracing::info!("Modo jogo ativado — overlay full-screen {sw}x{sh}.");
    }

    Ok(())
}

/// Desativa o modo in-game: esconde a janela flashcard (caso esteja
/// exibindo um alerta) e restaura a janela principal.
#[tauri::command]
pub async fn exit_game_mode(app: AppHandle) -> Result<(), String> {
    if let Some(fc) = app.get_webview_window("flashcard") {
        fc.set_ignore_cursor_events(false).map_err(|e| e.to_string())?;
        fc.hide().map_err(|e| e.to_string())?;
    }

    if let Some(main) = app.get_webview_window("main") {
        main.unminimize().map_err(|e| e.to_string())?;
        main.set_focus().map_err(|e| e.to_string())?;
    }

    tracing::info!("Modo jogo desativado — janela principal restaurada.");
    Ok(())
}

/// No-op: a janela overlay é gerenciada por enter/exit_game_mode.
/// Mantido para compatibilidade com código legado que ainda invoca este comando.
#[tauri::command]
pub async fn show_flashcard_window(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

/// No-op: a janela overlay é ocultada por exit_game_mode.
/// Mantido para compatibilidade com código legado que ainda invoca este comando.
#[tauri::command]
pub async fn hide_flashcard_window(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

/// Posiciona e exibe as duas janelinhas independentes de runas e build.
/// rune_panel: canto direito, verticalmente centrado.
/// build_panel: canto esquerdo, verticalmente centrado.
#[tauri::command]
pub async fn show_rune_overlay(app: AppHandle) -> Result<(), String> {
    let (sw, sh) = screen_size(&app);
    let sh = sh as i32;
    let sw = sw as i32;

    if let Some(rp) = app.get_webview_window("rune_panel") {
        rp.set_position(tauri::Position::Physical(PhysicalPosition {
            x: sw - 330,
            y: sh / 2 - 210,  // metade de 420px
        })).map_err(|e| e.to_string())?;
        rp.show().map_err(|e| e.to_string())?;
    }

    if let Some(bp) = app.get_webview_window("build_panel") {
        bp.set_position(tauri::Position::Physical(PhysicalPosition {
            x: 10,
            y: sh / 2 - 195,
        })).map_err(|e| e.to_string())?;
        bp.show().map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Esconde as duas janelinhas de runas e build.
#[tauri::command]
pub async fn hide_rune_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(rp) = app.get_webview_window("rune_panel") {
        rp.hide().map_err(|e| e.to_string())?;
    }
    if let Some(bp) = app.get_webview_window("build_panel") {
        bp.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}
