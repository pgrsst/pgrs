use std::collections::HashMap;

use super::completions::{Completion, CompletionKind};

/// Ranks completion candidates by access frequency.
///
/// This is the ranking strategy split out of `QueryCompletionService`: that
/// service decides *which* candidates to offer (trigger detection, generation,
/// prefix filtering), while this type owns the orthogonal concern of *ordering*
/// them. Tables and columns are ordered by how often they have been accessed
/// (most frequent first), falling back to shorter-then-lexicographic order;
/// keywords and mixed kinds use the length/lexicographic fallback only.
pub struct CompletionRanker {
    table_freq: HashMap<String, u64>,
    column_freq: HashMap<String, u64>,
}

impl CompletionRanker {
    pub fn new(table_freq: HashMap<String, u64>, column_freq: HashMap<String, u64>) -> Self {
        Self { table_freq, column_freq }
    }

    /// Sort `results` in place according to the frequency strategy.
    pub fn rank(&self, results: &mut [Completion]) {
        results.sort_by(|a, b| match (&a.kind, &b.kind) {
            (CompletionKind::Keyword, CompletionKind::Keyword) => a.value.cmp(&b.value),
            (CompletionKind::Table, CompletionKind::Table) if !self.table_freq.is_empty() => {
                let ca = self.table_freq.get(&a.value).copied().unwrap_or(0);
                let cb = self.table_freq.get(&b.value).copied().unwrap_or(0);
                cb.cmp(&ca).then_with(|| a.value.len().cmp(&b.value.len()).then_with(|| a.value.cmp(&b.value)))
            }
            (CompletionKind::Column, CompletionKind::Column)
                if !self.column_freq.is_empty() || !self.table_freq.is_empty() =>
            {
                let ta = a.value.find('.').map(|i| self.table_freq.get(&a.value[..i]).copied().unwrap_or(0)).unwrap_or(0);
                let tb = b.value.find('.').map(|i| self.table_freq.get(&b.value[..i]).copied().unwrap_or(0)).unwrap_or(0);
                let ca = self.column_freq.get(a.value.split('.').next_back().unwrap_or(&a.value)).copied().unwrap_or(0);
                let cb = self.column_freq.get(b.value.split('.').next_back().unwrap_or(&b.value)).copied().unwrap_or(0);
                tb.cmp(&ta)
                    .then_with(|| cb.cmp(&ca))
                    .then_with(|| a.value.len().cmp(&b.value.len()))
                    .then_with(|| a.value.cmp(&b.value))
            }
            _ => a.value.len().cmp(&b.value.len()).then_with(|| a.value.cmp(&b.value)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str) -> Completion {
        Completion { value: name.to_string(), kind: CompletionKind::Table }
    }
    fn column(name: &str) -> Completion {
        Completion { value: name.to_string(), kind: CompletionKind::Column }
    }

    #[test]
    fn ranks_tables_by_frequency_desc() {
        let ranker = CompletionRanker::new(
            HashMap::from([("orders".to_string(), 10), ("users".to_string(), 3)]),
            HashMap::new(),
        );
        let mut results = vec![table("users"), table("orders")];
        ranker.rank(&mut results);
        assert_eq!(results[0].value, "orders", "more frequent table should come first");
        assert_eq!(results[1].value, "users");
    }

    #[test]
    fn ranks_columns_by_frequency_desc() {
        let ranker = CompletionRanker::new(
            HashMap::new(),
            HashMap::from([("email".to_string(), 5), ("id".to_string(), 1)]),
        );
        let mut results = vec![column("id"), column("email")];
        ranker.rank(&mut results);
        assert_eq!(results[0].value, "email", "more frequent column should come first");
    }

    #[test]
    fn falls_back_to_length_then_lexicographic_without_frequencies() {
        let ranker = CompletionRanker::new(HashMap::new(), HashMap::new());
        let mut results = vec![table("orders"), table("id"), table("email")];
        ranker.rank(&mut results);
        let values: Vec<&str> = results.iter().map(|c| c.value.as_str()).collect();
        // No freq data → shorter first, then lexicographic.
        assert_eq!(values, vec!["id", "email", "orders"]);
    }

    #[test]
    fn keywords_sorted_lexicographically() {
        let ranker = CompletionRanker::new(HashMap::new(), HashMap::new());
        let mut results = vec![
            Completion { value: "WHERE".to_string(), kind: CompletionKind::Keyword },
            Completion { value: "FROM".to_string(), kind: CompletionKind::Keyword },
        ];
        ranker.rank(&mut results);
        assert_eq!(results[0].value, "FROM");
        assert_eq!(results[1].value, "WHERE");
    }
}
