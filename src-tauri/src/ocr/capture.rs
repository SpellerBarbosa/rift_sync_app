// ============================================================
// ocr/capture.rs — Captura de região de tela via xcap
//
// xcap usa GDI no Windows — funciona em windowed e borderless.
// Para fullscreen exclusivo, o LoL deve estar em borderless.
// ============================================================

use anyhow::{Context, Result};
use image::RgbaImage;
use xcap::Monitor;

use super::regions::ScreenRegion;

/// Captura uma região específica da tela e retorna como RgbaImage.
/// Identifica automaticamente qual monitor contém as coordenadas.
pub fn capture_region(region: ScreenRegion) -> Result<RgbaImage> {
    let monitors = Monitor::all().context("Falha ao enumerar monitores")?;

    // Encontra o monitor que contém o ponto (x, y)
    let monitor = monitors
        .iter()
        .find(|m| {
            let mx = m.x() as u32;
            let my = m.y() as u32;
            region.x >= mx
                && region.y >= my
                && region.x < mx + m.width()
                && region.y < my + m.height()
        })
        .or_else(|| monitors.first())
        .context("Nenhum monitor encontrado")?;

    let full = monitor
        .capture_image()
        .context("Falha ao capturar tela — LoL deve estar em modo borderless")?;

    // Coordenadas relativas ao monitor
    let rel_x = region.x.saturating_sub(monitor.x().max(0) as u32);
    let rel_y = region.y.saturating_sub(monitor.y().max(0) as u32);

    let crop_w = region.width.min(full.width().saturating_sub(rel_x));
    let crop_h = region.height.min(full.height().saturating_sub(rel_y));

    if crop_w == 0 || crop_h == 0 {
        anyhow::bail!(
            "Região ({},{} {}x{}) fora dos limites do monitor ({}x{})",
            region.x, region.y, region.width, region.height,
            monitor.width(), monitor.height()
        );
    }

    let cropped = image::imageops::crop_imm(&full, rel_x, rel_y, crop_w, crop_h).to_image();
    Ok(cropped)
}

/// Retorna as dimensões do monitor onde o League of Legends está rodando.
///
/// Estratégia de detecção (em ordem de prioridade):
///   1. Janela em andamento do LoL via Win32 API → monitor exato
///   2. Monitor primário do sistema (posição 0,0)
///   3. Primeiro monitor encontrado pelo xcap
///
/// Isso garante funcionamento correto em setups multi-monitor mesmo
/// quando o jogo não está no monitor principal.
pub fn game_monitor_size() -> Option<(u32, u32)> {
    #[cfg(windows)]
    if let Some(size) = lol_window_monitor_size() {
        tracing::debug!("[OCR] monitor do LoL detectado: {}x{}", size.0, size.1);
        return Some(size);
    }

    // Fallback: monitor primário (posição 0,0 no espaço virtual multi-monitor)
    let monitors = Monitor::all().ok()?;
    let result = monitors
        .iter()
        .find(|m| m.x() == 0 && m.y() == 0)
        .or_else(|| monitors.first())
        .map(|m| (m.width(), m.height()));

    if let Some((w, h)) = result {
        tracing::debug!("[OCR] monitor fallback: {}x{}", w, h);
    }
    result
}

/// Localiza a janela do LoL em andamento e retorna as dimensões do monitor
/// onde ela está sendo exibida, usando a Win32 API.
#[cfg(windows)]
fn lol_window_monitor_size() -> Option<(u32, u32)> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsWindowVisible};
    use windows::core::PCWSTR;

    /// Converte &str em wide string terminada em nulo para PCWSTR.
    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    // Títulos conhecidos da janela do LoL durante a partida
    const TITLES: &[&str] = &[
        "League of Legends (TM) Client",
        "League of Legends",
    ];

    let hwnd: HWND = TITLES
        .iter()
        .filter_map(|&title| {
            let wide = to_wide(title);
            // FindWindowW retorna Result<HWND> no windows 0.61
            let h = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) }.ok()?;
            if unsafe { IsWindowVisible(h) }.as_bool() {
                Some(h)
            } else {
                None
            }
        })
        .next()?;

    let hmonitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) };
    if hmonitor.is_invalid() {
        return None;
    }

    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

    if unsafe { GetMonitorInfoW(hmonitor, &mut info) }.as_bool() {
        let w = (info.rcMonitor.right  - info.rcMonitor.left) as u32;
        let h = (info.rcMonitor.bottom - info.rcMonitor.top)  as u32;
        Some((w, h))
    } else {
        None
    }
}

/// Alias mantido para compatibilidade interna — prefira `game_monitor_size`.
pub fn primary_monitor_size() -> Option<(u32, u32)> {
    game_monitor_size()
}
