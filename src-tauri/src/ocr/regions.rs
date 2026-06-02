// ============================================================
// ocr/regions.rs — Regiões de captura de tela do LoL
//
// Coordenadas calibradas para 1920x1080 (HUD padrão, escala 100%).
// Usa scale_to() para adaptar a outras resoluções automaticamente.
// ============================================================

/// Região retangular da tela para captura OCR.
#[derive(Debug, Clone, Copy)]
pub struct ScreenRegion {
    pub x:      u32,
    pub y:      u32,
    pub width:  u32,
    pub height: u32,
}

impl ScreenRegion {
    /// Escala a região de 1080p para a resolução do monitor atual.
    /// Ex: se o monitor é 2560x1440, multiplica por 1.33.
    pub fn scale_to(self, screen_w: u32, screen_h: u32) -> Self {
        let sx = screen_w as f32 / 1920.0;
        let sy = screen_h as f32 / 1080.0;
        Self {
            x:      (self.x as f32 * sx) as u32,
            y:      (self.y as f32 * sy) as u32,
            width:  ((self.width  as f32 * sx) as u32).max(1),
            height: ((self.height as f32 * sy) as u32).max(1),
        }
    }
}

/// Regiões do HUD do LoL em 1920x1080 com HUD padrão (escala 100%).
///
/// Calibradas na versão 14.x do cliente. Se a UI mudar em patches futuros,
/// ajuste as constantes abaixo — o resto do código não muda.
pub mod regions_1080p {
    use super::ScreenRegion;

    /// Timer de jogo — centro do topo (ex: "14:32")
    pub const GAME_TIMER: ScreenRegion = ScreenRegion {
        x: 858, y: 8, width: 165, height: 28,
    };

    /// Placar de kills — painel central do topo (ex: "12 - 7")
    pub const KILL_SCORE: ScreenRegion = ScreenRegion {
        x: 740, y: 8, width: 440, height: 28,
    };

    /// Summoner spell D do jogador local (ex: Flash)
    /// Localizado no HUD inferior esquerdo, acima do spell F
    pub const SPELL_D: ScreenRegion = ScreenRegion {
        x: 626, y: 944, width: 46, height: 46,
    };

    /// Summoner spell F do jogador local (ex: Ignite)
    pub const SPELL_F: ScreenRegion = ScreenRegion {
        x: 626, y: 994, width: 46, height: 46,
    };

    /// Timer de objetivo (baron/dragon) quando exibido na tela
    pub const OBJECTIVE_TIMER: ScreenRegion = ScreenRegion {
        x: 820, y: 40, width: 280, height: 30,
    };

    /// Minimapa completo — para futura análise de posicionamento
    pub const MINIMAP: ScreenRegion = ScreenRegion {
        x: 1630, y: 820, width: 290, height: 260,
    };

    // ── Summoner spells dos inimigos (painel superior direito) ──
    // Calibrado para 1080p, HUD padrão, jogador no time azul (inimigos à direita).
    // Cada slot de inimigo tem ~88px de largura; os spells ficam nos últimos ~34px.
    // TODO: ajustar coordenadas com print real se os alertas gerarem falsos positivos.

    /// Spell D dos 5 inimigos (índice 0 = mais à esquerda do grupo inimigo)
    pub const ENEMY_SPELL_D: [ScreenRegion; 5] = [
        ScreenRegion { x: 1540, y: 10, width: 16, height: 16 },
        ScreenRegion { x: 1628, y: 10, width: 16, height: 16 },
        ScreenRegion { x: 1716, y: 10, width: 16, height: 16 },
        ScreenRegion { x: 1804, y: 10, width: 16, height: 16 },
        ScreenRegion { x: 1892, y: 10, width: 16, height: 16 },
    ];

    /// Spell F dos 5 inimigos
    pub const ENEMY_SPELL_F: [ScreenRegion; 5] = [
        ScreenRegion { x: 1558, y: 10, width: 16, height: 16 },
        ScreenRegion { x: 1646, y: 10, width: 16, height: 16 },
        ScreenRegion { x: 1734, y: 10, width: 16, height: 16 },
        ScreenRegion { x: 1822, y: 10, width: 16, height: 16 },
        ScreenRegion { x: 1910, y: 10, width: 16, height: 16 },
    ];
}
