#[derive(Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub rows_affected: Option<u64>,
}

pub trait DbConnection {
    fn execute(&self, query: &str) -> Result<QueryResult, String>;
}

