#[derive(Debug, Clone)]
pub struct SchemaTable {
    pub connection_id: i64,
    pub table_name: String,
    pub cached_at: i64,
}
