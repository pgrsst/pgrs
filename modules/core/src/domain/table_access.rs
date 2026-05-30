#[derive(Debug, Clone)]
pub struct TableAccess {
    #[allow(dead_code)]
    pub id: i64,
    pub connection_id: i64,
    pub table_name: String,
    pub query_id: Option<i64>,
    pub accessed_at: i64,
}
