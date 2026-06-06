use std::collections::HashMap;

use crate::domain::error::DomainError;

pub trait SchemaPort {
    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError>;
}
