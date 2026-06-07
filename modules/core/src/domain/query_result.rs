/// A query result set: column names plus rows of stringified cell values, and
/// the affected-row count for non-SELECT statements. A pure value type shared
/// across the port boundary (every `DbConnection::execute` yields one).
#[derive(Clone, Debug)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub rows_affected: Option<u64>,
}
