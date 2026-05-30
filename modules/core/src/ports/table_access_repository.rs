use crate::domain::analytics::FreqEntry;
use crate::domain::error::DomainError;
use crate::domain::table_access::TableAccess;

pub trait TableAccessRepository: Send + Sync {
    fn save(&self, entity: &TableAccess) -> Result<(), DomainError>;
    fn list_frequent(&self, connection_name: &str, limit: usize) -> Vec<FreqEntry>;
}
