# Spec A — REPL Enhancements: EXPLAIN tree + Auto pager

Date: 2026-06-08
Status: Approved (design)

## Goal

Add two REPL quality-of-life features inspired by DataGrip/pgcli:

1. **`\explain` / `\explain+`** — run `EXPLAIN` and render the query plan as an
   indented ASCII tree (DataGrip-style), instead of psql's flat text.
2. **Auto pager** — page long query/explain output through `$PAGER` when it
   exceeds the terminal height, like psql.

`\timing` and output-format switching are explicitly **out of scope** (`\timing`
already exists; format switching was dropped by the user).

## A1. EXPLAIN with tree visualization

Follows the existing `describe_table` pattern: core parses the Postgres-specific
output into a domain value; the CLI renders it. Postgres JSON parsing is an
adapter concern, so it lives behind `CatalogPort` (alongside `describe_table` /
`list_databases`).

### Core changes

- **domain** `domain/explain.rs`:
  ```rust
  pub struct ExplainPlan { pub root: ExplainNode }
  pub struct ExplainNode {
      pub node_type: String,
      pub total_cost: f64,
      pub plan_rows: u64,
      pub actual_time_ms: Option<f64>,   // present only with ANALYZE
      pub actual_rows: Option<u64>,      // present only with ANALYZE
      pub detail: Vec<String>,           // e.g. "Filter: (active = true)", index name
      pub children: Vec<ExplainNode>,
  }
  ```
- **port** `ports/catalog_port.rs`: add
  `fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainPlan, DomainError>;`
- **adapter** `adapters/driven/postgres_catalog.rs`: run
  `EXPLAIN (FORMAT JSON, ANALYZE <analyze>, BUFFERS <analyze>) <sql>`, parse the
  returned JSON into `ExplainPlan` recursively.
- **dependency**: add `serde_json` to `pgrs-core` for plan parsing (approved —
  manual string parsing of the plan JSON is too fragile).
- **api** `api/query.rs`: `QueryApi::explain(sql, analyze) -> Result<ExplainPlan>`
  delegating to the `CatalogPort` on the live connection (same wiring as
  `describe_table`).
- **facade**: re-export `ExplainPlan` (and `ExplainNode` if the CLI needs it)
  from the public API.

### CLI changes

- **`repl/explain.rs`**: render `ExplainPlan` to an indented ASCII tree, e.g.:
  ```
  Seq Scan on users  (cost=0.00..18.50 rows=850)
  └─ Filter: (active = true)  (actual=0.012ms rows=842)
  ```
  Show `actual=…ms rows=…` only when ANALYZE was used (fields are `Some`).
- **`repl/mod.rs`**: parse `\explain <query>` (analyze=false) and
  `\explain+ <query>` (analyze=true) in `ReplCommand`; bare `\explain` /
  `\explain+` print a usage line (mirror the `\d+` usage pattern).
- **DML guard**: `\explain+` runs `ANALYZE`, which *executes* the statement. It
  must go through the existing `dml_requires_tx` check — `EXPLAIN ANALYZE INSERT`
  on an idle connection is rejected with the same message. `\explain` (no
  ANALYZE) is always safe.
- Output of explain is routed through the pager (see A2).

## A2. Auto pager (CLI-only)

- **`repl/pager.rs`**: `emit(content: &str, enabled: bool)`:
  - If `enabled` and the line count of `content` exceeds the terminal height
    (`crossterm::terminal::size()`), spawn `$PAGER` (fallback `less -SR` so ANSI
    colors and horizontal scroll work), write `content` to its stdin, and wait.
  - Otherwise (disabled, output fits, not a TTY, or spawn fails) print directly
    to stdout. Spawn failure must degrade gracefully to direct print.
- **Integration** (`repl/mod.rs`): for the `Sql` and `explain` branches, collect
  command output into a `Vec<u8>` buffer, then `pager::emit(&buf, pager_enabled)`.
  This keeps `handle_sql`'s signature unchanged (it still writes to a generic
  `Write`), so existing tests keep passing a `Vec` with no paging.
- **Toggle**: `\pager` flips paging on/off; default **on**. State lives in the
  run loop next to `expanded` / `timing`, with a `"Pager is on/off."` notice.
- **dependency**: add `crossterm` to `pgrs-cli` (already in the tree via
  reedline) for `terminal::size()`.

Interactive prompts (quit confirmation) read stdin directly and are **not**
routed through the pager.

## Testing

- `explain.rs` rendering: unit tests over hand-built `ExplainPlan` values
  (nested children, with/without ANALYZE fields) asserting tree shape and that
  actual-time only appears under ANALYZE.
- Plan parsing in core: unit test feeding representative `EXPLAIN (FORMAT JSON)`
  output strings into the parser → expected `ExplainPlan`.
- `pager::emit`: test the decision logic (page vs direct) via an injectable
  height + a fake "spawn" sink; assert graceful fallback to direct print.
- `ReplCommand::parse`: `\explain` / `\explain+` (with arg and bare) and
  `\pager` map to the right variants.

## Out of scope

- `\timing` (already implemented).
- Output-format switching (`\json` / `\markdown` / `\csv`) — dropped by user.
- EXPLAIN plan cost-coloring / hotspot highlighting (possible follow-up).
