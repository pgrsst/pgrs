use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::domain::connection::Connection;
use crate::core::ports::db_connection::{DbConnection, QueryResult};

pub struct PostgresDb {
    client: RefCell<postgres::Client>,
}

impl PostgresDb {
    pub fn new(connection: &Connection) -> Result<Self, String> {
        let conn_str = format!(
            "host={} port={} user={} password={} dbname={}",
            connection.host,
            connection.port,
            connection.username,
            connection.password,
            connection.database
        );
        let client = postgres::Client::connect(&conn_str, postgres::NoTls)
            .map_err(|e| format!("could not connect to '{}': {}", connection.name, e))?;
        Ok(Self {
            client: RefCell::new(client),
        })
    }
}

impl DbConnection for PostgresDb {
    fn execute(&self, query: &str) -> Result<QueryResult, String> {
        let mut client = self.client.borrow_mut();
        let rows = client.query(query, &[]).map_err(|e| e.to_string())?;

        if rows.is_empty() {
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
            });
        }

        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let data = rows
            .iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| cell_to_string(row, i))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        Ok(QueryResult {
            columns,
            rows: data,
        })
    }

    fn list_tables(&self) -> Result<Vec<String>, String> {
        let mut client = self.client.borrow_mut();
        let rows = client
            .query(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
                 ORDER BY table_name",
                &[],
            )
            .map_err(|e| e.to_string())?;
        Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
    }

    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
        let mut client = self.client.borrow_mut();
        let rows = client
            .query(
                "SELECT table_name, column_name FROM information_schema.columns \
                 WHERE table_schema = 'public' \
                 ORDER BY table_name, ordinal_position",
                &[],
            )
            .map_err(|e| e.to_string())?;

        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for row in &rows {
            let table: String = row.get(0);
            let column: String = row.get(1);
            map.entry(table).or_default().push(column);
        }
        Ok(map)
    }
}

fn cell_to_string(row: &postgres::Row, idx: usize) -> String {
    if let Ok(Some(v)) = row.try_get::<_, Option<String>>(idx) {
        return v;
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<i64>>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<i32>>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<f64>>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<bool>>(idx) {
        return v.to_string();
    }
    "NULL".to_string()
}
