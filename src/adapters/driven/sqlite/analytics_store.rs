use crate::core::domain::analytics::{FreqEntry, HistoryEntry};
use crate::core::ports::analytics_port::AnalyticsPort;
use super::SqliteRepository;

fn record_query_inner(
    conn: &mut rusqlite::Connection,
    connection_id: i64,
    query: &str,
    tables: &[String],
    columns: &[(String, String)],
    now: i64,
) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO query_history (connection_id, query, executed_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(connection_id, query) DO UPDATE SET executed_at = excluded.executed_at",
        rusqlite::params![connection_id, query, now],
    )?;
    let query_id: i64 = tx.query_row(
        "SELECT id FROM query_history WHERE connection_id = ?1 AND query = ?2",
        rusqlite::params![connection_id, query],
        |r| r.get(0),
    )?;
    for table in tables {
        tx.execute(
            "INSERT INTO table_access (connection_id, table_name, query_id, accessed_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![connection_id, table, query_id, now],
        )?;
    }
    for (table, column) in columns {
        tx.execute(
            "INSERT INTO column_access (connection_id, table_name, column_name, query_id, accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![connection_id, table, column, query_id, now],
        )?;
    }
    tx.commit()
}

impl AnalyticsPort for SqliteRepository {
    fn record_query(
        &self,
        connection_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String, String)],
    ) {
        let mut conn = self.conn.lock().unwrap();
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            eprintln!("pgrs: analytics: unknown connection '{connection_name}'");
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = record_query_inner(&mut conn, connection_id, query, tables, columns, now) {
            eprintln!("pgrs: analytics write failed: {e}");
        }
    }

    fn get_history(&self, connection_name: &str) -> Vec<HistoryEntry> {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<HistoryEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT id, query, executed_at FROM query_history
                 WHERE connection_id = ?1 ORDER BY executed_at DESC LIMIT 50",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
                Ok(HistoryEntry { id: r.get(0)?, query: r.get(1)?, executed_at: r.get(2)? })
            })?;
            rows.collect()
        })();
        match result {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("pgrs: analytics read failed: {e}");
                vec![]
            }
        }
    }

    fn get_frequent_tables(&self, connection_name: &str) -> Vec<FreqEntry> {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<FreqEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT table_name, COUNT(*) as cnt FROM table_access
                 WHERE connection_id = ?1 GROUP BY table_name ORDER BY cnt DESC",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
                Ok(FreqEntry { name: r.get(0)?, count: r.get::<_, i64>(1)? as u64 })
            })?;
            rows.collect()
        })();
        match result {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("pgrs: analytics read failed: {e}");
                vec![]
            }
        }
    }

    fn get_frequent_columns(&self, connection_name: &str, table: &str) -> Vec<FreqEntry> {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<FreqEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT column_name, COUNT(*) as cnt FROM column_access
                 WHERE connection_id = ?1 AND table_name = ?2
                 GROUP BY column_name ORDER BY cnt DESC",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id, table], |r| {
                Ok(FreqEntry { name: r.get(0)?, count: r.get::<_, i64>(1)? as u64 })
            })?;
            rows.collect()
        })();
        match result {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("pgrs: analytics read failed: {e}");
                vec![]
            }
        }
    }
}
