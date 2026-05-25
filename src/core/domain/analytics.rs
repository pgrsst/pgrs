#[derive(Debug, Clone)]
pub struct FreqEntry {
    pub name: String,
    pub count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freq_entry_stores_name_and_count() {
        let entry = FreqEntry { name: "users".to_string(), count: 42 };
        assert_eq!(entry.name, "users");
        assert_eq!(entry.count, 42);
    }
}
