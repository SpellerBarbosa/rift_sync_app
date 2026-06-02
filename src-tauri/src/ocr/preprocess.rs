// ============================================================
// ocr/preprocess.rs — Pré-processamento de imagem para OCR
//
// Pipeline: RgbaImage → escala 2x → grayscale → threshold
//
// A escala 2x é crítica para texto pequeno (< 20px) do HUD do LoL.
// O threshold adaptativo lida com variações de brilho na tela.
// ============================================================

use image::{DynamicImage, GrayImage, RgbaImage, imageops};

/// Prepara uma região capturada para leitura pelo OCR.
/// Retorna a imagem processada como DynamicImage (compatível com encoder BMP).
pub fn prepare_for_ocr(img: RgbaImage) -> DynamicImage {
    let dynamic = DynamicImage::ImageRgba8(img);

    // Escala 2x — melhora precisão em texto de HUD pequeno
    let scaled = dynamic.resize_exact(
        dynamic.width() * 2,
        dynamic.height() * 2,
        imageops::FilterType::Lanczos3,
    );

    // Grayscale
    let gray = scaled.to_luma8();

    // Threshold adaptativo
    DynamicImage::ImageLuma8(adaptive_threshold(&gray))
}

/// Threshold adaptativo: usa a média de brilho como ponto de corte.
/// Pixels acima da média → branco (texto), abaixo → preto (fundo).
/// Mais robusto que threshold fixo para diferentes condições de tela.
fn adaptive_threshold(img: &GrayImage) -> GrayImage {
    let sum: u64 = img.pixels().map(|p| p[0] as u64).sum();
    let pixel_count = (img.width() * img.height()) as u64;
    let mean = if pixel_count > 0 { (sum / pixel_count) as u8 } else { 128 };

    let mut out = img.clone();
    for pixel in out.pixels_mut() {
        pixel[0] = if pixel[0] > mean { 255 } else { 0 };
    }
    out
}

/// Inverte o threshold — útil para texto claro em fundo escuro (HUD noturno do LoL).
pub fn invert(img: GrayImage) -> GrayImage {
    let mut out = img;
    for pixel in out.pixels_mut() {
        pixel[0] = 255 - pixel[0];
    }
    out
}
