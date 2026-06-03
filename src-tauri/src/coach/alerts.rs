// ============================================================
// coach/alerts.rs — Definições de alertas por categoria e role
//
// Cada alerta tem: condição, mensagem, severidade e cooldown.
// Fase 6: alertas de timing (dragão, herald, summoner spells)
// Fase 9: alertas personalizados baseados no perfil do jogador
// ============================================================

/// Definição de um alerta de coaching bilíngue.
#[derive(Debug, Clone)]
pub struct CoachAlertDef {
    pub category:      &'static str,   // "VISION" | "MACRO" | etc.
    pub severity:      &'static str,   // "INFO" | "WARNING" | "CRITICAL"
    pub message_pt:    &'static str,
    pub message_en:    &'static str,
    pub cooldown_secs: u64,
}

impl CoachAlertDef {
    /// Retorna a mensagem no idioma solicitado ("en-US" → EN, qualquer outro → PT-BR).
    pub fn message(&self, lang: &str) -> &'static str {
        if lang == "en-US" { self.message_en } else { self.message_pt }
    }
}

/// Alertas de timing de objetivos (independentes de role).
pub const OBJECTIVE_ALERTS: &[CoachAlertDef] = &[
    CoachAlertDef {
        category: "OBJECTIVE", severity: "INFO",
        message_pt: "Primeiro dragão nasce em dois minutos",
        message_en: "First dragon spawns in two minutes",
        cooldown_secs: 300,
    },
    CoachAlertDef {
        category: "OBJECTIVE", severity: "WARNING",
        message_pt: "Dragão disponível agora",
        message_en: "Dragon is up now",
        cooldown_secs: 300,
    },
    CoachAlertDef {
        category: "OBJECTIVE", severity: "WARNING",
        message_pt: "Herald disponível, priorize bot lane",
        message_en: "Herald is up, prioritize bot lane",
        cooldown_secs: 300,
    },
];

/// Alertas de visão — disparados quando vision score cai.
pub const VISION_ALERTS: &[CoachAlertDef] = &[
    CoachAlertDef {
        category: "VISION", severity: "WARNING",
        message_pt: "Bot avançada sem visão de rio",
        message_en: "Bot lane overextended without river vision",
        cooldown_secs: 60,
    },
    CoachAlertDef {
        category: "VISION", severity: "CRITICAL",
        message_pt: "Midlaner desapareceu, cuidado com roam",
        message_en: "Mid laner disappeared, watch for roam",
        cooldown_secs: 45,
    },
];
