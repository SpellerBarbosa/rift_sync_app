// ============================================================
// ocr/minimap.rs — Análise do minimapa para detecção de aliados
//
// Estratégia: detecção de cor dos pontos de aliados (azul brilhante)
// em zonas específicas do minimapa correspondentes à bot lane.
//
// O minimapa exibe dots de aliados em azul saturado (~RGB 50,140,255).
// Dots de minions são menores e mais fracos — filtrados por tamanho de cluster.
//
// Zonas bot lane (dentro da imagem do minimapa):
//   Time azul (ORDER): canto inferior-direito  →  x 55–100%, y 58–100%
//   Time vermelho (CHAOS): canto superior-esquerdo → x 0–45%,  y 0–42%
// ============================================================

use image::RgbaImage;

/// Conta aliados (pontos azuis de jogador) na zona bot lane do minimapa.
///
/// Retorna o número de dots de jogador distintos detectados na zona.
/// Dois ou mais indica que ADC + suporte estão na lane juntos.
pub fn count_allies_in_bot(minimap: &RgbaImage, is_blue_side: bool) -> u8 {
    let w = minimap.width();
    let h = minimap.height();

    let (x0, y0, x1, y1) = if is_blue_side {
        (w * 55 / 100, h * 58 / 100, w, h)
    } else {
        (0, 0, w * 45 / 100, h * 42 / 100)
    };

    let candidates: Vec<(u32, u32)> = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| is_ally_dot(minimap.get_pixel(x, y).0))
        .collect();

    // Agrupa pixels próximos em clusters; filtra clusters pequenos (ruído / minions)
    let clusters = build_clusters(&candidates, 10);
    let player_dots = clusters.iter().filter(|c| c.len() >= 4).count();

    player_dots.min(5) as u8
}

// ── Detecção de inimigos por zona ────────────────────────────

/// Leitura de presença inimiga nas duas metades do mapa.
///
/// As zonas são fixas (o mapa de SR tem sempre a mesma orientação no minimapa):
///   top_zone  — área do jungle/lane do topo (canto superior-esquerdo)
///   bot_zone  — área do jungle/lane do bot  (canto inferior-direito)
#[derive(Debug, Clone, Default)]
pub struct EnemyZones {
    pub top_zone: u8,
    pub bot_zone: u8,
}

/// Conta dots de inimigo (vermelho brilhante) nas zonas top e bot do minimapa.
///
/// Não depende de `is_blue_side` — a orientação do mapa é sempre a mesma
/// no minimapa do LoL independente do time.
pub fn detect_enemy_zones(minimap: &RgbaImage) -> EnemyZones {
    let w = minimap.width();
    let h = minimap.height();

    // Zona topo: jungle/lane superior-esquerda (≈ x 7–48%, y 8–50%)
    let top = count_enemy_clusters(minimap, 0, w * 7 / 100, w * 48 / 100, h * 8 / 100, h * 50 / 100);

    // Zona bot: jungle/lane inferior-direita (≈ x 52–93%, y 50–92%)
    let bot = count_enemy_clusters(minimap, 0, w * 52 / 100, w * 93 / 100, h * 50 / 100, h * 92 / 100);

    EnemyZones { top_zone: top, bot_zone: bot }
}

/// Retorna a posição (x%, y%) do centróide de cada dot inimigo detectado
/// no minimapa inteiro. Usado para rastrear aparições individuais de inimigos
/// (ward reveals) e emitir alertas quando um novo inimigo é avistado.
pub fn detect_enemy_positions(minimap: &RgbaImage) -> Vec<(f32, f32)> {
    let w = minimap.width() as f32;
    let h = minimap.height() as f32;

    let points: Vec<(u32, u32)> = (0..minimap.height())
        .flat_map(|y| (0..minimap.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| is_enemy_dot(minimap.get_pixel(x, y).0))
        .collect();

    // Mínimo de 5 pixels por cluster — filtra ruído pontual e elementos
    // do mapa que têm apenas 1-2 pixels vermelhos (ícones, bordas, etc.)
    build_clusters(&points, 10)
        .iter()
        .filter(|c| c.len() >= 5)
        .map(|c| {
            let cx = c.iter().map(|&(x, _)| x as f32).sum::<f32>() / c.len() as f32;
            let cy = c.iter().map(|&(_, y)| y as f32).sum::<f32>() / c.len() as f32;
            (cx / w * 100.0, cy / h * 100.0)
        })
        .collect()
}

fn count_enemy_clusters(
    minimap: &RgbaImage,
    _pad: u32,
    x0: u32, x1: u32,
    y0: u32, y1: u32,
) -> u8 {
    let points: Vec<(u32, u32)> = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| is_enemy_dot(minimap.get_pixel(x, y).0))
        .collect();

    let clusters = build_clusters(&points, 10);
    clusters.iter().filter(|c| c.len() >= 3).count().min(5) as u8
}

/// Verifica se um pixel tem a cor característica de dot de inimigo (vermelho vivo).
fn is_enemy_dot(rgba: [u8; 4]) -> bool {
    let [r, g, b, a] = rgba;
    if a < 200 { return false; }
    let r = r as u16;
    let g = g as u16;
    let b = b as u16;
    r >= 180 && r > g + 100 && r > b + 100 && g < 120 && b < 120
}

// ── Detecção de cor (aliados) ─────────────────────────────────

/// Verifica se um pixel tem a cor característica de dot de aliado no minimapa.
///
/// Dot de aliado: azul vivo, canal B dominante e alto.
/// Dots de minions e fundo do mapa têm saturação/brilho menores.
fn is_ally_dot(rgba: [u8; 4]) -> bool {
    let [r, g, b, a] = rgba;
    if a < 200 {
        return false; // pixel transparente
    }
    let b = b as u16;
    let g = g as u16;
    let r = r as u16;

    // Azul brilhante e dominante, sem vermelho significativo
    b >= 170
        && b > g + 50
        && b > r + 90
        && r < 130
        && g < 190
}

// ── Clusterização ────────────────────────────────────────────

/// Agrupa pixels em clusters por proximidade (união simples).
/// Pixels dentro de `radius` pixels de qualquer membro do cluster são adicionados.
fn build_clusters(points: &[(u32, u32)], radius: u32) -> Vec<Vec<(u32, u32)>> {
    let mut clusters: Vec<Vec<(u32, u32)>> = Vec::new();

    'outer: for &(px, py) in points {
        for cluster in clusters.iter_mut() {
            if cluster.iter().any(|&(cx, cy)| {
                px.abs_diff(cx) <= radius && py.abs_diff(cy) <= radius
            }) {
                cluster.push((px, py));
                continue 'outer;
            }
        }
        clusters.push(vec![(px, py)]);
    }

    clusters
}
