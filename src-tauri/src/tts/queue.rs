// ============================================================
// tts/queue.rs — Fila priorizada de mensagens de áudio
//
// Regras da fila (blueprint, Fase 6):
//   - Mensagem CRITICAL interrompe INFO em andamento
//   - Cooldown por categoria para evitar spam
//   - Modo silencioso global via AtomicBool
//
// TODO (Fase 6): implementar fila com VecDeque + cooldowns
// ============================================================

use std::collections::VecDeque;
use std::time::Instant;

/// Prioridade de uma mensagem TTS (maior número = maior prioridade).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TtsPriority {
    Info     = 1,
    Warning  = 2,
    Critical = 3,
}

/// Mensagem na fila de áudio.
#[derive(Debug, Clone)]
pub struct TtsMessage {
    pub text: String,
    pub priority: TtsPriority,
    pub category: String,     // ex: "VISION", "MACRO" — usado para cooldown
    pub cooldown_secs: u64,
}

/// Stub — Fila TTS ainda não implementada.
pub struct TtsQueue {
    #[allow(dead_code)]
    queue: VecDeque<TtsMessage>,
    #[allow(dead_code)]
    last_played: Option<Instant>,
}

impl TtsQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            last_played: None,
        }
    }

    /// Adiciona uma mensagem à fila respeitando prioridade.
    /// TODO (Fase 6): processar fila e emitir via TtsClient
    pub fn enqueue(&mut self, msg: TtsMessage) {
        tracing::debug!("[TTS queue stub] enqueued: \"{}\"", msg.text);
        self.queue.push_back(msg);
    }
}
