// ============================================================
// ocr/spell_cd.rs — Detecção de cooldown de summoner spells
//
// Estratégia: análise de saturação de cor.
//   - Spell disponível: ícone colorido (saturação alta)
//   - Spell em cooldown: ícone acinzentado (saturação baixa)
//
// Não usa OCR — mais rápido e confiável para detecção binária.
// ============================================================

use image::RgbaImage;

/// Estado de disponibilidade de um summoner spell.
#[derive(Debug, Clone, PartialEq)]
pub enum SpellState {
    Available,
    OnCooldown,
    Unknown,  // região inválida ou fora da tela
}

impl SpellState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpellState::Available   => "disponível",
            SpellState::OnCooldown  => "em cooldown",
            SpellState::Unknown     => "desconhecido",
        }
    }
}

/// Detecta o estado de um summoner spell pela análise de saturação média.
///
/// Calibração: saturação < 25 → cooldown (cinza), ≥ 25 → disponível (colorido).
/// Esse limiar funciona bem para os ícones do LoL em 1080p.
pub fn detect_spell_state(icon: &RgbaImage) -> SpellState {
    if icon.width() == 0 || icon.height() == 0 {
        return SpellState::Unknown;
    }

    let total_sat: u64 = icon
        .pixels()
        .map(|p| {
            let [r, g, b, _] = p.0;
            let max = r.max(g).max(b) as u64;
            let min = r.min(g).min(b) as u64;
            // Saturação HSV normalizada [0, 255]
            if max == 0 { 0 } else { (max - min) * 255 / max }
        })
        .sum();

    let pixel_count = (icon.width() * icon.height()) as u64;
    let avg_sat = total_sat / pixel_count;

    if avg_sat < 25 {
        SpellState::OnCooldown
    } else {
        SpellState::Available
    }
}

/// Converte detecção de spell do JOGADOR em evento para o CoachEngine.
/// Só gera evento se o estado mudou em relação ao anterior.
/// Ignora transições a partir de Unknown — evita falsos positivos no
/// primeiro ciclo OCR quando nenhuma linha de base existe ainda.
pub fn spell_change_event(
    spell_name: &str,
    current:    &SpellState,
    previous:   &SpellState,
) -> Option<String> {
    if current == previous || *previous == SpellState::Unknown {
        return None;
    }
    match current {
        SpellState::Available  => Some(format!("{spell_name} aliado disponível")),
        SpellState::OnCooldown => Some(format!("{spell_name} aliado em cooldown")),
        SpellState::Unknown    => None,
    }
}

/// Converte detecção de spell INIMIGA em evento para o CoachEngine.
/// Formato: `"inimigo_{slot}_{key} em cooldown"` / `"inimigo_{slot}_{key} disponível"`
/// onde slot ∈ [1..5] e key ∈ ["D", "F"].
/// Ignora transições a partir de Unknown — evita falsos positivos no
/// primeiro ciclo OCR quando nenhuma linha de base existe ainda.
pub fn enemy_spell_event(
    slot:    usize,       // 1-based
    key:     &str,        // "D" ou "F"
    current: &SpellState,
    previous:&SpellState,
) -> Option<String> {
    if current == previous || *previous == SpellState::Unknown {
        return None;
    }
    match current {
        SpellState::Available  => Some(format!("inimigo_{slot}_{key} disponível")),
        SpellState::OnCooldown => Some(format!("inimigo_{slot}_{key} em cooldown")),
        SpellState::Unknown    => None,
    }
}
