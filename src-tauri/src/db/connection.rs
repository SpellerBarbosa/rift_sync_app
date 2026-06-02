// ============================================================
// db/connection.rs — Inicialização e configuração do SQLite
//
// O banco é armazenado em AppData/com.riftsync.ai/riftsync.db
// Configurado com WAL mode para melhor performance concorrente
// ============================================================

use anyhow::{Context, Result};
use rusqlite::Connection;
use tauri::Manager;

use super::migrations;

/// Inicializa o banco SQLite: cria o arquivo se necessário,
/// aplica configurações de performance e executa migrações.
pub fn init_db(app: &tauri::AppHandle) -> Result<Connection> {
    let db_path = resolve_db_path(app)?;

    // Garante que o diretório pai existe
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Falha ao criar diretório: {}", parent.display()))?;
    }

    let conn = Connection::open(&db_path)
        .with_context(|| format!("Falha ao abrir banco em: {}", db_path.display()))?;

    // WAL mode: permite leituras concorrentes sem bloquear escritas
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    // NORMAL: flush periódico — boa performance sem risco de corrupção
    conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
    // Garante integridade referencial em todas as operações
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    migrations::run_migrations(&conn)?;

    tracing::info!("Banco inicializado: {}", db_path.display());

    Ok(conn)
}

/// Resolve o caminho do arquivo do banco de dados.
/// Ex: C:\Users\Nome\AppData\Roaming\com.riftsync.ai\riftsync.db
fn resolve_db_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf> {
    let data_dir = app
        .path()
        .app_data_dir()
        .context("Não foi possível localizar o diretório AppData")?;

    Ok(data_dir.join("riftsync.db"))
}
