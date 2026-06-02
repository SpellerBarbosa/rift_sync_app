// ============================================================
// lcu/lockfile.rs — Parser do lockfile do League of Legends
//
// O LoL gera um arquivo `lockfile` enquanto o cliente está
// aberto. Ele contém a porta local e o token de autenticação
// necessários para acessar a LCU API.
//
// Formato: LeagueClient:PID:PORT:TOKEN:PROTOCOL
// Local padrão: C:\Riot Games\League of Legends\lockfile
// ============================================================

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

/// Dados extraídos do lockfile para autenticar na LCU API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LockfileData {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub protocol: String,  // quase sempre "https"
}

/// Lê e parseia o lockfile. Retorna Err se o LoL não estiver aberto
/// ou se o arquivo não puder ser localizado.
pub fn read_lockfile() -> Result<LockfileData> {
    let path = find_lockfile()?;
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Falha ao ler lockfile: {}", path.display()))?;

    parse_lockfile_content(&content)
        .with_context(|| format!("Formato inválido em: {}", path.display()))
}

/// Retorna `true` se o LoL está aberto (lockfile encontrado).
pub fn is_lol_running() -> bool {
    find_lockfile().is_ok()
}

/// Localiza o lockfile nos caminhos conhecidos.
/// Verifica drives A-Z e caminhos de variáveis de ambiente do Windows.
fn find_lockfile() -> Result<PathBuf> {
    let suffixes = [
        r"Riot Games\League of Legends\lockfile",
        r"Games\Riot Games\League of Legends\lockfile",
        r"Program Files\Riot Games\League of Legends\lockfile",
        r"Program Files (x86)\Riot Games\League of Legends\lockfile",
    ];

    // Todos os drives de A: a Z:
    for drive in b'A'..=b'Z' {
        let root = format!("{}:\\", drive as char);
        for suffix in &suffixes {
            let path = PathBuf::from(&root).join(suffix);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // Caminhos baseados em variáveis de ambiente Windows
    let env_roots: Vec<PathBuf> = [
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramFiles(x86)").ok(),
        std::env::var("LOCALAPPDATA").ok(),
        std::env::var("USERPROFILE").ok(),
    ]
    .into_iter()
    .flatten()
    .map(PathBuf::from)
    .collect();

    for root in &env_roots {
        let path = root.join(r"Riot Games\League of Legends\lockfile");
        if path.exists() {
            return Ok(path);
        }
    }

    bail!(
        "Lockfile do League of Legends não encontrado em nenhum caminho conhecido. \
         Verifique se o LoL está aberto."
    )
}

/// Parseia a string do lockfile no formato canônico da Riot.
/// Formato: `LeagueClient:PID:PORT:TOKEN:PROTOCOL`
fn parse_lockfile_content(content: &str) -> Result<LockfileData> {
    let parts: Vec<&str> = content.trim().splitn(5, ':').collect();

    if parts.len() != 5 {
        bail!(
            "Esperado 5 campos separados por ':', encontrado {}",
            parts.len()
        );
    }

    Ok(LockfileData {
        pid: parts[1].parse().context("PID inválido")?,
        port: parts[2].parse().context("Porta inválida")?,
        token: parts[3].to_string(),
        protocol: parts[4].to_string(),
    })
}

// ── Testes ───────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lockfile_valido() {
        let content = "LeagueClient:12345:57423:meu_token_secreto:https";
        let data = parse_lockfile_content(content).unwrap();
        assert_eq!(data.pid, 12345);
        assert_eq!(data.port, 57423);
        assert_eq!(data.token, "meu_token_secreto");
        assert_eq!(data.protocol, "https");
    }

    #[test]
    fn parse_lockfile_invalido_retorna_err() {
        let content = "formato:invalido";
        assert!(parse_lockfile_content(content).is_err());
    }
}
