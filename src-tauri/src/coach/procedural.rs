// ============================================================
// coach/procedural.rs — Motor de regras procedurais de coaching
//
// Sistema determinístico baseado em contexto das APIs da Riot e
// banco local. Dispara alertas em < 1ms sem chamadas externas.
//
// Complementa o Groq: regras cobrem timings e padrões previsíveis;
// o Groq cobre raciocínio situacional mais profundo.
//
// 14 regras em 5 categorias:
//   OBJECTIVE  — timers de drake/baron/herald/scuttle
//   VISION     — personalizado por ward_score do banco
//   MACRO      — conversão de vantagem/desvantagem
//   TRADE      — padrão de mortes por agressividade
//   POSITIONING — isolamento sem visão
// ============================================================

use std::collections::HashMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::db::models::{CoachAlert, PlayerPattern};

// ── Macro de tradução ─────────────────────────────────────────
/// Seleciona a string PT-BR ou EN-US com base no parâmetro `lang`.
/// Uso: t!(lang, "Texto em português", "English text")
macro_rules! t {
    ($lang:expr, $pt:literal, $en:literal) => {
        if $lang == "en-US" { $en } else { $pt }
    };
}

// ── Contexto procedural ───────────────────────────────────────

/// Snapshot do estado do jogo montado a partir de APIs + banco local.
/// Construído em cada tick antes da avaliação das regras.
#[derive(Debug, Clone)]
pub struct ProcContext {
    pub game_time:         u32,
    pub player_role:       String,
    pub champion:          Option<String>,
    pub is_ahead:          bool,
    pub ally_score:        u8,
    pub enemy_score:       u8,
    pub dragon_ally:       u8,
    pub dragon_enemy:      u8,
    /// game_time real em que o próximo drake spawna (vindo de DragonKill.EventTime)
    pub next_dragon_spawn: u32,
    /// game_time real em que o próximo Baron spawna; None antes dos 20:00 sem nenhum kill
    pub next_baron_spawn:  Option<u32>,
    /// true se Herald já foi pego nesta partida
    pub herald_killed:     bool,
    pub pattern:           Option<PlayerPattern>,
    /// true quando há 2+ aliados detectados na zona bot lane do minimapa.
    /// false = suporte saiu da lane (relevante para ADC e para o próprio suporte).
    pub support_in_lane:    bool,
    /// true quando o jungle inimigo é detectado no lado oposto ao do jogador.
    /// Indica janela de agressividade ou avanço de rota com menor risco de gank.
    pub enemy_in_opp_jungle: bool,
}

impl ProcContext {
    pub fn score_diff(&self) -> i16 {
        self.ally_score as i16 - self.enemy_score as i16
    }

    pub fn total_drakes(&self) -> u8 {
        self.dragon_ally + self.dragon_enemy
    }
}

/// Contexto completo de um ChampionKill para coaching.
/// Construído no engine antes de chamar handle_champion_kill.
#[derive(Debug)]
pub struct KillCtx {
    pub killer_champ:          String,
    pub victim_champ:          String,
    /// Role atribuída à vítima no champ select (TOP/JUNGLE/MIDDLE/BOTTOM/UTILITY).
    /// Não é a localização da morte — usada como identidade e para inferência macro.
    pub victim_role:           String,
    pub is_ally_kill:          bool,
    pub game_time:             u32,
    pub player_role:           String,
    /// Segundos até o próximo dragão (0 = disponível agora)
    pub dragon_secs:           u32,
    /// Segundos até o próximo barão (None = ainda não ativado no jogo)
    pub baron_secs:            Option<u32>,
    /// Algum inimigo vivo (não a vítima) tem Teleport disponível
    pub enemy_tp_threat:       bool,
    /// 3+ mortes em janela de 10s — teamfight; coaching individual não se aplica
    pub is_teamfight:          bool,
    /// O killer foi morto nos 10s seguintes no mesmo batch — window se fechou
    pub killer_counter_killed: bool,
    /// Jogador local tem ward (trinket 3340/3363 ou Control Ward 2055) no inventário
    pub has_ward_available:    bool,
}

// ── Resultado interno de regra ────────────────────────────────

struct RuleResult {
    id:       &'static str,
    category: &'static str,
    severity: &'static str,
    cooldown: u64,
    message:  String,
}

// ── Motor procedural ──────────────────────────────────────────

pub struct ProcEngine {
    /// Rastreia quando cada regra foi emitida pela última vez.
    /// Chave: rule_id (str estático — zero alocação).
    last_alert_at: HashMap<&'static str, Instant>,
    /// Rate limit global para alertas vindos de evaluate().
    /// Garante no máximo 1 alerta a cada 45s nesse caminho.
    last_eval_emit_at: Option<Instant>,
}

impl ProcEngine {
    pub fn new() -> Self {
        Self { last_alert_at: HashMap::new(), last_eval_emit_at: None }
    }

    /// Avalia todas as regras contra o contexto atual.
    /// Retorna no máximo 1 alerta por chamada — o de maior severidade entre
    /// os prontos. Um rate limit global de 45s entre qualquer dois alertas
    /// deste caminho evita rajadas. Alertas CRITICAL ignoram o rate limit.
    pub fn evaluate(&mut self, ctx: &ProcContext, lang: &str) -> Vec<CoachAlert> {
        let mut raw: Vec<RuleResult> = Vec::new();

        evaluate_objective_rules(ctx, lang, &mut raw);
        evaluate_vision_rules(ctx, lang, &mut raw);
        evaluate_macro_rules(ctx, lang, &mut raw);
        evaluate_trade_rules(ctx, lang, &mut raw);
        evaluate_positioning_rules(ctx, lang, &mut raw);
        evaluate_side_push_rules(ctx, lang, &mut raw);
        evaluate_bot_lane_rules(ctx, lang, &mut raw);
        evaluate_opp_jungle_rules(ctx, lang, &mut raw);
        evaluate_top_lane_rules(ctx, lang, &mut raw);
        evaluate_mid_lane_rules(ctx, lang, &mut raw);
        evaluate_support_rules(ctx, lang, &mut raw);
        evaluate_adc_rules(ctx, lang, &mut raw);
        evaluate_jungle_rules(ctx, lang, &mut raw);

        // Filtra por cooldown individual de cada regra
        let ready: Vec<RuleResult> = raw
            .into_iter()
            .filter(|r| self.cooldown_ok(r.id, r.cooldown))
            .collect();

        if ready.is_empty() { return Vec::new(); }

        // Seleciona o alerta de maior prioridade entre os prontos
        fn sev_rank(s: &str) -> u8 { match s { "CRITICAL" => 2, "WARNING" => 1, _ => 0 } }
        let best = ready.into_iter().max_by_key(|r| sev_rank(r.severity)).unwrap();

        // Rate limit global: mínimo 45s entre qualquer dois alertas de evaluate()
        // Alertas CRITICAL ignoram o rate limit para não suprimir emergências
        let global_blocked = self.last_eval_emit_at
            .map(|t| t.elapsed() < Duration::from_secs(45))
            .unwrap_or(false);

        if global_blocked && best.severity != "CRITICAL" {
            return Vec::new();
        }

        self.last_alert_at.insert(best.id, Instant::now());
        self.last_eval_emit_at = Some(Instant::now());

        tracing::debug!(
            "[Proc] alerta [{}/{}] ({}): {}",
            best.severity, best.category, best.id, best.message
        );

        vec![CoachAlert {
            id:        Uuid::new_v4().to_string(),
            tip_id:    best.id.to_string(),
            category:  best.category.to_string(),
            severity:  best.severity.to_string(),
            message:   best.message,
            timestamp: ctx.game_time as i64,
        }]
    }

    /// Reação imediata a um evento OCR — sem esperar o ciclo de 30s.
    ///
    /// Chamado a cada evento recebido do canal OCR antes de repassá-lo ao Groq.
    /// Cobre: spell do jogador usada/voltou, spell inimiga usada/voltou.
    pub fn react_to_ocr_event(
        &mut self,
        event:          &str,
        game_time:      u32,
        player_role:    &str,
        smite_on_cd:    bool,     // true = smite está em cooldown neste momento
        objective_near: bool,     // true = baron ou drake disponível ou em < 60s
        other_spell_available: bool, // true = o OUTRO spell (D ou F) está disponível
        lang:           &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();
        let is_jungle = player_role == "JUNGLE";

        // ── Spell DO JOGADOR caiu → jogar defensivo ───────────
        if event.contains("aliado em cooldown") {
            let key = parse_player_spell_key(event);
            let msg = if lang == "en-US" {
                format!("Your {} is on cooldown — avoid exposed positions for now", key)
            } else {
                format!("Seu {} caiu — evite posições expostas por enquanto", key)
            };
            if let Some(a) = self.make_alert(
                "own_spell_used", "POSITIONING", "INFO",
                &msg, game_time, 60,
            ) { alerts.push(a); }
        }

        // ── Spell DO JOGADOR voltou → pode engajar ────────────
        if event.contains("aliado disponível") {
            let key = parse_player_spell_key(event);
            let msg = if lang == "en-US" {
                format!("Your {} is back — window to engage or take a trade", key)
            } else {
                format!("Seu {} voltou — janela para engajar ou tomar trade", key)
            };
            if let Some(a) = self.make_alert(
                "own_spell_ready", "TRADE", "INFO",
                &msg, game_time, 60,
            ) { alerts.push(a); }
        }

        // ── Spell INIMIGA caiu → janela de agressividade ──────
        if event.contains("inimigo_") && event.ends_with("em cooldown") {
            let (slot, key) = parse_enemy_spell(event);

            if is_jungle {
                let msg = if lang == "en-US" {
                    format!("Enemy {} has no {} — gank window on that lane now", slot, key)
                } else {
                    format!("Inimigo {} sem {} — janela de gank nessa rota agora", slot, key)
                };
                if let Some(a) = self.make_alert(
                    "jungle_gank_spell_window", "TRADE", "WARNING",
                    &msg, game_time, 60,
                ) { alerts.push(a); }
            } else {
                let msg = if lang == "en-US" {
                    format!("Enemy {} used {} — window to be aggressive", slot, key)
                } else {
                    format!("Inimigo {} gastou {} — janela para ser agressivo", slot, key)
                };
                if let Some(a) = self.make_alert(
                    "enemy_spell_used", "TRADE", "WARNING",
                    &msg, game_time, 60,
                ) { alerts.push(a); }
            }
        }

        // ── Spell INIMIGA voltou → cuidado com engajes ────────
        if event.contains("inimigo_") && event.ends_with("disponível") {
            let (slot, key) = parse_enemy_spell(event);
            let msg = if is_jungle {
                if lang == "en-US" {
                    format!("Enemy {}'s {} is back — avoid ganking that lane for now", slot, key)
                } else {
                    format!("{} do inimigo {} voltou — evite gankar essa rota por enquanto", key, slot)
                }
            } else if lang == "en-US" {
                format!("Enemy {}'s {} is back — respect the engage", slot, key)
            } else {
                format!("{} do inimigo {} voltou — respeite o engage", key, slot)
            };
            if let Some(a) = self.make_alert(
                "enemy_spell_ready", "POSITIONING", "INFO",
                &msg, game_time, 60,
            ) { alerts.push(a); }
        }

        // ── Smite caiu com objetivo disponível → risco de smite ─
        if is_jungle && smite_on_cd && objective_near
            && event.contains("aliado em cooldown")
        {
            if let Some(a) = self.make_alert(
                "smite_cd_objective", "OBJECTIVE", "CRITICAL",
                t!(lang, "Seu smite caiu com objetivo próximo — não force a disputa agora",
                         "Smite on cooldown with objective nearby — don't contest it now"),
                game_time, 30,
            ) { alerts.push(a); }
        }

        // ── Ambas as spells voltaram → janela de plays grandes ─
        if event.contains("aliado disponível") && other_spell_available
            && game_time > 300
        {
            let msg = if is_jungle {
                t!(lang,
                   "Ambas suas spells estão disponíveis — janela ideal para gank decisivo",
                   "Both spells up — ideal window for a decisive gank")
            } else {
                t!(lang,
                   "Suas duas spells estão disponíveis — janela para play de alto impacto",
                   "Both spells ready — window for a high-impact play")
            };
            if let Some(a) = self.make_alert(
                "both_spells_ready", "TRADE", "INFO",
                msg, game_time, 60,
            ) { alerts.push(a); }
        }

        alerts
    }

    pub fn handle_score_change(
        &mut self,
        new_ally:    u8, new_enemy: u8,
        prev_ally:   u8, prev_enemy: u8,
        game_time:   u32,
        pattern:     Option<&PlayerPattern>,
        player_role: &str,
        lang:        &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        // Mensagens de kill individuais vêm de handle_champion_kill (ChampionKill event).
        // Aqui mantemos apenas enrichments de padrão comportamental do jogador.
        if new_ally > prev_ally {
            if let Some(p) = pattern {
                if p.lane_dominance > 0.7 && p.objective_control < 0.35 && game_time > 600 {
                    if let Some(a) = self.make_alert(
                        "lane_to_objectives", "TRADE", "INFO",
                        t!(lang,
                           "Você domina a lane — roame e converta em objetivos",
                           "You dominate the lane — roam and convert into objectives"),
                        game_time, 400,
                    ) { alerts.push(a); }
                }
                if player_role == "MID"
                    && p.roam_frequency < 0.30
                    && game_time > 300
                    && game_time < 1200
                {
                    if let Some(a) = self.make_alert(
                        "mid_low_roam_kill", "MACRO", "INFO",
                        t!(lang,
                           "Empurre a wave e roame — morte inimiga é sua janela de pressão no mapa",
                           "Shove the wave and roam — enemy death is your map pressure window"),
                        game_time, 300,
                    ) { alerts.push(a); }
                }
            }
        }

        // ── Inimigo com kill → danger window ──────────────────────
        if new_enemy > prev_enemy {
            if let Some(p) = pattern {
                if p.aggression_score > 0.65 && game_time < 1500 {
                    if let Some(a) = self.make_alert(
                        "enemy_kill_aggression_warning", "TRADE", "WARN",
                        t!(lang,
                           "Inimigo com kill — power spike ativo, evite trade direto agora",
                           "Enemy has a kill — power spike active, avoid direct trade now"),
                        game_time, 90,
                    ) { alerts.push(a); }
                }
                if p.avg_deaths_10_15 > 0.8 && game_time > 600 && game_time < 900 {
                    if let Some(a) = self.make_alert(
                        "enemy_kill_death_window", "MACRO", "WARN",
                        t!(lang,
                           "Você costuma morrer nessa janela — inimigo com kill aumenta o risco",
                           "You tend to die in this window — enemy kill raises the risk"),
                        game_time, 120,
                    ) { alerts.push(a); }
                }
            } else if game_time < 480 {
                if let Some(a) = self.make_alert(
                    "enemy_kill_early", "TRADE", "INFO",
                    t!(lang,
                       "Inimigo marcou — posicione atrás da wave até o spike passar",
                       "Enemy scored — position behind the wave until the spike fades"),
                    game_time, 90,
                ) { alerts.push(a); }
            }
        }

        alerts
    }

    /// Coaching de kill usando KillCtx — contexto enriquecido pelo engine.
    ///
    /// Filtros de anti-ruído aplicados aqui antes de gerar qualquer alerta:
    ///   · teamfight (3+ kills em 10s) → silencia coaching individual
    ///   · early game (<2min) / late game (>35min) → macro ainda/já irrelevante
    ///   · kill revertida (killer morreu nos 10s seguintes) → window fechou
    pub fn handle_champion_kill(
        &mut self,
        ctx:     &KillCtx,
        pattern: Option<&PlayerPattern>,
        lang:    &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        // ── Anti-ruído ────────────────────────────────────────────
        if ctx.is_teamfight                          { return alerts; }
        if ctx.game_time < 120                       { return alerts; }
        if ctx.game_time > 2100                      { return alerts; }
        if ctx.is_ally_kill && ctx.killer_counter_killed { return alerts; }

        let respawn   = calc_respawn_secs(ctx.game_time);
        let vlabel    = role_label(&ctx.victim_role, lang);
        let has_names = !ctx.killer_champ.is_empty() && !ctx.victim_champ.is_empty();

        if ctx.is_ally_kill {
            // Ação: objetivo disponível tem prioridade absoluta sobre push de lane
            let action = if ctx.dragon_secs <= 60 {
                if lang == "en-US" {
                    format!("Dragon in {}s — secure it, he can't contest", ctx.dragon_secs.max(1))
                } else {
                    format!("Dragão em {}s — pegue, ele não pode contestar", ctx.dragon_secs.max(1))
                }
            } else if ctx.baron_secs.map(|s| s <= 60).unwrap_or(false) {
                let s = ctx.baron_secs.unwrap().max(1);
                if lang == "en-US" { format!("Baron in {}s — take it now", s) }
                else               { format!("Baron em {}s — pegue agora", s) }
            } else {
                push_suggestion(&ctx.victim_role, &ctx.player_role, ctx.game_time, lang)
            };

            let tp_caveat = if ctx.enemy_tp_threat {
                if lang == "en-US" { ". Watch for enemy TP" } else { ". Cuidado com TP inimigo" }
            } else { "" };

            let who = if has_names {
                if lang == "en-US" {
                    format!("{} ({}) eliminated", ctx.victim_champ, vlabel)
                } else {
                    format!("{} ({}) morreu", ctx.victim_champ, vlabel)
                }
            } else {
                if lang == "en-US" { format!("Enemy {} down", vlabel) }
                else               { format!("{} inimigo morreu", vlabel) }
            };

            let msg = if lang == "en-US" {
                format!("{}. {} — back in ~{}s{}", who, action, respawn, tp_caveat)
            } else {
                format!("{}. {} — volta em ~{}s{}", who, action, respawn, tp_caveat)
            };

            if let Some(a) = self.make_alert("kill_ally", "MACRO", "INFO", &msg, ctx.game_time, 20) {
                alerts.push(a);
            }

            if let Some(p) = pattern {
                if p.lane_dominance > 0.7 && p.objective_control < 0.35 && ctx.game_time > 600 {
                    if let Some(a) = self.make_alert(
                        "lane_to_objectives", "TRADE", "INFO",
                        t!(lang,
                           "Você domina a lane — converta essa vantagem em objetivos",
                           "You dominate the lane — convert that advantage into objectives"),
                        ctx.game_time, 400,
                    ) { alerts.push(a); }
                }
            }
        } else {
            let defense_hint = defensive_hint(&ctx.player_role, &ctx.victim_role, ctx.game_time, ctx.has_ward_available, lang);

            let who = if has_names {
                if lang == "en-US" {
                    format!("{} killed {} ({})", ctx.killer_champ, ctx.victim_champ, vlabel)
                } else {
                    format!("{} eliminou {} ({})", ctx.killer_champ, ctx.victim_champ, vlabel)
                }
            } else if !ctx.killer_champ.is_empty() {
                if lang == "en-US" { format!("{} killed allied {}", ctx.killer_champ, vlabel) }
                else               { format!("{} eliminou {} aliado", ctx.killer_champ, vlabel) }
            } else {
                if lang == "en-US" { format!("Enemy killed allied {}", vlabel) }
                else               { format!("Inimigo eliminou {} aliado", vlabel) }
            };

            let msg = format!("{}. {}", who, defense_hint);

            if let Some(a) = self.make_alert("kill_enemy", "POSITIONING", "WARNING", &msg, ctx.game_time, 20) {
                alerts.push(a);
            }
        }

        alerts
    }

    /// Reação imediata a objetivos neutros capturados (Drake, Baron, Herald).
    /// Disparado pelo fast poll do engine quando detecta evento novo na Live API.
    pub fn handle_objective_event(
        &mut self,
        objective:    &str,
        ally_got_it:  bool,
        game_time:    u32,
        player_role:  &str,
        dragon_ally:  u8,
        dragon_enemy: u8,
        pattern:      Option<&PlayerPattern>,
        lang:         &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();
        let is_jungle = player_role == "JUNGLE";

        match objective {
            "dragon" => {
                let total = dragon_ally + dragon_enemy;
                let next_is_elder = total >= 4;
                let sev = if next_is_elder { "CRITICAL" } else { "INFO" };

                if ally_got_it {
                    let msg = if next_is_elder && is_jungle {
                        t!(lang,
                           "Drake pego — próximo é o Elder Dragon. Prepare visão e smite",
                           "Dragon secured — next is Elder Dragon. Set up vision and save smite")
                    } else if next_is_elder {
                        t!(lang,
                           "Drake pego — próximo será o Elder Dragon. Agrupe para contestar",
                           "Dragon secured — next is Elder Dragon. Group up to contest")
                    } else if is_jungle {
                        t!(lang,
                           "Drake pego — inicie o clear e procure gank antes do respawn",
                           "Dragon secured — start your clear and look for a gank before respawn")
                    } else {
                        t!(lang,
                           "Drake pego — mantenha pressão e prepare o próximo objetivo",
                           "Dragon secured — maintain pressure and prepare for the next objective")
                    };
                    if let Some(a) = self.make_alert(
                        "obj_ally_drake", "OBJECTIVE", sev, msg, game_time, 120,
                    ) { alerts.push(a); }
                } else {
                    let msg = if next_is_elder {
                        t!(lang,
                           "Inimigo pegou drake — próximo é o Elder Dragon. Conteste a todo custo",
                           "Enemy got the dragon — next is Elder Dragon. Contest at all costs")
                    } else if is_jungle {
                        t!(lang,
                           "Inimigo pegou drake — fortaleça visão e evite invade no momento",
                           "Enemy got the dragon — reinforce vision and avoid invading now")
                    } else {
                        t!(lang,
                           "Inimigo pegou drake — jogue seguro e foque em objetivos menores",
                           "Enemy got the dragon — play safe and focus on smaller objectives")
                    };
                    if let Some(a) = self.make_alert(
                        "obj_enemy_drake", "OBJECTIVE", if next_is_elder { "CRITICAL" } else { "WARNING" },
                        msg, game_time, 120,
                    ) { alerts.push(a); }
                }

                // TP efficiency — drake é momento ideal para TOP usar TP
                if let Some(p) = pattern {
                    if matches!(player_role, "TOP" | "MID") && p.tp_efficiency < 0.40 && game_time < 1200 {
                        if let Some(a) = self.make_alert(
                            "tp_low_efficiency_obj", "MACRO", "INFO",
                            t!(lang,
                               "Seu TP tem baixo impacto histórico — use TP para ajudar em objetivos como este",
                               "Your TP has low historical impact — use TP to help on objectives like this"),
                            game_time, 600,
                        ) { alerts.push(a); }
                    }
                }
            }
            "baron" => {
                if ally_got_it {
                    let msg = if is_jungle {
                        t!(lang,
                           "Baron pego — coordene o push com o buff, priorize torres e inibidores",
                           "Baron secured — coordinate the push with the buff, prioritize towers and inhibitors")
                    } else {
                        t!(lang,
                           "Baron pego — agrupe com o time e empurre com o buff agora",
                           "Baron secured — group up and push with the buff now")
                    };
                    if let Some(a) = self.make_alert(
                        "obj_ally_baron", "OBJECTIVE", "CRITICAL", msg, game_time, 180,
                    ) { alerts.push(a); }

                    if let Some(a) = self.make_alert(
                        "recall_after_baron", "MACRO", "INFO",
                        t!(lang,
                           "Após empurrar com o Baron buff — recall para comprar antes do próximo objetivo",
                           "After pushing with Baron buff — recall to buy before the next objective"),
                        game_time, 240,
                    ) { alerts.push(a); }
                } else {
                    let msg = if is_jungle {
                        t!(lang,
                           "Inimigo com Baron — recue, defenda torres e não force luta aberta",
                           "Enemy has Baron — back off, defend towers and don't force open fights")
                    } else {
                        t!(lang,
                           "Inimigo com Baron — recue e defenda, evite lutas longe da base",
                           "Enemy has Baron — back off and defend, avoid fights away from base")
                    };
                    if let Some(a) = self.make_alert(
                        "obj_enemy_baron", "OBJECTIVE", "CRITICAL", msg, game_time, 180,
                    ) { alerts.push(a); }
                }
            }
            "herald" => {
                if ally_got_it {
                    let msg = if is_jungle {
                        t!(lang,
                           "Herald pego — use na lane com mais pressão ou sob a torre mais fraca",
                           "Herald secured — use it in the highest-pressure lane or under the weakest tower")
                    } else {
                        t!(lang,
                           "Herald pego — force a torre com o herald agora",
                           "Herald secured — force the tower with the Herald now")
                    };
                    if let Some(a) = self.make_alert(
                        "obj_ally_herald", "OBJECTIVE", "INFO", msg, game_time, 120,
                    ) { alerts.push(a); }
                }
            }
            "tower" => {
                if ally_got_it {
                    if let Some(a) = self.make_alert(
                        "obj_ally_tower", "MACRO", "INFO",
                        t!(lang,
                           "Torre derrubada — mantenha a pressão e avance para o próximo objetivo",
                           "Tower destroyed — keep the pressure and advance to the next objective"),
                        game_time, 90,
                    ) { alerts.push(a); }
                } else {
                    if let Some(a) = self.make_alert(
                        "obj_enemy_tower", "POSITIONING", "WARNING",
                        t!(lang,
                           "Nossa torre caiu — reposicione, o inimigo tem acesso à sua lane",
                           "Our tower fell — reposition, the enemy has access to your lane"),
                        game_time, 90,
                    ) { alerts.push(a); }
                }
            }
            "inhibitor" => {
                if ally_got_it {
                    if let Some(a) = self.make_alert(
                        "obj_ally_inhib", "MACRO", "CRITICAL",
                        t!(lang,
                           "Inibidor inimigo caiu — super minions ajudam. Empurhe enquanto têm buff de minions",
                           "Enemy inhibitor down — super minions incoming. Push while you have the minion buff"),
                        game_time, 120,
                    ) { alerts.push(a); }
                } else {
                    if let Some(a) = self.make_alert(
                        "obj_enemy_inhib", "POSITIONING", "CRITICAL",
                        t!(lang,
                           "Nosso inibidor caiu — super minions inimigos em todas as rotas. Defenda e minimize dano",
                           "Our inhibitor is down — enemy super minions in all lanes. Defend and minimize damage"),
                        game_time, 120,
                    ) { alerts.push(a); }
                }
            }
            _ => {}
        }

        alerts
    }

    /// Coaching específico pelo tipo do drake capturado.
    /// Disparado pelo evento DragonKill da Live API (campo DragonType).
    pub fn handle_drake_type(
        &mut self,
        drake_type:  &str,
        ally_got_it: bool,
        game_time:   u32,
        lang:        &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        if drake_type == "Elder" { return alerts; } // coberto pelo elder_warning

        let (buff_desc_pt, buff_desc_en, strategy_pt, strategy_en) = match drake_type {
            "Fire"     => (
                "Infernal — buff de dano AD/AP",
                "Inferno — AD/AP damage buff",
                "Force trades agressivos e teamfights enquanto tem o buff",
                "Force aggressive trades and teamfights while you have the buff",
            ),
            "Water"    => (
                "Oceano — buff de regeneração",
                "Ocean — regeneration buff",
                "Trades prolongados te favorecem — não recue cedo das lutas",
                "Extended trades favor you — don't back off fights early",
            ),
            "Air"      => (
                "Nuvem — buff de velocidade de ult",
                "Cloud — ult cooldown buff",
                "Roaming e posicionamento ficam mais eficientes agora",
                "Roaming and positioning become more efficient now",
            ),
            "Earth"    => (
                "Montanha — buff de escudo",
                "Mountain — shield buff",
                "Mais resistente em objetivos — force lutas perto de torres",
                "More durable on objectives — force fights near towers",
            ),
            "Hextech"  => (
                "Hextech — slow em cadeia",
                "Hextech — chain slow",
                "Use em teamfights com chains de CC para maximizar o buff",
                "Use in teamfights with CC chains to maximize the buff",
            ),
            "Chemtech" => (
                "Chemtech — revive com baixo HP",
                "Chemtech — revive at low HP",
                "Lute até o fim — o buff ativa abaixo de 50% de HP",
                "Fight to the end — the buff activates below 50% HP",
            ),
            _ => return alerts,
        };

        let (buff_desc, strategy) = if lang == "en-US" {
            (buff_desc_en, strategy_en)
        } else {
            (buff_desc_pt, strategy_pt)
        };

        let (id, msg) = if ally_got_it {
            if lang == "en-US" {
                ("drake_type_ally", format!("{} dragon secured — {}", buff_desc, strategy))
            } else {
                ("drake_type_ally", format!("Drake {} conquistado — {}", buff_desc, strategy))
            }
        } else {
            let short = buff_desc.split(" —").next().unwrap_or(buff_desc);
            if lang == "en-US" {
                ("drake_type_enemy", format!("Enemy got {} dragon — watch out, {}", short, strategy.to_lowercase()))
            } else {
                ("drake_type_enemy", format!("Inimigo pegou drake {} — cuidado, {}", short, strategy.to_lowercase()))
            }
        };

        if let Some(a) = self.make_alert(id, "OBJECTIVE", "INFO", &msg, game_time, 30) {
            alerts.push(a);
        }

        alerts
    }

    /// Coaching de level-up e vantagem de nível sobre o oponente de lane.
    /// Disparado pelo fast poll quando o nível do jogador muda.
    pub fn handle_level_change(
        &mut self,
        new_level:       u8,
        prev_level:      u8,
        opponent_level:  u8,
        player_role:     &str,
        game_time:       u32,
        lang:            &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        if new_level <= prev_level { return alerts; }

        // ── Power spikes em níveis chave (role-aware) ─────────
        let spike_msg: Option<&'static str> = if lang == "en-US" {
            match (new_level, player_role) {
                (6,  "TOP") => Some("Level 6 — consider the 1v1 or force the enemy back. Save TP for teamfights"),
                (9,  "TOP") => Some("Level 9 — first ability maxed. Your lane damage is at its peak"),
                (11, "TOP") => Some("Level 11 — strong for duels. Push the lane or act globally with TP"),
                (16, "TOP") => Some("Level 16 — ult rank 3. You dominate 1v1. Force or group up"),
                (6,  "MID") => Some("Level 6 — shove the wave and roam bot or top. Ult creates global threat"),
                (9,  "MID") => Some("Level 9 — main ability maxed. Your wave clear and poke are at their peak"),
                (11, "MID") => Some("Level 11 — roam power spike. Push and rotate to create map pressure"),
                (16, "MID") => Some("Level 16 — ult rank 3. Play for teamfights or pressure the mid 1v1"),
                (6,  "SUPPORT") | (6,  "UTILITY") =>
                    Some("Level 6 — ult ready. Coordinate an engage or protect your ADC. You set the fight timing"),
                (9,  "SUPPORT") | (9,  "UTILITY") =>
                    Some("Level 9 — CC/shield/poke at peak. Time to dominate the lane or rotate to help"),
                (11, "SUPPORT") | (11, "UTILITY") =>
                    Some("Level 11 — ult rank 2. Your teamfight impact increased. Rotate to objectives"),
                (16, "SUPPORT") | (16, "UTILITY") =>
                    Some("Level 16 — ult max rank. You're at peak impact. Dominate teamfights"),
                (6,  "JUNGLE") =>
                    Some("Level 6 — ult ready. Your ganks hit harder now. Find lanes that can be turned around"),
                (9,  "JUNGLE") =>
                    Some("Level 9 — main ability maxed. Faster clears = more time for ganks and objectives"),
                (11, "JUNGLE") =>
                    Some("Level 11 — ult rank 2. Invade the enemy jungle or force an objective confidently"),
                (16, "JUNGLE") =>
                    Some("Level 16 — late game peak. Baron and Elder Dragon are the focus. Don't waste ult on small fights"),
                (6,  "BOTTOM") | (6,  "ADC") =>
                    Some("Level 6 — ult ready. Evaluate the fight with your support or save it for a decisive moment"),
                (9,  "BOTTOM") | (9,  "ADC") =>
                    Some("Level 9 — first ability maxed. Your DPS and range are at their peak this phase"),
                (11, "BOTTOM") | (11, "ADC") =>
                    Some("Level 11 — strong for teamfights. Prioritize dragons and stay alive in fights"),
                (16, "BOTTOM") | (16, "ADC") =>
                    Some("Level 16 — build nearly complete. Every teamfight decides the game. Position in the backline"),
                (6,  _) => Some("Level 6 reached — ultimate available. Force an engage"),
                (9,  _) => Some("Level 9 reached — abilities maxed. Time to dominate"),
                (11, _) => Some("Level 11 reached — critical power spike. Force a teamfight"),
                (16, _) => Some("Level 16 reached — ultimate rank 3. Play aggressive now"),
                _       => None,
            }
        } else {
            match (new_level, player_role) {
                (6,  "TOP") => Some("Nível 6 — avalie o 1v1 ou force o inimigo a recuar. Guarde TP para teamfights"),
                (9,  "TOP") => Some("Nível 9 — primeira habilidade maximizada. Seu dano de lane está no pico"),
                (11, "TOP") => Some("Nível 11 — forte para duelos. Avance na lane ou aja globalmente com TP"),
                (16, "TOP") => Some("Nível 16 — ult no rank máximo. Você domina o 1v1. Force ou group"),
                (6,  "MID") => Some("Nível 6 — empurre a wave e roame para bot ou top. Ult disponível cria ameaça global"),
                (9,  "MID") => Some("Nível 9 — habilidade principal maxada. Seu clear de wave e poke estão no pico"),
                (11, "MID") => Some("Nível 11 — power spike de roame. Empurre e rotacione para criar pressão de mapa"),
                (16, "MID") => Some("Nível 16 — ult rank 3. Jogue para teamfights ou crie pressão no 1v1 da mid"),
                (6,  "SUPPORT") | (6,  "UTILITY") =>
                    Some("Nível 6 — ult disponível. Coordene engage ou proteja o ADC. Você define o timing das lutas"),
                (9,  "SUPPORT") | (9,  "UTILITY") =>
                    Some("Nível 9 — CC/escudo/poke no pico. Momento de dominar a lane ou rotacionar para ajudar"),
                (11, "SUPPORT") | (11, "UTILITY") =>
                    Some("Nível 11 — ult rank 2. Seu impacto em teamfights aumentou. Rotacione para objetivos"),
                (16, "SUPPORT") | (16, "UTILITY") =>
                    Some("Nível 16 — ult rank máximo. Você está no auge do impacto. Domine as teamfights"),
                (6,  "JUNGLE") =>
                    Some("Nível 6 — ult disponível. Seu gank tem mais impacto agora. Procure lanes que podem ser viradas"),
                (9,  "JUNGLE") =>
                    Some("Nível 9 — habilidade principal maxada. Clears mais rápidos = mais tempo para ganks e objetivos"),
                (11, "JUNGLE") =>
                    Some("Nível 11 — ult rank 2. Invada o jungle inimigo ou force um objetivo com confiança"),
                (16, "JUNGLE") =>
                    Some("Nível 16 — pico de late game. Baron e Elder Dragon são o foco. Não disperdice o ult em fights pequenas"),
                (6,  "BOTTOM") | (6,  "ADC") =>
                    Some("Nível 6 — ult disponível. Avalie o fight com o suporte ou guarde para momento decisivo"),
                (9,  "BOTTOM") | (9,  "ADC") =>
                    Some("Nível 9 — primeira habilidade maxada. Seu DPS e range estão no pico desta fase"),
                (11, "BOTTOM") | (11, "ADC") =>
                    Some("Nível 11 — forte para teamfights. Priorize drakes e fique vivo durante as lutas"),
                (16, "BOTTOM") | (16, "ADC") =>
                    Some("Nível 16 — build quase completo. Cada teamfight decide o jogo. Posicione-se no backline"),
                (6,  _) => Some("Você atingiu nível 6 — ultimate disponível. Force engajamento"),
                (9,  _) => Some("Você atingiu nível 9 — habilidades maximizadas. Momento de dominar"),
                (11, _) => Some("Você atingiu nível 11 — power spike crítico. Force teamfight"),
                (16, _) => Some("Você atingiu nível 16 — ultimate rank 3. Jogue agressivo agora"),
                _       => None,
            }
        };
        if let Some(msg) = spike_msg {
            if let Some(a) = self.make_alert(
                "level_power_spike", "TRADE", "INFO", msg, game_time, 30,
            ) { alerts.push(a); }
        }

        // ── Vantagem de nível sobre oponente de lane ──────────
        if new_level > opponent_level && !matches!(player_role, "JUNGLE") {
            let advantage = new_level - opponent_level;
            let msg = if advantage >= 2 {
                if lang == "en-US" {
                    format!("You are {} levels ahead of your opponent — dominate the lane now", advantage)
                } else {
                    format!("Você está {} níveis à frente do oponente — domine a lane agora", advantage)
                }
            } else if lang == "en-US" {
                "You are one level ahead — window for a favorable trade".to_string()
            } else {
                "Você está um nível à frente — janela para trade favorável".to_string()
            };
            if let Some(a) = self.make_alert(
                "level_advantage", "TRADE", "WARNING", &msg, game_time, 60,
            ) { alerts.push(a); }
        }

        alerts
    }

    /// Coaching de CS vs oponente de lane.
    /// Disparado pelo fast poll quando o gap ultrapassa 20 CS.
    pub fn handle_cs_lane_gap(
        &mut self,
        player_cs:   u32,
        opponent_cs: u32,
        player_role: &str,
        game_time:   u32,
        lang:        &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        if matches!(player_role, "SUPPORT" | "UTILITY" | "JUNGLE") { return alerts; }
        if game_time < 180 { return alerts; }

        if opponent_cs > player_cs {
            let gap = opponent_cs - player_cs;
            if gap >= 20 {
                let msg = if lang == "en-US" {
                    format!("Opponent is {} CS ahead — focus on farming between objectives", gap)
                } else {
                    format!("Oponente está com {} CS a mais — foque no farm entre objetivos", gap)
                };
                if let Some(a) = self.make_alert(
                    "cs_lane_gap", "MACRO", "INFO", &msg, game_time, 120,
                ) { alerts.push(a); }
            }
        }

        alerts
    }

    /// Alerta quando o oponente de lane atinge um level spike crítico.
    /// Cada nível tem ID único — cooldown não bloqueia spikes futuros.
    /// Disparado pelo fast poll (5s) quando o nível do oponente muda.
    pub fn handle_enemy_level_spike(
        &mut self,
        new_level:   u8,
        prev_level:  u8,
        game_time:   u32,
        _player_role: &str,
        lang:        &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();
        if new_level <= prev_level { return alerts; }

        let (id, severity, msg_pt, msg_en): (&'static str, &'static str, &'static str, &'static str) = match new_level {
            6  => ("enemy_lvl6",  "WARNING",
                   "Inimigo atingiu nível 6 — ultimate disponível, jogue com cuidado",
                   "Enemy hit level 6 — ultimate available, play carefully"),
            11 => ("enemy_lvl11", "WARNING",
                   "Inimigo atingiu nível 11 — power spike crítico, evite trade desfavorável",
                   "Enemy hit level 11 — critical power spike, avoid unfavorable trades"),
            16 => ("enemy_lvl16", "CRITICAL",
                   "Inimigo atingiu nível 16 — ultimate rank 3, extremamente perigoso",
                   "Enemy hit level 16 — ultimate rank 3, extremely dangerous"),
            _  => return alerts,
        };

        let msg = if lang == "en-US" { msg_en } else { msg_pt };
        if let Some(a) = self.make_alert(id, "TRADE", severity, msg, game_time, 30) {
            alerts.push(a);
        }
        alerts
    }

    /// Coaching de CS — disparado pelo fast poll quando o CS real
    /// cai abaixo do esperado para o tempo de jogo.
    ///
    /// Benchmark: ~8 CS/min early, ~7 CS/min mid game.
    pub fn handle_cs_update(
        &mut self,
        actual_cs:   u32,
        game_time:   u32,
        player_role: &str,
        lang:        &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        if matches!(player_role, "SUPPORT" | "UTILITY" | "JUNGLE") {
            return alerts;
        }
        if game_time < 300 { return alerts; }

        let minutes = game_time / 60;
        let expected = if minutes <= 10 {
            minutes * 8
        } else {
            80 + (minutes - 10) * 7
        };

        if expected > 0 && actual_cs < expected * 3 / 4 {
            let deficit = expected.saturating_sub(actual_cs);
            let msg = if lang == "en-US" {
                format!("You are {deficit} CS below expected — focus on farming between objectives")
            } else {
                format!("Você está {deficit} CS abaixo do esperado — foque no farm entre objetivos")
            };
            if let Some(a) = self.make_alert(
                "cs_deficit", "MACRO", "INFO",
                &msg,
                game_time, 180,
            ) { alerts.push(a); }
        }

        alerts
    }

    /// Detecta brecha de gank quando um inimigo é detectado além do rio
    /// (território aliado). Disparado pelo template matching do OCR.
    ///
    /// Zonas calibradas para os dois lados do mapa (ORDER = azul, CHAOS = vermelho).
    /// Só emite alertas para o JUNGLE — outros roles não precisam dessa info diretamente.
    pub fn handle_gank_opportunity(
        &mut self,
        x_pct:        f32,
        y_pct:        f32,
        is_blue_side: bool,
        game_time:    u32,
        player_role:  &str,
        lang:         &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        if player_role != "JUNGLE" || game_time > 1200 { return alerts; }

        if let Some((lane, msg)) = classify_gank_zone(x_pct, y_pct, is_blue_side, lang) {
            let id: &'static str = match lane {
                "bot" => "gank_opp_bot",
                "top" => "gank_opp_top",
                "mid" => "gank_opp_mid",
                _     => "gank_opp",
            };
            if let Some(a) = self.make_alert(id, "TRADE", "WARNING", msg, game_time, 45) {
                alerts.push(a);
            }
        }

        alerts
    }

    /// Reseta cooldowns ao fim de uma partida.
    pub fn reset(&mut self) {
        self.last_alert_at.clear();
        self.last_eval_emit_at = None;
    }

    /// Retorna IDs dos spots de ward recomendados para o contexto atual.
    ///
    /// Prioridades (em ordem decrescente):
    ///   1. Objetivo iminente (≤ 90s)                → vision control imediata
    ///   2. Jungle inimigo detectado no lado oposto   → ward profundo urgente
    ///   3. Scuttler (3:30)                           → rio para disputa
    ///   4. Larvas do Vazio (5:00–14:45)              → Baron side vision
    ///   5. Herald (15:00–19:45)                      → Baron side vision
    ///   6. Pré-objetivo médio (≤ 180s)               → preparação de visão
    ///   7. Role-based por fase de jogo               → visão de lane/jungle
    ///
    /// Patch 26.10: Herald em 15:00, Larvas em 5:00, Atakhan removido.
    ///
    /// Retorna lista vazia → overlay some.
    pub fn suggest_ward_spots(&self, ctx: &ProcContext) -> Vec<&'static str> {
        let role = ctx.player_role.as_str();

        let dragon_secs  = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
        let baron_alive  = ctx.next_baron_spawn.is_some() || ctx.game_time >= 1200;
        let baron_secs   = match ctx.next_baron_spawn {
            Some(t)                            => Some(t.saturating_sub(ctx.game_time)),
            None if ctx.game_time >= 1200      => Some(0),
            None                               => None,
        };

        // ── 1. OBJETIVO IMINENTE (≤ 90s) — máxima prioridade ─
        if dragon_secs > 0 && dragon_secs <= 90 {
            let mut s = vec!["dragon_pit", "pixel_ward", "river_bot_brush"];
            if dragon_secs <= 60 { s.push("tribush_bot"); }
            // Elder ou 4º drake → ward profundo também
            if ctx.dragon_ally >= 3 || ctx.dragon_enemy >= 3 {
                s.push("mid_river_drake");
            }
            return dedup(s);
        }

        if baron_alive {
            if let Some(secs) = baron_secs {
                if secs <= 90 {
                    let mut s = vec!["baron_pit", "baron_river"];
                    if secs <= 60 { s.push("deep_top_enemy"); }
                    match role {
                        "MID"                         => s.push("mid_river_baron"),
                        "JUNGLE"                      => s.push("enemy_blue_buff"),
                        "SUPPORT" | "UTILITY"         => { s.push("deep_top_enemy"); }
                        _                             => {}
                    }
                    return dedup(s);
                }
            }
        }

        // ── 2. JUNGLE INIMIGO NO LADO OPOSTO ─────────────────
        if ctx.enemy_in_opp_jungle && ctx.game_time < 1200 {
            let s = match role {
                "BOTTOM" | "ADC"              => vec!["deep_bot_enemy", "river_bot_brush", "pixel_ward"],
                "SUPPORT" | "UTILITY"         => vec!["deep_bot_enemy", "pixel_ward"],
                "TOP"                         => vec!["deep_top_enemy", "top_river"],
                "JUNGLE"                      => vec!["deep_top_enemy", "deep_bot_enemy"],
                "MID"                         => vec!["mid_river_baron", "mid_river_drake"],
                _                             => vec![],
            };
            if !s.is_empty() { return dedup(s); }
        }

        // ── 3. SCUTTLER (3:30 = 210s) ────────────────────────
        // Aviso a partir de 2:55 (175s) para o jogador se posicionar.
        if (175..=215).contains(&ctx.game_time) {
            return dedup(vec!["river_bot_brush", "mid_river_drake"]);
        }

        // ── 4. LARVAS DO VAZIO (5:00–14:45) ──────────────────
        // Patch 26.10: Larvas spawnam às 5:00 (300s), mesma janela que o primeiro Drake.
        // Ficam disponíveis até 14:45 (885s). Ward na Baron side é fundamental.
        if (260..=320).contains(&ctx.game_time) || (ctx.game_time > 320 && ctx.game_time < 885) {
            let in_early_window = ctx.game_time < 400;
            if in_early_window || matches!(role, "TOP" | "JUNGLE") {
                return dedup(vec!["baron_pit", "baron_river"]);
            }
        }

        // ── 5. HERALD (15:00–19:45) ──────────────────────────
        // Patch 26.10: Herald em 15:00 (900s), despawna em 19:45 (1185s).
        // Ward na Baron side para controle da luta.
        if !ctx.herald_killed && (860..=1185).contains(&ctx.game_time) {
            return dedup(vec!["baron_pit", "baron_river", "deep_top_enemy"]);
        }

        // ── 6. PRÉ-OBJETIVO MÉDIO (≤ 180s) ───────────────────
        if dragon_secs > 0 && dragon_secs <= 180 {
            let s = match role {
                "JUNGLE"                      => vec!["pixel_ward", "dragon_pit"],
                "SUPPORT" | "UTILITY"         => vec!["pixel_ward", "river_bot_brush"],
                "ADC" | "BOTTOM"              => vec!["river_bot_brush", "pixel_ward"],
                "MID"                         => vec!["mid_river_drake", "pixel_ward"],
                _                             => vec!["pixel_ward"],
            };
            return dedup(s);
        }

        if baron_alive {
            if let Some(secs) = baron_secs {
                if secs <= 180 {
                    let s = match role {
                        "JUNGLE"              => vec!["baron_pit", "enemy_blue_buff"],
                        "TOP"                 => vec!["baron_river", "tribush_top"],
                        "MID"                 => vec!["mid_river_baron", "baron_pit"],
                        "SUPPORT" | "UTILITY" => vec!["baron_pit", "baron_river"],
                        _                     => vec!["baron_pit"],
                    };
                    return dedup(s);
                }
            }
        }

        // ── 5. ROLE-BASED POR FASE DO JOGO ───────────────────
        //
        // Early (0–8 min): visão de lane e primeiros objetivos
        if ctx.game_time < 480 {
            let s = match role {
                "SUPPORT" | "UTILITY" => vec!["tribush_bot", "river_bot_brush", "pixel_ward"],
                "ADC" | "BOTTOM"      => vec!["river_bot_brush", "pixel_ward"],
                "MID"                 => vec!["mid_river_drake", "mid_river_baron"],
                "TOP"                 => vec!["top_river", "tribush_top"],
                "JUNGLE"              => vec!["enemy_red_buff", "enemy_blue_buff", "mid_river_drake"],
                _                     => vec![],
            };
            return dedup(s);
        }

        // Mid-early (8–15 min): rotações + controle de objetivos
        if ctx.game_time < 900 {
            let s = match role {
                "SUPPORT" | "UTILITY" => vec!["pixel_ward", "mid_river_drake", "river_bot_brush"],
                "ADC" | "BOTTOM"      => vec!["river_bot_brush", "pixel_ward"],
                "MID"                 => vec!["mid_river_drake", "mid_river_baron"],
                "TOP"                 => vec!["top_river", "baron_river"],
                "JUNGLE"              => vec!["enemy_red_buff", "dragon_pit"],
                _                     => vec!["dragon_pit"],
            };
            return dedup(s);
        }

        // Mid (15–20 min): controle de baron + drake
        if ctx.game_time < 1200 {
            let s = match role {
                "SUPPORT" | "UTILITY" => vec!["baron_pit", "pixel_ward", "mid_river_baron"],
                "ADC" | "BOTTOM"      => vec!["river_bot_brush", "pixel_ward", "baron_pit"],
                "MID"                 => vec!["mid_river_baron", "mid_river_drake", "baron_pit"],
                "TOP"                 => vec!["baron_river", "baron_pit", "tribush_top"],
                "JUNGLE"              => vec!["baron_pit", "enemy_blue_buff", "dragon_pit"],
                _                     => vec!["baron_pit", "dragon_pit"],
            };
            return dedup(s);
        }

        // Late (20+ min): visão de baron é prioridade máxima
        let s = match role {
            "SUPPORT" | "UTILITY" => vec!["baron_pit", "baron_river", "deep_top_enemy"],
            "JUNGLE"              => vec!["baron_pit", "baron_river", "deep_top_enemy", "deep_bot_enemy"],
            "TOP"                 => vec!["baron_pit", "baron_river"],
            "MID"                 => vec!["baron_pit", "mid_river_baron"],
            _                     => vec!["baron_pit", "baron_river"],
        };
        dedup(s)
    }

    fn cooldown_ok(&self, id: &'static str, cooldown_secs: u64) -> bool {
        self.last_alert_at
            .get(id)
            .map(|t| t.elapsed() >= Duration::from_secs(cooldown_secs))
            .unwrap_or(true)
    }

    fn make_alert(
        &mut self,
        id:            &'static str,
        category:      &'static str,
        severity:      &'static str,
        message:       &str,
        game_time:     u32,
        cooldown_secs: u64,
    ) -> Option<CoachAlert> {
        if !self.cooldown_ok(id, cooldown_secs) {
            return None;
        }
        self.last_alert_at.insert(id, Instant::now());
        Some(CoachAlert {
            id:        Uuid::new_v4().to_string(),
            tip_id:    id.to_string(),
            category:  category.to_string(),
            severity:  severity.to_string(),
            message:   message.to_string(),
            timestamp: game_time as i64,
        })
    }
}

// ── Detecção de brecha de gank ────────────────────────────────

/// Classifica se a posição (x%, y%) está numa zona de overextension para cada lane.
/// Retorna `Some((lane, mensagem))` se for uma brecha de gank, `None` caso contrário.
///
/// Zonas para time azul (ORDER):
///   Bot overextended: inimigo cruzou o rio e está no lado aliado da bot lane
///   Top overextended: inimigo cruzou o rio e está no lado aliado da top lane
///   Mid overextended: inimigo está além do rio no lado aliado da mid lane
///
/// Para time vermelho (CHAOS), as zonas são espelhadas no eixo diagonal.
fn classify_gank_zone(x: f32, y: f32, is_blue_side: bool, lang: &str) -> Option<(&'static str, &'static str)> {
    let msg_bot = t!(lang,
        "Inimigo avançou além do rio na bot — brecha de gank, vá agora",
        "Enemy overextended past the river in bot — gank window, go now");
    let msg_top = t!(lang,
        "Inimigo avançou além do rio no topo — brecha de gank, vá agora",
        "Enemy overextended past the river in top — gank window, go now");
    let msg_mid = t!(lang,
        "Inimigo avançou além do rio na mid — brecha de gank, vá agora",
        "Enemy overextended past the river in mid — gank window, go now");

    if is_blue_side {
        if y > 73.0 && x < 58.0 {
            return Some(("bot", msg_bot));
        }
        if x < 22.0 && y < 32.0 {
            return Some(("top", msg_top));
        }
        if x < 40.0 && y > 60.0 && (x + y) > 95.0 {
            return Some(("mid", msg_mid));
        }
    } else {
        if y > 73.0 && x > 42.0 {
            return Some(("bot", msg_bot));
        }
        if x > 78.0 && y < 32.0 {
            return Some(("top", msg_top));
        }
        if x > 60.0 && y < 40.0 && (x + y) < 105.0 {
            return Some(("mid", msg_mid));
        }
    }
    None
}

// ── Regras por categoria ──────────────────────────────────────

fn evaluate_objective_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);

    if (55..=65).contains(&dragon_secs) {
        out.push(RuleResult {
            id: "dragon_60s",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 180,
            message: t!(lang,
                "Dragão em 1 minuto — monte visão no rio",
                "Dragon in 1 minute — set up vision in the river").into(),
        });
    }

    if (25..=35).contains(&dragon_secs) {
        let severity = if ctx.dragon_enemy == 3 { "CRITICAL" } else { "WARNING" };
        out.push(RuleResult {
            id: "dragon_30s",
            category: "OBJECTIVE",
            severity,
            cooldown: 180,
            message: if lang == "en-US" {
                format!("Dragon in {dragon_secs}s — rally your team now")
            } else {
                format!("Dragão em {dragon_secs}s — reúna o time agora")
            },
        });
    }

    if dragon_secs == 0 {
        let is_elder = ctx.dragon_ally >= 4 || ctx.dragon_enemy >= 4;
        let (msg, sev) = if is_elder {
            (t!(lang,
                "Elder Dragon disponível — conteste a todo custo, buff é decisivo",
                "Elder Dragon is up — contest at all costs, the buff is decisive"), "CRITICAL")
        } else {
            (t!(lang,
                "Dragão disponível — vá agora",
                "Dragon is up — go now"), "WARNING")
        };
        out.push(RuleResult {
            id: "dragon_now",
            category: "OBJECTIVE",
            severity: sev,
            cooldown: 240,
            message: msg.into(),
        });
    }

    if (ctx.dragon_ally >= 4 || ctx.dragon_enemy >= 4) && (25..=65).contains(&dragon_secs) {
        let who = if ctx.dragon_enemy >= 4 {
            t!(lang, "Inimigo tem alma", "Enemy has soul")
        } else {
            t!(lang, "Você tem alma", "You have soul")
        };
        out.push(RuleResult {
            id: "elder_warning",
            category: "OBJECTIVE",
            severity: "CRITICAL",
            cooldown: 180,
            message: if lang == "en-US" {
                format!("Elder Dragon in {dragon_secs}s — {who}. Contest or protect your life")
            } else {
                format!("Elder Dragon em {dragon_secs}s — {who}. Conteste ou proteja vida")
            },
        });
    }

    if ctx.dragon_enemy == 3 {
        out.push(RuleResult {
            id: "dragon_soul_danger",
            category: "OBJECTIVE",
            severity: "CRITICAL",
            cooldown: 600,
            message: t!(lang,
                "Inimigo a 1 drake da alma — conteste a todo custo",
                "Enemy is 1 dragon from soul — contest at all costs").into(),
        });
    }

    let baron_secs = match ctx.next_baron_spawn {
        Some(spawn_at) => Some(spawn_at.saturating_sub(ctx.game_time)),
        None if ctx.game_time >= 1200 => Some(0),
        None => None,
    };

    if let Some(secs) = baron_secs {
        if (55..=65).contains(&secs) {
            out.push(RuleResult {
                id: "baron_60s",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 180,
                message: t!(lang,
                    "Baron em 1 minuto — posicione o time",
                    "Baron in 1 minute — position your team").into(),
            });
        }
        if (25..=35).contains(&secs) {
            out.push(RuleResult {
                id: "baron_30s",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 180,
                message: if lang == "en-US" {
                    format!("Baron in {secs}s — set up smite and vision now")
                } else {
                    format!("Baron em {secs}s — monte smite e visão agora")
                },
            });
        }
        if secs == 0 {
            out.push(RuleResult {
                id: "baron_now",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 120,
                message: t!(lang,
                    "Baron disponível — coordene com o time",
                    "Baron is up — coordinate with your team").into(),
            });
        }
    }

    // ── Larvas do Vazio (Voidgrubs) ───────────────────────────
    {
        let vg_secs = 300u32.saturating_sub(ctx.game_time);

        if (25..=55).contains(&vg_secs) {
            let msg = match ctx.player_role.as_str() {
                "JUNGLE" => t!(lang,
                    "Larvas do Vazio em 30s — decidir: Larvas (pressão de lane) ou Drake (buff). Larvas estão na Baron side",
                    "Voidgrubs in 30s — decide: Voidgrubs (lane pressure) or Dragon (team buff). Voidgrubs are on Baron side"),
                "TOP"    => t!(lang,
                    "Larvas do Vazio em 30s no Baron side — avise o jungle se pode ajudar a pegar",
                    "Voidgrubs in 30s on Baron side — tell jungle if you can help take them"),
                _        => t!(lang,
                    "Larvas do Vazio em 30s — Drake e Larvas spawnam juntos. Jungle decide a prioridade",
                    "Voidgrubs in 30s — Dragon and Voidgrubs spawn together. Jungle decides priority"),
            };
            out.push(RuleResult {
                id: "voidgrubs_soon",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 150,
                message: msg.into(),
            });
        }

        if vg_secs == 0 && ctx.game_time < 885 {
            let msg = match ctx.player_role.as_str() {
                "JUNGLE" => t!(lang,
                    "Larvas do Vazio disponíveis — são 3 na Baron side. Cada uma dá stack de Voidmite que empurra suas lanes",
                    "Voidgrubs are up — 3 on Baron side. Each one gives a Voidmite stack that passively pushes your lanes"),
                "TOP"    => t!(lang,
                    "Larvas do Vazio disponíveis no Baron side — você pode assistir o jungle agora",
                    "Voidgrubs up on Baron side — you can help the jungle take them now"),
                _        => t!(lang,
                    "Larvas do Vazio disponíveis — objetivo na Baron side. Jungle define prioridade vs Drake",
                    "Voidgrubs are up — objective on Baron side. Jungle sets priority vs Dragon"),
            };
            out.push(RuleResult {
                id: "voidgrubs_available",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 300,
                message: msg.into(),
            });
        }
    }

    // ── Herald ────────────────────────────────────────────────
    if !ctx.herald_killed && (870..=1185).contains(&ctx.game_time) {
        let herald_secs = 900u32.saturating_sub(ctx.game_time);

        if (1..=35).contains(&herald_secs) {
            out.push(RuleResult {
                id: "herald_30s",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 300,
                message: if lang == "en-US" {
                    format!("Herald in {herald_secs}s — position your team. Despawns after Baron (20:00)")
                } else {
                    format!("Herald em {herald_secs}s — posicione o time. Após Baron (20:00) o Herald despawna")
                },
            });
        }

        if herald_secs == 0 {
            out.push(RuleResult {
                id: "herald_available",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 240,
                message: t!(lang,
                    "Herald disponível (15:00–19:45) — use para derrubar torre antes do Baron spawnar",
                    "Herald is up (15:00–19:45) — use it to take a tower before Baron spawns").into(),
            });
        }
    }
}

fn evaluate_vision_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    let ward_score = ctx
        .pattern
        .as_ref()
        .map(|p| p.ward_score)
        .unwrap_or(0.0);

    if ward_score < 0.3 {
        let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
        let drake_soon  = dragon_secs <= 90;

        if drake_soon {
            out.push(RuleResult {
                id: "low_ward_pre_drake",
                category: "VISION",
                severity: "WARNING",
                cooldown: 180,
                message: if lang == "en-US" {
                    format!("Your ward score is low ({:.0}%) — ward the river before the dragon", ward_score * 100.0)
                } else {
                    format!("Seu ward score é baixo ({:.0}%) — ward o rio antes do dragão", ward_score * 100.0)
                },
            });
        }

        if ctx.game_time > 300 {
            out.push(RuleResult {
                id: "low_ward_reminder",
                category: "VISION",
                severity: "INFO",
                cooldown: 120,
                message: t!(lang,
                    "Coloque wards — visão reduz mortes desnecessárias",
                    "Place wards — vision reduces unnecessary deaths").into(),
            });
        }
    }

    if let Some(ref p) = ctx.pattern {
        if p.deaths_without_vision > 12 && ctx.game_time > 600 {
            out.push(RuleResult {
                id: "deaths_without_vision",
                category: "VISION",
                severity: "WARNING",
                cooldown: 300,
                message: if lang == "en-US" {
                    format!("{} of your deaths were without vision — ward before advancing", p.deaths_without_vision)
                } else {
                    format!("{} mortes suas foram sem visão — ward antes de avançar", p.deaths_without_vision)
                },
            });
        }
    }
}

fn evaluate_macro_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    let diff = ctx.score_diff();

    if diff >= 5 && ctx.game_time > 300 && ctx.game_time < 1200 {
        out.push(RuleResult {
            id: "ahead_convert",
            category: "MACRO",
            severity: "INFO",
            cooldown: 240,
            message: if lang == "en-US" {
                format!("You are ahead {}-{} — convert into towers and objectives", ctx.ally_score, ctx.enemy_score)
            } else {
                format!("Você está na frente {}-{} — converta em torres e objetivos", ctx.ally_score, ctx.enemy_score)
            },
        });
    }

    if diff <= -5 && ctx.game_time > 300 {
        out.push(RuleResult {
            id: "behind_objectives",
            category: "MACRO",
            severity: "WARNING",
            cooldown: 300,
            message: t!(lang,
                "Você está atrás — evite teamfights e foque em objetivos menores",
                "You are behind — avoid teamfights and focus on smaller objectives").into(),
        });
    }

    if ctx.dragon_enemy >= 4 {
        out.push(RuleResult {
            id: "enemy_dragon_soul",
            category: "MACRO",
            severity: "CRITICAL",
            cooldown: 600,
            message: t!(lang,
                "Inimigo tem alma do dragão — evite lutas abertas",
                "Enemy has dragon soul — avoid open fights").into(),
        });
    }

    if ctx.dragon_ally >= 3 && ctx.dragon_enemy < 3 {
        out.push(RuleResult {
            id: "ally_drake_advantage",
            category: "MACRO",
            severity: "INFO",
            cooldown: 600,
            message: t!(lang,
                "Você domina os drakes — force teamfights agora",
                "You dominate dragons — force teamfights now").into(),
        });
    }
}

fn evaluate_trade_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    let Some(ref pattern) = ctx.pattern else {
        return;
    };

    if pattern.aggression_score > 0.75
        && pattern.avg_deaths_10_15 > 2.5
        && (600..=900).contains(&ctx.game_time)
    {
        out.push(RuleResult {
            id: "aggression_deaths_midgame",
            category: "TRADE",
            severity: "WARNING",
            cooldown: 300,
            message: if lang == "en-US" {
                format!("Careful — you die an average of {:.1}x between min 10-15. Play safe now", pattern.avg_deaths_10_15)
            } else {
                format!("Cuidado — você morre em média {:.1}x entre min 10-15. Jogue seguro agora", pattern.avg_deaths_10_15)
            },
        });
    }

    // lane_to_objectives movido para handle_score_change
}

fn evaluate_positioning_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    let ward_score = ctx.pattern.as_ref().map(|p| p.ward_score).unwrap_or(0.5);
    let is_solo_role = matches!(ctx.player_role.as_str(), "TOP" | "JUNGLE");

    if is_solo_role && ward_score < 0.2 && ctx.game_time > 600 {
        out.push(RuleResult {
            id: "solo_no_vision",
            category: "VISION",
            severity: "WARNING",
            cooldown: 180,
            message: t!(lang,
                "Sem visão na sua rota — recue ou ward antes de avançar",
                "No vision in your lane — back off or ward before advancing").into(),
        });
    }

    if let Some(ref p) = ctx.pattern {
        if p.aggression_score > 0.8 && !ctx.is_ahead && ctx.game_time > 900 {
            out.push(RuleResult {
                id: "aggression_while_behind",
                category: "POSITIONING",
                severity: "WARNING",
                cooldown: 300,
                message: t!(lang,
                    "Você está atrás — sua agressividade pode custar o jogo. Jogue seguro",
                    "You are behind — your aggression can cost you the game. Play safe").into(),
            });
        }
    }
}

// ── Helpers para handle_champion_kill ────────────────────────

/// Tempo de respawn estimado: 10s + 0.5s por minuto de jogo.
fn calc_respawn_secs(game_time: u32) -> u32 {
    10 + game_time / 120
}

/// Converte o campo `position` da Live API em label legível para o alerta.
/// Usa a role como IDENTIDADE da vítima, nunca como localização da morte.
fn role_label(role: &str, lang: &str) -> &'static str {
    if lang == "en-US" {
        match role {
            "TOP"     => "top laner",
            "JUNGLE"  => "jungler",
            "MIDDLE"  => "mid laner",
            "BOTTOM"  => "bot laner",
            "UTILITY" => "support",
            _         => "enemy",
        }
    } else {
        match role {
            "TOP"     => "toplaner",
            "JUNGLE"  => "jungler",
            "MIDDLE"  => "midlaner",
            "BOTTOM"  => "bot laner",
            "UTILITY" => "suporte",
            _         => "inimigo",
        }
    }
}

/// Retorna sugestão de ação baseada em QUEM morreu e QUAL É O ROLE do jogador.
///
/// Regra central: a dica sempre fala da perspectiva do jogador, não da vítima.
///   · Inimigo morreu NA MINHA LANE  → avance na minha lane agora
///   · Inimigo morreu NOUTRA LANE    → ação role-específica (roam, push own lane, gank)
fn push_suggestion(victim_lane: &str, player_role: &str, game_time: u32, lang: &str) -> String {
    let my_lane_norm = match player_role {
        "TOP"                 => "TOP",
        "JUNGLE"              => "JUNGLE",
        "MID" | "MIDDLE"      => "MIDDLE",
        "BOTTOM" | "ADC"      => "BOTTOM",
        "SUPPORT" | "UTILITY" => "BOTTOM",
        _                     => "",
    };

    let killed_in_my_lane = !my_lane_norm.is_empty()
        && (victim_lane == my_lane_norm
            || (my_lane_norm == "BOTTOM" && victim_lane == "UTILITY"));

    if lang == "en-US" {
        let phase = if game_time >= 1200 { "force the inhibitor" }
                    else if game_time >= 600 { "force the tower" }
                    else { "pressure the wave" };

        if killed_in_my_lane {
            let my_label = match my_lane_norm {
                "TOP"    => "top",
                "MIDDLE" => "mid",
                "BOTTOM" => "bot",
                "JUNGLE" => "enemy jungle",
                _        => "your lane",
            };
            format!("push {} and {}", my_label, phase)
        } else {
            let killed_label = match victim_lane {
                "TOP"               => "top",
                "MIDDLE"            => "mid",
                "BOTTOM" | "UTILITY" => "bot",
                "JUNGLE"            => "jungle",
                _                   => "another lane",
            };
            match player_role {
                "MID" | "MIDDLE" => {
                    if game_time < 900 {
                        format!("shove the wave and rotate to {} while the enemy respawns", killed_label)
                    } else {
                        format!("push mid and group up at {}", killed_label)
                    }
                }
                "JUNGLE" => {
                    let adjacent = match victim_lane {
                        "TOP"                => "mid or bot",
                        "MIDDLE"             => "top or bot",
                        "BOTTOM" | "UTILITY" => "top or mid",
                        _                    => "another lane",
                    };
                    if game_time < 900 {
                        format!("invade the enemy jungle or gank {} while {} respawns", adjacent, killed_label)
                    } else {
                        format!("take the nearest objective or pressure {} while enemy respawns", adjacent)
                    }
                }
                "TOP" => format!("push top and {} while enemy is dead in {}", phase, killed_label),
                "BOTTOM" | "ADC" => format!("push bot and {} while enemy is dead in {}", phase, killed_label),
                "SUPPORT" | "UTILITY" => {
                    if game_time < 900 {
                        format!("create vision and pressure {} with your ADC", killed_label)
                    } else {
                        format!("push bot and {} while enemy is dead", phase)
                    }
                }
                _ => format!("push your lane and {}", phase),
            }
        }
    } else {
        let phase = if game_time >= 1200 { "force inibidor" }
                    else if game_time >= 600 { "force torre" }
                    else { "pressione a onda" };

        if killed_in_my_lane {
            let my_label = match my_lane_norm {
                "TOP"    => "top",
                "MIDDLE" => "mid",
                "BOTTOM" => "bot",
                "JUNGLE" => "jungle inimigo",
                _        => "sua lane",
            };
            format!("avance no {} e {}", my_label, phase)
        } else {
            let killed_label = match victim_lane {
                "TOP"               => "top",
                "MIDDLE"            => "mid",
                "BOTTOM" | "UTILITY" => "bot",
                "JUNGLE"            => "jungle",
                _                   => "outra rota",
            };
            match player_role {
                "MID" | "MIDDLE" => {
                    if game_time < 900 {
                        format!("empurre a wave e rotacione para o {} enquanto inimigo respawna", killed_label)
                    } else {
                        format!("empurre a mid e agrupe no {}", killed_label)
                    }
                }
                "JUNGLE" => {
                    let adjacent = match victim_lane {
                        "TOP"                => "mid ou bot",
                        "MIDDLE"             => "top ou bot",
                        "BOTTOM" | "UTILITY" => "top ou mid",
                        _                    => "outra lane",
                    };
                    if game_time < 900 {
                        format!("invada o jungle inimigo ou ganke o {} enquanto {} respawna", adjacent, killed_label)
                    } else {
                        format!("force objetivo próximo ou pressione o {} enquanto inimigo respawna", adjacent)
                    }
                }
                "TOP" => format!("avance no top e {} enquanto inimigo está morto no {}", phase, killed_label),
                "BOTTOM" | "ADC" => format!("avance no bot e {} enquanto inimigo está morto no {}", phase, killed_label),
                "SUPPORT" | "UTILITY" => {
                    if game_time < 900 {
                        format!("crie visão e pressione o {} com o ADC", killed_label)
                    } else {
                        format!("avance no bot e {} enquanto inimigo está morto", phase)
                    }
                }
                _ => format!("empurre sua lane e {}", phase),
            }
        }
    }
}

/// Retorna o objetivo principal para converter a vantagem, baseado no tempo de jogo.
fn objective_from_time(game_time: u32, lang: &str) -> &'static str {
    if lang == "en-US" {
        if      game_time < 300  { "lane pressure" }
        else if game_time < 600  { "scuttler or vision" }
        else if game_time < 900  { "dragon or tower" }
        else if game_time < 1200 { "tower or herald" }
        else                     { "baron or inhibitor" }
    } else {
        if      game_time < 300  { "pressão de lane" }
        else if game_time < 600  { "scuttler ou visão" }
        else if game_time < 900  { "drake ou torre" }
        else if game_time < 1200 { "torre ou herald" }
        else                     { "baron ou inibidor" }
    }
}

/// Retorna dica defensiva contextualizada ao role, lane onde aliado morreu e inventário do jogador.
/// Se não tem ward disponível, sugere recuar e esperar — nunca pede ward que não existe.
fn defensive_hint(player_role: &str, victim_lane: &str, game_time: u32, has_ward: bool, lang: &str) -> &'static str {
    let is_jungle = player_role == "JUNGLE";

    if is_jungle {
        if has_ward {
            if game_time < 900 {
                t!(lang,
                   "ward a rota adjacente e evite overextend",
                   "ward the adjacent lane and avoid overextending")
            } else {
                t!(lang,
                   "ward e reposicione antes de reiniciar pressão",
                   "ward and reposition before resuming pressure")
            }
        } else {
            if game_time < 900 {
                t!(lang,
                   "recue para posição segura — sem visão disponível no momento",
                   "fall back to a safe position — no vision available right now")
            } else {
                t!(lang,
                   "reposicione e defenda até ter mais informação",
                   "reposition and defend until you have more information")
            }
        }
    } else {
        match victim_lane {
            "TOP" => {
                if has_ward {
                    if game_time < 900 {
                        t!(lang,
                           "cuidado com roam do killer — ward o mid e recue se necessário",
                           "watch for killer roam — ward mid and back off if needed")
                    } else {
                        t!(lang,
                           "ward antes de avançar no top — killer pode rotacionar",
                           "ward before advancing top — killer may rotate")
                    }
                } else {
                    if game_time < 900 {
                        t!(lang,
                           "recue para a torre — sem ward, não avance sem informação",
                           "back to tower — no ward available, don't advance without info")
                    } else {
                        t!(lang,
                           "recue e defenda o top até ter visão",
                           "fall back and defend top until you have vision")
                    }
                }
            }
            "BOTTOM" | "UTILITY" => {
                if has_ward {
                    if game_time < 900 {
                        t!(lang,
                           "cuidado com roam da bot — ward o mid e segure posição",
                           "watch for bot roam — ward mid and hold your position")
                    } else {
                        t!(lang,
                           "ward o bot e evite avançar sem visão",
                           "ward bot and avoid pushing without vision")
                    }
                } else {
                    if game_time < 900 {
                        t!(lang,
                           "recue para a torre — sem ward, não segure posição adiantada",
                           "back to tower — no ward, don't hold an advanced position")
                    } else {
                        t!(lang,
                           "recue e aguarde reabastecimento — evite bot sem visão",
                           "back off and wait to restock — avoid bot without vision")
                    }
                }
            }
            "MIDDLE" => {
                if has_ward {
                    if game_time < 900 {
                        t!(lang,
                           "mid está perigoso — ward os flancos e recue se estiver exposto",
                           "mid is dangerous — ward the flanks and back off if exposed")
                    } else {
                        t!(lang,
                           "recolha para posição segura e ward antes de avançar",
                           "fall back to a safe position and ward before advancing")
                    }
                } else {
                    if game_time < 900 {
                        t!(lang,
                           "recue para a torre — mid perigoso sem visão disponível",
                           "back to tower — mid is dangerous with no vision available")
                    } else {
                        t!(lang,
                           "recue e espere comprar ward antes de avançar no mid",
                           "fall back and wait to buy wards before advancing mid")
                    }
                }
            }
            _ => {
                if has_ward {
                    if game_time < 900 {
                        t!(lang,
                           "ward e recue — possível rotação ou gank iminente",
                           "ward and back off — possible rotation or incoming gank")
                    } else {
                        t!(lang,
                           "ward e reposicione antes de avançar",
                           "ward and reposition before advancing")
                    }
                } else {
                    if game_time < 900 {
                        t!(lang,
                           "recue imediatamente — sem visão e inimigo vivo sem posição conhecida",
                           "back off immediately — no vision and enemy alive at unknown position")
                    } else {
                        t!(lang,
                           "recue até ter mais informação — não avance sem visão",
                           "back off until you have more information — don't advance without vision")
                    }
                }
            }
        }
    }
}

// ── Parsers de eventos OCR de spell ──────────────────────────

/// Extrai a tecla do spell do jogador.
/// Ex: "Flash_D aliado em cooldown" → "Flash (D)"
fn parse_player_spell_key(event: &str) -> String {
    // Formato: "<SpellName>_<key> aliado ..."
    let part = event.split_whitespace().next().unwrap_or("");
    let mut split = part.rsplitn(2, '_');
    let key  = split.next().unwrap_or("?");
    let name = split.next().unwrap_or("Spell");
    format!("{name} ({key})")
}

/// Extrai slot e tecla de um evento de spell inimiga.
/// Ex: "inimigo_3_D em cooldown" → ("3", "D")
fn parse_enemy_spell(event: &str) -> (&'static str, String) {
    let part = event.split_whitespace().next().unwrap_or("");
    // part = "inimigo_3_D"
    let segments: Vec<&str> = part.split('_').collect();
    let slot = match segments.get(1).copied().unwrap_or("?") {
        "1" => "1", "2" => "2", "3" => "3", "4" => "4", "5" => "5", _ => "?",
    };
    let key = segments.get(2).copied().unwrap_or("?").to_string();
    (slot, key)
}

fn evaluate_bot_lane_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    let is_adc     = matches!(ctx.player_role.as_str(), "BOTTOM" | "ADC");
    let is_support = matches!(ctx.player_role.as_str(), "SUPPORT" | "UTILITY");

    if !is_adc && !is_support {
        return;
    }

    if ctx.game_time > 900 {
        return;
    }

    if is_adc && !ctx.support_in_lane && ctx.game_time >= 120 {
        out.push(RuleResult {
            id: "adc_support_absent",
            category: "VISION",
            severity: "WARNING",
            cooldown: 90,
            message: t!(lang,
                "Suporte não está na lane — ward o arbusto e recue se necessário",
                "Support is not in lane — ward the bush and back off if needed").into(),
        });
    }

    if is_support && ctx.support_in_lane {
        if (60..=90).contains(&ctx.game_time) {
            out.push(RuleResult {
                id: "sup_bush_lvl1",
                category: "POSITIONING",
                severity: "INFO",
                cooldown: 120,
                message: t!(lang,
                    "Ocupe o arbusto da lane agora — nega visão inimiga e garante nível 2 primeiro",
                    "Control the lane bush now — denies enemy vision and secures level 2 first").into(),
            });
        }

        if (95..=115).contains(&ctx.game_time) {
            out.push(RuleResult {
                id: "sup_bush_lvl2",
                category: "TRADE",
                severity: "INFO",
                cooldown: 120,
                message: t!(lang,
                    "Você chega ao nível 2 agora — engaje pelo arbusto antes do inimigo",
                    "You hit level 2 now — engage from the bush before the enemy does").into(),
            });
        }

        if (175..=210).contains(&ctx.game_time) {
            out.push(RuleResult {
                id: "sup_bush_scuttler",
                category: "VISION",
                severity: "INFO",
                cooldown: 120,
                message: t!(lang,
                    "Scuttler em breve — ward a bush do rio antes dos 3:30 para visão do drake",
                    "Scuttler soon — ward the river bush before 3:30 for dragon vision").into(),
            });
        }

        if (255..=295).contains(&ctx.game_time) {
            out.push(RuleResult {
                id: "sup_bush_drake_prep",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 120,
                message: t!(lang,
                    "Drake se aproxima — controle a bush do pit e ward o lado inimigo do rio",
                    "Dragon approaching — control the pit bush and ward the enemy side of the river").into(),
            });
        }

        if ctx.game_time > 90 && ctx.game_time < 600 {
            let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
            let pre_obj = dragon_secs <= 120;
            if pre_obj {
                out.push(RuleResult {
                    id: "sup_bush_pre_obj",
                    category: "VISION",
                    severity: "WARNING",
                    cooldown: 150,
                    message: t!(lang,
                        "Objetivo se aproxima — domine a bush do rio e force o inimigo a jogar sem visão",
                        "Objective approaching — control the river bush and force the enemy to play without vision").into(),
                });
            }
        }
    }
}

fn evaluate_opp_jungle_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    if !ctx.enemy_in_opp_jungle {
        return;
    }

    if ctx.game_time > 1200 {
        return;
    }

    let role = ctx.player_role.as_str();

    if ctx.game_time < 480 {
        let msg = if lang == "en-US" {
            match role {
                "TOP"                 => "Enemy jungle is away — push your lane and force a trade now",
                "BOTTOM" | "ADC"      => "Enemy jungle is away — push bot and be aggressive",
                "SUPPORT" | "UTILITY" => "Enemy jungle is away — control the bush and pressure bot",
                "MID"                 => "Enemy jungle is away — push mid and roam if you have an advantage",
                "JUNGLE"              => "Enemy jungle is on the opposite side — invade and steal camps now",
                _                     => return,
            }
        } else {
            match role {
                "TOP"                 => "Jungle inimigo longe — avance na rota e force um trade agora",
                "BOTTOM" | "ADC"      => "Jungle inimigo longe — avance na bot e seja agressivo",
                "SUPPORT" | "UTILITY" => "Jungle inimigo longe — domine a bush e crie pressão na bot",
                "MID"                 => "Jungle inimigo longe — avance na mid e roame se tiver vantagem",
                "JUNGLE"              => "Jungle inimigo no lado oposto — invada o jungle dele e roube camps agora",
                _                     => return,
            }
        };
        out.push(RuleResult {
            id: "opp_jungle_early",
            category: "TRADE",
            severity: "INFO",
            cooldown: 60,
            message: msg.to_string(),
        });
        return;
    }

    if ctx.game_time < 900 {
        let msg = if lang == "en-US" {
            match role {
                "TOP"                 => "Enemy jungle is bot side — pressure top and force tower or herald",
                "BOTTOM" | "ADC"      => "Enemy jungle is top side — push bot and force a tower now",
                "SUPPORT" | "UTILITY" => "Enemy jungle is top side — deep ward and force an engage in bot",
                "MID"                 => "Enemy jungle is away — push mid and roam to create pressure",
                "JUNGLE"              => "Enemy jungle is on the opposite side — invade, steal objectives and create pressure",
                _                     => return,
            }
        } else {
            match role {
                "TOP"                 => "Jungle inimigo no bot — pressione o topo e force torre ou herald",
                "BOTTOM" | "ADC"      => "Jungle inimigo no topo — empurre a bot e force torre agora",
                "SUPPORT" | "UTILITY" => "Jungle inimigo no topo — ward profundo e force engajamento na bot",
                "MID"                 => "Jungle inimigo longe — empurre a mid e roame para criar pressão",
                "JUNGLE"              => "Jungle inimigo no lado oposto — invada, roube objetivos e crie pressão",
                _                     => return,
            }
        };
        out.push(RuleResult {
            id: "opp_jungle_mid",
            category: "MACRO",
            severity: "INFO",
            cooldown: 90,
            message: msg.to_string(),
        });
        return;
    }

    let msg = if lang == "en-US" {
        match role {
            "TOP"                 => "Enemy jungle is away — convert your top lead into tower or baron",
            "BOTTOM" | "ADC"      => "Enemy jungle is top side — push bot and force inhibitor or dragon",
            "SUPPORT" | "UTILITY" => "Enemy jungle is top side — deep ward and set up an objective in bot",
            "MID"                 => "Enemy jungle is away — force the nearest objective now",
            "JUNGLE"              => "Enemy jungle is away — take Baron or Dragon before they come back",
            _                     => return,
        }
    } else {
        match role {
            "TOP"                 => "Jungle inimigo longe — converta a vantagem no topo em torre ou baron",
            "BOTTOM" | "ADC"      => "Jungle inimigo no topo — empurre o bot e force inibidor ou drake",
            "SUPPORT" | "UTILITY" => "Jungle inimigo no topo — crie visão profunda e prepare objetivo no bot",
            "MID"                 => "Jungle inimigo longe — force o objetivo mais próximo agora",
            "JUNGLE"              => "Jungle inimigo longe — tome o Baron ou Drake antes que ele volte",
            _                     => return,
        }
    };
    out.push(RuleResult {
        id: "opp_jungle_late_early",
        category: "MACRO",
        severity: "WARNING",
        cooldown: 120,
        message: msg.to_string(),
    });
}

fn evaluate_side_push_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    if ctx.game_time < 600 {
        return;
    }

    let diff = ctx.score_diff();
    let is_side_pusher = matches!(ctx.player_role.as_str(), "TOP" | "BOTTOM" | "ADC" | "MID");

    if diff >= 3 && is_side_pusher {
        let msg = if lang == "en-US" {
            if ctx.game_time >= 900 {
                "You are ahead — push the side lane and force a tower or inhibitor".to_string()
            } else {
                "You are ahead — pressure the side lane while you have the advantage".to_string()
            }
        } else if ctx.game_time >= 900 {
            "Você está na frente — empurre a side lane e force torre ou inibidor".to_string()
        } else {
            "Você está na frente — pressione a side lane enquanto tem vantagem".to_string()
        };
        out.push(RuleResult {
            id: "side_push_advantage",
            category: "MACRO",
            severity: "INFO",
            cooldown: 300,
            message: msg,
        });
    }

    if ctx.dragon_ally >= 2 && ctx.dragon_ally > ctx.dragon_enemy && is_side_pusher {
        let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
        if dragon_secs > 90 {
            out.push(RuleResult {
                id: "side_push_between_drakes",
                category: "MACRO",
                severity: "INFO",
                cooldown: 360,
                message: if lang == "en-US" {
                    format!("Next dragon in {dragon_secs}s — push the side lane and create pressure before the objective")
                } else {
                    format!("Próximo dragão em {dragon_secs}s — empurre a side lane e crie pressão antes do objetivo")
                },
            });
        }
    }

    if is_side_pusher && ctx.is_ahead && ctx.game_time >= 900 {
        let role_msg = if lang == "en-US" {
            match ctx.player_role.as_str() {
                "TOP"            => "TOP with lead — split push top lane to create map pressure",
                "BOTTOM" | "ADC" => "ADC with lead — push bot and force the inhibitor",
                "MID"            => "MID with lead — pressure mid and force central objectives",
                _                => return,
            }
        } else {
            match ctx.player_role.as_str() {
                "TOP"            => "TOP com vantagem — split push no topo para criar pressão de mapa",
                "BOTTOM" | "ADC" => "ADC com vantagem — empurre o bot e force o inibidor",
                "MID"            => "MID com vantagem — pressione a mid e force objetivos centrais",
                _                => return,
            }
        };
        out.push(RuleResult {
            id: "side_push_role",
            category: "MACRO",
            severity: "INFO",
            cooldown: 360,
            message: role_msg.to_string(),
        });
    }
}

// ── Regras específicas de TOP lane ───────────────────────────
//
// Cobertura das fases do TOP lane:
//   Early  (0–8min):    visão de nível 1, anti-gank, wave management
//   Mid    (8–15min):   herald, TP para drakes, freeze vs push
//   Late   (15–20min):  TP para baron, transição para group
//   EndGame (20+min):   controle de baron, split push vs group
fn evaluate_top_lane_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    if ctx.player_role.as_str() != "TOP" {
        return;
    }

    let diff        = ctx.score_diff();
    let ward_score  = ctx.pattern.as_ref().map(|p| p.ward_score).unwrap_or(0.0);
    let tp_eff      = ctx.pattern.as_ref().map(|p| p.tp_efficiency).unwrap_or(1.0);
    let lane_dom    = ctx.pattern.as_ref().map(|p| p.lane_dominance).unwrap_or(0.5);

    let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
    let baron_secs  = match ctx.next_baron_spawn {
        Some(t) => Some(t.saturating_sub(ctx.game_time)),
        None if ctx.game_time >= 1200 => Some(0),
        None => None,
    };

    if ctx.game_time < 65 {
        out.push(RuleResult {
            id: "top_lvl1_tribush",
            category: "VISION",
            severity: "INFO",
            cooldown: 120,
            message: t!(lang,
                "Ocupe o tribush antes dos minions — nega invade e garante visão do rio do topo",
                "Control the tribush before minions — denies invades and secures top river vision").into(),
        });
        return;
    }

    if ward_score < 0.25 && (120..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "top_anti_gank_early",
            category: "VISION",
            severity: "WARNING",
            cooldown: 240,
            message: t!(lang,
                "Ward o rio do topo — sem visão você está exposto a ganks. Mude de posição ou recue",
                "Ward the top river — without vision you are exposed to ganks. Reposition or back off").into(),
        });
    }

    if diff <= -3 && (120..=540).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "top_freeze_behind",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 300,
            message: t!(lang,
                "Você está atrás na lane — congele a wave próximo da sua torre e espere ajuda do jungle",
                "You are behind in lane — freeze the wave near your tower and wait for jungle help").into(),
        });
    }

    if ctx.is_ahead && (180..=480).contains(&ctx.game_time) && dragon_secs > 150 {
        out.push(RuleResult {
            id: "top_shove_recall",
            category: "MACRO",
            severity: "INFO",
            cooldown: 300,
            message: t!(lang,
                "Você está na frente — empurre a wave até a torre e volte para base com segurança",
                "You are ahead — shove the wave to the tower and safely recall to base").into(),
        });
    }

    if !ctx.herald_killed && (870..=1160).contains(&ctx.game_time) {
        if ctx.is_ahead {
            out.push(RuleResult {
                id: "top_herald_push",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 240,
                message: t!(lang,
                    "Herald disponível (15–19min) e você está na frente — coordene com o jungle. Use antes do Baron spawnar às 20min",
                    "Herald is up (15–19min) and you are ahead — coordinate with jungle. Use it before Baron spawns at 20min").into(),
            });
        } else if diff >= -2 {
            out.push(RuleResult {
                id: "top_herald_balanced",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 360,
                message: t!(lang,
                    "Herald disponível — pegar o Herald antes do Baron pode equilibrar o jogo pela top lane",
                    "Herald is up — taking Herald before Baron can swing the game through top lane").into(),
            });
        }
    }

    if tp_eff < 0.40 && (600..=1200).contains(&ctx.game_time) {
        if dragon_secs <= 120 && dragon_secs > 0 {
            out.push(RuleResult {
                id: "top_tp_drake_prep",
                category: "MACRO",
                severity: "INFO",
                cooldown: 300,
                message: t!(lang,
                    "Drake em 2 minutos — empurhe a wave e posicione-se para usar TP na luta do dragão",
                    "Dragon in 2 minutes — shove the wave and position for a TP into the dragon fight").into(),
            });
        }
    }

    if lane_dom > 0.65 && ctx.is_ahead && (600..=1200).contains(&ctx.game_time) {
        let drake_far = dragon_secs > 180;
        if drake_far {
            out.push(RuleResult {
                id: "top_lane_dom_convert",
                category: "MACRO",
                severity: "INFO",
                cooldown: 400,
                message: t!(lang,
                    "Você domina a lane — pressione até a torre ou roame para criar pressão global enquanto tem vantagem",
                    "You dominate the lane — push to the tower or roam to create global pressure while you have the lead").into(),
            });
        }
    }

    if let Some(secs) = baron_secs {
        if secs <= 90 && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "top_tp_baron",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 180,
                message: t!(lang,
                    "Baron iminente — empurhe a wave agora e guarde TP para chegar na luta do Baron",
                    "Baron incoming — shove the wave now and save TP to join the Baron fight").into(),
            });
        }
    }

    if ctx.game_time > 1200 {
        if let Some(secs) = baron_secs {
            if secs == 0 {
                out.push(RuleResult {
                    id: "top_baron_group",
                    category: "OBJECTIVE",
                    severity: "WARNING",
                    cooldown: 120,
                    message: t!(lang,
                        "Baron disponível — group com o time. TOP isolado no split é vulnerável sem baron",
                        "Baron is up — group with your team. Split push without Baron makes you easy to ignore").into(),
                });
            } else if secs > 120 && ctx.is_ahead {
                out.push(RuleResult {
                    id: "top_split_window",
                    category: "MACRO",
                    severity: "INFO",
                    cooldown: 360,
                    message: t!(lang,
                        "Entre objetivos — split push na top lane e force o time inimigo a responder 1v1",
                        "Between objectives — split push top lane and force the enemy team to respond 1v1").into(),
                });
            }
        }
    }

    if tp_eff < 0.25 && ctx.game_time > 900 && diff >= -2 {
        out.push(RuleResult {
            id: "top_tp_impact_reminder",
            category: "MACRO",
            severity: "INFO",
            cooldown: 600,
            message: t!(lang,
                "Seu TP tem baixo impacto histórico — TOP sem TP global é fácil de ignorar. Use TP em fights importantes",
                "Your TP has low historical impact — a TOP without global TP is easy to ignore. Use TP in important fights").into(),
        });
    }
}

// ── Regras específicas de MID lane ───────────────────────────
//
// Filosofia do MID:
//   A mid lane controla o acesso a ambos os objetivos (drake E baron).
//   O MID tem duas responsabilidades: dominar a lane 1v1 e criar pressão
//   global através de roame, rotações e wave management.
//
// Cobertura das fases:
//   Early  (0–8min):    visão bilateral, anti-gank dos dois rios, nível 1
//   Mid    (8–15min):   roame proativo, wave → baron/drake, freeze quando atrás
//   Late   (15–20min):  wave antes de baron, acesso de mid, rotações
//   EndGame (20+min):   controle central do mapa, groupfight vs split
fn evaluate_mid_lane_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    if ctx.player_role.as_str() != "MID" {
        return;
    }

    let diff       = ctx.score_diff();
    let ward_score = ctx.pattern.as_ref().map(|p| p.ward_score).unwrap_or(0.0);
    let roam_freq  = ctx.pattern.as_ref().map(|p| p.roam_frequency).unwrap_or(1.0);
    let lane_dom   = ctx.pattern.as_ref().map(|p| p.lane_dominance).unwrap_or(0.5);
    let tp_eff     = ctx.pattern.as_ref().map(|p| p.tp_efficiency).unwrap_or(1.0);

    let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
    let baron_secs  = match ctx.next_baron_spawn {
        Some(t) => Some(t.saturating_sub(ctx.game_time)),
        None if ctx.game_time >= 1200 => Some(0),
        None => None,
    };

    // ── EARLY (0–70s): visão de pixel antes dos minions ──────
    // O pixel bush é o ponto de visão mais crítico da mid lane:
    // cobre o rio central e previne invades de nível 1 coordenados.
    if ctx.game_time < 70 {
        out.push(RuleResult {
            id: "mid_lvl1_pixel",
            category: "VISION",
            severity: "INFO",
            cooldown: 120,
            message: t!(lang,
                "Ward o pixel bush antes dos minions — protege contra invade e garante visão dos dois lados do rio",
                "Ward the pixel bush before minions — protects against invades and secures vision on both sides of the river").into(),
        });
        return;
    }

    if ward_score < 0.25 && (120..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "mid_anti_gank_bilateral",
            category: "VISION",
            severity: "WARNING",
            cooldown: 240,
            message: t!(lang,
                "Ward ambos os lados do rio — MID está exposto a ganks coordenados sem visão bilateral",
                "Ward both sides of the river — MID is exposed to coordinated ganks without bilateral vision").into(),
        });
    }

    if diff <= -3 && (180..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "mid_freeze_behind",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 300,
            message: t!(lang,
                "Você está atrás — congele a wave próximo da sua torre e farm seguro antes de tentar roame",
                "You are behind — freeze the wave near your tower and farm safely before attempting to roam").into(),
        });
    }

    if roam_freq < 0.25
        && ctx.is_ahead
        && (300..=900).contains(&ctx.game_time)
        && dragon_secs > 90
    {
        out.push(RuleResult {
            id: "mid_roam_proactive",
            category: "MACRO",
            severity: "INFO",
            cooldown: 360,
            message: t!(lang,
                "Seu roam histórico é baixo — empurre a wave e roame para bot ou top antes de voltar para mid",
                "Your historical roam rate is low — shove the wave and roam to bot or top before coming back to mid").into(),
        });
    }

    if ctx.is_ahead
        && (300..=900).contains(&ctx.game_time)
        && dragon_secs > 120
    {
        out.push(RuleResult {
            id: "mid_push_roam_window",
            category: "MACRO",
            severity: "INFO",
            cooldown: 300,
            message: t!(lang,
                "Wave empurrada e sem objetivo iminente — roame para a side lane mais fraca e crie pressão",
                "Wave shoved with no imminent objective — roam to the weakest side lane and create pressure").into(),
        });
    }

    if dragon_secs <= 120 && dragon_secs > 0 && (480..=1200).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "mid_wave_before_drake",
            category: "MACRO",
            severity: "INFO",
            cooldown: 240,
            message: t!(lang,
                "Drake em breve — empurre a wave da mid antes de rotacionar para não perder farm",
                "Dragon soon — shove the mid wave before rotating so you don't lose farm").into(),
        });
    }

    if let Some(secs) = baron_secs {
        if secs <= 120 && secs > 0 && ctx.game_time > 1000 {
            out.push(RuleResult {
                id: "mid_wave_before_baron",
                category: "MACRO",
                severity: "WARNING",
                cooldown: 180,
                message: t!(lang,
                    "Baron iminente — empurre a wave da mid agora. Garante acesso e pressão durante a luta",
                    "Baron incoming — shove the mid wave now. Secures access and pressure during the fight").into(),
            });
        }
    }

    if let Some(secs) = baron_secs {
        if secs <= 60 && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "mid_baron_access",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 150,
                message: t!(lang,
                    "Baron iminente — você é o acesso central. Ward o baron pela mid e coordene com o time",
                    "Baron incoming — you are the central access. Ward the baron side from mid and coordinate with your team").into(),
            });
        }
    }

    if lane_dom > 0.65 && ctx.is_ahead && (480..=1200).contains(&ctx.game_time) {
        let between_objs = dragon_secs > 150
            && baron_secs.map(|s| s > 150).unwrap_or(true);
        if between_objs {
            out.push(RuleResult {
                id: "mid_dom_convert",
                category: "MACRO",
                severity: "INFO",
                cooldown: 360,
                message: t!(lang,
                    "Você domina a mid — converta o domínio em roame e crie vantagem nas side lanes",
                    "You dominate mid — convert that dominance into roams and create advantages in the side lanes").into(),
            });
        }
    }

    if ctx.game_time > 1200 {
        if baron_secs.map(|s| s == 0).unwrap_or(false) {
            out.push(RuleResult {
                id: "mid_baron_group",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 120,
                message: t!(lang,
                    "Baron disponível — group com o time pela mid. MID isolado no split perde influência decisiva",
                    "Baron is up — group with your team through mid. MID isolated in a split loses decisive influence").into(),
            });
        } else if ctx.is_ahead && baron_secs.map(|s| s > 120).unwrap_or(true) {
            out.push(RuleResult {
                id: "mid_split_or_group",
                category: "MACRO",
                severity: "INFO",
                cooldown: 360,
                message: t!(lang,
                    "Entre objetivos — pressione a mid para criar espaço. Não fique parado em base late game",
                    "Between objectives — pressure mid to create space. Don't stand still at base in late game").into(),
            });
        }
    }

    if tp_eff < 0.25 && ctx.game_time > 600 && diff >= -2 {
        out.push(RuleResult {
            id: "mid_tp_impact",
            category: "MACRO",
            severity: "INFO",
            cooldown: 600,
            message: t!(lang,
                "Seu TP tem baixo impacto histórico — use TP para flanquear em teamfights e aumentar seu valor de mapa",
                "Your TP has low historical impact — use TP to flank in teamfights and increase your map value").into(),
        });
    }
}

// ── Regras específicas de SUPPORT ────────────────────────────
//
// Filosofia do SUPPORT:
//   O suporte tem três responsabilidades centrais:
//   1. VISÃO — colocar e limpar wards; é O ward placer do time
//   2. LANE PHASE — dominar a bot lane com controle de bush e timing
//   3. ROAME — transicionar da bot para objetivos e side lanes no mid game
//
// Cobertura das fases:
//   Early  (0–8min):   lane phase já coberta em evaluate_bot_lane_rules
//   Mid    (8–15min):  transição lane → roame, visão de rio, drake
//   Late   (15–20min): baron vision, posicionamento em objective
//   EndGame (20+min):  peeling ADC, visão de base, negar entrada inimiga
fn evaluate_support_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    let is_support = matches!(ctx.player_role.as_str(), "SUPPORT" | "UTILITY");
    if !is_support {
        return;
    }

    let diff       = ctx.score_diff();
    let ward_score = ctx.pattern.as_ref().map(|p| p.ward_score).unwrap_or(0.0);

    let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
    let baron_secs  = match ctx.next_baron_spawn {
        Some(t) => Some(t.saturating_sub(ctx.game_time)),
        None if ctx.game_time >= 1200 => Some(0),
        None => None,
    };

    // ── MID GAME (8–15min): transição lane → roame ────────────
    // O suporte deve sair da bot lane assim que ela estiver estabilizada.
    // Roame para ajudar o jungle, criar visão no rio e participar de drakes.
    if (480..=900).contains(&ctx.game_time) {
        // Roame ativo quando à frente ou empatado sem objetivo iminente
        if diff >= -1 && dragon_secs > 150 && baron_secs.map(|s| s > 150).unwrap_or(true) {
            out.push(RuleResult {
                id: "sup_roam_transition",
                category: "MACRO",
                severity: "INFO",
                cooldown: 300,
                message: t!(lang,
                    "Entre objetivos — rotacione para o rio e ajude o jungle. Suporte deve sair da bot assim que a wave estiver safe",
                    "Between objectives — rotate to the river and help the jungle. Support should leave bot as soon as the wave is safe").into(),
            });
        }
    }

    if ward_score < 0.35 && (480..=900).contains(&ctx.game_time) && diff >= -2 {
        out.push(RuleResult {
            id: "sup_ward_before_roam",
            category: "VISION",
            severity: "INFO",
            cooldown: 240,
            message: t!(lang,
                "Ward o rio antes de roamar — proteja o ADC enquanto você está fora da bot lane",
                "Ward the river before roaming — protect your ADC while you are away from bot lane").into(),
        });
    }

    if (60..=120).contains(&dragon_secs) {
        out.push(RuleResult {
            id: "sup_drake_vision_deny",
            category: "VISION",
            severity: "INFO",
            cooldown: 180,
            message: t!(lang,
                "Drake em 1-2 min — limpe os wards inimigos do pit antes de colocar os seus. Negue a visão deles",
                "Dragon in 1-2 min — clear enemy wards from the pit before placing yours. Deny their vision").into(),
        });
    }

    if (25..=55).contains(&dragon_secs) {
        out.push(RuleResult {
            id: "sup_drake_positioning",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 150,
            message: t!(lang,
                "Drake em 30-55s — posicione-se do lado inimigo do pit. Você nega o engage deles e protege o smite",
                "Dragon in 30-55s — position on the enemy side of the pit. You deny their engage and protect the smite").into(),
        });
    }

    if let Some(secs) = baron_secs {
        if (60..=120).contains(&secs) && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "sup_baron_ward",
                category: "VISION",
                severity: "WARNING",
                cooldown: 150,
                message: t!(lang,
                    "Baron em 1-2 min — coloque ward no pit e no rio. Você é o responsável pela visão do baron",
                    "Baron in 1-2 min — place wards in the pit and river. You are responsible for Baron vision").into(),
            });
        }
        if (20..=55).contains(&secs) && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "sup_baron_positioning",
                category: "POSITIONING",
                severity: "WARNING",
                cooldown: 120,
                message: t!(lang,
                    "Baron em 20-55s — posicione-se do lado deles com CC pronto. Nega o engage e garante a luta",
                    "Baron in 20-55s — position on their side with CC ready. Deny their engage and secure the fight").into(),
            });
        }
    }

    if ctx.game_time > 900 && ctx.is_ahead {
        out.push(RuleResult {
            id: "sup_peel_adc",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 400,
            message: t!(lang,
                "Seu ADC está na frente — fique próximo e proteja em teamfights. Seu CC salva carries, não inicia",
                "Your ADC is ahead — stay close and protect in teamfights. Your CC saves carries, don't initiate").into(),
        });
    }

    if ctx.game_time > 1200 {
        if let Some(secs) = baron_secs {
            if secs > 60 {
                out.push(RuleResult {
                    id: "sup_late_vision_control",
                    category: "VISION",
                    severity: "INFO",
                    cooldown: 360,
                    message: t!(lang,
                        "Late game — mantenha wards no baron, rio e acesso inimigo. Visão ganha mais jogos do que dano no late",
                        "Late game — maintain wards at baron, river and enemy access. Vision wins more games than damage in late").into(),
                });
            }
        }
    }

    if ward_score < 0.25 && ctx.game_time > 300 {
        out.push(RuleResult {
            id: "sup_low_ward_score",
            category: "VISION",
            severity: "WARNING",
            cooldown: 200,
            message: if lang == "en-US" {
                format!("Your ward score is low ({:.0}%) — support is the primary ward placer. Place wards frequently", ward_score * 100.0)
            } else {
                format!("Seu ward score está baixo ({:.0}%) — suporte é o principal ward placer. Coloque wards com frequência", ward_score * 100.0)
            },
        });
    }

    if ctx.game_time > 900 {
        let pre_obj = dragon_secs <= 120 || baron_secs.map(|s| s <= 120).unwrap_or(false);
        if pre_obj {
            out.push(RuleResult {
                id: "sup_sweep_enemy_wards",
                category: "VISION",
                severity: "INFO",
                cooldown: 180,
                message: t!(lang,
                    "Antes do objetivo — use o revelador para limpar wards inimigos. Visão deles facilita o engage",
                    "Before the objective — use your sweeper to clear enemy wards. Their vision enables their engage").into(),
            });
        }
    }
}

// ── Regras específicas de ADC / BOTTOM ───────────────────────
//
// Filosofia do ADC:
//   O ADC é o carry principal de dano físico tardio. Depende
//   do suporte e do jungle para sobreviver no early e tem a
//   responsabilidade de fazer o máximo de DPS em fights seguras.
//   Erros comuns: agressividade excessiva sem suporte, split push
//   no late, mortes evitáveis por má posição em teamfights.
//
// Cobertura das fases:
//   Early  (0–8min):  controle de nível 1, freeze ou pressão com suporte
//   Mid    (8–15min): drakes, posicionamento sem suporte, evitar mortes
//   Late   (15–20min): groupar para baron, ficar vivo
//   EndGame (20+min): backline em teamfights, não split push
fn evaluate_adc_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    let is_adc = matches!(ctx.player_role.as_str(), "BOTTOM" | "ADC");
    if !is_adc { return; }

    let diff       = ctx.score_diff();
    let ward_score = ctx.pattern.as_ref().map(|p| p.ward_score).unwrap_or(0.0);
    let aggression = ctx.pattern.as_ref().map(|p| p.aggression_score).unwrap_or(0.5);
    let avg_deaths = ctx.pattern.as_ref().map(|p| p.avg_deaths_10_15).unwrap_or(0.0);
    let obj_ctrl   = ctx.pattern.as_ref().map(|p| p.objective_control).unwrap_or(0.5);

    let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
    let baron_secs  = match ctx.next_baron_spawn {
        Some(t) => Some(t.saturating_sub(ctx.game_time)),
        None if ctx.game_time >= 1200 => Some(0),
        None => None,
    };

    if ctx.game_time < 65 {
        out.push(RuleResult {
            id: "adc_lvl1_tribush",
            category: "VISION",
            severity: "INFO",
            cooldown: 120,
            message: t!(lang,
                "Controle o tribush junto ao suporte — nega visão inimiga e abre opção de fight de nível 1",
                "Control the tribush with your support — denies enemy vision and opens a level 1 fight option").into(),
        });
        return;
    }

    if diff <= -3 && !ctx.support_in_lane && (120..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_freeze_solo",
            category: "POSITIONING",
            severity: "WARNING",
            cooldown: 240,
            message: t!(lang,
                "Atrás e sem suporte — congele próximo da torre. Não avance para farm arriscado sozinho",
                "Behind and no support — freeze near the tower. Don't push for risky farm alone").into(),
        });
    }

    if diff <= -3 && ctx.support_in_lane && (120..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_freeze_with_sup",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 300,
            message: t!(lang,
                "Atrás na lane — congele a wave e espere uma abertura com o suporte antes de tentar trades",
                "Behind in lane — freeze the wave and wait for an opening with your support before trading").into(),
        });
    }

    if ctx.support_in_lane && ctx.is_ahead && (180..=720).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_exploit_support",
            category: "TRADE",
            severity: "INFO",
            cooldown: 360,
            message: t!(lang,
                "Suporte presente e você está na frente — seja agressivo. ADC com suporte saudável deve pressionar a lane",
                "Support is here and you are ahead — be aggressive. ADC with a healthy support should pressure the lane").into(),
        });
    }

    if !ctx.support_in_lane && ward_score < 0.30 && (300..=720).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_ward_solo",
            category: "VISION",
            severity: "WARNING",
            cooldown: 180,
            message: t!(lang,
                "Sem suporte — ward o tribush e o rio. ADC sozinho sem visão é alvo prioritário de ganks coordenados",
                "No support — ward the tribush and river. Solo ADC without vision is the primary target for coordinated ganks").into(),
        });
    }

    if obj_ctrl < 0.35 && dragon_secs <= 90 && dragon_secs > 0 && ctx.game_time > 300 {
        out.push(RuleResult {
            id: "adc_drake_priority",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 240,
            message: t!(lang,
                "Drake é seu objetivo mais valioso — buffs de drake maximizam o ADC. Sempre participe das lutas de drake",
                "Dragon is your most valuable objective — dragon buffs maximize ADC. Always participate in dragon fights").into(),
        });
    }

    if ctx.dragon_ally >= 2 && ctx.dragon_ally > ctx.dragon_enemy && ctx.game_time > 600 {
        out.push(RuleResult {
            id: "adc_drake_buff_active",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 480,
            message: if lang == "en-US" {
                format!("You dominate dragons ({}-{}) — your damage buffs are a real advantage. Force fights now", ctx.dragon_ally, ctx.dragon_enemy)
            } else {
                format!("Vocês dominam os drakes ({}-{}) — seus buffs de dano são uma vantagem real. Force fights agora", ctx.dragon_ally, ctx.dragon_enemy)
            },
        });
    }

    if aggression > 0.70 && avg_deaths > 2.0 && (600..=900).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_positioning_deaths",
            category: "POSITIONING",
            severity: "WARNING",
            cooldown: 300,
            message: if lang == "en-US" {
                format!("You die {:.1}x on average in mid game — a dead ADC doesn't carry. Prioritize safe positioning over aggression", avg_deaths)
            } else {
                format!("Você morre {:.1}x em média no mid game — ADC morto não carrega. Priorize posicionamento seguro sobre agressividade", avg_deaths)
            },
        });
    }

    if ctx.game_time > 900 && !ctx.is_ahead {
        out.push(RuleResult {
            id: "adc_no_split_push",
            category: "MACRO",
            severity: "INFO",
            cooldown: 400,
            message: t!(lang,
                "ADC não faz split push — seu dano é insubstituível em teamfights. Fique com o grupo no late game",
                "ADC doesn't split push — your damage is irreplaceable in teamfights. Stay with the group in late game").into(),
        });
    }

    if let Some(secs) = baron_secs {
        if secs <= 90 && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "adc_group_baron",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 150,
                message: t!(lang,
                    "Baron iminente — group com o time agora. ADC é o carry da luta; sua presença é insubstituível",
                    "Baron incoming — group with your team now. ADC is the fight carry; your presence is irreplaceable").into(),
            });
        }
    }

    if aggression > 0.75 && ctx.game_time > 1200 {
        out.push(RuleResult {
            id: "adc_survive_late",
            category: "POSITIONING",
            severity: "WARNING",
            cooldown: 400,
            message: t!(lang,
                "Late game — posicione-se atrás do frontline e ataque de longe. Sua vida vale mais que qualquer kill",
                "Late game — position behind the frontline and attack from range. Your life is worth more than any kill").into(),
        });
    }
}

// ── Regras específicas de JUNGLE ─────────────────────────────
//
// Filosofia do Jungle:
//   O jungle controla o ritmo do jogo. Suas decisões de rota,
//   timing de gank e prioridade de objetivo determinam qual time
//   ganha a vantagem de mapa. Erros comuns: gankar feeds sem
//   preparo, perder farm por gankar sem impacto, desperdiçar
//   smite em camps e não preparar baron corretamente.
//
// Cobertura das fases:
//   Early  (0–8min):   scuttler, leitura de lanes, rota clear vs gank
//   Mid    (8–15min):  pressão de objetivos, visão de rio, contrajungle
//   Late   (15–20min): setup de baron, farm quando atrás, snowball
//   EndGame (20+min):  baron/elder gestão, proteção de carry
fn evaluate_jungle_rules(ctx: &ProcContext, lang: &str, out: &mut Vec<RuleResult>) {
    if ctx.player_role.as_str() != "JUNGLE" {
        return;
    }

    let diff      = ctx.score_diff();
    let ward_score = ctx.pattern.as_ref().map(|p| p.ward_score).unwrap_or(0.0);
    let obj_ctrl  = ctx.pattern.as_ref().map(|p| p.objective_control).unwrap_or(0.5);
    let aggression = ctx.pattern.as_ref().map(|p| p.aggression_score).unwrap_or(0.5);

    let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
    let baron_secs  = match ctx.next_baron_spawn {
        Some(t) => Some(t.saturating_sub(ctx.game_time)),
        None if ctx.game_time >= 1200 => Some(0),
        None => None,
    };

    if (60..=90).contains(&ctx.game_time) {
        let ally_ahead = diff >= 0;
        let msg = if lang == "en-US" {
            if ally_ahead {
                "Read the lanes: early advantage → gank level 2-3. Disadvantage → full clear first"
            } else {
                "Do a full clear to level 3-4 before the first gank — smite and level advantage matter"
            }
        } else if ally_ahead {
            "Analise as lanes: vantagem no early → gank nível 2-3. Desvantagem → full clear primeiro"
        } else {
            "Faça o full clear para nível 3-4 antes do primeiro gank — smite e level advantage importam"
        };
        out.push(RuleResult {
            id: "jungle_path_decision",
            category: "MACRO",
            severity: "INFO",
            cooldown: 120,
            message: msg.into(),
        });
    }

    if (170..=215).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "jungle_scuttler_timing",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 120,
            message: t!(lang,
                "Scuttler em 3:30 — garanta o rio agora. Visão e regen antes das Larvas do Vazio e Drake (ambos às 5:00)",
                "Scuttler at 3:30 — secure the river now. Vision and regen before Voidgrubs and Dragon (both at 5:00)").into(),
        });
    }

    {
        let vg_secs = 300u32.saturating_sub(ctx.game_time);
        if (20..=70).contains(&vg_secs) {
            out.push(RuleResult {
                id: "jungle_voidgrubs_decision",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 120,
                message: t!(lang,
                    "Larvas do Vazio em ~30s (Baron side) — Drake também spawna agora. Decida: Larvas (pressão de torre) ou Drake (buff de dano)",
                    "Voidgrubs in ~30s (Baron side) — Dragon also spawns now. Decide: Voidgrubs (tower pressure) or Dragon (damage buff)").into(),
            });
        }
        if vg_secs == 0 && ctx.game_time < 600 {
            out.push(RuleResult {
                id: "jungle_voidgrubs_take",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 240,
                message: t!(lang,
                    "Larvas do Vazio disponíveis na Baron side — cada stack de Voidmite empurra suas lanes passivamente",
                    "Voidgrubs are up on Baron side — each Voidmite stack passively pushes your lanes").into(),
            });
        }
    }

    if ward_score < 0.25 && (180..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "jungle_ward_river",
            category: "VISION",
            severity: "INFO",
            cooldown: 240,
            message: t!(lang,
                "Ward o rio ao passar — jungle sem visão de rio perde rastreamento do inimigo e abre contra-ganks",
                "Ward the river as you pass — jungle without river vision loses enemy tracking and opens counter-ganks").into(),
        });
    }

    if (dragon_secs > 0 && dragon_secs <= 60)
        || baron_secs.map(|s| s > 0 && s <= 60).unwrap_or(false)
    {
        out.push(RuleResult {
            id: "jungle_smite_preserve",
            category: "OBJECTIVE",
            severity: "WARNING",
            cooldown: 90,
            message: t!(lang,
                "Objetivo iminente — não use smite em camps agora. Guarde para garantir o objetivo neutro",
                "Objective incoming — don't use smite on camps now. Save it to secure the neutral objective").into(),
        });
    }

    if diff <= -5 && ctx.game_time < 1200 {
        out.push(RuleResult {
            id: "jungle_farm_behind",
            category: "MACRO",
            severity: "WARNING",
            cooldown: 400,
            message: t!(lang,
                "Você está muito atrás — faça farm full em vez de gankar. Farm de jungle volta golpe por golpe no late",
                "You are very behind — full farm instead of ganking. Jungle farm claws back advantage in late game").into(),
        });
    }

    if obj_ctrl < 0.35 && dragon_secs <= 120 && dragon_secs > 0 && ctx.game_time > 300 {
        out.push(RuleResult {
            id: "jungle_obj_ctrl_reminder",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 300,
            message: t!(lang,
                "Seu controle de objetivos histórico é baixo — priorize esse drake. Objetivos vencem partidas, ganks não",
                "Your historical objective control is low — prioritize this dragon. Objectives win games, ganks don't").into(),
        });
    }

    if diff >= 4 && ctx.game_time < 1200 && dragon_secs > 120 {
        out.push(RuleResult {
            id: "jungle_snowball",
            category: "MACRO",
            severity: "INFO",
            cooldown: 360,
            message: t!(lang,
                "Você está na frente — invada o jungle inimigo ou crie pressão nas lanes. Não deixe o jogo se equilibrar",
                "You are ahead — invade the enemy jungle or create lane pressure. Don't let the game equalize").into(),
        });
    }

    if ctx.enemy_in_opp_jungle && (480..=1200).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "jungle_steal_buff",
            category: "TRADE",
            severity: "INFO",
            cooldown: 90,
            message: t!(lang,
                "Jungle inimigo visível no lado oposto — invada e roube o buff ou camp. Acumule vantagem de recursos",
                "Enemy jungle spotted on the opposite side — invade and steal the buff or camp. Stack up resource advantages").into(),
        });
    }

    if let Some(secs) = baron_secs {
        if (90..=180).contains(&secs) && ctx.game_time > 1050 {
            out.push(RuleResult {
                id: "jungle_baron_setup",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 180,
                message: t!(lang,
                    "Baron em 1-3 min — orquestre o setup: empurre mid com o time, ward o pit e garanta smite",
                    "Baron in 1-3 min — orchestrate the setup: push mid with your team, ward the pit and secure smite").into(),
            });
        }
    }

    if aggression > 0.75 && ctx.game_time > 1200 && !ctx.is_ahead {
        out.push(RuleResult {
            id: "jungle_aggressive_late",
            category: "POSITIONING",
            severity: "WARNING",
            cooldown: 400,
            message: t!(lang,
                "Late game sem vantagem — evite invades solo no jungle inimigo. Um erro de jungle late pode custar o jogo",
                "Late game without lead — avoid solo invades in the enemy jungle. One late-game jungle mistake can lose the game").into(),
        });
    }

    if (ctx.dragon_ally == 3 || ctx.dragon_enemy == 3)
        && dragon_secs <= 90
        && dragon_secs > 0
    {
        let (who_pt, who_en, urgency) = if ctx.dragon_ally == 3 {
            ("Você está a 1 drake da alma", "You are 1 dragon from soul", "CRITICAL")
        } else {
            ("Inimigo está a 1 drake da alma", "Enemy is 1 dragon from soul", "CRITICAL")
        };
        out.push(RuleResult {
            id: "jungle_soul_drake",
            category: "OBJECTIVE",
            severity: urgency,
            cooldown: 120,
            message: if lang == "en-US" {
                format!("{who_en} — this dragon is top priority. Secure smite and ward the pit now")
            } else {
                format!("{who_pt} — esse drake é prioridade máxima. Garanta smite e ward o pit agora")
            },
        });
    }
}

/// Remove duplicatas preservando a ordem de inserção.
fn dedup(mut v: Vec<&'static str>) -> Vec<&'static str> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|s| seen.insert(*s));
    v
}
