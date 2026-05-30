#[derive(Debug, Clone)]
pub struct QueryHistory {
    pub id: i64,
    pub connection_id: i64,
    pub query: String,
    pub executed_at: i64,
}
