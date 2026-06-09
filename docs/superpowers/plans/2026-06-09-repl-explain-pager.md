# REPL EXPLAIN tree + Auto pager — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `\explain`/`\explain+` (query plan rendered as an indented tree) and an auto pager (route long output through `$PAGER`) to the pgrs `shell` REPL.

**Architecture:** EXPLAIN follows the existing `describe_table` pattern — core runs `EXPLAIN (FORMAT JSON …)` behind `CatalogPort`, parses the Postgres JSON into a pure `ExplainPlan` domain value (serde_json), and the CLI renders it. The pager is CLI-only: REPL output is collected into a buffer, then a `pager` module decides whether to page based on terminal height and `$PAGER`.

**Tech Stack:** Rust (Cargo workspace), `serde_json` (new core dep), `crossterm` (new CLI dep, already in the tree via reedline), `postgres`, `rusqlite`, `reedline`.

Spec: `docs/superpowers/specs/2026-06-08-repl-explain-pager-design.md`

---

## File Structure

**pgrs-core:**
- Create `modules/core/src/domain/explain.rs` — `ExplainPlan` / `ExplainNode` value types.
- Modify `modules/core/src/domain/mod.rs` — register `pub mod explain;`.
- Modify `modules/core/src/ports/catalog_port.rs` — add `explain` to the trait.
- Modify `modules/core/src/adapters/driven/postgres_catalog.rs` — implement `explain` + JSON parser.
- Modify `modules/core/src/api/query.rs` — `QueryApi::explain` delegation.
- Modify `modules/core/src/lib.rs` — re-export `ExplainPlan` / `ExplainNode`.
- Modify `modules/core/Cargo.toml` — add `serde_json`.

**pgrs-cli:**
- Create `modules/cli/src/repl/explain.rs` — render `ExplainPlan` to an ASCII tree + `\explain` handler.
- Create `modules/cli/src/repl/pager.rs` — paging decision + spawn `$PAGER`.
- Modify `modules/cli/src/repl/mod.rs` — register modules, parse `\explain`/`\explain+`/`\pager`, route output through the pager.
- Modify `modules/cli/src/repl/ui.rs` — help text entries.
- Modify `modules/cli/Cargo.toml` — add `crossterm`.
- Modify `CLAUDE.md` — document the new commands.

---

## Task 1: Add `serde_json` dep and the `ExplainPlan` domain type

**Files:**
- Modify: `modules/core/Cargo.toml`
- Create: `modules/core/src/domain/explain.rs`
- Modify: `modules/core/src/domain/mod.rs:1-11`
- Modify: `modules/core/src/lib.rs:50` (value-type re-exports block)

- [ ] **Step 1: Add the dependency**

In `modules/core/Cargo.toml`, under `[dependencies]` (after the `sqlparser = "0.62"` line):

```toml
serde_json = "1"
```

- [ ] **Step 2: Create the domain value types**

Create `modules/core/src/domain/explain.rs`:

```rust
//! Pure value types describing a query execution plan, produced by the
//! `CatalogPort` (`\explain` / `\explain+`). They carry no behaviour and no
//! DB-dialect knowledge — the PostgreSQL JSON that fills them lives in the
//! driven adapter (`adapters::driven::postgres_catalog`).

/// A parsed query plan: a single root node and its descendants.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplainPlan {
    pub root: ExplainNode,
}

/// One node in the plan tree.
///
/// `actual_time_ms` / `actual_rows` are populated only when the plan was run
/// with ANALYZE (`\explain+`); they are `None` for a plain `\explain`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplainNode {
    pub node_type: String,
    pub relation: Option<String>,
    pub total_cost: f64,
    pub plan_rows: u64,
    pub actual_time_ms: Option<f64>,
    pub actual_rows: Option<u64>,
    /// Extra scalar attributes (e.g. "Filter: (active = true)"), in a fixed order.
    pub detail: Vec<String>,
    pub children: Vec<ExplainNode>,
}
```

- [ ] **Step 3: Register the module**

In `modules/core/src/domain/mod.rs`, add the line in alphabetical position (after `pub mod error;`):

```rust
pub mod explain;
```

- [ ] **Step 4: Re-export from the public facade**

In `modules/core/src/lib.rs`, in the "Value types used in API signatures" block (around line 50, next to the `catalog` re-export), add:

```rust
pub use domain::explain::{ExplainNode, ExplainPlan};
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p pgrs-core`
Expected: builds cleanly (a `serde_json` download + an unused-import-free build; `ExplainPlan`/`ExplainNode` are exported but not yet used — no warning because they're `pub`).

- [ ] **Step 6: Commit**

```bash
git add modules/core/Cargo.toml modules/core/src/domain/explain.rs modules/core/src/domain/mod.rs modules/core/src/lib.rs
git commit -m "feat(core): add ExplainPlan domain type and serde_json dep"
```

---

## Task 2: Parse Postgres EXPLAIN JSON in the catalog adapter

The `EXPLAIN (FORMAT JSON) <sql>` result is a single row / single column whose
value is a JSON array `[{ "Plan": { "Node Type": ..., "Plans": [...] } }]`. With
ANALYZE the nodes additionally carry `"Actual Total Time"` / `"Actual Rows"`.

**Files:**
- Modify: `modules/core/src/ports/catalog_port.rs:14-21`
- Modify: `modules/core/src/adapters/driven/postgres_catalog.rs` (add parser + impl + tests)

- [ ] **Step 1: Add the port method**

In `modules/core/src/ports/catalog_port.rs`, add to the `CatalogPort` trait (after `list_databases`), and add the import:

```rust
use crate::domain::explain::ExplainPlan;
```

```rust
    /// Run `EXPLAIN` on `sql` and return the parsed plan tree. When `analyze`
    /// is true the statement is actually executed (`EXPLAIN ANALYZE`).
    fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainPlan, DomainError>;
```

- [ ] **Step 2: Write failing tests for the parser**

In `modules/core/src/adapters/driven/postgres_catalog.rs`, inside the existing `#[cfg(test)] mod tests`, add (the `StubDb` there already routes by substring; an `EXPLAIN` query contains `"EXPLAIN"`):

```rust
    fn explain_json_row(json: &str) -> QueryResult {
        QueryResult {
            columns: vec!["QUERY PLAN".into()],
            rows: vec![vec![json.into()]],
            rows_affected: None,
        }
    }

    #[test]
    fn explain_parses_single_node() {
        let json = r#"[{"Plan":{"Node Type":"Seq Scan","Relation Name":"users","Total Cost":18.50,"Plan Rows":850,"Filter":"(active = true)"}}]"#;
        let db = StubDb::new().with("EXPLAIN", Ok(explain_json_row(json)));
        let plan = db.explain("SELECT * FROM users", false).unwrap();
        assert_eq!(plan.root.node_type, "Seq Scan");
        assert_eq!(plan.root.relation.as_deref(), Some("users"));
        assert_eq!(plan.root.total_cost, 18.50);
        assert_eq!(plan.root.plan_rows, 850);
        assert!(plan.root.actual_time_ms.is_none(), "no ANALYZE -> no actuals");
        assert_eq!(plan.root.detail, vec!["Filter: (active = true)".to_string()]);
        assert!(plan.root.children.is_empty());
    }

    #[test]
    fn explain_parses_nested_plans_and_actuals() {
        let json = r#"[{"Plan":{"Node Type":"Hash Join","Total Cost":42.0,"Plan Rows":10,"Actual Total Time":1.25,"Actual Rows":9,"Plans":[{"Node Type":"Seq Scan","Relation Name":"a","Total Cost":1.0,"Plan Rows":1}]}}]"#;
        let db = StubDb::new().with("EXPLAIN", Ok(explain_json_row(json)));
        let plan = db.explain("SELECT 1", true).unwrap();
        assert_eq!(plan.root.node_type, "Hash Join");
        assert_eq!(plan.root.actual_time_ms, Some(1.25));
        assert_eq!(plan.root.actual_rows, Some(9));
        assert_eq!(plan.root.children.len(), 1);
        assert_eq!(plan.root.children[0].relation.as_deref(), Some("a"));
    }

    #[test]
    fn explain_errors_on_unparseable_output() {
        let db = StubDb::new().with("EXPLAIN", Ok(explain_json_row("not json")));
        let err = db.explain("SELECT 1", false).unwrap_err();
        assert!(matches!(err, DomainError::QueryError(_)), "got: {err:?}");
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p pgrs-core explain_parses_single_node`
Expected: FAIL — `explain` not implemented (compile error: no method `explain`).

- [ ] **Step 4: Implement the parser and the impl**

In `modules/core/src/adapters/driven/postgres_catalog.rs`, add the import at the top (with the other `use crate::...` lines):

```rust
use crate::domain::explain::{ExplainNode, ExplainPlan};
```

Add these free functions above the `impl<T: DbConnection + ?Sized> CatalogPort for T` block:

```rust
/// Scalar attribute keys surfaced as detail lines, in display order.
const EXPLAIN_DETAIL_KEYS: &[&str] = &["Join Type", "Index Cond", "Hash Cond", "Filter"];

fn parse_explain_node(node: &serde_json::Value) -> ExplainNode {
    let detail = EXPLAIN_DETAIL_KEYS
        .iter()
        .filter_map(|key| node.get(*key).and_then(|v| v.as_str()).map(|v| format!("{key}: {v}")))
        .collect();

    let children = node
        .get("Plans")
        .and_then(|p| p.as_array())
        .map(|arr| arr.iter().map(parse_explain_node).collect())
        .unwrap_or_default();

    ExplainNode {
        node_type: node.get("Node Type").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
        relation: node.get("Relation Name").and_then(|v| v.as_str()).map(String::from),
        total_cost: node.get("Total Cost").and_then(|v| v.as_f64()).unwrap_or(0.0),
        plan_rows: node.get("Plan Rows").and_then(|v| v.as_u64()).unwrap_or(0),
        actual_time_ms: node.get("Actual Total Time").and_then(|v| v.as_f64()),
        actual_rows: node.get("Actual Rows").and_then(|v| v.as_u64()),
        detail,
        children,
    }
}
```

Add the `explain` method inside the `impl<T: DbConnection + ?Sized> CatalogPort for T` block (after `list_databases`):

```rust
    fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainPlan, DomainError> {
        let options = if analyze {
            "FORMAT JSON, ANALYZE true, BUFFERS true"
        } else {
            "FORMAT JSON"
        };
        let query = format!("EXPLAIN ({options}) {sql}");
        let result = self.execute(&query)?;

        let json_text = result
            .rows
            .into_iter()
            .next()
            .and_then(|row| row.into_iter().next())
            .ok_or_else(|| DomainError::QueryError("EXPLAIN returned no plan".to_string()))?;

        let parsed: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|e| DomainError::QueryError(format!("could not parse EXPLAIN output: {e}")))?;

        let plan_obj = parsed
            .get(0)
            .and_then(|entry| entry.get("Plan"))
            .ok_or_else(|| DomainError::QueryError("EXPLAIN output missing Plan".to_string()))?;

        Ok(ExplainPlan { root: parse_explain_node(plan_obj) })
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p pgrs-core explain_`
Expected: PASS (all three `explain_*` tests).

- [ ] **Step 6: Commit**

```bash
git add modules/core/src/ports/catalog_port.rs modules/core/src/adapters/driven/postgres_catalog.rs
git commit -m "feat(core): parse Postgres EXPLAIN JSON into ExplainPlan"
```

---

## Task 3: Expose `explain` on `QueryApi`

**Files:**
- Modify: `modules/core/src/api/query.rs:3-7` (imports) and `:43-45` (after `list_databases`)

- [ ] **Step 1: Write the failing test**

In `modules/core/src/api/query.rs`, add a test module at the end of the file (there is none yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::DomainError;
    use crate::ports::schema_port::SchemaPort;
    use std::collections::HashMap;

    struct StubDb {
        json: String,
    }

    impl DbConnection for StubDb {
        fn execute(&self, _sql: &str) -> Result<QueryResult, DomainError> {
            Ok(QueryResult {
                columns: vec!["QUERY PLAN".into()],
                rows: vec![vec![self.json.clone()]],
                rows_affected: None,
            })
        }
    }

    impl SchemaPort for StubDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
            Ok(HashMap::new())
        }
    }

    #[test]
    fn explain_delegates_to_port_and_returns_plan() {
        let json = r#"[{"Plan":{"Node Type":"Seq Scan","Total Cost":1.0,"Plan Rows":1}}]"#;
        let api = QueryApi::from_port(Box::new(StubDb { json: json.to_string() }));
        let plan = api.explain("SELECT 1", false).unwrap();
        assert_eq!(plan.root.node_type, "Seq Scan");
    }
}
```

Note: this requires `DbConnection` and `QueryResult` to be in scope in the test
(`super::*` brings `QueryResult`; add `use crate::ports::db_connection::DbConnection;`
inside the test module if not already reachable — `super::*` does not re-export it,
so add that import line too).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pgrs-core explain_delegates_to_port`
Expected: FAIL — no method `explain` on `QueryApi`.

- [ ] **Step 3: Add the delegating method**

In `modules/core/src/api/query.rs`, add the import near the top (with the other domain imports):

```rust
use crate::domain::explain::ExplainPlan;
```

Add the method inside `impl QueryApi` (after `list_databases`, before the `#[cfg(...)] from_repl`):

```rust
    /// Run `EXPLAIN` (`\explain`) or `EXPLAIN ANALYZE` (`\explain+`) and return
    /// the parsed plan tree. The pg-specific SQL/JSON lives in the adapter.
    pub fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainPlan, DomainError> {
        self.db.explain(sql, analyze)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pgrs-core explain_delegates_to_port`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add modules/core/src/api/query.rs
git commit -m "feat(core): expose QueryApi::explain"
```

---

## Task 4: CLI — render `ExplainPlan` as an ASCII tree

**Files:**
- Create: `modules/cli/src/repl/explain.rs`
- Modify: `modules/cli/src/repl/mod.rs:1-7` (module declarations)

- [ ] **Step 1: Register the module**

In `modules/cli/src/repl/mod.rs`, add to the module list at the top (keep alphabetical-ish ordering, after `mod describe;`):

```rust
mod explain;
```

- [ ] **Step 2: Write the failing tests and the file skeleton**

Create `modules/cli/src/repl/explain.rs`:

```rust
use std::io::Write;

use pgrs_core::{ExplainNode, ExplainPlan, QueryApi};

/// `\explain` / `\explain+`: ask the core for a parsed plan and render it as an
/// indented tree. All EXPLAIN/JSON knowledge lives in the core; this is pure
/// presentation.
pub(super) fn handle_explain(
    db: &QueryApi,
    sql: &str,
    analyze: bool,
    writer: &mut impl Write,
) {
    match db.explain(sql, analyze) {
        Ok(plan) => {
            write!(writer, "{}", render_plan(&plan)).ok();
        }
        Err(e) => {
            writeln!(writer, "error: {}", e).ok();
        }
    }
}

/// Render a plan tree to a string: one line per node, two-space indent per
/// depth, `->` arrows for child nodes (psql-familiar), with detail attributes
/// printed beneath each node.
fn render_plan(plan: &ExplainPlan) -> String {
    let mut out = String::new();
    render_node(&plan.root, 0, &mut out);
    out
}

fn render_node(node: &ExplainNode, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let arrow = if depth == 0 { "" } else { "-> " };
    let relation = node
        .relation
        .as_ref()
        .map(|r| format!(" on {r}"))
        .unwrap_or_default();

    let mut line = format!(
        "{indent}{arrow}{}{relation}  (cost={:.2} rows={})",
        node.node_type, node.total_cost, node.plan_rows
    );
    if let Some(t) = node.actual_time_ms {
        line.push_str(&format!(" (actual={:.3}ms rows={})", t, node.actual_rows.unwrap_or(0)));
    }
    out.push_str(&line);
    out.push('\n');

    for d in &node.detail {
        out.push_str(&format!("{indent}  {d}\n"));
    }
    for child in &node.children {
        render_node(child, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(node_type: &str) -> ExplainNode {
        ExplainNode {
            node_type: node_type.to_string(),
            relation: None,
            total_cost: 1.0,
            plan_rows: 1,
            actual_time_ms: None,
            actual_rows: None,
            detail: vec![],
            children: vec![],
        }
    }

    #[test]
    fn renders_single_node_with_cost() {
        let plan = ExplainPlan { root: ExplainNode { relation: Some("users".into()), total_cost: 18.5, plan_rows: 850, ..leaf("Seq Scan") } };
        let out = render_plan(&plan);
        assert!(out.contains("Seq Scan on users"), "got:\n{out}");
        assert!(out.contains("(cost=18.50 rows=850)"), "got:\n{out}");
        assert!(!out.contains("actual="), "no ANALYZE -> no actuals, got:\n{out}");
    }

    #[test]
    fn renders_actuals_when_present() {
        let plan = ExplainPlan { root: ExplainNode { actual_time_ms: Some(0.012), actual_rows: Some(842), ..leaf("Seq Scan") } };
        let out = render_plan(&plan);
        assert!(out.contains("actual=0.012ms rows=842"), "got:\n{out}");
    }

    #[test]
    fn renders_detail_lines() {
        let plan = ExplainPlan { root: ExplainNode { detail: vec!["Filter: (active = true)".into()], ..leaf("Seq Scan") } };
        let out = render_plan(&plan);
        assert!(out.contains("Filter: (active = true)"), "got:\n{out}");
    }

    #[test]
    fn renders_children_indented_with_arrow() {
        let plan = ExplainPlan { root: ExplainNode { children: vec![leaf("Index Scan")], ..leaf("Hash Join") } };
        let out = render_plan(&plan);
        assert!(out.contains("Hash Join"), "got:\n{out}");
        assert!(out.contains("  -> Index Scan"), "child should be indented with arrow, got:\n{out}");
    }
}
```

Note the `..leaf(...)` struct-update spread relies on `ExplainNode` fields being
public — they are (Task 1).

- [ ] **Step 3: Run tests to verify they fail, then pass**

Run: `cargo test -p pgrs-cli render`
Expected: this file's four tests compile and PASS (the code and tests are added
together; if any fail, fix the renderer until green). The `handle_explain` path
is exercised in Task 5's integration wiring.

- [ ] **Step 4: Commit**

```bash
git add modules/cli/src/repl/explain.rs modules/cli/src/repl/mod.rs
git commit -m "feat(cli): render ExplainPlan as an indented tree"
```

---

## Task 5: Wire `\explain` / `\explain+` into the REPL loop

**Files:**
- Modify: `modules/cli/src/repl/mod.rs` — `ReplCommand` enum + `parse` + dispatch
- Modify: `modules/cli/src/repl/ui.rs:72-90` — help entries

- [ ] **Step 1: Write failing parser tests**

In `modules/cli/src/repl/mod.rs`, inside the existing `#[cfg(test)] mod tests`, add:

```rust
    #[test]
    fn explain_variants_parse() {
        assert!(matches!(ReplCommand::parse("\\explain"), ReplCommand::ExplainUsage));
        assert!(matches!(ReplCommand::parse("\\explain+"), ReplCommand::ExplainUsage));
        assert!(matches!(
            ReplCommand::parse("\\explain SELECT 1"),
            ReplCommand::Explain { sql: "SELECT 1", analyze: false }
        ));
        assert!(matches!(
            ReplCommand::parse("\\explain+ SELECT 1"),
            ReplCommand::Explain { sql: "SELECT 1", analyze: true }
        ));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p pgrs-cli explain_variants_parse`
Expected: FAIL — `ExplainUsage` / `Explain` variants don't exist.

- [ ] **Step 3: Add the enum variants**

In `modules/cli/src/repl/mod.rs`, add to `enum ReplCommand<'a>` (after `Export(...)`):

```rust
    Explain { sql: &'a str, analyze: bool }, // \explain <sql> / \explain+ <sql>
    ExplainUsage,                            // bare \explain / \explain+
```

- [ ] **Step 4: Add the parse arms**

In `ReplCommand::parse`, add the two bare matches in the literal match block (next to `"\\export"`):

```rust
            "\\explain" | "\\explain+" => ReplCommand::ExplainUsage,
```

And in the `_ =>` prefix-stripping block, add these **before** the `\export ` arm
(order matters — check `\explain+ ` before `\explain `):

```rust
                if let Some(sql) = trimmed.strip_prefix("\\explain+ ") {
                    ReplCommand::Explain { sql, analyze: true }
                } else if let Some(sql) = trimmed.strip_prefix("\\explain ") {
                    ReplCommand::Explain { sql, analyze: false }
                } else if let Some(t) = trimmed.strip_prefix("\\d+ ") {
```

(i.e. prepend the two `if let` branches to the existing chain that currently
starts with `if let Some(t) = trimmed.strip_prefix("\\d+ ")`).

- [ ] **Step 5: Add the dispatch arm**

In `Repl::run`'s `match ReplCommand::parse(trimmed)`, add (after the `Export(Some(...))` arm). This routes output through the pager added in Task 7; for now write directly to `stdout` and revisit in Task 7:

```rust
                        ReplCommand::ExplainUsage => {
                            writeln!(stdout, "Usage: \\explain <query>  (\\explain+ runs ANALYZE)").ok();
                        }
                        ReplCommand::Explain { sql, analyze } => {
                            if analyze && dml_requires_tx(*tx.lock().unwrap(), sql) {
                                writeln!(
                                    stdout,
                                    "error: \\explain+ runs ANALYZE which executes the statement; INSERT/UPDATE/DELETE requires an open transaction. Run BEGIN (or \\begin) first."
                                ).ok();
                                continue;
                            }
                            explain::handle_explain(&query, sql, analyze, &mut stdout);
                        }
```

- [ ] **Step 6: Add help text**

In `modules/cli/src/repl/ui.rs`, add to the `REPL_COMMANDS` array (after the `\\timing` entry):

```rust
    ("\\explain <query>",    "show query plan as a tree (\\explain+ runs ANALYZE)"),
```

Add a help-text test in `ui.rs`'s test module:

```rust
    #[test]
    fn help_text_mentions_explain_command() {
        let text = repl_help_text();
        assert!(text.contains("\\explain"), "help should mention \\explain, got: {text}");
    }
```

- [ ] **Step 7: Run the full CLI test suite**

Run: `cargo test -p pgrs-cli`
Expected: PASS (parser test, help test, and all existing tests).

- [ ] **Step 8: Commit**

```bash
git add modules/cli/src/repl/mod.rs modules/cli/src/repl/ui.rs
git commit -m "feat(cli): wire \\explain and \\explain+ commands with DML guard"
```

---

## Task 6: CLI pager module (decision + spawn)

**Files:**
- Modify: `modules/cli/Cargo.toml`
- Create: `modules/cli/src/repl/pager.rs`
- Modify: `modules/cli/src/repl/mod.rs:1-7` (module declarations)

- [ ] **Step 1: Add the crossterm dependency**

In `modules/cli/Cargo.toml`, under `[dependencies]` (after `reedline = "0.47"`):

```toml
crossterm = "0.28"
```

(crossterm 0.28 is what reedline 0.47 pulls in; matching it avoids a second copy.
If `cargo build` reports a version mismatch, set this to the version shown by
`cargo tree -p crossterm`.)

- [ ] **Step 2: Register the module**

In `modules/cli/src/repl/mod.rs`, add to the module list (after `mod explain;`):

```rust
mod pager;
```

- [ ] **Step 3: Write the file with failing tests**

Create `modules/cli/src/repl/pager.rs`:

```rust
use std::io::Write;

/// Decide whether `content` should be paged: paging must be enabled and the
/// content must have more lines than the terminal can show at once.
pub(super) fn should_page(content: &str, enabled: bool, term_rows: u16) -> bool {
    enabled && term_rows > 0 && content.lines().count() > term_rows as usize
}

/// Emit `content`: page it through `$PAGER` when appropriate, otherwise write it
/// to `writer`. Paging only happens for an interactive stdout; on any failure
/// (not a TTY, unknown size, spawn error) it falls back to a direct write so
/// output is never lost.
pub(super) fn emit(content: &str, enabled: bool, writer: &mut impl Write) {
    use std::io::IsTerminal;

    if enabled && std::io::stdout().is_terminal() {
        if let Ok((_cols, rows)) = crossterm::terminal::size()
            && should_page(content, true, rows)
            && page(content).is_ok()
        {
            return;
        }
    }
    write!(writer, "{}", content).ok();
}

/// Spawn `$PAGER` (fallback `less -SR`) and feed `content` to its stdin.
fn page(content: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};

    let pager = std::env::var("PAGER").unwrap_or_default();
    let mut cmd = if pager.trim().is_empty() {
        let mut c = Command::new("less");
        c.arg("-SR");
        c
    } else {
        let mut parts = pager.split_whitespace();
        // split_whitespace on a non-empty string yields at least one token.
        let mut c = Command::new(parts.next().unwrap());
        for arg in parts {
            c.arg(arg);
        }
        c
    };

    let mut child = cmd.stdin(Stdio::piped()).spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_page_when_disabled() {
        let content = "a\nb\nc\nd\n";
        assert!(!should_page(content, false, 2));
    }

    #[test]
    fn pages_when_content_taller_than_terminal() {
        let content = "1\n2\n3\n4\n5\n";
        assert!(should_page(content, true, 3));
    }

    #[test]
    fn does_not_page_when_content_fits() {
        let content = "1\n2\n";
        assert!(!should_page(content, true, 24));
    }

    #[test]
    fn does_not_page_when_terminal_size_unknown() {
        let content = "1\n2\n3\n";
        assert!(!should_page(content, true, 0));
    }

    #[test]
    fn emit_writes_directly_when_not_a_tty() {
        // In the test harness stdout is not a TTY, so emit always writes to the
        // provided writer regardless of `enabled`.
        let mut buf = Vec::new();
        emit("hello\nworld\n", true, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "hello\nworld\n");
    }
}
```

Note: the `let Ok(...) = ... && ... && ...` chain uses `let`-chains (stable in
Rust edition 2024, already used in `command_handler.rs:131-133`).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p pgrs-cli pager`
Expected: all five tests PASS. (`emit_writes_directly_when_not_a_tty` confirms
the fallback; the real paging path is not unit-tested as it spawns a process.)

- [ ] **Step 5: Commit**

```bash
git add modules/cli/Cargo.toml modules/cli/src/repl/pager.rs modules/cli/src/repl/mod.rs
git commit -m "feat(cli): add pager module (decision + \$PAGER spawn)"
```

---

## Task 7: Route REPL output through the pager + `\pager` toggle

**Files:**
- Modify: `modules/cli/src/repl/mod.rs` — `ReplCommand` enum, parse, loop state, dispatch
- Modify: `modules/cli/src/repl/ui.rs` — help entry

- [ ] **Step 1: Write failing parser test**

In `modules/cli/src/repl/mod.rs` test module:

```rust
    #[test]
    fn pager_toggle_parses() {
        assert!(matches!(ReplCommand::parse("\\pager"), ReplCommand::TogglePager));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p pgrs-cli pager_toggle_parses`
Expected: FAIL — `TogglePager` variant missing.

- [ ] **Step 3: Add the enum variant and parse arm**

In `enum ReplCommand<'a>`, add (after `ToggleTiming`):

```rust
    TogglePager,     // \pager
```

In `ReplCommand::parse`, add to the literal match block (next to `"\\timing"`):

```rust
            "\\pager" => ReplCommand::TogglePager,
```

- [ ] **Step 4: Add loop state and dispatch**

In `Repl::run`, next to `let mut timing = false;`, add:

```rust
        let mut pager_enabled = true;
```

Add the dispatch arm (after `ToggleTiming`):

```rust
                        ReplCommand::TogglePager => {
                            pager_enabled = !pager_enabled;
                            println!("Pager is {}.", if pager_enabled { "on" } else { "off" });
                        }
```

- [ ] **Step 5: Route SQL result output through the pager**

Replace the `ReplCommand::Sql(sql)` arm body so command output is buffered and
then paged. The current arm writes via `&mut stdout`; change it to write to a
`Vec<u8>` buffer and emit. Full replacement arm:

```rust
                        ReplCommand::Sql(sql) => {
                            if dml_requires_tx(*tx.lock().unwrap(), sql) {
                                writeln!(
                                    stdout,
                                    "error: INSERT/UPDATE/DELETE requires an explicit transaction. Run BEGIN (or \\begin) first."
                                ).ok();
                                continue;
                            }
                            let mut buf: Vec<u8> = Vec::new();
                            let ok = handler.handle_sql(
                                &query,
                                sql,
                                &SqlOptions {
                                    expanded,
                                    timing,
                                    connection_name: &connection_name,
                                    analytics: Some(&analytics),
                                },
                                &mut schema,
                                &mut |s| rebuild_reedline(&mut rl, &analytics, &connection_name, s),
                                &mut buf,
                            );
                            pager::emit(&String::from_utf8_lossy(&buf), pager_enabled, &mut stdout);
                            let prev = *tx.lock().unwrap();
                            let next = next_tx_state(prev, tx_effect(sql), ok);
                            *tx.lock().unwrap() = next;
                            if prev == TxState::InTransaction && next == TxState::Failed {
                                writeln!(
                                    stdout,
                                    "Transaction aborted. Run \\rollback (or ROLLBACK) to recover."
                                ).ok();
                            }
                        }
```

- [ ] **Step 6: Route EXPLAIN output through the pager**

Replace the `ReplCommand::Explain { sql, analyze }` arm body (from Task 5) so it
buffers then pages:

```rust
                        ReplCommand::Explain { sql, analyze } => {
                            if analyze && dml_requires_tx(*tx.lock().unwrap(), sql) {
                                writeln!(
                                    stdout,
                                    "error: \\explain+ runs ANALYZE which executes the statement; INSERT/UPDATE/DELETE requires an open transaction. Run BEGIN (or \\begin) first."
                                ).ok();
                                continue;
                            }
                            let mut buf: Vec<u8> = Vec::new();
                            explain::handle_explain(&query, sql, analyze, &mut buf);
                            pager::emit(&String::from_utf8_lossy(&buf), pager_enabled, &mut stdout);
                        }
```

- [ ] **Step 7: Add help text**

In `modules/cli/src/repl/ui.rs`, add to `REPL_COMMANDS` (after the `\\explain` entry):

```rust
    ("\\pager",              "toggle paging long output through $PAGER (default on)"),
```

Add a help test in `ui.rs`'s test module:

```rust
    #[test]
    fn help_text_mentions_pager_command() {
        let text = repl_help_text();
        assert!(text.contains("\\pager"), "help should mention \\pager, got: {text}");
    }
```

- [ ] **Step 8: Run the full workspace test suite + clippy**

Run: `cargo test --workspace && cargo clippy --workspace`
Expected: all tests PASS; clippy clean.

- [ ] **Step 9: Manual smoke test (optional but recommended)**

Run against a real DB (or skip if unavailable):
```bash
cargo run -- shell <some-connection>
# then in the REPL:
#   \explain SELECT * FROM <table>
#   \explain+ SELECT * FROM <table>     (read-only query)
#   run a SELECT returning many rows -> should open in less; q to quit
#   \pager   -> "Pager is off."; rerun the big SELECT -> prints inline
```
Expected: explain prints an indented tree; large output pages; `\pager` toggles it.

- [ ] **Step 10: Commit**

```bash
git add modules/cli/src/repl/mod.rs modules/cli/src/repl/ui.rs
git commit -m "feat(cli): page long REPL output through \$PAGER with \\pager toggle"
```

---

## Task 8: Documentation

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update CLAUDE.md**

In `CLAUDE.md`:

1. In the `repl/` file map, update the `command_handler.rs` / add `explain.rs` and
   `pager.rs` entries:
   ```
     explain.rs            — \explain / \explain+: renders QueryApi::explain (ExplainPlan) as an ASCII tree; no SQL
     pager.rs              — routes long REPL output through $PAGER (fallback less -SR); \pager toggles
   ```

2. In the API-boundary list, add `ExplainPlan, ExplainNode` to the imported types.

3. Add a short subsection near "Schema refresh":
   ```
   **EXPLAIN:** `\explain <query>` renders the plan tree (core runs `EXPLAIN (FORMAT JSON)` behind `CatalogPort`, returns an `ExplainPlan`; the CLI renders it). `\explain+` adds `ANALYZE` and therefore executes the statement — it is subject to the same DML transaction guard.

   **Pager:** REPL query/EXPLAIN output is buffered and routed through `repl/pager.rs`, which pages via `$PAGER` (fallback `less -SR`) only when output exceeds the terminal height and stdout is a TTY. `\pager` toggles it (default on).
   ```

- [ ] **Step 2: Verify the build is still green**

Run: `cargo test --workspace`
Expected: PASS (docs-only change; confirms nothing drifted).

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: document \\explain, \\explain+, and the auto pager"
```

---

## Self-Review notes

- **Spec coverage:** A1 EXPLAIN tree → Tasks 1–5; `\explain+` DML guard → Task 5
  Step 5 + Task 7 Step 6; A2 pager (auto, `$PAGER`, fallback `less -SR`, TTY-only,
  `\pager` toggle) → Tasks 6–7; tests called out per spec's Testing section →
  every task's TDD steps; docs → Task 8. `\timing` / format-switch correctly
  absent (out of scope).
- **Type consistency:** `ExplainPlan { root }`, `ExplainNode { node_type,
  relation, total_cost, plan_rows, actual_time_ms, actual_rows, detail, children }`,
  `CatalogPort::explain(&self, sql, analyze)`, `QueryApi::explain(sql, analyze)`,
  `handle_explain(db, sql, analyze, writer)`, `render_plan(&ExplainPlan)`,
  `pager::should_page(content, enabled, term_rows)`, `pager::emit(content,
  enabled, writer)` — used consistently across tasks.
- **Ordering hazards documented:** `\explain+ ` must be matched before `\explain `
  (Task 5 Step 4); crossterm version should track reedline's (Task 6 Step 1).
