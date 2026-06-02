// ============================================================
// ocr/engine.rs — Pipeline completo de OCR para o HUD do LoL
//
// Pipeline por região:
//   1. capture_region()  → RgbaImage (xcap / GDI)
//   2. prepare_for_ocr() → DynamicImage (escala 2x + threshold)
//   3. recognize()       → String (Windows OCR API)
//
// Windows OCR (Windows.Media.Ocr) está disponível em todo Windows 10+
// sem instalação extra.
//
// Nota de threading: WinRT IAsyncOperation não implementa Future para
// Tokio diretamente — rodamos o OCR em spawn_blocking com .get() (API
// de espera síncrona do windows-future).
// ============================================================

use anyhow::{Context, Result};
use image::DynamicImage;

use super::regions::ScreenRegion;

pub struct OcrEngine;

impl OcrEngine {
    pub fn new() -> Self {
        Self
    }

    /// Captura uma região da tela, processa e extrai texto via OCR.
    /// Retorna None se a captura falhar ou não houver texto reconhecível.
    pub async fn read_region(&self, region: ScreenRegion) -> Option<String> {
        let img = super::capture::capture_region(region)
            .map_err(|e| tracing::warn!("[OCR] captura falhou: {e}"))
            .ok()?;

        let processed = super::preprocess::prepare_for_ocr(img);

        self.recognize(processed)
            .await
            .map_err(|e| tracing::warn!("[OCR] reconhecimento falhou: {e}"))
            .ok()
            .flatten()
    }

    // ── Backend Windows OCR ───────────────────────────────────

    #[cfg(windows)]
    async fn recognize(&self, img: DynamicImage) -> Result<Option<String>> {
        use std::io::Cursor;

        // Serializa a imagem como BMP antes de mover para a thread
        let mut bmp_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bmp_bytes), image::ImageFormat::Bmp)
            .context("Falha ao codificar imagem BMP")?;

        // WinRT IAsyncOperation não implementa Future para Tokio —
        // rodamos no pool de threads bloqueante com .get() síncrono
        let result = tokio::task::spawn_blocking(move || {
            win_ocr_blocking(bmp_bytes)
        })
        .await
        .context("Thread de OCR gerou panic")?;

        result
    }

    #[cfg(not(windows))]
    async fn recognize(&self, _img: DynamicImage) -> Result<Option<String>> {
        Ok(None) // OCR de texto só disponível no Windows
    }
}

// ── Implementação bloqueante do Windows OCR ───────────────────

/// Executa o Windows OCR de forma síncrona.
/// Deve ser chamada a partir de spawn_blocking — nunca de async direto.
#[cfg(windows)]
fn win_ocr_blocking(bmp_bytes: Vec<u8>) -> Result<Option<String>> {
    use windows::{
        Graphics::Imaging::BitmapDecoder,
        Media::Ocr::OcrEngine as WinOcr,
        Storage::Streams::{DataWriter, InMemoryRandomAccessStream},
    };

    // Stream de memória + escrita dos bytes BMP
    let stream = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&stream)?;
    writer.WriteBytes(&bmp_bytes)?;

    // .get() bloqueia até completar (API síncrona do windows-future)
    writer.StoreAsync()?.get()?;
    writer.DetachStream()?;

    // Rebobina para leitura pelo decoder
    stream.Seek(0)?;

    let decoder = BitmapDecoder::CreateAsync(&stream)?.get()?;
    let bitmap  = decoder.GetSoftwareBitmapAsync()?.get()?;

    // Inicializa o engine com os idiomas do perfil do usuário
    let engine = WinOcr::TryCreateFromUserProfileLanguages()
        .context("Windows OCR indisponível — verifique os pacotes de idioma instalados")?;

    let result = engine.RecognizeAsync(&bitmap)?.get()?;
    let text   = result.Text()?.to_string();

    if text.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}
