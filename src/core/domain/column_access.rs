#[derive(Debug, Clone)]
pub struct ColumnAccess {
    pub id: i64,
    pub connection_id: i64,
    pub table_name: String,
    pub column_name: String,
    pub query_id: Option<i64>,
    pub accessed_at: i64,
}
