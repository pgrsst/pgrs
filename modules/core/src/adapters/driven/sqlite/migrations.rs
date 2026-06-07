use rusqlite::Connection;

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS connections (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE,
    host        TEXT    NOT NULL,
    port        INTEGER NOT NULL DEFAULT 5432,
    username    TEXT    NOT NULL,
    password    TEXT    NOT NULL,
    database    TEXT    NOT NULL,
    tls         TEXT    NOT NULL DEFAULT 'disable',
    environment TEXT,
    uuid        TEXT
);

CREATE TABLE IF NOT EXISTS query_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    query       TEXT    NOT NULL,
    executed_at INTEGER NOT NULL,
    UNIQUE(db_name, query)
);
CREATE INDEX IF NOT EXISTS idx_history_db ON query_history(db_name, executed_at);

CREATE TABLE IF NOT EXISTS table_access (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    table_name  TEXT    NOT NULL,
    query_id    INTEGER REFERENCES query_history(id),
    accessed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_table_access_db ON table_access(db_name, table_name);

CREATE TABLE IF NOT EXISTS column_access (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    table_name  TEXT    NOT NULL,
    column_name TEXT    NOT NULL,
    query_id    INTEGER REFERENCES query_history(id),
    accessed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_column_access_db ON column_access(db_name, table_name);

CREATE TABLE IF NOT EXISTS schema_tables (
    db_name    TEXT    NOT NULL,
    table_name TEXT    NOT NULL,
    cached_at  INTEGER NOT NULL,
    PRIMARY KEY (db_name, table_name)
);

CREATE TABLE IF NOT EXISTS schema_columns (
    db_name     TEXT NOT NULL,
    table_name  TEXT NOT NULL,
    column_name TEXT NOT NULL,
    data_type   TEXT,
    cached_at   INTEGER NOT NULL,
    PRIMARY KEY (db_name, table_name, column_name)
);
";

const SCHEMA_V2: &str = "
DROP TABLE IF EXISTS schema_columns;
DROP TABLE IF EXISTS schema_tables;
DROP TABLE IF EXISTS column_access;
DROP TABLE IF EXISTS table_access;
DROP TABLE IF EXISTS query_history;

CREATE TABLE query_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    query         TEXT    NOT NULL,
    executed_at   INTEGER NOT NULL,
    UNIQUE(connection_id, query)
);
CREATE INDEX IF NOT EXISTS idx_history_conn ON query_history(connection_id, executed_at);

CREATE TABLE table_access (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    query_id      INTEGER REFERENCES query_history(id),
    accessed_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_table_access_conn ON table_access(connection_id, table_name);

CREATE TABLE column_access (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    column_name   TEXT    NOT NULL,
    query_id      INTEGER REFERENCES query_history(id),
    accessed_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_column_access_conn ON column_access(connection_id, table_name);

CREATE TABLE schema_tables (
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    cached_at     INTEGER NOT NULL,
    PRIMARY KEY (connection_id, table_name)
);

CREATE TABLE schema_columns (
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    column_name   TEXT    NOT NULL,
    data_type     TEXT,
    cached_at     INTEGER NOT NULL,
    PRIMARY KEY (connection_id, table_name, column_name)
);
";

pub(super) fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    if version < 2 {
        eprintln!("pgrs: migrating database schema (v1 → v2): query history will be cleared.");
        conn.execute_batch(SCHEMA_V2)?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    Ok(())
}
