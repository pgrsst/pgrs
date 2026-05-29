# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`pgrs` is a CLI tool for managing named PostgreSQL connection configurations. All state is stored in `~/.pgrs/pgrs.db` (SQLite). The tool also supports launching an interactive SQL REPL (`shell`) and handing off to `psql` (`connect`).

## Commands

```bash
# Build
cargo build

# Build release binary
cargo build --release

# Check (fast compile check, no binary)
cargo check

# Lint
cargo clippy

# Test all
cargo test

# Run a single test by name
cargo test <test_name>

# Run (debug)
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

Hexagonal architecture (ports and adapters):

```
src/
  main.rs               — entry point, calls app::run()
  app.rs                — wires SqliteRepository → services → CLI; intercepts "shell"/"test" before Cli dispatch
  core/
    domain/             — pure value types: Connection, DomainError, and analytics/access/schema domain types
    enums/tls_mode.rs   — TlsMode enum (Disable | Require | VerifyFull)
    ports/              — one trait file per repository/capability boundary
      connection_repository.rs     — ConnectionRepository (add, list, delete, get_connection, update, rename)
      db_connection.rs             — DbConnection (execute, list_tables, list_columns) + QueryResult
      schema_port.rs               — SchemaPort (list_columns → HashMap<table, Vec<col>>)
      repl_port.rs                 — ReplPort = DbConnection + SchemaPort (blanket impl)
      query_history_repository.rs  — QueryHistoryRepository (save, list_recent)
      table_access_repository.rs   — TableAccessRepository (save, list_frequent)
      column_access_repository.rs  — ColumnAccessRepository (save, list_frequent_by_table)
      schema_table_repository.rs   — SchemaTableRepository (upsert/load cached table names)
      schema_column_repository.rs  — SchemaColumnRepository (upsert/load cached column names)
    services/
      connection/service.rs        — ConnectionService: add/edit/rename/delete/find, validation
      schema/service.rs            — SchemaService: loads table+column metadata for REPL completion
      analytics/service.rs         — AnalyticsService: records queries/table/column access
      schema_cache/service.rs      — SchemaCacheService: persists/loads schema per connection
      query_history/service.rs     — QueryHistoryService: wraps history repository
      table_access/service.rs      — TableAccessService: wraps table access repository
      column_access/service.rs     — ColumnAccessService: wraps column access repository
      schema_table/service.rs      — SchemaTableService: wraps schema table repository
      schema_column/service.rs     — SchemaColumnService: wraps schema column repository
  adapters/
    driven/
      sqlite/                      — SqliteRepository split across sub-store modules:
        mod.rs              — SqliteRepository struct, open/open_in_memory, migrations call
        connection_store.rs — implements ConnectionRepository
        query_history_store.rs, table_access_store.rs, column_access_store.rs — analytics repositories
        schema_table_store.rs, schema_column_store.rs — schema cache repositories
        migrations.rs       — SQL schema migrations (user_version pragma)
      postgres_db.rs        — PostgresDb: implements DbConnection via postgres crate
    driving/
      cli.rs                — Cli: parses argv, dispatches to ConnectionService (no generics)
      repl/                 — interactive SQL REPL (reedline-based)
        mod.rs        — REPL loop, dispatches backslash commands, DDL auto-refresh
        commands.rs   — backslash command dispatch (\dt, \d, \x, \export, \refresh, \q)
        completer.rs  — SqlCompleter, SqlHighlighter, SqlHinter backed by SchemaService
        executor.rs   — formats and prints QueryResult (normal and expanded \x mode)
        csv.rs        — CSV export for \export
        tokenizer.rs  — SqlToken enum + tokenize()
        alias.rs      — AliasMap (alias→table), build_alias_map, extract_join_context
        describe.rs   — \d <table>: fetches columns, indexes, FK, constraints via pg_catalog
        sql_utils.rs  — is_ddl, is_mutation helpers
        ui.rs         — builds reedline editor, PgrsPrompt
      completions.rs / completions/ — shell completion scripts (bash, zsh, fish)
```

**Dependency direction:** `cli` / `repl` → services → port traits ← adapters. The core never imports from adapters.

**`SqliteRepository` wiring:** Created once as `Arc<SqliteRepository>` in `app.rs` and cast to each port trait (`Arc<dyn ConnectionRepository>`, `Arc<dyn QueryHistoryRepository>`, etc.) as needed. All analytics and schema-cache state is backed by the same SQLite file via sub-store modules.

**`shell` command wiring:** `app.rs` intercepts `shell` before `Cli` dispatch and manually constructs all services (analytics, schema cache, etc.) before calling `repl::run`.

**CLI argument parsing:** No external arg-parsing library. Args are matched with `--key=value` prefix stripping via `optional_option` / `required_option` in `cli.rs`. Port defaults to 5432.

**`shell` vs `connect`:** `shell` opens the built-in pgrs REPL (reedline, tab-completion, `\x` expanded display). `connect` execs `psql` directly, replacing the process.

**Schema refresh:** After DDL queries the REPL auto-refreshes `SchemaService` (via `SchemaCacheService`). Manual refresh via `\refresh`.

**Multi-line statements:** The REPL buffers input until a `;` terminates the statement (respecting open string literals and quoted identifiers via `sql_utils.rs`).

**Tab-completion pipeline:** `tokenizer.rs` tokenizes the current line → `alias.rs` builds an `AliasMap` and `JoinContext` → `completer.rs` suggests keywords, tables, or columns based on the preceding SQL keyword (`FROM`/`JOIN` → tables; `SELECT`/`WHERE`/`ON` → columns).

**Testing patterns:** Service unit tests use `StubConnectionRepository` from `ports/connection_repository.rs` (in-memory, `#[cfg(test)]`). Adapter tests use `SqliteRepository::open_in_memory()`.

## Known Limitations

- **`\export` does not block CTE-wrapped DML.** Queries starting with `WITH` that wrap `INSERT`/`UPDATE`/`DELETE` are not detected as mutations and will be re-executed. Matches the same gap in `is_ddl` detection.

- **Tab-completion schema-qualified names.** Schema-qualified names (`public.users`) partially disrupt alias extraction — the dot emits `Other('.')` which breaks the state machine for that table.
