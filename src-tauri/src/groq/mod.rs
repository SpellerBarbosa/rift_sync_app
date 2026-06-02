// ============================================================
// groq/ — Integração com a API Groq (pós-game + perfil)
//
// Usado exclusivamente na tela de pós-game (Fase 8) e no
// perfil comportamental do jogador (Fase 9).
// NÃO é usado em tempo real — para coaching in-game usa o
// ProcEngine determinístico (src/coach/procedural.rs).
// ============================================================

pub mod client;
