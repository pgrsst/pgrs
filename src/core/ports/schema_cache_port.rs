use std::collections::HashMap;

pub trait SchemaCachePort: Send + Sync {
    fn save_schema(&self, connection_name: &str, schema: &HashMap<String, Vec<String>>);
    fn load_schema(&self, connection_name: &str) -> Option<HashMap<String, Vec<String>>>;
    fn invalidate(&self, connection_name: &str);
}
