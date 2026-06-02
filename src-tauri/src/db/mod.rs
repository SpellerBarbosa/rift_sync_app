// ============================================================
// db/ — Módulo de banco de dados SQLite
//
// Sub-módulos:
//   connection  — inicializa e configura a conexão SQLite
//   migrations  — aplica migrações em ordem idempotente
//   models      — structs Rust que espelham o schema
// ============================================================

pub mod connection;
pub mod migrations;
pub mod models;
