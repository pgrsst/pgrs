//! Pure value types describing database catalog objects, produced by the
//! `CatalogPort` (`\d` / `\d+`). They carry no behaviour and no DB-dialect
//! knowledge — the PostgreSQL SQL that fills them lives in the driven adapter.

use crate::domain::query_result::QueryResult;

/// A named database object together with its definition — an index, a
/// foreign-key/check constraint, or a trigger.
#[derive(Debug, Clone)]
pub struct NamedDef {
    pub name: String,
    pub definition: String,
}

/// Structured result of describing a single table (`\d` / `\d+`).
///
/// The column listing is kept as a [`QueryResult`] so front-ends can reuse
/// their generic table formatter; the remaining sections are plain value lists.
#[derive(Debug, Clone)]
pub struct TableDescription {
    pub schema: String,
    pub name: String,
    pub extended: bool,
    pub columns: QueryResult,
    pub indexes: Vec<NamedDef>,
    pub foreign_keys: Vec<NamedDef>,
    pub checks: Vec<NamedDef>,
    pub triggers: Vec<NamedDef>,
}
