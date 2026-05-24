use std::collections::HashMap;

pub trait SchemaCachePort: Send + Sync {
    fn save_schema(&self, db_name: &str, schema: &HashMap<String, Vec<String>>);
    fn load_schema(&self, db_name: &str) -> Option<HashMap<String, Vec<String>>>;
    fn invalidate(&self, db_name: &str);
}
