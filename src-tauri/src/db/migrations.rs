// ============================================================
// db/migrations.rs — Sistema de migrações versionadas
//
// Cada migração é idempotente: segura para reexecutar.
// Adicione novas migrações ao final do slice MIGRATIONS.
// ============================================================

use anyhow::Result;
use rusqlite::Connection;

/// Executa todas as migrações pendentes em ordem sequencial.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // Tabela de controle de migrações aplicadas
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            name       TEXT NOT NULL UNIQUE,
            applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );",
    )?;

    for (name, sql) in MIGRATIONS {
        let already_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !already_applied {
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT INTO _migrations (name) VALUES (?1)",
                [name],
            )?;
            tracing::debug!("Migração aplicada: {}", name);
        }
    }

    tracing::info!("{} migração(ões) verificada(s)", MIGRATIONS.len());
    Ok(())
}

// ── Lista de migrações ───────────────────────────────────────
// Nomenclatura: NNN_descricao_curta
// Nunca modifique uma migração já aplicada — adicione uma nova.

const MIGRATIONS: &[(&str, &str)] = &[
    ("001_schema_inicial",        MIGRATION_001),
    ("002_player_icon_level",     MIGRATION_002),
    ("003_champion_meta_stats",   MIGRATION_003),
    ("004_champion_builds_wards", MIGRATION_004),
    ("005_wards_pk_role_tier",    MIGRATION_005),
    ("006_patterns_unique_player",MIGRATION_006),
];

// ── SQL das migrações ────────────────────────────────────────

/// Adiciona UNIQUE(player_id) em player_patterns para suportar upsert direto.
/// Recria a tabela preservando os dados existentes.
const MIGRATION_006: &str = "
CREATE TABLE IF NOT EXISTS player_patterns_new (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id             INTEGER UNIQUE REFERENCES players(id),
    aggression_score      REAL DEFAULT 0.5,
    deaths_without_vision INTEGER DEFAULT 0,
    avg_deaths_10_15      REAL DEFAULT 0,
    lane_dominance        REAL DEFAULT 0.5,
    objective_control     REAL DEFAULT 0.5,
    roam_frequency        REAL DEFAULT 0.5,
    tp_efficiency         REAL DEFAULT 0.5,
    ward_score            REAL DEFAULT 0.5,
    updated_at            DATETIME DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO player_patterns_new
    SELECT id, player_id, aggression_score, deaths_without_vision,
           avg_deaths_10_15, lane_dominance, objective_control,
           roam_frequency, tp_efficiency, ward_score, updated_at
    FROM player_patterns;

DROP TABLE player_patterns;
ALTER TABLE player_patterns_new RENAME TO player_patterns;

CREATE INDEX IF NOT EXISTS idx_patterns_player_id ON player_patterns(player_id);
";

/// Expande PK de champion_wards para (champion_name, side, role, tier).
/// Permite armazenar posicionamentos separados por role e tier (Challenger vs Diamond etc.).
/// A tabela antiga é substituída — dados antigos serão re-sincronizados no próximo lock-in.
const MIGRATION_005: &str = "
DROP TABLE IF EXISTS champion_wards;

CREATE TABLE IF NOT EXISTS champion_wards (
    champion_name   TEXT    NOT NULL,
    side            TEXT    NOT NULL,
    role            TEXT    NOT NULL DEFAULT '',
    tier            TEXT    NOT NULL DEFAULT '',
    total_games     INTEGER NOT NULL DEFAULT 0,
    placements_json TEXT,
    heatmap_json    TEXT,
    synced_at       DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (champion_name, side, role, tier)
);

CREATE INDEX IF NOT EXISTS idx_wards_lookup ON champion_wards(champion_name, side, tier, role);
";

/// Builds, runas, itens, habilidades, spikes e wards por campeão.
/// Dados armazenados como JSON para preservar a estrutura original da API.
const MIGRATION_004: &str = "
-- Build completa por campeão (runas, itens, habilidades, spikes).
-- PRIMARY KEY é o nome do campeão (ex: 'Garen', 'LeeSin').
CREATE TABLE IF NOT EXISTS champion_builds (
    champion_name  TEXT    PRIMARY KEY,
    role           TEXT    NOT NULL DEFAULT '',
    tier           TEXT    NOT NULL DEFAULT '',
    runes_json     TEXT,
    items_json     TEXT,
    skills_json    TEXT,
    spikes_json    TEXT,
    synced_at      DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Posicionamentos de ward por campeão e lado do mapa.
CREATE TABLE IF NOT EXISTS champion_wards (
    champion_name   TEXT    NOT NULL,
    side            TEXT    NOT NULL,
    role            TEXT    NOT NULL DEFAULT '',
    tier            TEXT    NOT NULL DEFAULT '',
    total_games     INTEGER NOT NULL DEFAULT 0,
    placements_json TEXT,
    heatmap_json    TEXT,
    synced_at       DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (champion_name, side)
);

CREATE INDEX IF NOT EXISTS idx_builds_name  ON champion_builds(champion_name);
CREATE INDEX IF NOT EXISTS idx_wards_name   ON champion_wards(champion_name);
";

/// Cache de estatísticas de meta de campeões vindas da SpellCoach API.
/// PRIMARY KEY (champion_id, role, tier) garante upsert idempotente.
const MIGRATION_003: &str = "
CREATE TABLE IF NOT EXISTS champion_meta_stats (
    champion_id               INTEGER NOT NULL,
    role                      TEXT    NOT NULL,
    tier                      TEXT    NOT NULL,
    win_rate                  REAL    NOT NULL DEFAULT 0,
    wins                      INTEGER NOT NULL DEFAULT 0,
    losses                    INTEGER NOT NULL DEFAULT 0,
    total_games               INTEGER NOT NULL DEFAULT 0,
    avg_kills                 REAL    NOT NULL DEFAULT 0,
    avg_deaths                REAL    NOT NULL DEFAULT 0,
    avg_assists               REAL    NOT NULL DEFAULT 0,
    avg_kda                   REAL    NOT NULL DEFAULT 0,
    avg_damage_taken          REAL    NOT NULL DEFAULT 0,
    avg_damage_to_champions   REAL    NOT NULL DEFAULT 0,
    avg_damage_to_objectives  REAL    NOT NULL DEFAULT 0,
    avg_vision_score          REAL    NOT NULL DEFAULT 0,
    avg_wards_placed          REAL    NOT NULL DEFAULT 0,
    avg_wards_killed          REAL    NOT NULL DEFAULT 0,
    avg_control_wards_bought  REAL    NOT NULL DEFAULT 0,
    pick_rate                 REAL    NOT NULL DEFAULT 0,
    power_phase_early         REAL    NOT NULL DEFAULT 0,
    power_phase_mid           REAL    NOT NULL DEFAULT 0,
    power_phase_late          REAL    NOT NULL DEFAULT 0,
    synced_at                 DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (champion_id, role, tier)
);
CREATE INDEX IF NOT EXISTS idx_meta_champion    ON champion_meta_stats(champion_id);
CREATE INDEX IF NOT EXISTS idx_meta_role_tier   ON champion_meta_stats(role, tier);
";

/// Adiciona ícone e nível do invocador para exibição no Dashboard.
const MIGRATION_002: &str = "
ALTER TABLE players ADD COLUMN profile_icon_id INTEGER NOT NULL DEFAULT 0;
ALTER TABLE players ADD COLUMN summoner_level  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE players ADD COLUMN tier_en         TEXT;
";

/// Cria todas as tabelas do schema definido no blueprint (seção 6).
const MIGRATION_001: &str = "
-- Jogadores monitorados pelo RiftSync AI
CREATE TABLE IF NOT EXISTS players (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    puuid      TEXT UNIQUE NOT NULL,
    riot_name  TEXT NOT NULL,
    tag        TEXT NOT NULL,
    region     TEXT NOT NULL DEFAULT 'BR1',
    role       TEXT,           -- TOP | JGL | MID | ADC | SUP
    rank       TEXT,           -- IRON .. CHALLENGER
    lp         INTEGER,
    winrate    REAL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Partidas registradas para análise e perfil comportamental
CREATE TABLE IF NOT EXISTS matches (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id     INTEGER REFERENCES players(id),
    match_id      TEXT UNIQUE NOT NULL,
    champion_id   INTEGER NOT NULL,
    champion_name TEXT NOT NULL,
    role          TEXT,
    result        TEXT NOT NULL,   -- WIN | LOSS | REMAKE
    kills         INTEGER,
    deaths        INTEGER,
    assists       INTEGER,
    cs            INTEGER,
    cs_per_min    REAL,
    vision_score  INTEGER,
    damage_dealt  INTEGER,
    gold_earned   INTEGER,
    duration      INTEGER,         -- segundos
    played_at     DATETIME
);

-- Métricas comportamentais acumuladas por jogador
CREATE TABLE IF NOT EXISTS player_patterns (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id             INTEGER REFERENCES players(id),
    aggression_score      REAL DEFAULT 0.5,   -- 0=passivo, 1=agressivo
    deaths_without_vision INTEGER DEFAULT 0,
    avg_deaths_10_15      REAL DEFAULT 0,      -- mortes entre min 10-15
    lane_dominance        REAL DEFAULT 0.5,
    objective_control     REAL DEFAULT 0.5,
    roam_frequency        REAL DEFAULT 0.5,
    tp_efficiency         REAL DEFAULT 0.5,
    ward_score            REAL DEFAULT 0.5,
    updated_at            DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Alertas e feedbacks gerados durante cada sessão de coaching
CREATE TABLE IF NOT EXISTS coaching_sessions (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    match_id  TEXT REFERENCES matches(match_id),
    timestamp INTEGER NOT NULL,  -- segundo da partida
    category  TEXT NOT NULL,     -- VISION | MACRO | TRADE | OBJECTIVE | POSITIONING
    severity  TEXT NOT NULL,     -- INFO | WARNING | CRITICAL
    message   TEXT NOT NULL,
    was_heard BOOLEAN DEFAULT FALSE
);

-- Cache local de dados do Data Dragon (campeões, itens, runas)
CREATE TABLE IF NOT EXISTS champion_cache (
    champion_id INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    key         TEXT NOT NULL,
    data_json   TEXT NOT NULL,  -- JSON completo do campeão
    cached_at   DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Configurações do usuário (key-value flexível)
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Índices para consultas frequentes
CREATE INDEX IF NOT EXISTS idx_matches_player_id   ON matches(player_id);
CREATE INDEX IF NOT EXISTS idx_matches_played_at   ON matches(played_at);
CREATE INDEX IF NOT EXISTS idx_coaching_match_id   ON coaching_sessions(match_id);
CREATE INDEX IF NOT EXISTS idx_patterns_player_id  ON player_patterns(player_id);
";
