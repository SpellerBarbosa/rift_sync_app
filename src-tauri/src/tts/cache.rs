// ============================================================
// tts/cache.rs — Cache LRU de respostas TTS com GC por TTL
//
// Evita chamadas repetidas à API para a mesma frase+voz.
// Em uma sessão de jogo as mesmas ~30 frases se repetem;
// com cache a primeira chamada paga o custo, as seguintes
// são instant (sem rede, sem latência).
//
// Limites:
//   MAX_ENTRIES = 80    entradas únicas (frase+voz)
//   MAX_BYTES   = 10 MB total de data URLs em memória
//   TTL         = 2 h   — expiração por inatividade
//   GC_INTERVAL = 10 min — tarefa de coleta em background
//
// Evicção:
//   1. GC remove entradas expiradas (TTL)
//   2. LRU remove as menos usadas quando limites são atingidos
// ============================================================

use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::Result;

pub const MAX_ENTRIES:   usize    = 80;
pub const MAX_BYTES:     usize    = 10 * 1024 * 1024; // 10 MB
pub const TTL:           Duration = Duration::from_secs(7_200); // 2 h
pub const GC_INTERVAL:   Duration = Duration::from_secs(600);   // 10 min

// ── Entrada individual ────────────────────────────────────────

struct Entry {
    /// Data URL base64 pronta para `new Audio(dataUrl)` no frontend.
    data_url:  String,
    /// Tamanho em bytes do data_url (pré-calculado).
    bytes:     usize,
    /// Última vez que esta entrada foi acessada (cache hit ou insert).
    last_used: Instant,
    /// Contador de hits — para logging.
    hits:      u32,
}

// ── Cache principal ───────────────────────────────────────────

pub struct TtsCache {
    /// Cliente HTTP para chamadas à API HuggingFace Kokoro (vozes pt-BR).
    pub client: reqwest::Client,
    /// Mapa de chave → entrada. Chave = "voice\0text".
    map: HashMap<String, Entry>,
    /// Soma dos bytes de todas as entradas (atualizado a cada insert/evict).
    total_bytes: usize,
}

impl TtsCache {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()?;
        Ok(Self { client, map: HashMap::new(), total_bytes: 0 })
    }

    // ── Leitura ───────────────────────────────────────────────

    /// Retorna o data URL cacheado e atualiza `last_used`. `None` = cache miss.
    pub fn get(&mut self, text: &str, voice: &str) -> Option<String> {
        let key = Self::make_key(voice, text);
        let entry = self.map.get_mut(&key)?;
        entry.last_used = Instant::now();
        entry.hits      = entry.hits.saturating_add(1);
        tracing::debug!(
            "[TTS cache] HIT (hits={}) {:.50}",
            entry.hits, text
        );
        Some(entry.data_url.clone())
    }

    // ── Escrita ───────────────────────────────────────────────

    /// Insere ou atualiza uma entrada, aplicando evicção se necessário.
    /// Se o data_url por si só ultrapassa MAX_BYTES, a entrada é descartada
    /// (proteção contra respostas anormalmente grandes da API).
    pub fn insert(&mut self, text: &str, voice: &str, data_url: String) {
        let bytes = data_url.len();

        // Guarda de segurança: não armazena áudio gigante
        if bytes > MAX_BYTES / 4 {
            tracing::warn!(
                "[TTS cache] resposta muito grande ({:.0}KB), não cacheada: {:.40}",
                bytes as f64 / 1024.0, text
            );
            return;
        }

        let key = Self::make_key(voice, text);

        // Remove entrada anterior (atualização)
        if let Some(old) = self.map.remove(&key) {
            self.total_bytes = self.total_bytes.saturating_sub(old.bytes);
        }

        // Evicção antes de inserir
        self.evict(bytes);

        // Insere
        self.total_bytes += bytes;
        self.map.insert(key, Entry {
            data_url,
            bytes,
            last_used: Instant::now(),
            hits:      0,
        });

        tracing::debug!(
            "[TTS cache] INSERT {} entradas | {:.1}KB / {:.0}KB",
            self.map.len(),
            self.total_bytes as f64 / 1024.0,
            MAX_BYTES as f64 / 1024.0,
        );
    }

    // ── GC e evicção ──────────────────────────────────────────

    /// Coleta de lixo completa: remove entradas com TTL expirado.
    /// Chamada pela tarefa GC em background a cada GC_INTERVAL.
    pub fn gc(&mut self) {
        let before_count = self.map.len();
        let before_bytes = self.total_bytes;

        self.map.retain(|_, e| e.last_used.elapsed() < TTL);
        self.total_bytes = self.map.values().map(|e| e.bytes).sum();

        let freed_count = before_count - self.map.len();
        let freed_bytes = before_bytes - self.total_bytes;

        if freed_count > 0 {
            tracing::info!(
                "[TTS cache] GC: {} entradas removidas, {:.1}KB liberados \
                 | restam {} entradas ({:.1}KB)",
                freed_count,
                freed_bytes as f64 / 1024.0,
                self.map.len(),
                self.total_bytes as f64 / 1024.0,
            );
        } else {
            tracing::debug!(
                "[TTS cache] GC: nada expirado \
                 | {} entradas ({:.1}KB)",
                self.map.len(),
                self.total_bytes as f64 / 1024.0,
            );
        }
    }

    /// Métricas para logging/debug: (entradas, bytes_usados).
    pub fn stats(&self) -> (usize, usize) {
        (self.map.len(), self.total_bytes)
    }

    // ── Internos ──────────────────────────────────────────────

    /// Evicção em duas passagens:
    ///   1. Remove expiradas por TTL (grátis — já não úteis)
    ///   2. Remove LRU até que (len + 1 ≤ MAX_ENTRIES) e
    ///      (total + incoming ≤ MAX_BYTES)
    fn evict(&mut self, incoming_bytes: usize) {
        // Passagem 1 — TTL
        self.map.retain(|_, e| e.last_used.elapsed() < TTL);
        self.total_bytes = self.map.values().map(|e| e.bytes).sum();

        // Passagem 2 — LRU
        loop {
            let over_count = self.map.len() >= MAX_ENTRIES;
            let over_bytes = self.total_bytes + incoming_bytes > MAX_BYTES;

            if !over_count && !over_bytes {
                break;
            }
            if self.map.is_empty() {
                break;
            }

            // Encontra a entrada menos usada recentemente
            let lru_key = self
                .map
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());

            if let Some(key) = lru_key {
                if let Some(removed) = self.map.remove(&key) {
                    self.total_bytes = self.total_bytes.saturating_sub(removed.bytes);
                    tracing::debug!(
                        "[TTS cache] LRU evict {:.1}KB (restam {})",
                        removed.bytes as f64 / 1024.0,
                        self.map.len(),
                    );
                }
            }
        }
    }

    #[inline]
    fn make_key(voice: &str, text: &str) -> String {
        // Separador '\0' nunca aparece em texto ou nome de voz
        format!("{voice}\0{text}")
    }
}
