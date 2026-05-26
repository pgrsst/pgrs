use std::collections::HashMap;
use crate::core::domain::schema_column::SchemaColumn;
use crate::core::domain::schema_table::SchemaTable;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use crate::core::ports::schema_column_repository::SchemaColumnRepository;
use crate::core::ports::schema_table_repository::SchemaTableRepository;
use super::SqliteRepository;

impl SchemaCachePort for SqliteRepository {
    fn save_schema(&self, connection_name: &str, schema: &HashMap<String, Vec<String>>) {
        let connection_id = {
            let conn = self.conn.lock().unwrap();
            match SqliteRepository::connection_id_for(&conn, connection_name) {
                Some(id) => id,
                None => {
                    eprintln!("pgrs: schema cache: unknown connection '{connection_name}'");
                    return;
                }
            }
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = <Self as SchemaTableRepository>::delete_by_connection(self, connection_id) {
            eprintln!("pgrs: schema cache write failed: {e}");
            return;
        }
        if let Err(e) = <Self as SchemaColumnRepository>::delete_by_connection(self, connection_id) {
            eprintln!("pgrs: schema cache write failed: {e}");
            return;
        }

        for (table_name, columns) in schema {
            if let Err(e) = <Self as SchemaTableRepository>::save(self, &SchemaTable {
                connection_id,
                table_name: table_name.clone(),
                cached_at: now,
            }) {
                eprintln!("pgrs: schema cache write failed: {e}");
                return;
            }
            for column_name in columns {
                if let Err(e) = <Self as SchemaColumnRepository>::save(self, &SchemaColumn {
                    connection_id,
                    table_name: table_name.clone(),
                    column_name: column_name.clone(),
                    data_type: None,
                    cached_at: now,
                }) {
                    eprintln!("pgrs: schema cache write failed: {e}");
                    return;
                }
            }
        }
    }

    fn load_schema(&self, connection_name: &str) -> Option<HashMap<String, Vec<String>>> {
        let connection_id = {
            let conn = self.conn.lock().unwrap();
            SqliteRepository::connection_id_for(&conn, connection_name)?
        };

        let rows = <Self as SchemaColumnRepository>::list_by_connection(self, connection_id);
        if rows.is_empty() {
            return None;
        }

        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for col in rows {
            map.entry(col.table_name).or_default().push(col.column_name);
        }
        Some(map)
    }

    fn invalidate(&self, connection_name: &str) {
        let connection_id = {
            let conn = self.conn.lock().unwrap();
            match SqliteRepository::connection_id_for(&conn, connection_name) {
                Some(id) => id,
                None => return,
            }
        };

        if let Err(e) = <Self as SchemaTableRepository>::delete_by_connection(self, connection_id) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
        }
        if let Err(e) = <Self as SchemaColumnRepository>::delete_by_connection(self, connection_id) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
        }
    }
}
