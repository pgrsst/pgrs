#[derive(Debug, Clone)]
pub struct SchemaColumn {
    pub connection_id: i64,
    pub table_name: String,
    pub column_name: String,
    pub data_type: Option<String>,
    pub cached_at: i64,
}
