use std::io::Write;

use pgrs_core::{AnalyticsApi, DEFAULT_HISTORY_LIMIT, SavedQueryApi};

/// Max SQL width shown in the `\saved` listing before truncating with `…`.
const PREVIEW_WIDTH: usize = 60;

/// Parse the rest of `\save <name> <id>` (everything after `\save `): a name
/// followed by an integer history id. Uses the shared quote-aware tokenizer so a
/// quoted name with spaces (`"my query" 42`) is one token; rejects anything that
/// isn't exactly two tokens.
pub(super) fn parse_save_args(rest: &str) -> Option<(String, i64)> {
    let toks = crate::repl::args::tokenize_args(rest);
    if toks.len() != 2 {
        return None;
    }
    let id: i64 = toks[1].parse().ok()?;
    Some((toks[0].clone(), id))
}

fn preview(sql: &str) -> String {
    let oneline: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    if oneline.chars().count() > PREVIEW_WIDTH {
        let truncated: String = oneline.chars().take(PREVIEW_WIDTH - 1).collect();
        format!("{truncated}…")
    } else {
        oneline
    }
}

/// `\save <name> <id>`: look up history entry `id` for the active connection
/// and persist its SQL under `name`.
pub(super) fn handle_save(
    name: &str,
    id: i64,
    connection_name: &str,
    analytics: &AnalyticsApi,
    saved_query: &SavedQueryApi,
    writer: &mut impl Write,
) {
    let history = analytics.history(connection_name, DEFAULT_HISTORY_LIMIT);
    let entry = match history.iter().find(|e| e.id == id) {
        Some(e) => e,
        None => {
            writeln!(writer, "error: no history entry with id {}", id).ok();
            return;
        }
    };
    match saved_query.save(connection_name, name, &entry.query) {
        Ok(()) => {
            writeln!(writer, "Saved query '{}'.", name).ok();
        }
        Err(e) => {
            writeln!(writer, "error: {}", e).ok();
        }
    }
}

/// `\saved`: list saved queries for the active connection (name + SQL preview).
pub(super) fn handle_saved(
    connection_name: &str,
    saved_query: &SavedQueryApi,
    writer: &mut impl Write,
) {
    let saved = saved_query.list(connection_name);
    if saved.is_empty() {
        writeln!(writer, "No saved queries.").ok();
        return;
    }
    let name_w = saved.iter().map(|q| q.name.len()).max().unwrap_or(4).max(4);
    writeln!(writer, "  {:<name_w$}  sql", "name").ok();
    writeln!(writer, "  {:-<name_w$}  {:-<PREVIEW_WIDTH$}", "", "").ok();
    for q in &saved {
        writeln!(writer, "  {:<name_w$}  {}", q.name, preview(&q.sql)).ok();
    }
    writeln!(writer, "({} saved)", saved.len()).ok();
}

/// `\unsave <name>`: delete a saved query.
pub(super) fn handle_unsave(
    name: &str,
    connection_name: &str,
    saved_query: &SavedQueryApi,
    writer: &mut impl Write,
) {
    match saved_query.delete(connection_name, name) {
        Ok(()) => {
            writeln!(writer, "Removed saved query '{}'.", name).ok();
        }
        Err(e) => {
            writeln!(writer, "error: {}", e).ok();
        }
    }
}

/// Resolve the SQL to execute for `\run <name>`, or an error message to print.
/// Execution itself is handled by the REPL loop (same path as plain SQL) so the
/// DML transaction guard, analytics, and DDL auto-refresh all apply.
pub(super) fn resolve_saved_sql(
    name: &str,
    connection_name: &str,
    saved_query: &SavedQueryApi,
) -> Result<String, String> {
    match saved_query.get(connection_name, name) {
        Some(q) => Ok(q.sql),
        None => Err(format!("no saved query named '{name}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use pgrs_core::{AddConnectionInput, Core, SavedQueryApi, SchemaApi, TlsMode, DEFAULT_PORT};

    /// In-memory `Core` seeded with a connection and the given queries (as
    /// history), returning the Core plus the assigned history id for each query.
    fn seed(connection_name: &str, queries: &[&str]) -> (Core, Vec<i64>) {
        let core = Core::in_memory();
        core.connection
            .add(AddConnectionInput {
                name: connection_name.to_string(),
                host: "localhost".to_string(),
                port: DEFAULT_PORT,
                username: "u".to_string(),
                password: "p".to_string(),
                database: "db".to_string(),
                tls: TlsMode::Disable,
                environment: None,
            })
            .unwrap();
        let analytics = core.analytics_api();
        let schema = SchemaApi::for_test(HashMap::new());
        for q in queries {
            analytics.record_query(connection_name, q, &schema).unwrap();
        }
        let history = analytics.history(connection_name, DEFAULT_HISTORY_LIMIT);
        let ids = queries
            .iter()
            .map(|q| history.iter().find(|e| &e.query == q).map(|e| e.id).unwrap())
            .collect();
        (core, ids)
    }

    #[test]
    fn parse_save_args_valid() {
        assert_eq!(parse_save_args("myquery 42"), Some(("myquery".to_string(), 42)));
    }

    #[test]
    fn parse_save_args_extra_whitespace() {
        assert_eq!(parse_save_args("  myquery   42  "), Some(("myquery".to_string(), 42)));
    }

    #[test]
    fn parse_save_args_quoted_name_with_space() {
        assert_eq!(parse_save_args("\"my query\" 42"), Some(("my query".to_string(), 42)));
    }

    #[test]
    fn parse_save_args_missing_id() {
        assert!(parse_save_args("myquery").is_none());
    }

    #[test]
    fn parse_save_args_non_integer_id() {
        assert!(parse_save_args("myquery abc").is_none());
    }

    #[test]
    fn parse_save_args_rejects_extra_tokens() {
        assert!(parse_save_args("myquery 42 extra").is_none());
    }

    #[test]
    fn handle_save_persists_from_history_id() {
        let (core, ids) = seed("mydb", &["SELECT * FROM users"]);
        let analytics = core.analytics_api();
        let saved_query = core.saved_query_api();
        let mut out = Vec::new();
        handle_save("favorite", ids[0], "mydb", &analytics, &saved_query, &mut out);
        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("Saved query 'favorite'"), "got: {msg}");
        assert_eq!(saved_query.get("mydb", "favorite").unwrap().sql, "SELECT * FROM users");
    }

    #[test]
    fn handle_save_unknown_id_errors() {
        let (core, _ids) = seed("mydb", &["SELECT 1"]);
        let analytics = core.analytics_api();
        let saved_query = core.saved_query_api();
        let mut out = Vec::new();
        handle_save("x", 999, "mydb", &analytics, &saved_query, &mut out);
        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("no history entry with id 999"), "got: {msg}");
        assert!(saved_query.get("mydb", "x").is_none());
    }

    #[test]
    fn handle_save_duplicate_name_errors() {
        let (core, ids) = seed("mydb", &["SELECT 1", "SELECT 2"]);
        let analytics = core.analytics_api();
        let saved_query = core.saved_query_api();
        let mut out = Vec::new();
        handle_save("dup", ids[0], "mydb", &analytics, &saved_query, &mut out);
        let mut out = Vec::new();
        handle_save("dup", ids[1], "mydb", &analytics, &saved_query, &mut out);
        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("error"), "expected duplicate error, got: {msg}");
    }

    #[test]
    fn handle_saved_lists_names() {
        let (core, ids) = seed("mydb", &["SELECT 1", "SELECT 2"]);
        let analytics = core.analytics_api();
        let saved_query = core.saved_query_api();
        let mut out = Vec::new();
        handle_save("first", ids[0], "mydb", &analytics, &saved_query, &mut out);
        let mut out = Vec::new();
        handle_save("second", ids[1], "mydb", &analytics, &saved_query, &mut out);

        let mut out = Vec::new();
        handle_saved("mydb", &saved_query, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("first"), "got: {text}");
        assert!(text.contains("second"), "got: {text}");
        assert!(text.contains("2 saved"), "got: {text}");
    }

    #[test]
    fn handle_saved_empty_shows_message() {
        let core = Core::in_memory();
        let saved_query: SavedQueryApi = core.saved_query_api();
        let mut out = Vec::new();
        handle_saved("mydb", &saved_query, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("No saved queries"), "got: {text}");
    }

    #[test]
    fn handle_unsave_removes() {
        let (core, ids) = seed("mydb", &["SELECT 1"]);
        let analytics = core.analytics_api();
        let saved_query = core.saved_query_api();
        let mut out = Vec::new();
        handle_save("q", ids[0], "mydb", &analytics, &saved_query, &mut out);

        let mut out = Vec::new();
        handle_unsave("q", "mydb", &saved_query, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Removed saved query 'q'"), "got: {text}");
        assert!(saved_query.get("mydb", "q").is_none());
    }

    #[test]
    fn handle_unsave_unknown_errors() {
        let (core, _ids) = seed("mydb", &["SELECT 1"]);
        let saved_query = core.saved_query_api();
        let mut out = Vec::new();
        handle_unsave("ghost", "mydb", &saved_query, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("error"), "got: {text}");
    }

    #[test]
    fn resolve_saved_sql_returns_sql() {
        let (core, ids) = seed("mydb", &["SELECT * FROM users"]);
        let analytics = core.analytics_api();
        let saved_query = core.saved_query_api();
        let mut out = Vec::new();
        handle_save("q", ids[0], "mydb", &analytics, &saved_query, &mut out);

        let sql = resolve_saved_sql("q", "mydb", &saved_query).unwrap();
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn resolve_saved_sql_unknown_errors() {
        let core = Core::in_memory();
        let saved_query = core.saved_query_api();
        let err = resolve_saved_sql("ghost", "mydb", &saved_query).unwrap_err();
        assert!(err.contains("no saved query named 'ghost'"), "got: {err}");
    }
}
