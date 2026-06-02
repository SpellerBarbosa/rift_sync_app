// ============================================================
// ocr/template.rs — Template matching de ícones de campeão no minimapa
//
// Algoritmo: SAD (Sum of Absolute Differences) em escala de cinza.
// Simples, rápido (~5ms por template em 290×260px) e sem deps extras.
//
// Fluxo:
//   1. Coach baixa ícones dos campeões inimigos no início da partida
//   2. prepare_template() → redimensiona para ICON_SIZE e converte p/ cinza
//   3. find_best_match() → varre o minimapa e retorna posição com menor SAD
//   4. Score abaixo de MATCH_THRESHOLD indica detecção confiável
// ============================================================

use image::{DynamicImage, GrayImage, RgbaImage};

/// Tamanho alvo dos templates em pixels (diâmetro do ícone no minimapa).
/// Ajuste se o minimapa do usuário tiver escala diferente.
pub const ICON_SIZE: u32 = 16;

/// Score SAD normalizado (0–255) abaixo do qual consideramos match.
/// Menor = mais rigoroso. Começa em 50 — ajustar com testes reais.
pub const MATCH_THRESHOLD: u8 = 50;

/// Resultado de uma busca de template.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Posição do centro do match em percentual do minimapa (0–100).
    pub x_pct: f32,
    pub y_pct: f32,
    /// Score SAD normalizado — menor = mais parecido.
    pub score: u8,
}

/// Converte ícone RGBA (qualquer tamanho) em template de trabalho:
/// grayscale, redimensionado para ICON_SIZE×ICON_SIZE.
pub fn prepare_template(rgba: RgbaImage) -> GrayImage {
    DynamicImage::ImageRgba8(rgba)
        .resize_exact(ICON_SIZE, ICON_SIZE, image::imageops::FilterType::Lanczos3)
        .to_luma8()
}

/// Decodifica bytes PNG/JPEG em GrayImage template pronta para matching.
pub fn decode_and_prepare(bytes: &[u8]) -> Option<GrayImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    Some(prepare_template(rgba))
}

/// Procura o melhor match de `template` dentro de `minimap`.
/// Retorna None se o minimapa for menor que o template.
pub fn find_best_match(minimap: &GrayImage, template: &GrayImage) -> Option<MatchResult> {
    let (mw, mh) = minimap.dimensions();
    let (tw, th) = template.dimensions();

    if mw < tw || mh < th {
        return None;
    }

    let pixels = tw * th;
    let mut best_score = u32::MAX;
    let mut best_x = 0u32;
    let mut best_y = 0u32;

    for y in 0..=(mh - th) {
        for x in 0..=(mw - tw) {
            let score = sad(minimap, template, x, y);
            if score < best_score {
                best_score = score;
                best_x = x;
                best_y = y;
            }
        }
    }

    let normalized = ((best_score / pixels) as u8).min(255);

    Some(MatchResult {
        x_pct: (best_x + tw / 2) as f32 / mw as f32 * 100.0,
        y_pct: (best_y + th / 2) as f32 / mh as f32 * 100.0,
        score: normalized,
    })
}

// ── Helpers internos ──────────────────────────────────────────

fn sad(img: &GrayImage, template: &GrayImage, ox: u32, oy: u32) -> u32 {
    let (tw, th) = template.dimensions();
    let mut sum = 0u32;

    for y in 0..th {
        for x in 0..tw {
            let iv = img.get_pixel(ox + x, oy + y)[0] as i32;
            let tv = template.get_pixel(x, y)[0] as i32;
            sum += (iv - tv).unsigned_abs();
        }
    }

    sum
}
