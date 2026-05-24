#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub query: String,
    pub executed_at: i64,
}

#[derive(Debug, Clone)]
pub struct FreqEntry {
    pub name: String,
    pub count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_entry_stores_query_and_timestamp() {
        let entry = HistoryEntry {
            query: "SELECT 1".to_string(),
            executed_at: 1234567890,
        };
        assert_eq!(entry.query, "SELECT 1");
        assert_eq!(entry.executed_at, 1234567890);
    }

    #[test]
    fn freq_entry_stores_name_and_count() {
        let entry = FreqEntry { name: "users".to_string(), count: 42 };
        assert_eq!(entry.name, "users");
        assert_eq!(entry.count, 42);
    }
}
