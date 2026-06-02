// ============================================================
// coach/alerts.rs — Definições de alertas por categoria e role
//
// Cada alerta tem: condição, mensagem, severidade e cooldown.
// Fase 6: alertas de timing (dragão, herald, summoner spells)
// Fase 9: alertas personalizados baseados no perfil do jogador
// ============================================================

/// Definição de um alerta de coaching.
#[derive(Debug, Clone)]
pub struct CoachAlertDef {
    pub category:      &'static str,   // "VISION" | "MACRO" | etc.
    pub severity:      &'static str,   // "INFO" | "WARNING" | "CRITICAL"
    pub message:       &'static str,
    pub cooldown_secs: u64,
}

/// Alertas de timing de objetivos (independentes de role).
/// Fase 6: disparados com base no tempo de jogo.
pub const OBJECTIVE_ALERTS: &[CoachAlertDef] = &[
    CoachAlertDef {
        category: "OBJECTIVE", severity: "INFO",
        message:  "Primeiro dragão nasce em dois minutos",
        cooldown_secs: 300,
    },
    CoachAlertDef {
        category: "OBJECTIVE", severity: "WARNING",
        message:  "Dragão disponível agora",
        cooldown_secs: 300,
    },
    CoachAlertDef {
        category: "OBJECTIVE", severity: "WARNING",
        message:  "Herald disponível, priorize bot lane",
        cooldown_secs: 300,
    },
];

/// Alertas de visão — disparados quando vision score cai.
pub const VISION_ALERTS: &[CoachAlertDef] = &[
    CoachAlertDef {
        category: "VISION", severity: "WARNING",
        message:  "Bot avançada sem visão de rio",
        cooldown_secs: 60,
    },
    CoachAlertDef {
        category: "VISION", severity: "CRITICAL",
        message:  "Midlaner desapareceu, cuidado com roam",
        cooldown_secs: 45,
    },
];
