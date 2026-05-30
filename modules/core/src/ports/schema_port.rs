use std::collections::HashMap;

pub trait SchemaPort {
    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String>;
}
