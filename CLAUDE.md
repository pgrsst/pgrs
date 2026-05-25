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
  app.rs                — wires repository → service → CLI; intercepts "shell" before CLI dispatch
  core/
    domain/connection.rs           — Connection struct + TlsMode enum
    domain/analytics.rs            — HistoryEntry, FreqEntry value types
    domain/error.rs                — DomainError enum
    ports/connection_repository.rs — ConnectionRepository trait (add, list, delete, get_connection)
    ports/db_connection.rs         — DbConnection trait (execute, list_tables, list_columns) + QueryResult
    ports/schema_port.rs           — SchemaPort trait (list_columns → HashMap<table, Vec<col>>)
    ports/schema_cache_port.rs     — SchemaCachePort trait (save/load/invalidate per connection)
    ports/analytics_port.rs        — AnalyticsPort trait (record_query, get_history, get_frequent_*)
    ports/repl_port.rs             — ReplPort = DbConnection + SchemaPort (blanket impl)
    services/connection/service.rs — ConnectionService<R>: business logic, validation
    services/schema/service.rs     — SchemaService: loads table/column metadata for REPL completion
  adapters/
    driven/sqlite_repository.rs    — SqliteRepository: single SQLite file implementing ConnectionRepository
                                     + AnalyticsPort + SchemaCachePort (replaces old connections.json)
    driven/postgres_db.rs          — PostgresDb: implements DbConnection via postgres crate
    driving/cli.rs                 — Cli<R>: parses argv, dispatches to ConnectionService
    driving/repl/                  — interactive SQL REPL (reedline-based)
      mod.rs        — REPL loop, backslash commands (\dt, \d <table>, \x, \export, \refresh, \q), DDL auto-refresh
      completer.rs  — SqlCompleter, SqlHighlighter, SqlHinter backed by SchemaService
      executor.rs   — formats and prints QueryResult (normal and expanded \x mode)
      tokenizer.rs  — SqlToken enum + tokenize(); handles words, string literals, numbers, comments
      alias.rs      — AliasMap (alias→table), build_alias_map, extract_join_context for tab-completion
      describe.rs   — \d <table>: fetches columns, indexes, FK, check constraints, triggers via pg_catalog
    driving/completions.rs / completions/ — shell completion scripts (bash, zsh, fish)
```

**Dependency direction:** `cli` / `repl` → `ConnectionService` / `SchemaService` → port traits ← adapters. The core never imports from adapters.

**`SqliteRepository` triple role:** The single `SqliteRepository` struct implements `ConnectionRepository`, `AnalyticsPort`, and `SchemaCachePort`. In `app.rs` it is created once as `Arc<SqliteRepository>` and then cast to each trait as needed. The SQLite DB lives at `~/.pgrs/pgrs.db` and is auto-migrated on first open.

**Generics over trait objects:** `Cli<R>` and `ConnectionService<R>` are generic over `R: ConnectionRepository`. The REPL takes `Box<dyn DbConnection>` because the concrete type isn't known at compile time in `app.rs`.

**CLI argument parsing:** No external arg-parsing library. Args are matched with `--key=value` prefix stripping via `optional_option` / `required_option` in `cli.rs`. Port defaults to 5432.

**`shell` vs `connect`:** `shell` opens the built-in pgrs REPL (reedline, tab-completion, `\x` expanded display). `connect` execs `psql` directly, replacing the process.

**Schema refresh:** After DDL queries (`CREATE/DROP/ALTER/TRUNCATE`) the REPL auto-refreshes `SchemaService`. Manual refresh via `\refresh`.

**Multi-line statements:** The REPL buffers input until a `;` terminates the statement (respecting open string literals and quoted identifiers).

**Tab-completion pipeline:** `tokenizer.rs` tokenizes the current line → `alias.rs` builds an `AliasMap` (alias→real table) and `JoinContext` → `completer.rs` uses those plus `SchemaService` to suggest keywords, tables, or columns depending on the preceding SQL keyword (`FROM`/`JOIN` → tables; `SELECT`/`WHERE`/`ON` → columns). Known limitation: schema-qualified names (`public.users`) partially disrupt alias extraction — the dot emits `Other('.')` which breaks the state machine for that table.

## Known Limitations

- **`\export` does not block CTE-wrapped DML.** Queries starting with `WITH` that wrap `INSERT`/`UPDATE`/`DELETE` (e.g., `WITH rows AS (...) INSERT INTO ...`) are not detected as mutations and will be re-executed. This matches the same limitation in the `is_ddl` detection used for schema auto-refresh.

- **Tab-completion schema-qualified names.** Schema-qualified names (`public.users`) partially disrupt alias extraction — the dot emits `Other('.')` which breaks the state machine for that table.
