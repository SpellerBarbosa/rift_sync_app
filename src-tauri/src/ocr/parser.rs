// ============================================================
// ocr/parser.rs — Parseia texto bruto extraído pelo OCR
//
// Converte strings brutas do Windows OCR em dados estruturados.
// Usa regex simples — o texto do HUD do LoL é previsível.
// ============================================================

/// Dados estruturados extraídos da tela via OCR.
#[derive(Debug, Default, Clone)]
pub struct GameScreenData {
    pub game_time_secs: Option<u32>,
    pub ally_kills:     Option<u8>,
    pub enemy_kills:    Option<u8>,
}

/// Parseia tempo de jogo no formato "MM:SS" → segundos.
///
/// O Windows OCR pode retornar caracteres extras em volta do timer.
/// A função extrai o primeiro match do padrão "D+:D+" no texto.
pub fn parse_game_time(raw: &str) -> Option<u32> {
    // Limpa espaços e busca padrão MM:SS
    for token in raw.split_whitespace() {
        let parts: Vec<&str> = token.split(':').collect();
        if parts.len() == 2 {
            if let (Ok(m), Ok(s)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                if s < 60 {
                    return Some(m * 60 + s);
                }
            }
        }
    }
    None
}

/// Parseia o placar de kills do painel central do topo.
/// Formato esperado: "12 - 7" ou "12-7" (aliados - inimigos).
pub fn parse_kill_score(raw: &str) -> Option<(u8, u8)> {
    // Remove tudo exceto dígitos e traços
    let normalized = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect::<String>();

    let parts: Vec<&str> = normalized.splitn(2, '-').collect();
    if parts.len() == 2 {
        let ally: u8  = parts[0].parse().ok()?;
        let enemy: u8 = parts[1].parse().ok()?;
        return Some((ally, enemy));
    }
    None
}

/// Extrai GameScreenData completo do texto bruto do OCR.
pub fn parse_screen_data(timer_text: &str, score_text: &str) -> GameScreenData {
    GameScreenData {
        game_time_secs: parse_game_time(timer_text),
        ally_kills:     parse_kill_score(score_text).map(|(a, _)| a),
        enemy_kills:    parse_kill_score(score_text).map(|(_, e)| e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_game_time_valido() {
        assert_eq!(parse_game_time("14:32"), Some(872));
        assert_eq!(parse_game_time("0:45"),  Some(45));
        assert_eq!(parse_game_time("  23:01  "), Some(1381));
    }

    #[test]
    fn parse_game_time_com_ruido_ocr() {
        // OCR às vezes adiciona caracteres extras ao redor
        assert_eq!(parse_game_time("| 14:32 |"), Some(872));
        assert_eq!(parse_game_time("Time: 5:00"), Some(300));
    }

    #[test]
    fn parse_game_time_invalido() {
        assert_eq!(parse_game_time("abc"),    None);
        assert_eq!(parse_game_time("14:99"),  None); // segundos inválidos
        assert_eq!(parse_game_time(""),       None);
    }

    #[test]
    fn parse_kill_score_valido() {
        assert_eq!(parse_kill_score("12 - 7"),  Some((12, 7)));
        assert_eq!(parse_kill_score("12-7"),    Some((12, 7)));
        assert_eq!(parse_kill_score("0 - 0"),   Some((0, 0)));
    }

    #[test]
    fn parse_kill_score_invalido() {
        assert_eq!(parse_kill_score("abc"), None);
        assert_eq!(parse_kill_score(""),    None);
    }
}
