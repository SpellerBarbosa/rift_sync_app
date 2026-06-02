// ============================================================
// tts/client.rs — HTTP client para a TTS API
//
// Endpoint: POST https://spell2014-riftsyncai.hf.space/tts
// Payload: { text: string, voice?: string }
// Resposta: stream de áudio (MP3 ou WAV)
//
// TODO (Fase 6): implementar envio, recebimento e reprodução
// ============================================================

/// Stub — Cliente TTS ainda não implementado.
pub struct TtsClient {
    #[allow(dead_code)]
    base_url: String,
}

impl TtsClient {
    pub fn new() -> Self {
        Self {
            base_url: "https://spell2014-riftsyncai.hf.space/tts".to_string(),
        }
    }

    /// Envia texto para a API e reproduz o áudio recebido.
    /// TODO (Fase 6): implementar com reqwest + rodio para reprodução
    pub async fn speak(&self, text: &str) -> anyhow::Result<()> {
        tracing::debug!("[TTS stub] fala: \"{}\"", text);
        // TODO: POST {text} → receber bytes de áudio → reproduzir com rodio
        Ok(())
    }
}
