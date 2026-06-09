//! Pure value types describing a query execution plan, produced by the
//! `CatalogPort` (`\explain` / `\explain+`). They carry no behaviour and no
//! DB-dialect knowledge — the PostgreSQL JSON that fills them lives in the
//! driven adapter (`adapters::driven::postgres_catalog`).

/// A parsed query plan: a single root node and its descendants.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplainPlan {
    pub root: ExplainNode,
}

/// One node in the plan tree.
///
/// `actual_time_ms` / `actual_rows` are populated only when the plan was run
/// with ANALYZE (`\explain+`); they are `None` for a plain `\explain`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplainNode {
    pub node_type: String,
    pub relation: Option<String>,
    pub total_cost: f64,
    pub plan_rows: u64,
    pub actual_time_ms: Option<f64>,
    pub actual_rows: Option<u64>,
    /// Extra scalar attributes (e.g. "Filter: (active = true)"), in a fixed order.
    pub detail: Vec<String>,
    pub children: Vec<ExplainNode>,
}
