use std::collections::HashMap;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use super::SqliteRepository;

fn save_schema_inner(
    conn: &mut rusqlite::Connection,
    connection_id: i64,
    schema: &HashMap<String, Vec<String>>,
    now: i64,
) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM schema_columns WHERE connection_id = ?1",
        rusqlite::params![connection_id],
    )?;
    tx.execute(
        "DELETE FROM schema_tables WHERE connection_id = ?1",
        rusqlite::params![connection_id],
    )?;
    for (table, columns) in schema {
        tx.execute(
            "INSERT OR REPLACE INTO schema_tables (connection_id, table_name, cached_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![connection_id, table, now],
        )?;
        for col in columns {
            tx.execute(
                "INSERT OR REPLACE INTO schema_columns (connection_id, table_name, column_name, cached_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![connection_id, table, col, now],
            )?;
        }
    }
    tx.commit()
}

impl SchemaCachePort for SqliteRepository {
    fn save_schema(&self, connection_name: &str, schema: &HashMap<String, Vec<String>>) {
        let mut conn = self.conn.lock().unwrap();
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            eprintln!("pgrs: schema cache: unknown connection '{connection_name}'");
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = save_schema_inner(&mut conn, connection_id, schema, now) {
            eprintln!("pgrs: schema cache write failed: {e}");
        }
    }

    fn load_schema(&self, connection_name: &str) -> Option<HashMap<String, Vec<String>>> {
        let conn = self.conn.lock().unwrap();
        let connection_id = SqliteRepository::connection_id_for(&conn, connection_name)?;
        let result: Result<Vec<(String, String)>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT table_name, column_name FROM schema_columns
                 WHERE connection_id = ?1 ORDER BY table_name, rowid",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            rows.collect()
        })();

        match result {
            Ok(rows) if rows.is_empty() => None,
            Ok(rows) => {
                let mut map: HashMap<String, Vec<String>> = HashMap::new();
                for (table, col) in rows {
                    map.entry(table).or_default().push(col);
                }
                Some(map)
            }
            Err(e) => {
                eprintln!("pgrs: schema cache read failed: {e}");
                None
            }
        }
    }

    fn invalidate(&self, connection_name: &str) {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            return;
        };
        if let Err(e) = conn.execute(
            "DELETE FROM schema_columns WHERE connection_id = ?1",
            rusqlite::params![connection_id],
        ) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
            return;
        }
        if let Err(e) = conn.execute(
            "DELETE FROM schema_tables WHERE connection_id = ?1",
            rusqlite::params![connection_id],
        ) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
        }
    }
}
