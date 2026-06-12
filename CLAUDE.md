# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`pgrs` is a CLI tool for managing named PostgreSQL connection configurations. All state is stored in `~/.pgrs/pgrs.db` (SQLite). The tool also supports launching an interactive SQL REPL (`shell`) and handing off to `psql` (`connect`).

It is a **Cargo workspace** with two crates:

- **`pgrs-core`** (`modules/core`, lib name `pgrs_core`) — all logic: domain, ports, services, driven adapters (SQLite, Postgres). Exposes a thin public **API facade** (`api/`); everything else is crate-private.
- **`pgrs-cli`** (`modules/cli`) — the UI: arg parsing, the reedline REPL, shell completions. Produces the `pgrs` binary. Depends only on `pgrs_core`'s public API + re-exported value types — never on core internals (the boundary is compiler-enforced).

Future `pgrs-desktop` / `pgrs-web` front-ends are intended to consume `pgrs-core` the same way.

## Commands

```bash
# Build whole workspace (produces the `pgrs` binary from pgrs-cli)
cargo build
cargo build --release
cargo build -p pgrs-cli          # just the CLI crate + its deps

# Check / lint / test the whole workspace
cargo check
cargo clippy --workspace
cargo test --workspace
cargo test -p pgrs-core          # core only
cargo test <test_name>           # single test by name

# Run (debug) — `cargo run` resolves to the only binary, pgrs-cli
cargo run -- add <name> --host=<host> --username=<user> --password=<pass> --database=<db> [--port=<port>] [--tls=disable|require|verify-full]
cargo run -- list
cargo run -- edit <name> [--host=<host>] [--username=<user>] ...
cargo run -- rename <old-name> <new-name>
cargo run -- delete <name>
cargo run -- connect <name>       # hands off to psql
cargo run -- shell <name>         # opens pgrs interactive SQL REPL
cargo run -- test <name>          # verifies connectivity (SELECT 1)
cargo run -- completions <bash|zsh|fish>
```

## Architecture

Hexagonal architecture (ports and adapters), split across two crates. Dependency
direction: **`pgrs-cli` → `pgrs_core::api` (facade) → services → port traits ← driven adapters.**
The core never imports from the CLI; the CLI never reaches into core internals.

### `pgrs-core` (`modules/core/src/`)

```
lib.rs                  — Core::init(db_path) composition root + public re-exports.
                          Owns Arc<SqliteRepository>; hands out API facades.
api/                    — the ONLY surface pgrs-cli may use:
  connection.rs         — ConnectionApi: add/list/delete/edit/rename/find/get
  query.rs              — QueryApi: execute(&str) → QueryResult, describe_table/
                          list_databases (delegated to the CatalogPort on the live
                          connection), explain(&str, bool) → ExplainPlan; impls SchemaPort
                          by delegating to the live DB.
                          Built by Core::connect via the DbConnector port (no driver ref)
  schema.rs             — SchemaApi: load/refresh(&QueryApi, conn), tables(), columns_for()
  completions.rs        — CompletionsApi: completions(query, cursor) → Vec<Completion>
  analytics.rs          — AnalyticsApi: record_query(conn, sql, &SchemaApi) [extracts
                          referenced tables/columns internally], history(), frequent_tables/columns()
  saved_query.rs        — SavedQueryApi: save/list/get/delete per-connection saved
                          queries (the REPL's "favorites"); built by Core::saved_query_api()
domain/                 — pure value types: Connection, DomainError, QueryResult,
                          catalog (TableDescription/NamedDef), analytics/access/schema types,
                          explain (ExplainPlan/ExplainNode), SavedQuery
enums/tls_mode.rs       — TlsMode (Disable | Require | VerifyFull)
ports/                  — one trait per repository/capability boundary (live-connection
                          ports omit Send+Sync by design — REPL is single-threaded):
  connection_repository.rs, db_connection.rs (DbConnection),
  db_connector.rs (DbConnector: opens a Connection → Box<dyn ReplPort>),
  catalog_port.rs (CatalogPort; value types live in domain::catalog),
  schema_port.rs (SchemaPort), repl_port.rs (ReplPort = DbConnection + SchemaPort),
  query_history_repository.rs, saved_query_repository.rs, table_access_repository.rs,
  column_access_repository.rs, schema_table_repository.rs, schema_column_repository.rs
services/               — connection, schema, analytics, schema_cache, query_history,
                          saved_query, table_access, column_access, schema_table, schema_column;
                          query/ holds completions + command_completion + query_completion
query/                  — tokenizer.rs (SqlToken + tokenize), alias.rs (AliasMap,
                          build_alias_map, extract_join_context, extract_referenced_tables, SQL_KEYWORDS),
                          classify.rs (is_ddl / is_dml, sqlparser-based),
                          transaction.rs (TxState, TxEffect, tx_effect, next_tx_state — client-side transaction-state tracking)
adapters/driven/
  sqlite/               — SqliteRepository across sub-stores (connection_store,
                          query_history_store, saved_query_store, table_access_store,
                          column_access_store, schema_table_store, schema_column_store) +
                          migrations.rs (open / open_in_memory[test-support] / user_version
                          migrations; v3 adds the saved_queries table)
  postgres_db.rs        — PostgresDb: implements DbConnection via the postgres crate;
                          PostgresConnector: implements DbConnector (opens connections)
  postgres_catalog.rs   — Postgres CatalogPort impl: pg_catalog \d/\l SQL →
                          TableDescription/NamedDef (blanket impl over DbConnection)
```

### `pgrs-cli` (`modules/cli/src/`)

```
main.rs                 — entry point, calls app::run()
app.rs                  — wiring: Core::init() → Cli / Repl; intercepts "shell"/"test"
                          before Cli dispatch, builds QueryApi/SchemaApi/AnalyticsApi
cli/
  mod.rs                — Cli: parses argv, dispatches to handlers (takes a ConnectionApi)
  connection_handler.rs — add/list/edit/rename/delete/connect via ConnectionApi
  common_handler.rs     — help / version / completions
  args.rs               — --key=value parsing, URL parsing, TLS-mode parsing
repl/                   — interactive SQL REPL (reedline-based)
  mod.rs                — REPL loop, dispatches backslash commands, DDL auto-refresh;
                          run_statement (shared SQL exec path: DML guard + handle_sql + tx state)
  command_handler.rs    — \d / \dt / \l / \history / \stats / SQL exec / \refresh
  completer.rs          — SqlCompleter, SqlHighlighter, SqlHinter backed by CompletionsApi/SchemaApi
  executor.rs           — formats and prints QueryResult (normal and expanded \x mode)
  csv.rs                — CSV export for \export
  saved.rs              — \save / \saved / \run / \unsave: parse_save_args + handlers
                          over SavedQueryApi; \run resolves SQL, the loop executes it via run_statement
  describe.rs           — \d <table>: formats QueryApi::describe_table (TableDescription); no SQL
  explain.rs            — \explain / \explain+: renders QueryApi::explain (ExplainPlan) as an ASCII tree; no SQL
  pager.rs              — routes long REPL output through $PAGER (fallback less -SR); \pager toggles
  sql_utils.rs          — is_complete_statement (multi-line buffering; SQL classification is in core)
  ui.rs                 — builds reedline editor, PgrsPrompt, validator, help text
completions.rs /
completions/            — shell completion scripts (bash, zsh, fish)
```

**Composition root:** `Core::init(db_path)` opens (and migrates) the single `Arc<SqliteRepository>`, injects the `PostgresConnector` (as `Arc<dyn DbConnector>`), and exposes `core.connection` (ConnectionApi) plus `core.analytics_api()` / `core.schema_api()` / `core.saved_query_api()` / `core.connect(&conn)`. `app.rs` wires these into `Cli` or, for `shell`/`test`, into `core.connect(&conn)` + `Repl::new(...)`. The Postgres adapter is named only here — `QueryApi` opens connections through the `DbConnector` port. All analytics and schema-cache state is backed by the same SQLite file.

**API boundary (strict):** `pgrs-cli` imports only from `pgrs_core::{ConnectionApi, QueryApi, SchemaApi, CompletionsApi, AnalyticsApi, Completion, CompletionKind, Connection, QueryResult, DbConnection, SchemaPort, ReplPort, TlsMode, AddConnectionInput, EditConnectionInput, QueryHistory, SavedQueryApi, SavedQuery, TableDescription, NamedDef, ExplainPlan, ExplainNode, SqlToken, tokenize, is_ddl, is_dml, SQL_KEYWORDS, DEFAULT_PORT, tx_effect, next_tx_state, TxState, TxEffect, ...}`. Core's `ports`/`services`/`adapters`/`query` modules are `pub(crate)` — not reachable from the CLI.

**CLI argument parsing:** No external arg-parsing library. Args are matched with `--key=value` prefix stripping via `optional_option` in `cli/args.rs`. Port defaults to 5432 (`DEFAULT_PORT`).

**`shell` vs `connect`:** `shell` opens the built-in pgrs REPL (reedline, tab-completion, `\x` expanded display). `connect` execs `psql` directly, replacing the process.

**DML transaction guard:** In the `shell` REPL, `INSERT`/`UPDATE`/`DELETE` (and CTE-wrapped DML) are rejected unless a transaction is open — the user must run `BEGIN`/`\begin` first. Enforced in `repl/mod.rs` via `dml_requires_tx` (built on core's `is_dml` + the tracked `TxState`); `connect`/`psql` is unaffected.

**Schema refresh:** After DDL queries the REPL auto-refreshes `SchemaApi` (cache invalidate + reload). Manual refresh via `\refresh`. `sql_utils::is_ddl` decides when.

**EXPLAIN:** `\explain <query>` renders the plan tree — core runs `EXPLAIN (FORMAT JSON)` behind the `CatalogPort` and returns an `ExplainPlan`; the CLI (`repl/explain.rs`) renders it. `\explain+` adds `ANALYZE` and therefore executes the statement, so it is subject to the same DML transaction guard as INSERT/UPDATE/DELETE.

**Saved queries:** Per-connection "favorites" persisted in `~/.pgrs/pgrs.db` (table `saved_queries`, migration v3). `\save <name> <id>` looks up history entry `<id>` for the active connection and stores its SQL under `<name>` (errors if the id is unknown or the name is taken — no overwrite). `\saved` lists them, `\unsave <name>` deletes one, and `\run <name>` executes the saved SQL through the same `run_statement` path as typed SQL — so analytics, DDL auto-refresh, and the DML transaction guard all still apply. Backed by `SavedQueryApi`; the SQL-lookup/handlers live in `repl/saved.rs`.

**Pager:** REPL query/EXPLAIN output is buffered and routed through `repl/pager.rs`, which pages via `$PAGER` (fallback `less -SR`) only when output exceeds the terminal height and stdout is a TTY. `\pager` toggles it (default on); on any failure it falls back to a direct write so output is never lost.

**Multi-line statements:** The REPL buffers input until a `;` terminates the statement (respecting open string literals and quoted identifiers via `sql_utils::is_complete_statement`).

**REPL line history:** reedline line-editing history (up-arrow recall, Ctrl+R reverse-search) is persisted **per connection** to `~/.pgrs/history-<connection>` via `FileBackedHistory` (cap 1000). The path is threaded from `app.rs` → `Repl::new` → `build_reedline`/`rebuild_reedline`; on a schema refresh the reedline is rebuilt against the **same** file so history survives. This is separate from the analytics `query_history` table. The `\edit` editor deliberately gets no history. Unsafe chars in the connection name are sanitized via `app::sanitize_filename`.

**Backslash-command args:** `repl/args.rs::tokenize_args` is the shared quote-aware tokenizer (single/double quotes group a token, no escapes, unterminated quote taken literally); `single_name_token` enforces exactly-one-token. `\save`/`\run`/`\unsave` route through it, so a quoted name with spaces (`\save "my query" 7`) works. `\export` keeps its own specialized parser (`csv::parse_export_args`) for `~` expansion + id/path split. An unrecognized `\`-prefixed line parses to `ReplCommand::Unknown` and prints `unknown command '…'` rather than being forwarded to Postgres.

**Tab-completion pipeline:** `tokenizer` (core) tokenizes the line → `alias` (core) builds an `AliasMap` and join context → `CompletionsApi` suggests keywords, tables, or columns based on the preceding SQL keyword (`FROM`/`JOIN` → tables; `SELECT`/`WHERE`/`ON` → columns). The CLI's `completer.rs` only adapts these into reedline `Suggestion`s and styling.

**Testing patterns:** Core unit tests use `StubConnectionRepository` and `SqliteRepository::open_in_memory()`. Downstream (pgrs-cli) tests rely on the `test-support` feature of `pgrs-core` (a dev-dependency), which exposes in-memory constructors: `Core::in_memory()`, `ConnectionApi::in_memory()/in_memory_with(&[..])`, `QueryApi::from_repl(Box<dyn ReplPort>)`, and `SchemaApi::for_test(HashMap<table, Vec<col>>)`.

## Known Limitations

- **Tab-completion schema-qualified names.** Schema-qualified names (`public.users`) partially disrupt alias extraction — the dot emits `Other('.')` which breaks the state machine for that table.
