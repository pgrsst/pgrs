#[derive(Debug, Clone, PartialEq)]
pub struct SavedQuery {
    pub id: i64,
    pub name: String,
    pub sql: String,
    pub created_at: i64,
}
