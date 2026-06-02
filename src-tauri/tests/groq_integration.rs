// ============================================================
// tests/groq_integration.rs — Teste de integração com a API Groq
//
// Executa chamadas reais à API com contexto enriquecido.
// Rodar com: cargo test --test groq_integration -- --nocapture
// ============================================================

use riftsync_ai_lib::groq::client::{DragonCount, GameContext, GroqClient, PatternSummary};
use riftsync_ai_lib::db::models::PlayerPattern;

fn load_client() -> GroqClient {
    dotenvy::dotenv().ok();
    let key = std::env::var("GROQ_API_KEY")
        .expect("GROQ_API_KEY não definida — adicione ao .env antes de rodar os testes");
    GroqClient::new(key).expect("Falha ao criar GroqClient")
}

fn assert_alert_schema(alerts: &[riftsync_ai_lib::groq::client::GroqAlert]) {
    let valid_categories = ["OBJECTIVE", "VISION", "MACRO", "TRADE", "POSITIONING"];
    let valid_severities  = ["INFO", "WARNING", "CRITICAL"];

    assert!(alerts.len() <= 2, "Máximo 2 alertas — recebeu {}", alerts.len());

    for a in alerts {
        assert!(
            valid_categories.contains(&a.category.as_str()),
            "Categoria inválida: '{}' — alucinação de schema", a.category
        );
        assert!(
            valid_severities.contains(&a.severity.as_str()),
            "Severidade inválida: '{}'", a.severity
        );
        assert!(!a.message.is_empty(), "message vazio");
        assert!(!a.reason.is_empty(),  "reason vazio — modelo deve justificar o alerta");
        assert!(
            a.message.split_whitespace().count() <= 15,
            "Mensagem muito longa ({} palavras): '{}'",
            a.message.split_whitespace().count(), a.message
        );
    }
}

// ── Cenário 1: Jungle agressivo, atrás no jogo, dragão iminente ─

#[tokio::test]
async fn test_jungle_behind_dragon_imminent() {
    let client = load_client();

    let ctx = GameContext {
        game_time_secs: 1320,  // 22 minutos
        player_role:    "JUNGLE".to_string(),
        champion_name:  Some("Vi".to_string()),
        game_phase:     "INGAME".to_string(),
        is_ahead:       false,
        ally_score:     4,
        enemy_score:    11,
        dragon_count:   DragonCount { ally: 1, enemy: 2 },
        recent_events:  vec![
            "Dragão em 30 segundos".to_string(),
            "Inimigo jungler visto bot".to_string(),
            "Sem visão no rio sul".to_string(),
        ],
        player_patterns: Some(PatternSummary {
            aggression:       0.8,
            ward_score:       0.2,
            objective_ctrl:   0.3,
            avg_deaths_10_15: 2.5,
        }),
    };

    let alerts = client.get_coaching_alerts(&ctx).await
        .expect("Falha na chamada ao Groq");

    println!("\n=== Jungle atrás, dragão em 30s (22min) ===");
    for a in &alerts {
        println!("  [{}/{}] {} | motivo: {}", a.severity, a.category, a.message, a.reason);
    }

    assert_alert_schema(&alerts);

    // Situação urgente — esperamos pelo menos 1 alerta
    assert!(!alerts.is_empty(), "Groq deveria alertar: dragão em 30s + atrás 4-11");

    // Com dragão iminente e atrás, deve mencionar OBJECTIVE ou VISION (ward pré-drake)
    let relevant = alerts.iter().any(|a| matches!(a.category.as_str(), "OBJECTIVE" | "VISION"));
    assert!(relevant, "Esperava OBJECTIVE ou VISION nessa situação");
}

// ── Cenário 2: Mid early game tranquilo sem urgência ──────────

#[tokio::test]
async fn test_mid_early_no_urgency() {
    let client = load_client();

    let ctx = GameContext {
        game_time_secs: 180,  // 3 minutos
        player_role:    "MID".to_string(),
        champion_name:  Some("Orianna".to_string()),
        game_phase:     "INGAME".to_string(),
        is_ahead:       true,
        ally_score:     1,
        enemy_score:    0,
        dragon_count:   DragonCount { ally: 0, enemy: 0 },
        recent_events:  vec!["scuttlecrab ativa".to_string()],
        player_patterns: Some(PatternSummary {
            aggression:       0.5,
            ward_score:       0.6,
            objective_ctrl:   0.5,
            avg_deaths_10_15: 0.8,
        }),
    };

    let alerts = client.get_coaching_alerts(&ctx).await
        .expect("Falha na chamada ao Groq");

    println!("\n=== Mid early tranquilo, 3min, na frente ===");
    if alerts.is_empty() {
        println!("  (sem alertas — comportamento correto para early sem urgência)");
    }
    for a in &alerts {
        println!("  [{}/{}] {} | motivo: {}", a.severity, a.category, a.message, a.reason);
    }

    assert_alert_schema(&alerts);
    // Sem CRITICAL em early tranquilo
    let has_critical = alerts.iter().any(|a| a.severity == "CRITICAL");
    assert!(!has_critical, "CRITICAL num early tranquilo é over-alerting");
}

// ── Cenário 3: ADC na frente, Baron disponível agora ──────────

#[tokio::test]
async fn test_adc_ahead_baron_up() {
    let client = load_client();

    let ctx = GameContext {
        game_time_secs: 1260,  // 21 minutos
        player_role:    "ADC".to_string(),
        champion_name:  Some("Jinx".to_string()),
        game_phase:     "INGAME".to_string(),
        is_ahead:       true,
        ally_score:     18,
        enemy_score:    7,
        dragon_count:   DragonCount { ally: 3, enemy: 0 },
        recent_events:  vec![
            "Baron disponível agora".to_string(),
            "Inimigo mid e top mortos".to_string(),
            "Time aliado com visão no poço".to_string(),
        ],
        player_patterns: Some(PatternSummary {
            aggression:       0.6,
            ward_score:       0.7,
            objective_ctrl:   0.8,
            avg_deaths_10_15: 0.5,
        }),
    };

    let alerts = client.get_coaching_alerts(&ctx).await
        .expect("Falha na chamada ao Groq");

    println!("\n=== ADC na frente 18-7, Baron up, 2 inimigos mortos ===");
    for a in &alerts {
        println!("  [{}/{}] {} | motivo: {}", a.severity, a.category, a.message, a.reason);
    }

    assert_alert_schema(&alerts);

    // Baron disponível + 2 inimigos mortos = deve ter alerta OBJECTIVE
    assert!(!alerts.is_empty(), "Baron disponível com 2 mortos deve gerar alerta");
    let has_objective = alerts.iter().any(|a| a.category == "OBJECTIVE");
    assert!(has_objective, "Esperava OBJECTIVE para Baron com inimigos mortos");
}

// ── Cenário 4: Top splitpush, time atrás, visão zerada ───────

#[tokio::test]
async fn test_top_splitpush_behind_no_vision() {
    let client = load_client();

    let ctx = GameContext {
        game_time_secs: 1500,  // 25 minutos
        player_role:    "TOP".to_string(),
        champion_name:  Some("Camille".to_string()),
        game_phase:     "INGAME".to_string(),
        is_ahead:       false,
        ally_score:     6,
        enemy_score:    14,
        dragon_count:   DragonCount { ally: 0, enemy: 2 },
        recent_events:  vec![
            "Top isolado no mapa".to_string(),
            "Sem visão no rio norte".to_string(),
            "Inimigo jungler posição desconhecida".to_string(),
            "Baron disponível em 1 minuto".to_string(),
        ],
        player_patterns: Some(PatternSummary {
            aggression:       0.7,
            ward_score:       0.15,
            objective_ctrl:   0.2,
            avg_deaths_10_15: 3.2,
        }),
    };

    let alerts = client.get_coaching_alerts(&ctx).await
        .expect("Falha na chamada ao Groq");

    println!("\n=== Top splitpush atrás 6-14, sem visão, Baron em 1min ===");
    for a in &alerts {
        println!("  [{}/{}] {} | motivo: {}", a.severity, a.category, a.message, a.reason);
    }

    assert_alert_schema(&alerts);
    // Sem visão + jungler desconhecido = risco — deve alertar
    assert!(!alerts.is_empty(), "Situação de risco com jungler desconhecido deve gerar alerta");
}

// ── Cenário 5: Perfil comportamental completo ─────────────────

#[tokio::test]
async fn test_player_profile_full_context() {
    let client = load_client();

    let pattern = PlayerPattern {
        id:                    1,
        player_id:             42,
        aggression_score:      0.85,
        deaths_without_vision: 18,
        avg_deaths_10_15:      2.8,
        lane_dominance:        0.70,
        objective_control:     0.35,
        roam_frequency:        0.60,
        tp_efficiency:         0.45,
        ward_score:            0.20,
    };

    let matches_summary = "\
        Últimas 10 partidas: 6W 4L | KDA médio: 8/6/4 | \
        Campeões: Zed, Akali, Katarina | CS médio: 180@20min | \
        Ward score médio: 12 | Mortes 10-15min: 2.8/partida | \
        18 mortes totais sem visão próxima";

    let insights = client.analyze_player_profile(&pattern, matches_summary).await
        .expect("Falha na análise de perfil");

    println!("\n=== Perfil comportamental ===");
    println!("Playstyle:      {}", insights.playstyle);
    println!("Pontos fortes:  {:?}", insights.strengths);
    println!("Fraquezas:      {:?}", insights.weaknesses);
    println!("Dica principal: {}", insights.priority_tip);
    println!("Nota evolução:  {}", insights.progress_note);

    assert!(
        ["aggressive", "passive", "balanced"].contains(&insights.playstyle.as_str()),
        "Playstyle inválido: '{}'", insights.playstyle
    );
    assert!(insights.strengths.len() >= 1 && insights.strengths.len() <= 3);
    assert!(insights.weaknesses.len() >= 1 && insights.weaknesses.len() <= 3);
    assert!(!insights.priority_tip.is_empty());
    assert!(!insights.progress_note.is_empty());
    assert_eq!(insights.playstyle, "aggressive",
        "aggression=0.85 deveria resultar em 'aggressive', recebeu '{}'", insights.playstyle);
}
