// ============================================================
// tts/ — Sistema de Text-to-Speech para alertas de voz
//
// Sub-módulos:
//   client — envia texto para a TTS API e recebe áudio
//   queue  — gerencia fila priorizada de mensagens de áudio
//
// API TTS: https://spell2014-riftsyncai.hf.space/tts
// Implementado na Fase 6 do blueprint.
// ============================================================

pub mod cache;
pub mod client;
pub mod queue;
pub mod winrt;
