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
}

impl ProcEngine {
    pub fn new() -> Self {
        Self { last_alert_at: HashMap::new() }
    }

    /// Avalia todas as regras contra o contexto atual.
    /// Retorna alertas prontos para emissão Tauri.
    pub fn evaluate(&mut self, ctx: &ProcContext) -> Vec<CoachAlert> {
        let mut raw: Vec<RuleResult> = Vec::new();

        evaluate_objective_rules(ctx, &mut raw);
        evaluate_vision_rules(ctx, &mut raw);
        evaluate_macro_rules(ctx, &mut raw);
        evaluate_trade_rules(ctx, &mut raw);
        evaluate_positioning_rules(ctx, &mut raw);
        evaluate_side_push_rules(ctx, &mut raw);
        evaluate_bot_lane_rules(ctx, &mut raw);
        evaluate_opp_jungle_rules(ctx, &mut raw);
        evaluate_top_lane_rules(ctx, &mut raw);
        evaluate_mid_lane_rules(ctx, &mut raw);
        evaluate_support_rules(ctx, &mut raw);
        evaluate_adc_rules(ctx, &mut raw);
        evaluate_jungle_rules(ctx, &mut raw);

        // Separa filter e map para evitar conflito de borrow em &mut self
        let ready: Vec<RuleResult> = raw
            .into_iter()
            .filter(|r| self.cooldown_ok(r.id, r.cooldown))
            .collect();

        ready.into_iter()
            .map(|r| {
                self.last_alert_at.insert(r.id, Instant::now());

                tracing::debug!(
                    "[Proc] alerta [{}/{}] ({}): {}",
                    r.severity, r.category, r.id, r.message
                );

                CoachAlert {
                    id:        Uuid::new_v4().to_string(),
                    category:  r.category.to_string(),
                    severity:  r.severity.to_string(),
                    message:   r.message,
                    timestamp: ctx.game_time as i64,
                }
            })
            .collect()
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
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();
        let is_jungle = player_role == "JUNGLE";

        // ── Spell DO JOGADOR caiu → jogar defensivo ───────────
        if event.contains("aliado em cooldown") {
            let key = parse_player_spell_key(event);
            let msg = format!("Seu {} caiu — evite posições expostas por enquanto", key);
            if let Some(a) = self.make_alert(
                "own_spell_used", "POSITIONING", "INFO",
                &msg, game_time, 30,
            ) { alerts.push(a); }
        }

        // ── Spell DO JOGADOR voltou → pode engajar ────────────
        if event.contains("aliado disponível") {
            let key = parse_player_spell_key(event);
            let msg = format!("Seu {} voltou — janela para engajar ou tomar trade", key);
            if let Some(a) = self.make_alert(
                "own_spell_ready", "TRADE", "INFO",
                &msg, game_time, 30,
            ) { alerts.push(a); }
        }

        // ── Spell INIMIGA caiu → janela de agressividade ──────
        if event.contains("inimigo_") && event.ends_with("em cooldown") {
            let (slot, key) = parse_enemy_spell(event);

            if is_jungle {
                // Jungle: spell inimiga caindo = janela concreta de gank
                let msg = format!(
                    "Inimigo {} sem {} — janela de gank nessa rota agora",
                    slot, key
                );
                if let Some(a) = self.make_alert(
                    "jungle_gank_spell_window", "TRADE", "WARNING",
                    &msg, game_time, 45,
                ) { alerts.push(a); }
            } else {
                let msg = format!("Inimigo {} gastou {} — janela para ser agressivo", slot, key);
                if let Some(a) = self.make_alert(
                    "enemy_spell_used", "TRADE", "WARNING",
                    &msg, game_time, 15,
                ) { alerts.push(a); }
            }
        }

        // ── Spell INIMIGA voltou → cuidado com engajes ────────
        if event.contains("inimigo_") && event.ends_with("disponível") {
            let (slot, key) = parse_enemy_spell(event);
            let msg = if is_jungle {
                format!("{} do inimigo {} voltou — evite gankar essa rota por enquanto", key, slot)
            } else {
                format!("{} do inimigo {} voltou — respeite o engage", key, slot)
            };
            if let Some(a) = self.make_alert(
                "enemy_spell_ready", "POSITIONING", "INFO",
                &msg, game_time, 20,
            ) { alerts.push(a); }
        }

        // ── Smite caiu com objetivo disponível → risco de smite ─
        if is_jungle && smite_on_cd && objective_near
            && event.contains("aliado em cooldown")
        {
            if let Some(a) = self.make_alert(
                "smite_cd_objective", "OBJECTIVE", "CRITICAL",
                "Seu smite caiu com objetivo próximo — não force a disputa agora",
                game_time, 30,
            ) { alerts.push(a); }
        }

        // ── Ambas as spells voltaram → janela de plays grandes ─
        // Dispara quando o spell que voltou completa o par (o outro já estava disponível)
        if event.contains("aliado disponível") && other_spell_available
            && game_time > 300
        {
            let msg = if is_jungle {
                "Ambas suas spells estão disponíveis — janela ideal para gank decisivo"
            } else {
                "Suas duas spells estão disponíveis — janela para play de alto impacto"
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
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        // Mensagens de kill individuais vêm de handle_champion_kill (ChampionKill event).
        // Aqui mantemos apenas enrichments de padrão comportamental do jogador.
        if new_ally > prev_ally {
            if let Some(p) = pattern {
                if p.lane_dominance > 0.7 && p.objective_control < 0.35 && game_time > 600 {
                    if let Some(a) = self.make_alert(
                        "lane_to_objectives", "TRADE", "INFO",
                        "Você domina a lane — roame e converta em objetivos",
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
                        "Empurre a wave e roame — morte inimiga é sua janela de pressão no mapa",
                        game_time, 300,
                    ) { alerts.push(a); }
                }
            }
        }

        alerts
    }

    /// Coaching específico de kill com nome do campeão, vítima e lane onde ocorreu.
    /// Chamado a partir de eventos ChampionKill da Live API — substitui mensagens genéricas.
    pub fn handle_champion_kill(
        &mut self,
        killer_champ: &str,
        victim_champ:  &str,
        victim_lane:   &str,   // posição da vítima: TOP, JUNGLE, MIDDLE, BOTTOM, UTILITY
        is_ally_kill:  bool,
        game_time:     u32,
        player_role:   &str,
        pattern:       Option<&PlayerPattern>,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        let lane_label = match victim_lane {
            "TOP"             => "Top",
            "JUNGLE"          => "Jungle",
            "MIDDLE"          => "Mid",
            "BOTTOM"          => "Bot",
            "UTILITY"         => "Bot",
            _                 => "",
        };

        let has_names = !killer_champ.is_empty() && !victim_champ.is_empty();

        if is_ally_kill {
            let push_hint   = push_suggestion(victim_lane, player_role, game_time);
            let obj_hint    = objective_from_time(game_time);

            let msg = if has_names && !lane_label.is_empty() {
                format!(
                    "Seu aliado {} eliminou {} no {} — {} e converta em {}",
                    killer_champ, victim_champ, lane_label, push_hint, obj_hint
                )
            } else if has_names {
                format!(
                    "Seu aliado {} eliminou {} — {} e converta em {}",
                    killer_champ, victim_champ, push_hint, obj_hint
                )
            } else {
                format!("Aliado eliminou — {} e converta em {}", push_hint, obj_hint)
            };

            if let Some(a) = self.make_alert("kill_ally", "MACRO", "INFO", &msg, game_time, 20) {
                alerts.push(a);
            }

            // Enrichment de padrão: lane dominance mas pouco controle de obj
            if let Some(p) = pattern {
                if p.lane_dominance > 0.7 && p.objective_control < 0.35 && game_time > 600 {
                    if let Some(a) = self.make_alert(
                        "lane_to_objectives", "TRADE", "INFO",
                        "Você domina a lane — converta essa vantagem em objetivos",
                        game_time, 400,
                    ) { alerts.push(a); }
                }
            }
        } else {
            let defense_hint = defensive_hint(player_role, victim_lane, game_time);

            let msg = if has_names && !lane_label.is_empty() {
                format!(
                    "Inimigo {} eliminou {} no {} — {}",
                    killer_champ, victim_champ, lane_label, defense_hint
                )
            } else if has_names {
                format!(
                    "Inimigo {} eliminou {} — {}",
                    killer_champ, victim_champ, defense_hint
                )
            } else {
                format!("Inimigo eliminou aliado — {}", defense_hint)
            };

            if let Some(a) = self.make_alert("kill_enemy", "POSITIONING", "WARNING", &msg, game_time, 20) {
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
                        "Drake pego — próximo é o Elder Dragon. Prepare visão e smite"
                    } else if next_is_elder {
                        "Drake pego — próximo será o Elder Dragon. Agrupe para contestar"
                    } else if is_jungle {
                        "Drake pego — inicie o clear e procure gank antes do respawn"
                    } else {
                        "Drake pego — mantenha pressão e prepare o próximo objetivo"
                    };
                    if let Some(a) = self.make_alert(
                        "obj_ally_drake", "OBJECTIVE", sev, msg, game_time, 120,
                    ) { alerts.push(a); }
                } else {
                    let msg = if next_is_elder {
                        "Inimigo pegou drake — próximo é o Elder Dragon. Conteste a todo custo"
                    } else if is_jungle {
                        "Inimigo pegou drake — fortaleça visão e evite invade no momento"
                    } else {
                        "Inimigo pegou drake — jogue seguro e foque em objetivos menores"
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
                            "Seu TP tem baixo impacto histórico — use TP para ajudar em objetivos como este",
                            game_time, 600,
                        ) { alerts.push(a); }
                    }
                }
            }
            "baron" => {
                if ally_got_it {
                    let msg = if is_jungle {
                        "Baron pego — coordene o push com o buff, priorize torres e inibidores"
                    } else {
                        "Baron pego — agrupe com o time e empurre com o buff agora"
                    };
                    if let Some(a) = self.make_alert(
                        "obj_ally_baron", "OBJECTIVE", "CRITICAL", msg, game_time, 180,
                    ) { alerts.push(a); }

                    // Recall sugerido quando não há objetivo iminente após baron
                    // (baron leva 6min até o próximo, sempre há tempo para recall)
                    if let Some(a) = self.make_alert(
                        "recall_after_baron", "MACRO", "INFO",
                        "Após empurrar com o Baron buff — recall para comprar antes do próximo objetivo",
                        game_time, 240,
                    ) { alerts.push(a); }
                } else {
                    let msg = if is_jungle {
                        "Inimigo com Baron — recue, defenda torres e não force luta aberta"
                    } else {
                        "Inimigo com Baron — recue e defenda, evite lutas longe da base"
                    };
                    if let Some(a) = self.make_alert(
                        "obj_enemy_baron", "OBJECTIVE", "CRITICAL", msg, game_time, 180,
                    ) { alerts.push(a); }
                }
            }
            "herald" => {
                if ally_got_it {
                    let msg = if is_jungle {
                        "Herald pego — use na lane com mais pressão ou sob a torre mais fraca"
                    } else {
                        "Herald pego — force a torre com o herald agora"
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
                        "Torre derrubada — mantenha a pressão e avance para o próximo objetivo",
                        game_time, 90,
                    ) { alerts.push(a); }
                } else {
                    if let Some(a) = self.make_alert(
                        "obj_enemy_tower", "POSITIONING", "WARNING",
                        "Nossa torre caiu — reposicione, o inimigo tem acesso à sua lane",
                        game_time, 90,
                    ) { alerts.push(a); }
                }
            }
            "inhibitor" => {
                if ally_got_it {
                    if let Some(a) = self.make_alert(
                        "obj_ally_inhib", "MACRO", "CRITICAL",
                        "Inibidor inimigo caiu — super minions ajudam. Empurhe enquanto têm buff de minions",
                        game_time, 120,
                    ) { alerts.push(a); }
                } else {
                    if let Some(a) = self.make_alert(
                        "obj_enemy_inhib", "POSITIONING", "CRITICAL",
                        "Nosso inibidor caiu — super minions inimigos em todas as rotas. Defenda e minimize dano",
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
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        if drake_type == "Elder" { return alerts; } // coberto pelo elder_warning

        let (buff_desc, strategy) = match drake_type {
            "Fire"      => ("Infernal — buff de dano AD/AP",   "Force trades agressivos e teamfights enquanto tem o buff"),
            "Water"     => ("Oceano — buff de regeneração",     "Trades prolongados te favorecem — não recue cedo das lutas"),
            "Air"       => ("Nuvem — buff de velocidade de ult","Roaming e posicionamento ficam mais eficientes agora"),
            "Earth"     => ("Montanha — buff de escudo",        "Mais resistente em objetivos — force lutas perto de torres"),
            "Hextech"   => ("Hextech — slow em cadeia",         "Use em teamfights com chains de CC para maximizar o buff"),
            "Chemtech"  => ("Chemtech — revive com baixo HP",   "Lute até o fim — o buff ativa abaixo de 50% de HP"),
            _           => return alerts,
        };

        let (id, msg) = if ally_got_it {
            ("drake_type_ally",
             format!("Drake {} conquistado — {}", buff_desc, strategy))
        } else {
            ("drake_type_enemy",
             format!("Inimigo pegou drake {} — cuidado, {}",
                buff_desc.split(" —").next().unwrap_or(buff_desc),
                strategy.to_lowercase()))
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
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        if new_level <= prev_level { return alerts; }

        // ── Power spikes em níveis chave (role-aware) ─────────
        let spike_msg: Option<&'static str> = match (new_level, player_role) {
            // TOP: spikes focados em 1v1, TP e impacto global
            (6,  "TOP") => Some("Nível 6 — avalie o 1v1 ou force o inimigo a recuar. Guarde TP para teamfights"),
            (9,  "TOP") => Some("Nível 9 — primeira habilidade maximizada. Seu dano de lane está no pico"),
            (11, "TOP") => Some("Nível 11 — forte para duelos. Avance na lane ou aja globalmente com TP"),
            (16, "TOP") => Some("Nível 16 — ult no rank máximo. Você domina o 1v1. Force ou group"),
            // MID: spikes focados em roame, wave management e impacto de mapa
            (6,  "MID") => Some("Nível 6 — empurre a wave e roame para bot ou top. Ult disponível cria ameaça global"),
            (9,  "MID") => Some("Nível 9 — habilidade principal maxada. Seu clear de wave e poke estão no pico"),
            (11, "MID") => Some("Nível 11 — power spike de roame. Empurre e rotacione para criar pressão de mapa"),
            (16, "MID") => Some("Nível 16 — ult rank 3. Jogue para teamfights ou crie pressão no 1v1 da mid"),
            // SUPPORT: spikes focados em CC, proteção e impacto de teamfight
            (6,  "SUPPORT") | (6, "UTILITY") =>
                Some("Nível 6 — ult disponível. Coordene engage ou proteja o ADC. Você define o timing das lutas"),
            (9,  "SUPPORT") | (9, "UTILITY") =>
                Some("Nível 9 — CC/escudo/poke no pico. Momento de dominar a lane ou rotacionar para ajudar"),
            (11, "SUPPORT") | (11, "UTILITY") =>
                Some("Nível 11 — ult rank 2. Seu impacto em teamfights aumentou. Rotacione para objetivos"),
            (16, "SUPPORT") | (16, "UTILITY") =>
                Some("Nível 16 — ult rank máximo. Você está no auge do impacto. Domine as teamfights"),
            // JUNGLE: spikes focados em ganks, objetivos e pressão de mapa
            (6,  "JUNGLE") =>
                Some("Nível 6 — ult disponível. Seu gank tem mais impacto agora. Procure lanes que podem ser viradas"),
            (9,  "JUNGLE") =>
                Some("Nível 9 — habilidade principal maxada. Clears mais rápidos = mais tempo para ganks e objetivos"),
            (11, "JUNGLE") =>
                Some("Nível 11 — ult rank 2. Invada o jungle inimigo ou force um objetivo com confiança"),
            (16, "JUNGLE") =>
                Some("Nível 16 — pico de late game. Baron e Elder Dragon são o foco. Não disperdice o ult em fights pequenas"),
            // ADC: spikes focados em DPS, posicionamento e objetivos
            (6,  "BOTTOM") | (6,  "ADC") =>
                Some("Nível 6 — ult disponível. Avalie o fight com o suporte ou guarde para momento decisivo"),
            (9,  "BOTTOM") | (9,  "ADC") =>
                Some("Nível 9 — primeira habilidade maxada. Seu DPS e range estão no pico desta fase"),
            (11, "BOTTOM") | (11, "ADC") =>
                Some("Nível 11 — forte para teamfights. Priorize drakes e fique vivo durante as lutas"),
            (16, "BOTTOM") | (16, "ADC") =>
                Some("Nível 16 — build quase completo. Cada teamfight decide o jogo. Posicione-se no backline"),
            // Outros roles — genérico
            (6,  _) => Some("Você atingiu nível 6 — ultimate disponível. Force engajamento"),
            (9,  _) => Some("Você atingiu nível 9 — habilidades maximizadas. Momento de dominar"),
            (11, _) => Some("Você atingiu nível 11 — power spike crítico. Force teamfight"),
            (16, _) => Some("Você atingiu nível 16 — ultimate rank 3. Jogue agressivo agora"),
            _         => None,
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
                format!("Você está {} níveis à frente do oponente — domine a lane agora", advantage)
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
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        if matches!(player_role, "SUPPORT" | "UTILITY" | "JUNGLE") { return alerts; }
        if game_time < 180 { return alerts; } // aguarda 3min para estabilizar

        if opponent_cs > player_cs {
            let gap = opponent_cs - player_cs;
            if gap >= 20 {
                let msg = format!(
                    "Oponente está com {} CS a mais — foque no farm entre objetivos",
                    gap
                );
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
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();
        if new_level <= prev_level { return alerts; }

        let (id, severity, msg): (&'static str, &'static str, &'static str) = match new_level {
            6  => ("enemy_lvl6",  "WARNING",  "Inimigo atingiu nível 6 — ultimate disponível, jogue com cuidado"),
            11 => ("enemy_lvl11", "WARNING",  "Inimigo atingiu nível 11 — power spike crítico, evite trade desfavorável"),
            16 => ("enemy_lvl16", "CRITICAL", "Inimigo atingiu nível 16 — ultimate rank 3, extremamente perigoso"),
            _  => return alerts,
        };

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
        actual_cs: u32,
        game_time:  u32,
        player_role: &str,
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        // Só roles que farmam — SUPPORT e JUNGLE têm CS baixo por design
        if matches!(player_role, "SUPPORT" | "UTILITY" | "JUNGLE") {
            return alerts;
        }
        // Só avaliar depois de 5 minutos
        if game_time < 300 { return alerts; }

        let minutes = game_time / 60;
        let expected = if minutes <= 10 {
            minutes * 8
        } else {
            80 + (minutes - 10) * 7
        };

        // Alerta se mais de 25% abaixo do esperado
        if expected > 0 && actual_cs < expected * 3 / 4 {
            let deficit = expected.saturating_sub(actual_cs);
            if let Some(a) = self.make_alert(
                "cs_deficit", "MACRO", "INFO",
                &format!(
                    "Você está {deficit} CS abaixo do esperado — foque no farm entre objetivos",
                ),
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
    ) -> Vec<CoachAlert> {
        let mut alerts = Vec::new();

        // Só relevante no early/mid game e para jungle
        if player_role != "JUNGLE" || game_time > 1200 { return alerts; }

        if let Some((lane, msg)) = classify_gank_zone(x_pct, y_pct, is_blue_side) {
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
        id:           &'static str,
        category:     &'static str,
        severity:     &'static str,
        message:      &str,
        game_time:    u32,
        cooldown_secs: u64,
    ) -> Option<CoachAlert> {
        if !self.cooldown_ok(id, cooldown_secs) {
            return None;
        }
        self.last_alert_at.insert(id, Instant::now());
        Some(CoachAlert {
            id:        Uuid::new_v4().to_string(),
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
fn classify_gank_zone(x: f32, y: f32, is_blue_side: bool) -> Option<(&'static str, &'static str)> {
    if is_blue_side {
        // ── Time azul: inimigo overextended no território aliado (base no canto inferior-esquerdo)
        // Bot: inimigo avançou além do rio na bot — y alto + x não muito alto (lado aliado)
        if y > 73.0 && x < 58.0 {
            return Some(("bot", "Inimigo avançou além do rio na bot — brecha de gank, vá agora"));
        }
        // Top: inimigo avançou além do rio na top — x baixo + y baixo (lado aliado do topo)
        if x < 22.0 && y < 32.0 {
            return Some(("top", "Inimigo avançou além do rio no topo — brecha de gank, vá agora"));
        }
        // Mid: inimigo além do rio no lado aliado da mid
        if x < 40.0 && y > 60.0 && (x + y) > 95.0 {
            return Some(("mid", "Inimigo avançou além do rio na mid — brecha de gank, vá agora"));
        }
    } else {
        // ── Time vermelho: zonas espelhadas (base no canto superior-direito)
        if y > 73.0 && x > 42.0 {
            return Some(("bot", "Inimigo avançou além do rio na bot — brecha de gank, vá agora"));
        }
        if x > 78.0 && y < 32.0 {
            return Some(("top", "Inimigo avançou além do rio no topo — brecha de gank, vá agora"));
        }
        if x > 60.0 && y < 40.0 && (x + y) < 105.0 {
            return Some(("mid", "Inimigo avançou além do rio na mid — brecha de gank, vá agora"));
        }
    }
    None
}

// ── Regras por categoria ──────────────────────────────────────

fn evaluate_objective_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
    // ── Drake: baseado no EventTime real do último kill ───────
    // dragon_secs == 0  → drake vivo e disponível no mapa
    // dragon_secs > 0   → faltam N segundos para spawnar
    let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);

    if (55..=65).contains(&dragon_secs) {
        out.push(RuleResult {
            id: "dragon_60s",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 180,
            message: "Dragão em 1 minuto — monte visão no rio".into(),
        });
    }

    if (25..=35).contains(&dragon_secs) {
        let severity = if ctx.dragon_enemy == 3 { "CRITICAL" } else { "WARNING" };
        out.push(RuleResult {
            id: "dragon_30s",
            category: "OBJECTIVE",
            severity,
            cooldown: 180,
            message: format!("Dragão em {dragon_secs}s — reúna o time agora"),
        });
    }

    if dragon_secs == 0 {
        // Elder Dragon surge quando algum time atingiu a alma (4 drakes)
        let is_elder = ctx.dragon_ally >= 4 || ctx.dragon_enemy >= 4;
        let (msg, sev) = if is_elder {
            ("Elder Dragon disponível — conteste a todo custo, buff é decisivo", "CRITICAL")
        } else {
            ("Dragão disponível — vá agora", "WARNING")
        };
        out.push(RuleResult {
            id: "dragon_now",
            category: "OBJECTIVE",
            severity: sev,
            cooldown: 240,
            message: msg.into(),
        });
    }

    // ── Elder Dragon se aproximando (pré-aviso) ───────────────
    // Dispara quando o próximo drake vai ser o Elder (já tem alma)
    if (ctx.dragon_ally >= 4 || ctx.dragon_enemy >= 4) && (25..=65).contains(&dragon_secs) {
        let who = if ctx.dragon_enemy >= 4 { "Inimigo tem alma" } else { "Você tem alma" };
        out.push(RuleResult {
            id: "elder_warning",
            category: "OBJECTIVE",
            severity: "CRITICAL",
            cooldown: 180,
            message: format!("Elder Dragon em {dragon_secs}s — {who}. Conteste ou proteja vida"),
        });
    }

    // ── Alma do Dragão: inimigo em 3 drakes ───────────────────
    if ctx.dragon_enemy == 3 {
        out.push(RuleResult {
            id: "dragon_soul_danger",
            category: "OBJECTIVE",
            severity: "CRITICAL",
            cooldown: 600,
            message: "Inimigo a 1 drake da alma — conteste a todo custo".into(),
        });
    }

    // ── Baron: spawn a 20:00 ou 360s após último kill ─────────
    // Se nunca morreu e game_time >= 1200, baron está vivo (spawn_secs == 0)
    let baron_secs = match ctx.next_baron_spawn {
        Some(spawn_at) => Some(spawn_at.saturating_sub(ctx.game_time)),
        None if ctx.game_time >= 1200 => Some(0), // nunca foi morto, vivo desde 20:00
        None => None,
    };

    if let Some(secs) = baron_secs {
        if (55..=65).contains(&secs) {
            out.push(RuleResult {
                id: "baron_60s",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 180,
                message: "Baron em 1 minuto — posicione o time".into(),
            });
        }
        if (25..=35).contains(&secs) {
            out.push(RuleResult {
                id: "baron_30s",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 180,
                message: format!("Baron em {secs}s — monte smite e visão agora"),
            });
        }
        if secs == 0 {
            out.push(RuleResult {
                id: "baron_now",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 120,
                message: "Baron disponível — coordene com o time".into(),
            });
        }
    }

    // ── Larvas do Vazio (Voidgrubs) — spawn às 5:00 ──────────
    // 3 larvas spawnam na Baron side ao mesmo tempo que o primeiro Drake.
    // Cada larva morta dá um stack de Voidmite que empurra lanes automaticamente.
    // Decisão estratégica: Larvas (pressão de lane) vs Drake (buff de time).
    // Disponíveis de 5:00 (300s) até aproximadamente 14:45 (885s).
    // Patch 26.10: spawn confirmado em 5:00.
    {
        let vg_secs = 300u32.saturating_sub(ctx.game_time);

        if (25..=55).contains(&vg_secs) {
            let msg = match ctx.player_role.as_str() {
                "JUNGLE" => "Larvas do Vazio em 30s — decidir: Larvas (pressão de lane) ou Drake (buff). \
                             Larvas estão na Baron side",
                "TOP"    => "Larvas do Vazio em 30s no Baron side — avise o jungle se pode ajudar a pegar",
                _        => "Larvas do Vazio em 30s — Drake e Larvas spawnam juntos. \
                             Jungle decide a prioridade",
            };
            out.push(RuleResult {
                id: "voidgrubs_soon",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 150,
                message: msg.into(),
            });
        }

        // Larvas disponíveis agora (5:00–14:45)
        if vg_secs == 0 && ctx.game_time < 885 {
            let msg = match ctx.player_role.as_str() {
                "JUNGLE" => "Larvas do Vazio disponíveis — são 3 na Baron side. \
                             Cada uma dá stack de Voidmite que empurra suas lanes",
                "TOP"    => "Larvas do Vazio disponíveis no Baron side — você pode assistir o jungle agora",
                _        => "Larvas do Vazio disponíveis — objetivo na Baron side. \
                             Jungle define prioridade vs Drake",
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

    // ── Herald (Arauto da Fenda) — spawn às 15:00 ─────────────
    // Patch 26.10: Herald spawna em 15:00 (900s), despawna em 19:45 (1185s).
    // Mudança do Season 2026: Herald saiu do slot das 8:00 (agora Voidgrubs)
    // para 15:00, tornando-o um objetivo de mid-late game.
    if !ctx.herald_killed && (870..=1185).contains(&ctx.game_time) {
        let herald_secs = 900u32.saturating_sub(ctx.game_time);

        if (1..=35).contains(&herald_secs) {
            out.push(RuleResult {
                id: "herald_30s",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 300,
                message: format!("Herald em {herald_secs}s — posicione o time. \
                                  Após Baron (20:00) o Herald despawna"),
            });
        }

        if herald_secs == 0 {
            out.push(RuleResult {
                id: "herald_available",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 240,
                message: "Herald disponível (15:00–19:45) — use para derrubar torre antes do Baron spawnar".into(),
            });
        }
    }
}

fn evaluate_vision_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
    // Sem dados históricos de pattern, assume ward_score zero para que o
    // lembrete de ward dispare por padrão — melhor dar dica desnecessária
    // do que silêncio total durante um replay sem histórico no banco.
    let ward_score = ctx
        .pattern
        .as_ref()
        .map(|p| p.ward_score)
        .unwrap_or(0.0);

    // ── Baixo ward_score + objetivo se aproximando ─────────────
    if ward_score < 0.3 {
        let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
        let drake_soon  = dragon_secs <= 90;

        if drake_soon {
            out.push(RuleResult {
                id: "low_ward_pre_drake",
                category: "VISION",
                severity: "WARNING",
                cooldown: 180,
                message: format!(
                    "Seu ward score é baixo ({:.0}%) — ward o rio antes do dragão",
                    ward_score * 100.0
                ),
            });
        }

        // Ward reminder geral — disparado por contexto (ward baixo), cooldown controla frequência
        if ctx.game_time > 300 {
            out.push(RuleResult {
                id: "low_ward_reminder",
                category: "VISION",
                severity: "INFO",
                cooldown: 120,
                message: "Coloque wards — visão reduz mortes desnecessárias".into(),
            });
        }
    }

    // ── Muitas mortes sem visão (padrão do banco) ──────────────
    if let Some(ref p) = ctx.pattern {
        if p.deaths_without_vision > 12 && ctx.game_time > 600 {
            out.push(RuleResult {
                id: "deaths_without_vision",
                category: "VISION",
                severity: "WARNING",
                cooldown: 300,
                message: format!(
                    "{} mortes suas foram sem visão — ward antes de avançar",
                    p.deaths_without_vision
                ),
            });
        }
    }
}

fn evaluate_macro_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
    let diff = ctx.score_diff();

    // ── Na frente: converta em objetivos ──────────────────────
    if diff >= 5 && ctx.game_time > 300 && ctx.game_time < 1200 {
        out.push(RuleResult {
            id: "ahead_convert",
            category: "MACRO",
            severity: "INFO",
            cooldown: 240,
            message: format!(
                "Você está na frente {}-{} — converta em torres e objetivos",
                ctx.ally_score, ctx.enemy_score
            ),
        });
    }

    // ── Atrás: priorize objetivos para reagir ─────────────────
    if diff <= -5 && ctx.game_time > 300 {
        out.push(RuleResult {
            id: "behind_objectives",
            category: "MACRO",
            severity: "WARNING",
            cooldown: 300,
            message: "Você está atrás — evite teamfights e foque em objetivos menores".into(),
        });
    }

    // ── Inimigo com alma do dragão ─────────────────────────────
    if ctx.dragon_enemy >= 4 {
        out.push(RuleResult {
            id: "enemy_dragon_soul",
            category: "MACRO",
            severity: "CRITICAL",
            cooldown: 600,
            message: "Inimigo tem alma do dragão — evite lutas abertas".into(),
        });
    }

    // ── Aliado com vantagem de drakes (3+) ────────────────────
    if ctx.dragon_ally >= 3 && ctx.dragon_enemy < 3 {
        out.push(RuleResult {
            id: "ally_drake_advantage",
            category: "MACRO",
            severity: "INFO",
            cooldown: 600,
            message: "Você domina os drakes — force teamfights agora".into(),
        });
    }
}

fn evaluate_trade_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
    let Some(ref pattern) = ctx.pattern else {
        return;
    };

    // ── Alta agressividade + muitas mortes no mid game ─────────
    // Período crítico 10-15min (600-900s)
    if pattern.aggression_score > 0.75
        && pattern.avg_deaths_10_15 > 2.5
        && (600..=900).contains(&ctx.game_time)
    {
        out.push(RuleResult {
            id: "aggression_deaths_midgame",
            category: "TRADE",
            severity: "WARNING",
            cooldown: 300,
            message: format!(
                "Cuidado — você morre em média {:.1}x entre min 10-15. Jogue seguro agora",
                pattern.avg_deaths_10_15
            ),
        });
    }

    // lane_to_objectives movido para handle_score_change — dispara no evento de kill aliado
}

fn evaluate_positioning_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
    let ward_score = ctx.pattern.as_ref().map(|p| p.ward_score).unwrap_or(0.5);
    let is_solo_role = matches!(ctx.player_role.as_str(), "TOP" | "JUNGLE");

    // ── Jogador solo sem visão, mid/late game ─────────────────
    if is_solo_role && ward_score < 0.2 && ctx.game_time > 600 {
        out.push(RuleResult {
            id: "solo_no_vision",
            category: "VISION",   // explicitamente pede ward → VISION dispara WardOverlay
            severity: "WARNING",
            cooldown: 180,
            message: "Sem visão na sua rota — recue ou ward antes de avançar".into(),
        });
    }

    // ── Alta agressividade em posição isolada ──────────────────
    if let Some(ref p) = ctx.pattern {
        if p.aggression_score > 0.8 && !ctx.is_ahead && ctx.game_time > 900 {
            out.push(RuleResult {
                id: "aggression_while_behind",
                category: "POSITIONING",
                severity: "WARNING",
                cooldown: 300,
                message: "Você está atrás — sua agressividade pode custar o jogo. Jogue seguro".into(),
            });
        }
    }
}

// ── Helpers para handle_champion_kill ────────────────────────

/// Retorna sugestão de ação baseada em QUEM morreu e QUAL É O ROLE do jogador.
///
/// Regra central: a dica sempre fala da perspectiva do jogador, não da vítima.
///   · Inimigo morreu NA MINHA LANE  → avance na minha lane agora
///   · Inimigo morreu NOUTRA LANE    → ação role-específica (roam, push own lane, gank)
fn push_suggestion(victim_lane: &str, player_role: &str, game_time: u32) -> String {
    // Normaliza a lane do jogador para comparar com a posição da vítima
    let my_lane_norm = match player_role {
        "TOP"                          => "TOP",
        "JUNGLE"                       => "JUNGLE",
        "MID" | "MIDDLE"               => "MIDDLE",
        "BOTTOM" | "ADC"               => "BOTTOM",
        "SUPPORT" | "UTILITY"          => "BOTTOM",  // suporte é bot lane
        _                              => "",
    };

    // Verifica se o inimigo que morreu era da minha lane
    let killed_in_my_lane = !my_lane_norm.is_empty()
        && (victim_lane == my_lane_norm
            || (my_lane_norm == "BOTTOM" && victim_lane == "UTILITY"));

    let phase = if game_time >= 1200 { "force inibidor" }
                else if game_time >= 600 { "force torre" }
                else { "pressione a onda" };

    if killed_in_my_lane {
        // Meu oponente direto morreu → avançar na MINHA lane é a jogada óbvia
        let my_label = match my_lane_norm {
            "TOP"    => "top",
            "MIDDLE" => "mid",
            "BOTTOM" => "bot",
            "JUNGLE" => "jungle inimigo",
            _        => "sua lane",
        };
        format!("avance no {} e {}", my_label, phase)
    } else {
        // Kill em outra lane → conselho depende do meu role
        let killed_label = match victim_lane {
            "TOP"            => "top",
            "MIDDLE"         => "mid",
            "BOTTOM"| "UTILITY" => "bot",
            "JUNGLE"         => "jungle",
            _                => "outra rota",
        };

        match player_role {
            "MID" | "MIDDLE" => {
                // Mid pode rotacionar — maior impacto de roam de todos os laners
                if game_time < 900 {
                    format!("empurre a wave e rotacione para o {} enquanto inimigo respawna", killed_label)
                } else {
                    format!("empurre a mid e agrupe no {}", killed_label)
                }
            }
            "JUNGLE" => {
                // O inimigo nessa lane JÁ está morto — gankear ela não faz sentido.
                // Use a janela para invadir o jungle inimigo ou pressionar as outras lanes.
                let adjacent = match victim_lane {
                    "TOP"            => "mid ou bot",
                    "MIDDLE"         => "top ou bot",
                    "BOTTOM"| "UTILITY" => "top ou mid",
                    _                => "outra lane",
                };
                if game_time < 900 {
                    format!(
                        "invada o jungle inimigo ou ganke o {} enquanto {} respawna",
                        adjacent, killed_label
                    )
                } else {
                    format!(
                        "force objetivo próximo ou pressione o {} enquanto inimigo respawna",
                        adjacent
                    )
                }
            }
            "TOP" => {
                // Top laner dificilmente rota — empurra sua própria lane
                format!("avance no top e {} enquanto inimigo está morto no {}", phase, killed_label)
            }
            "BOTTOM" | "ADC" => {
                format!("avance no bot e {} enquanto inimigo está morto no {}", phase, killed_label)
            }
            "SUPPORT" | "UTILITY" => {
                if game_time < 900 {
                    format!("crie visão e pressione o {} com o ADC", killed_label)
                } else {
                    format!("avance no bot e {} enquanto inimigo está morto", phase)
                }
            }
            _ => format!("empurre sua lane e {}", phase)
        }
    }
}

/// Retorna o objetivo principal para converter a vantagem, baseado no tempo de jogo.
fn objective_from_time(game_time: u32) -> &'static str {
    if      game_time < 300  { "pressão de lane" }
    else if game_time < 600  { "scuttler ou visão" }
    else if game_time < 900  { "drake ou torre" }
    else if game_time < 1200 { "torre ou herald" }
    else                     { "baron ou inibidor" }
}

/// Retorna dica defensiva contextualizada ao role e lane onde aliado morreu.
fn defensive_hint(player_role: &str, victim_lane: &str, game_time: u32) -> &'static str {
    let is_jungle = player_role == "JUNGLE";

    if is_jungle {
        if game_time < 900 {
            "proteja a rota adjacente e evite overextend"
        } else {
            "reposicione e defenda antes de reiniciar pressão"
        }
    } else {
        // Aviso de roam inimigo depende de onde o aliado morreu
        match victim_lane {
            "TOP" => {
                if game_time < 900 {
                    "cuidado com roam do killer — ward o mid e recue se necessário"
                } else {
                    "defenda o top e ward antes de avançar"
                }
            }
            "BOTTOM" | "UTILITY" => {
                if game_time < 900 {
                    "cuidado com roam da bot — ward o mid e segure posição"
                } else {
                    "defenda o bot e evite avançar sem visão"
                }
            }
            "MIDDLE" => {
                if game_time < 900 {
                    "mid está perigoso — ward e recue se estiver exposto"
                } else {
                    "recolha para uma posição segura e ward"
                }
            }
            _ => {
                if game_time < 900 {
                    "ward e recue — possível rotação ou gank iminente"
                } else {
                    "reposicione e ward antes de avançar"
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

fn evaluate_bot_lane_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
    let is_adc     = matches!(ctx.player_role.as_str(), "BOTTOM" | "ADC");
    let is_support = matches!(ctx.player_role.as_str(), "SUPPORT" | "UTILITY");

    if !is_adc && !is_support {
        return;
    }

    // Só relevante no early/mid game (antes dos 15 minutos)
    if ctx.game_time > 900 {
        return;
    }

    // ── ADC: suporte saiu da lane → aviso de posicionamento ──────
    // Só dispara depois de 2min (o duo pode ter ido ward ou recall juntos no início)
    if is_adc && !ctx.support_in_lane && ctx.game_time >= 120 {
        out.push(RuleResult {
            id: "adc_support_absent",
            category: "VISION",   // diz explicitamente "ward o arbusto" → VISION
            severity: "WARNING",
            cooldown: 90,
            message: "Suporte não está na lane — ward o arbusto e recue se necessário".into(),
        });
    }

    // ── SUPORTE: duo junto na lane → ensinar domínio de bush ─────

    if is_support && ctx.support_in_lane {
        // Nível 1 — controlar bushes antes dos minions chegarem
        if (60..=90).contains(&ctx.game_time) {
            out.push(RuleResult {
                id: "sup_bush_lvl1",
                category: "POSITIONING",
                severity: "INFO",
                cooldown: 120,
                message: "Ocupe o arbusto da lane agora — nega visão inimiga e garante nível 2 primeiro".into(),
            });
        }

        // Nível 2 power spike (~1:40) — engajar pelo arbusto
        if (95..=115).contains(&ctx.game_time) {
            out.push(RuleResult {
                id: "sup_bush_lvl2",
                category: "TRADE",
                severity: "INFO",
                cooldown: 120,
                message: "Você chega ao nível 2 agora — engaje pelo arbusto antes do inimigo".into(),
            });
        }

        // Scuttler se aproxima (~3:15) — ward o rio
        if (175..=210).contains(&ctx.game_time) {
            out.push(RuleResult {
                id: "sup_bush_scuttler",
                category: "VISION",
                severity: "INFO",
                cooldown: 120,
                message: "Scuttler em breve — ward a bush do rio antes dos 3:30 para visão do drake".into(),
            });
        }

        // Janela pre-drake (~4:30) — controlar arbusto do pit
        if (255..=295).contains(&ctx.game_time) {
            out.push(RuleResult {
                id: "sup_bush_drake_prep",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 120,
                message: "Drake se aproxima — controle a bush do pit e ward o lado inimigo do rio".into(),
            });
        }

        // Lembrete geral de domínio de bush quando suporte está parado na lane
        if ctx.game_time > 90 && ctx.game_time < 600 {
            let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
            let pre_obj = dragon_secs <= 120;
            if pre_obj {
                out.push(RuleResult {
                    id: "sup_bush_pre_obj",
                    category: "VISION",
                    severity: "WARNING",
                    cooldown: 150,
                    message: "Objetivo se aproxima — domine a bush do rio e force o inimigo a jogar sem visão".into(),
                });
            }
        }
    }
}

fn evaluate_opp_jungle_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
    if !ctx.enemy_in_opp_jungle {
        return;
    }

    // Só relevante antes dos 20 minutos — depois o jogo é mais rotacional
    if ctx.game_time > 1200 {
        return;
    }

    let role = ctx.player_role.as_str();

    // ── Early game (< 8min): jungle longe → avançar e trocar ───
    if ctx.game_time < 480 {
        let msg = match role {
            "TOP"                         =>
                "Jungle inimigo longe — avance na rota e force um trade agora",
            "BOTTOM" | "ADC"              =>
                "Jungle inimigo longe — avance na bot e seja agressivo",
            "SUPPORT" | "UTILITY"         =>
                "Jungle inimigo longe — domine a bush e crie pressão na bot",
            "MID"                         =>
                "Jungle inimigo longe — avance na mid e roame se tiver vantagem",
            "JUNGLE"                      =>
                "Jungle inimigo no lado oposto — invada o jungle dele e roube camps agora",
            _                             => return,
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

    // ── Mid game (8–15min): janela maior para pressionar ────────
    if ctx.game_time < 900 {
        let msg = match role {
            "TOP"                         =>
                "Jungle inimigo no bot — pressione o topo e force torre ou herald",
            "BOTTOM" | "ADC"              =>
                "Jungle inimigo no topo — empurre a bot e force torre agora",
            "SUPPORT" | "UTILITY"         =>
                "Jungle inimigo no topo — ward profundo e force engajamento na bot",
            "MID"                         =>
                "Jungle inimigo longe — empurre a mid e roame para criar pressão",
            "JUNGLE"                      =>
                "Jungle inimigo no lado oposto — invada, roube objetivos e crie pressão",
            _                             => return,
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

    // ── Late early / mid-game (15–20min): objetivos ─────────────
    let msg = match role {
        "TOP"                         =>
            "Jungle inimigo longe — converta a vantagem no topo em torre ou baron",
        "BOTTOM" | "ADC"              =>
            "Jungle inimigo no topo — empurre o bot e force inibidor ou drake",
        "SUPPORT" | "UTILITY"         =>
            "Jungle inimigo no topo — crie visão profunda e prepare objetivo no bot",
        "MID"                         =>
            "Jungle inimigo longe — force o objetivo mais próximo agora",
        "JUNGLE"                      =>
            "Jungle inimigo longe — tome o Baron ou Drake antes que ele volte",
        _                             => return,
    };
    out.push(RuleResult {
        id: "opp_jungle_late_early",
        category: "MACRO",
        severity: "WARNING",
        cooldown: 120,
        message: msg.to_string(),
    });
}

fn evaluate_side_push_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
    // Só relevante a partir do mid game (10min+)
    if ctx.game_time < 600 {
        return;
    }

    let diff = ctx.score_diff();
    let is_side_pusher = matches!(ctx.player_role.as_str(), "TOP" | "BOTTOM" | "ADC" | "MID");

    // ── Vantagem no placar → pressionar a side ────────────────
    // < 15min: mensagem de pressão imediata
    // >= 15min: mensagem de conversão — side_push_role complementa com dica role-específica
    if diff >= 3 && is_side_pusher {
        let msg = if ctx.game_time >= 900 {
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

    // ── Entre drakes com vantagem → split push ────────────────
    // Janela entre objetivos é o momento certo para pressionar a side
    if ctx.dragon_ally >= 2 && ctx.dragon_ally > ctx.dragon_enemy && is_side_pusher {
        let dragon_secs = ctx.next_dragon_spawn.saturating_sub(ctx.game_time);
        if dragon_secs > 90 {
            out.push(RuleResult {
                id: "side_push_between_drakes",
                category: "MACRO",
                severity: "INFO",
                cooldown: 360,
                message: format!(
                    "Próximo dragão em {dragon_secs}s — empurre a side lane e crie pressão antes do objetivo"
                ),
            });
        }
    }

    // ── Role-específico: TOP/ADC/MID como split pushers ──────
    if is_side_pusher && ctx.is_ahead && ctx.game_time >= 900 {
        let role_msg = match ctx.player_role.as_str() {
            "TOP"            => "TOP com vantagem — split push no topo para criar pressão de mapa",
            "BOTTOM" | "ADC" => "ADC com vantagem — empurre o bot e force o inibidor",
            "MID"            => "MID com vantagem — pressione a mid e force objetivos centrais",
            _                => return,
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
fn evaluate_top_lane_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
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

    // ── EARLY (0–65s): nível 1 — controlar tribush ───────────
    // O TOP deve sempre ocupar o tribush antes dos minions chegar
    // para negar invade e garantir stack de primeiro nível.
    if ctx.game_time < 65 {
        out.push(RuleResult {
            id: "top_lvl1_tribush",
            category: "VISION",   // pede controle de visão no tribush → VISION
            severity: "INFO",
            cooldown: 120,
            message: "Ocupe o tribush antes dos minions — nega invade e garante visão do rio do topo".into(),
        });
        return; // evita ruído logo no início
    }

    // ── EARLY (2–8min): anti-gank awareness ──────────────────
    // TOP é a lane mais isolada — ganks são a principal causa de morte
    if ward_score < 0.25 && (120..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "top_anti_gank_early",
            category: "VISION",
            severity: "WARNING",
            cooldown: 240,
            message: "Ward o rio do topo — sem visão você está exposto a ganks. Mude de posição ou recue".into(),
        });
    }

    // ── EARLY (2–8min): freeze quando atrás ──────────────────
    // Quando atrás, freezar a wave próximo da torre é a jogada mais segura:
    // garante farm, dificulta gank do jungle inimigo e aguarda jungler aliado.
    if diff <= -3 && (120..=540).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "top_freeze_behind",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 300,
            message: "Você está atrás na lane — congele a wave próximo da sua torre e espere ajuda do jungle".into(),
        });
    }

    // ── EARLY (3–8min): push + recall quando na frente ───────
    // Quando à frente no score, empurrar antes de recall garante pressão
    // de wave e itens adiantados — padrão de jogo de split push TOP.
    if ctx.is_ahead && (180..=480).contains(&ctx.game_time) && dragon_secs > 150 {
        out.push(RuleResult {
            id: "top_shove_recall",
            category: "MACRO",
            severity: "INFO",
            cooldown: 300,
            message: "Você está na frente — empurre a wave até a torre e volte para base com segurança".into(),
        });
    }

    // ── MID-LATE (15:00–19:45): herald — prioridade do TOP ───
    // Patch 26.10: Herald agora spawna em 15:00 (não mais 8:00).
    // É o objetivo mais impactante que o TOP pode usar: derruba torre,
    // abre a base antes do Baron (20:00). Janela estreita — use logo.
    if !ctx.herald_killed && (870..=1160).contains(&ctx.game_time) {
        if ctx.is_ahead {
            out.push(RuleResult {
                id: "top_herald_push",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 240,
                message: "Herald disponível (15–19min) e você está na frente — coordene com o jungle. \
                          Use antes do Baron spawnar às 20min".into(),
            });
        } else if diff >= -2 {
            out.push(RuleResult {
                id: "top_herald_balanced",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 360,
                message: "Herald disponível — pegar o Herald antes do Baron pode equilibrar o jogo pela top lane".into(),
            });
        }
    }

    // ── MID (8–20min): TP para drake — impacto global ────────
    // TOP com baixa eficiência de TP perde influência no mapa.
    // Lembrar de se posicionar para TP em lutas de drake é essencial.
    if tp_eff < 0.40 && (600..=1200).contains(&ctx.game_time) {
        if dragon_secs <= 120 && dragon_secs > 0 {
            out.push(RuleResult {
                id: "top_tp_drake_prep",
                category: "MACRO",
                severity: "INFO",
                cooldown: 300,
                message: "Drake em 2 minutos — empurre a wave e posicione-se para usar TP na luta do dragão".into(),
            });
        }
    }

    // ── MID (8–20min): lane dominance alta → converter ───────
    // Jogadores com alto lane_dominance histórico que não convertem
    // em objetivos repetem o mesmo erro: dominam a lane mas perdem o jogo.
    if lane_dom > 0.65 && ctx.is_ahead && (600..=1200).contains(&ctx.game_time) {
        let drake_far = dragon_secs > 180;
        if drake_far {
            out.push(RuleResult {
                id: "top_lane_dom_convert",
                category: "MACRO",
                severity: "INFO",
                cooldown: 400,
                message: "Você domina a lane — pressione até a torre ou roame para criar pressão global enquanto tem vantagem".into(),
            });
        }
    }

    // ── LATE (15–20min): TP para baron iminente ──────────────
    // Baron é o objetivo decisivo do late game para TOP.
    // TOP deve empurrar a wave e então usar TP para participar.
    if let Some(secs) = baron_secs {
        if secs <= 90 && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "top_tp_baron",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 180,
                message: "Baron iminente — empurre a wave agora e guarde TP para chegar na luta do Baron".into(),
            });
        }
    }

    // ── LATE (20+min): split push vs group ───────────────────
    // Com Baron disponível, o TOP precisa decidir: split ou group?
    // Se o time tem baron buff → group. Se não → split push e forçar resposta.
    if ctx.game_time > 1200 {
        if let Some(secs) = baron_secs {
            if secs == 0 {
                // Baron disponível agora: TOP deve ajudar o time a pegar
                out.push(RuleResult {
                    id: "top_baron_group",
                    category: "OBJECTIVE",
                    severity: "WARNING",
                    cooldown: 120,
                    message: "Baron disponível — group com o time. TOP isolado no split é vulnerável sem baron".into(),
                });
            } else if secs > 120 && ctx.is_ahead {
                // Entre objetivos e na frente: split push é a jogada
                out.push(RuleResult {
                    id: "top_split_window",
                    category: "MACRO",
                    severity: "INFO",
                    cooldown: 360,
                    message: "Entre objetivos — split push na top lane e force o time inimigo a responder 1v1".into(),
                });
            }
        }
    }

    // ── GLOBAL: TP efficiency baixo → educar ─────────────────
    // TOP com tp_efficiency < 0.25 raramente usa TP em lutas globais.
    // Lembrar do impacto do TP no late game.
    if tp_eff < 0.25 && ctx.game_time > 900 && diff >= -2 {
        out.push(RuleResult {
            id: "top_tp_impact_reminder",
            category: "MACRO",
            severity: "INFO",
            cooldown: 600,
            message: "Seu TP tem baixo impacto histórico — TOP sem TP global é fácil de ignorar. Use TP em fights importantes".into(),
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
fn evaluate_mid_lane_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
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
            message: "Ward o pixel bush antes dos minions — protege contra invade e garante visão dos dois lados do rio".into(),
        });
        return;
    }

    // ── EARLY (2–8min): anti-gank bilateral ──────────────────
    // MID é única lane que recebe ganks de dois lados simultaneamente.
    // Visão dos dois rios é obrigatória para jogar com agressividade.
    if ward_score < 0.25 && (120..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "mid_anti_gank_bilateral",
            category: "VISION",
            severity: "WARNING",
            cooldown: 240,
            message: "Ward ambos os lados do rio — MID está exposto a ganks coordenados sem visão bilateral".into(),
        });
    }

    // ── EARLY-MID (3–8min): freeze quando atrás ──────────────
    // Diferente do TOP, o MID freezando permite ao jogador
    // focar em farm seguro e esperar pelo jungle aliado.
    if diff <= -3 && (180..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "mid_freeze_behind",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 300,
            message: "Você está atrás — congele a wave próximo da sua torre e farm seguro antes de tentar roame".into(),
        });
    }

    // ── MID (5–15min): roame proativo — padrão histórico baixo ──
    // Jogadores de MID com roam_frequency histórico < 0.25 perdem
    // consistentemente influência de mapa. Lembrete proativo de roame
    // quando a wave está empurrada e há tempo para agir.
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
            message: "Seu roam histórico é baixo — empurre a wave e roame para bot ou top antes de voltar para mid".into(),
        });
    }

    // ── MID (5–15min): janela de roame após push ──────────────
    // Quando o MID está na frente e sem objetivo iminente,
    // empurrar e roamer é a jogada de maior impacto global.
    if ctx.is_ahead
        && (300..=900).contains(&ctx.game_time)
        && dragon_secs > 120
    {
        out.push(RuleResult {
            id: "mid_push_roam_window",
            category: "MACRO",
            severity: "INFO",
            cooldown: 300,
            message: "Wave empurrada e sem objetivo iminente — roame para a side lane mais fraca e crie pressão".into(),
        });
    }

    // ── MID (8–15min): wave antes do drake ────────────────────
    // MID deve empurrar a mid lane antes de rotacionar para drake.
    // Garante acesso, minions não ficam stacked e cria pressão posterior.
    if dragon_secs <= 120 && dragon_secs > 0 && (480..=1200).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "mid_wave_before_drake",
            category: "MACRO",
            severity: "INFO",
            cooldown: 240,
            message: "Drake em breve — empurre a wave da mid antes de rotacionar para não perder farm".into(),
        });
    }

    // ── MID (15–20min): wave antes do baron ───────────────────
    // Empurrar a wave da mid antes do baron é FUNDAMENTAL:
    // 1. Garante acesso pela mid ao baron
    // 2. Cria pressão de minions enquanto o time luta
    // 3. Após baron, a mid empurrada dá mais tempo para base
    if let Some(secs) = baron_secs {
        if secs <= 120 && secs > 0 && ctx.game_time > 1000 {
            out.push(RuleResult {
                id: "mid_wave_before_baron",
                category: "MACRO",
                severity: "WARNING",
                cooldown: 180,
                message: "Baron iminente — empurre a wave da mid agora. Garante acesso e pressão durante a luta".into(),
            });
        }
    }

    // ── MID (15+min): acesso central ao baron ─────────────────
    // O MID controla o acesso mais rápido ao baron de ambos os lados.
    // Lembrar de se posicionar para controle de visão de mid antes da luta.
    if let Some(secs) = baron_secs {
        if secs <= 60 && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "mid_baron_access",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 150,
                message: "Baron iminente — você é o acesso central. Ward o baron pela mid e coordene com o time".into(),
            });
        }
    }

    // ── MID (8–20min): lane dominance alta → converter em objetivos ──
    // MID com alta dominância de lane que não roama perde o impacto
    // da vantagem individualmente gerada.
    if lane_dom > 0.65 && ctx.is_ahead && (480..=1200).contains(&ctx.game_time) {
        let between_objs = dragon_secs > 150
            && baron_secs.map(|s| s > 150).unwrap_or(true);
        if between_objs {
            out.push(RuleResult {
                id: "mid_dom_convert",
                category: "MACRO",
                severity: "INFO",
                cooldown: 360,
                message: "Você domina a mid — converta o domínio em roame e crie vantagem nas side lanes".into(),
            });
        }
    }

    // ── MID (20+min): teamfight vs split push ─────────────────
    // MID late game deve priorizar teamfights — fica no centro
    // onde seu impacto é máximo, não em split push isolado.
    if ctx.game_time > 1200 {
        if baron_secs.map(|s| s == 0).unwrap_or(false) {
            out.push(RuleResult {
                id: "mid_baron_group",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 120,
                message: "Baron disponível — group com o time pela mid. MID isolado no split perde influência decisiva".into(),
            });
        } else if ctx.is_ahead && baron_secs.map(|s| s > 120).unwrap_or(true) {
            out.push(RuleResult {
                id: "mid_split_or_group",
                category: "MACRO",
                severity: "INFO",
                cooldown: 360,
                message: "Entre objetivos — pressione a mid para criar espaço. Não fique parado em base late game".into(),
            });
        }
    }

    // ── GLOBAL: TP efficiency baixo para MID ─────────────────
    // MID com TP e baixa eficiência perde a capacidade de
    // impactar teamfights globalmente no mid/late.
    if tp_eff < 0.25 && ctx.game_time > 600 && diff >= -2 {
        out.push(RuleResult {
            id: "mid_tp_impact",
            category: "MACRO",
            severity: "INFO",
            cooldown: 600,
            message: "Seu TP tem baixo impacto histórico — use TP para flanquear em teamfights e aumentar seu valor de mapa".into(),
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
fn evaluate_support_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
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
                message: "Entre objetivos — rotacione para o rio e ajude o jungle. Suporte deve sair da bot assim que a wave estiver safe".into(),
            });
        }
    }

    // ── MID GAME (8–15min): visão de rio para roame seguro ────
    // Antes de rotacionar, o suporte deve deixar wards para proteger o ADC.
    if ward_score < 0.35 && (480..=900).contains(&ctx.game_time) && diff >= -2 {
        out.push(RuleResult {
            id: "sup_ward_before_roam",
            category: "VISION",
            severity: "INFO",
            cooldown: 240,
            message: "Ward o rio antes de roamar — proteja o ADC enquanto você está fora da bot lane".into(),
        });
    }

    // ── PRÉ-DRAKE: posicionamento e negação de visão ──────────
    // O suporte é o responsável por NEGAR a visão inimiga do drake
    // antes do time chegar — não só colocar wards, mas limpar os deles.
    if (60..=120).contains(&dragon_secs) {
        out.push(RuleResult {
            id: "sup_drake_vision_deny",
            category: "VISION",
            severity: "INFO",
            cooldown: 180,
            message: "Drake em 1-2 min — limpe os wards inimigos do pit antes de colocar os seus. Negue a visão deles".into(),
        });
    }

    // ── PRÉ-DRAKE: posicionamento do lado inimigo ─────────────
    // O suporte deve se posicionar do lado inimigo do drake,
    // negando o approach e garantindo que o time cobre a luta.
    if (25..=55).contains(&dragon_secs) {
        out.push(RuleResult {
            id: "sup_drake_positioning",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 150,
            message: "Drake em 30-55s — posicione-se do lado inimigo do pit. Você nega o engage deles e protege o smite".into(),
        });
    }

    // ── PRÉ-BARON: visão do baron é SUA responsabilidade ──────
    // O suporte É o ward placer do time — baron ward é obrigatório.
    if let Some(secs) = baron_secs {
        if (60..=120).contains(&secs) && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "sup_baron_ward",
                category: "VISION",
                severity: "WARNING",
                cooldown: 150,
                message: "Baron em 1-2 min — coloque ward no pit e no rio. Você é o responsável pela visão do baron".into(),
            });
        }
        // Posicionamento antes do baron
        if (20..=55).contains(&secs) && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "sup_baron_positioning",
                category: "POSITIONING",
                severity: "WARNING",
                cooldown: 120,
                message: "Baron em 20-55s — posicione-se do lado deles com CC pronto. Nega o engage e garante a luta".into(),
            });
        }
    }

    // ── LATE (15+min): peeling o ADC ─────────────────────────
    // Quando o ADC está à frente ou o jogo está equilibrado,
    // o suporte deve priorizar proteger o carry em vez de engajar.
    if ctx.game_time > 900 && ctx.is_ahead {
        out.push(RuleResult {
            id: "sup_peel_adc",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 400,
            message: "Seu ADC está na frente — fique próximo e proteja em teamfights. Seu CC salva carries, não inicia".into(),
        });
    }

    // ── LATE (20+min): prioridade máxima de visão ─────────────
    // No late game, suporte deve manter visão de baron, rio e base.
    // É a job mais importante para abrir baron sem morrer.
    if ctx.game_time > 1200 {
        if let Some(secs) = baron_secs {
            if secs > 60 {
                out.push(RuleResult {
                    id: "sup_late_vision_control",
                    category: "VISION",
                    severity: "INFO",
                    cooldown: 360,
                    message: "Late game — mantenha wards no baron, rio e acesso inimigo. Visão ganha mais jogos do que dano no late".into(),
                });
            }
        }
    }

    // ── GLOBAL: ward score baixo para suporte ─────────────────
    // Suporte com ward score baixo está falhando em sua função primária.
    // Lembrete mais urgente que o genérico de visão.
    if ward_score < 0.25 && ctx.game_time > 300 {
        out.push(RuleResult {
            id: "sup_low_ward_score",
            category: "VISION",
            severity: "WARNING",
            cooldown: 200,
            message: format!(
                "Seu ward score está baixo ({:.0}%) — suporte é o principal ward placer. Coloque wards com frequência",
                ward_score * 100.0
            ),
        });
    }

    // ── LATE: negação de wards inimigos ───────────────────────
    // Antes de objetivos, suporte deve LIMPAR wards inimigos além de colocar os seus.
    if ctx.game_time > 900 {
        let pre_obj = dragon_secs <= 120 || baron_secs.map(|s| s <= 120).unwrap_or(false);
        if pre_obj {
            out.push(RuleResult {
                id: "sup_sweep_enemy_wards",
                category: "VISION",
                severity: "INFO",
                cooldown: 180,
                message: "Antes do objetivo — use o revelador para limpar wards inimigos. Visão deles facilita o engage".into(),
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
fn evaluate_adc_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
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

    // ── EARLY (0–65s): nível 1 — controle do tribush ─────────
    // O ADC junto com o suporte pode controlar o tribush para
    // negar a visão inimiga e garantir um level 1 mais seguro.
    if ctx.game_time < 65 {
        out.push(RuleResult {
            id: "adc_lvl1_tribush",
            category: "VISION",
            severity: "INFO",
            cooldown: 120,
            message: "Controle o tribush junto ao suporte — nega visão inimiga e abre opção de fight de nível 1".into(),
        });
        return;
    }

    // ── EARLY (2–8min): freeze quando atrás e sem suporte ────
    // ADC sozinho atrás no CS é altamente vulnerável sem suporte.
    if diff <= -3 && !ctx.support_in_lane && (120..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_freeze_solo",
            category: "POSITIONING",
            severity: "WARNING",
            cooldown: 240,
            message: "Atrás e sem suporte — congele próximo da torre. Não avance para farm arriscado sozinho".into(),
        });
    }

    // ── EARLY (2–8min): freeze quando atrás com suporte ──────
    if diff <= -3 && ctx.support_in_lane && (120..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_freeze_with_sup",
            category: "POSITIONING",
            severity: "INFO",
            cooldown: 300,
            message: "Atrás na lane — congele a wave e espere uma abertura com o suporte antes de tentar trades".into(),
        });
    }

    // ── EARLY-MID: aproveite o suporte presente ───────────────
    // Muitos ADC jogam passivo com suporte presente.
    // Quando suporte está vivo e à frente, pressão é a jogada certa.
    if ctx.support_in_lane && ctx.is_ahead && (180..=720).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_exploit_support",
            category: "TRADE",
            severity: "INFO",
            cooldown: 360,
            message: "Suporte presente e você está na frente — seja agressivo. ADC com suporte saudável deve pressionar a lane".into(),
        });
    }

    // ── EARLY-MID: visão quando suporte sai da lane ──────────
    // ADC sozinho na bot sem wards é alvo primário de ganks.
    // Mais específico que o genérico `adc_support_absent`.
    if !ctx.support_in_lane && ward_score < 0.30 && (300..=720).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_ward_solo",
            category: "VISION",
            severity: "WARNING",
            cooldown: 180,
            message: "Sem suporte — ward o tribush e o rio. ADC sozinho sem visão é alvo prioritário de ganks coordenados".into(),
        });
    }

    // ── OBJETIVO: drake é o mais valioso para ADC ─────────────
    // Buffs de drake (infernal, nuvem, hextech) beneficiam o ADC
    // mais do que qualquer outro role. Baixo objective_control
    // histórico = oportunidades perdidas de vantagem de buff.
    if obj_ctrl < 0.35 && dragon_secs <= 90 && dragon_secs > 0 && ctx.game_time > 300 {
        out.push(RuleResult {
            id: "adc_drake_priority",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 240,
            message: "Drake é seu objetivo mais valioso — buffs de drake maximizam o ADC. Sempre participe das lutas de drake".into(),
        });
    }

    // ── VANTAGEM DE DRAKES: use-a ─────────────────────────────
    // Quando o time domina drakes, o ADC fica mais forte.
    // Momento ideal para forçar teamfights antes do próximo objetivo.
    if ctx.dragon_ally >= 2 && ctx.dragon_ally > ctx.dragon_enemy && ctx.game_time > 600 {
        out.push(RuleResult {
            id: "adc_drake_buff_active",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 480,
            message: format!(
                "Vocês dominam os drakes ({}-{}) — seus buffs de dano são uma vantagem real. Force fights agora",
                ctx.dragon_ally, ctx.dragon_enemy
            ),
        });
    }

    // ── POSICIONAMENTO: mortes evitáveis no mid game ──────────
    // ADC com agressividade alta + mortes entre 10-15min está
    // fazendo trades ruins ou se expondo sem suporte/jungle.
    if aggression > 0.70 && avg_deaths > 2.0 && (600..=900).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "adc_positioning_deaths",
            category: "POSITIONING",
            severity: "WARNING",
            cooldown: 300,
            message: format!(
                "Você morre {:.1}x em média no mid game — ADC morto não carrega. Priorize posicionamento seguro sobre agressividade",
                avg_deaths
            ),
        });
    }

    // ── MID-LATE: ADC não faz split push ─────────────────────
    // Diferente de TOP e MID, ADC perde muito valor saindo do grupo.
    // Sem o suporte e jungle por perto, é eliminado facilmente.
    // Dispara quando o placar está equilibrado ou atrás no late.
    if ctx.game_time > 900 && !ctx.is_ahead {
        out.push(RuleResult {
            id: "adc_no_split_push",
            category: "MACRO",
            severity: "INFO",
            cooldown: 400,
            message: "ADC não faz split push — seu dano é insubstituível em teamfights. Fique com o grupo no late game".into(),
        });
    }

    // ── LATE: group para baron ────────────────────────────────
    // Baron é a teamfight mais decisiva do jogo.
    // ADC NUNCA deve estar separado do time na luta de baron.
    if let Some(secs) = baron_secs {
        if secs <= 90 && ctx.game_time > 1100 {
            out.push(RuleResult {
                id: "adc_group_baron",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 150,
                message: "Baron iminente — group com o time agora. ADC é o carry da luta; sua presença é insubstituível".into(),
            });
        }
    }

    // ── LATE: backline e sobrevivência ────────────────────────
    // ADC agressivo que morre cedo em teamfights late game
    // elimina a principal fonte de dano sustentado do time.
    if aggression > 0.75 && ctx.game_time > 1200 {
        out.push(RuleResult {
            id: "adc_survive_late",
            category: "POSITIONING",
            severity: "WARNING",
            cooldown: 400,
            message: "Late game — posicione-se atrás do frontline e ataque de longe. Sua vida vale mais que qualquer kill".into(),
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
fn evaluate_jungle_rules(ctx: &ProcContext, out: &mut Vec<RuleResult>) {
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

    // ── EARLY (0–90s): decisão de rota — clear vs gank ───────
    // A decisão mais impactante do early jungle:
    // clear completo (mais farm, mais nível) vs gank precoce (mata, buff de lane).
    // Regra geral: se uma lane tem vantagem, ganke cedo. Se não, farm e evolua.
    if (60..=90).contains(&ctx.game_time) {
        let ally_ahead  = diff >= 0; // proxy grosseiro — placar ainda é 0-0
        let msg = if ally_ahead {
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

    // ── EARLY (2:55–3:30): scuttler — objetivo prioritário ───
    // O scuttler dá visão do rio E bônus de velocidade/HP regen.
    // Garantir o scuttler facilita o posicionamento para as Larvas (5:00) e o Drake (5:00).
    if (170..=215).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "jungle_scuttler_timing",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 120,
            message: "Scuttler em 3:30 — garanta o rio agora. Visão e regen antes das Larvas do Vazio e Drake (ambos às 5:00)".into(),
        });
    }

    // ── EARLY (4:30–5:10): Larvas do Vazio — decisão crítica ─
    // Patch 26.10: Larvas spawnam em 5:00 (300s) na Baron side,
    // ao mesmo tempo que o primeiro Drake (bot side).
    // O jungle PRECISA decidir qual objetivo priorizar.
    // Larvas → stacks de Voidmite (pressão de lane)
    // Drake → buff de time
    // Regra geral: Larvas se time precisa de pressão de torre; Drake se time tem engage.
    {
        let vg_secs = 300u32.saturating_sub(ctx.game_time);
        if (20..=70).contains(&vg_secs) {
            out.push(RuleResult {
                id: "jungle_voidgrubs_decision",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 120,
                message: "Larvas do Vazio em ~30s (Baron side) — Drake também spawna agora. \
                          Decida: Larvas (pressão de torre) ou Drake (buff de dano)".into(),
            });
        }
        // Larvas disponíveis e jungle ainda não foi lá
        if vg_secs == 0 && ctx.game_time < 600 {
            out.push(RuleResult {
                id: "jungle_voidgrubs_take",
                category: "OBJECTIVE",
                severity: "INFO",
                cooldown: 240,
                message: "Larvas do Vazio disponíveis na Baron side — cada stack de Voidmite empurra suas lanes passivamente".into(),
            });
        }
    }

    // ── EARLY (3–8min): ward no rio ao passar ────────────────
    // Junglers passam pelo rio frequentemente e raramente aproveitam
    // para colocar wards. Um ward de rio ao passar bloqueia ganks inimigos
    // e revela o jungle inimigo passando.
    if ward_score < 0.25 && (180..=480).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "jungle_ward_river",
            category: "VISION",
            severity: "INFO",
            cooldown: 240,
            message: "Ward o rio ao passar — jungle sem visão de rio perde rastreamento do inimigo e abre contra-ganks".into(),
        });
    }

    // ── EARLY-MID: guardar smite para objetivos ───────────────
    // Junglers iniciantes desperdiçam smite em camps próximos
    // ao spawn de um objetivo neutro. Smite é seu único seguro.
    if (dragon_secs > 0 && dragon_secs <= 60)
        || baron_secs.map(|s| s > 0 && s <= 60).unwrap_or(false)
    {
        out.push(RuleResult {
            id: "jungle_smite_preserve",
            category: "OBJECTIVE",
            severity: "WARNING",
            cooldown: 90,
            message: "Objetivo iminente — não use smite em camps agora. Guarde para garantir o objetivo neutro".into(),
        });
    }

    // ── EARLY-MID: farm quando muito atrás ────────────────────
    // Jungle que continua gankando quando está muito atrás
    // alimenta o inimigo e perde farm → espiral negativa.
    // A jogada correta é fazer farm full e esperar pelo jungle inimigo cometer erros.
    if diff <= -5 && ctx.game_time < 1200 {
        out.push(RuleResult {
            id: "jungle_farm_behind",
            category: "MACRO",
            severity: "WARNING",
            cooldown: 400,
            message: "Você está muito atrás — faça farm full em vez de gankar. Farm de jungle volta golpe por golpe no late".into(),
        });
    }

    // ── MID (5–15min): low objective_control — lembrete ──────
    // Jungle com baixo objective_control histórico perde o benefício
    // de objetivos mesmo quando tem oportunidade de pegá-los.
    if obj_ctrl < 0.35 && dragon_secs <= 120 && dragon_secs > 0 && ctx.game_time > 300 {
        out.push(RuleResult {
            id: "jungle_obj_ctrl_reminder",
            category: "OBJECTIVE",
            severity: "INFO",
            cooldown: 300,
            message: "Seu controle de objetivos histórico é baixo — priorize esse drake. Objetivos vencem partidas, ganks não".into(),
        });
    }

    // ── MID (8–15min): snowball quando na frente ──────────────
    // Quando o jungle está na frente em placar/objetivos,
    // a jogada correta é aumentar a pressão em vez de recuar.
    if diff >= 4 && ctx.game_time < 1200 && dragon_secs > 120 {
        out.push(RuleResult {
            id: "jungle_snowball",
            category: "MACRO",
            severity: "INFO",
            cooldown: 360,
            message: "Você está na frente — invada o jungle inimigo ou crie pressão nas lanes. Não deixe o jogo se equilibrar".into(),
        });
    }

    // ── MID (8–20min): contrajungle quando inimigo visível ───
    // Quando o jungle inimigo está do lado oposto do mapa,
    // há uma janela para invadir e roubar buffs/camps.
    // (opp_jungle_early/mid já cobrem o básico — aqui damos o detalhe do buff)
    if ctx.enemy_in_opp_jungle && (480..=1200).contains(&ctx.game_time) {
        out.push(RuleResult {
            id: "jungle_steal_buff",
            category: "TRADE",
            severity: "INFO",
            cooldown: 90,
            message: "Jungle inimigo visível no lado oposto — invada e roube o buff ou camp. Acumule vantagem de recursos".into(),
        });
    }

    // ── LATE (15–20min): setup de baron ──────────────────────
    // Baron exige preparação: empurrar mid, visão do baron
    // e smite disponível. Jungle é o responsável por orquestrar isso.
    if let Some(secs) = baron_secs {
        if (90..=180).contains(&secs) && ctx.game_time > 1050 {
            out.push(RuleResult {
                id: "jungle_baron_setup",
                category: "OBJECTIVE",
                severity: "WARNING",
                cooldown: 180,
                message: "Baron em 1-3 min — orquestre o setup: empurre mid com o time, ward o pit e garanta smite".into(),
            });
        }
    }

    // ── LATE: agressividade alta + contexto de late game ─────
    // Jungle agressivo que força fights no late sem visão
    // frequentemente alimenta o time inimigo no momento decisivo.
    if aggression > 0.75 && ctx.game_time > 1200 && !ctx.is_ahead {
        out.push(RuleResult {
            id: "jungle_aggressive_late",
            category: "POSITIONING",
            severity: "WARNING",
            cooldown: 400,
            message: "Late game sem vantagem — evite invades solo no jungle inimigo. Um erro de jungle late pode custar o jogo".into(),
        });
    }

    // ── LATE: drake para alma / Elder ────────────────────────
    // Jungle é responsável por garantir a alma do dragão.
    // Quando próximo de 4 drakes, esse objetivo se torna acima de tudo.
    if (ctx.dragon_ally == 3 || ctx.dragon_enemy == 3)
        && dragon_secs <= 90
        && dragon_secs > 0
    {
        let (who, urgency) = if ctx.dragon_ally == 3 {
            ("Você está a 1 drake da alma", "CRITICAL")
        } else {
            ("Inimigo está a 1 drake da alma", "CRITICAL")
        };
        out.push(RuleResult {
            id: "jungle_soul_drake",
            category: "OBJECTIVE",
            severity: urgency,
            cooldown: 120,
            message: format!("{who} — esse drake é prioridade máxima. Garanta smite e ward o pit agora"),
        });
    }
}

/// Remove duplicatas preservando a ordem de inserção.
fn dedup(mut v: Vec<&'static str>) -> Vec<&'static str> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|s| seen.insert(*s));
    v
}
