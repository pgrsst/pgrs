use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::domain::connection::{Connection, TlsMode};
use crate::core::ports::db_connection::{DbConnection, QueryResult};

pub struct PostgresDb {
    client: RefCell<postgres::Client>,
}

impl PostgresDb {
    pub fn new(connection: &Connection) -> Result<Self, String> {
        let mut config = postgres::Config::new();
        config
            .host(&connection.host)
            .port(connection.port)
            .user(&connection.username)
            .password(connection.password.as_bytes())
            .dbname(&connection.database);

        let client = match connection.tls {
            TlsMode::Disable => config
                .connect(postgres::NoTls)
                .map_err(|e| format!("could not connect to '{}': {}", connection.name, e))?,
            TlsMode::Require => {
                let tls = native_tls::TlsConnector::new()
                    .map_err(|e| format!("failed to build TLS connector: {}", e))?;
                let tls = postgres_native_tls::MakeTlsConnector::new(tls);
                config
                    .connect(tls)
                    .map_err(|e| format!("could not connect to '{}': {}", connection.name, e))?
            }
        };

        Ok(Self {
            client: RefCell::new(client),
        })
    }
}

impl DbConnection for PostgresDb {
    fn execute(&self, query: &str) -> Result<QueryResult, String> {
        use postgres::SimpleQueryMessage;

        let mut client = self.client.borrow_mut();
        let messages = client.simple_query(query).map_err(|e| e.to_string())?;

        let mut columns: Vec<String> = vec![];
        let mut rows: Vec<Vec<String>> = vec![];
        let mut rows_affected: Option<u64> = None;

        for msg in messages {
            match msg {
                SimpleQueryMessage::Row(row) => {
                    if columns.is_empty() {
                        columns = row.columns().iter().map(|c| c.name().to_string()).collect();
                    }
                    rows.push(
                        (0..row.len())
                            .map(|i| row.get(i).unwrap_or("NULL").to_string())
                            .collect(),
                    );
                }
                SimpleQueryMessage::CommandComplete(n) => {
                    rows_affected = Some(n);
                }
                _ => {}
            }
        }

        // Zero-row SELECT: simple_query sends no Row messages, so column names
        // are lost. Use PREPARE to retrieve them from the server's plan.
        // DML/DDL also land here but return no columns from prepare — fine.
        if columns.is_empty() && rows.is_empty()
            && let Ok(stmt) = client.prepare(query) {
            columns = stmt.columns().iter().map(|c| c.name().to_string()).collect();
        }

        Ok(QueryResult { columns, rows, rows_affected })
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

