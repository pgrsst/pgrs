# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`pgrs` is a CLI tool for managing named PostgreSQL connection configurations. Connections are stored in `~/.pgrs/connections.json`. The tool also supports launching an interactive SQL REPL (`shell`) and handing off to `psql` (`connect`).

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
    ports/connection_repository.rs — ConnectionRepository trait (add, list, delete, get_connection)
    ports/db_connection.rs         — DbConnection trait (execute, list_tables, list_columns) + QueryResult
    services/connection/service.rs — ConnectionService<R>: business logic, validation
    services/schema/service.rs     — SchemaService: loads table/column metadata for REPL completion
  adapters/
    driven/file_connection_repository.rs — reads/writes connections.json
    driven/postgres_db.rs                — PostgresDb: implements DbConnection via postgres crate
    driving/cli.rs                       — Cli<R>: parses argv, dispatches to ConnectionService
    driving/repl/                        — interactive SQL REPL (reedline-based)
      mod.rs      — REPL loop, backslash commands (\dt, \x, \refresh, \q), DDL auto-refresh
      completer.rs — SqlCompleter, SqlHighlighter, SqlHinter backed by SchemaService
      executor.rs  — formats and prints QueryResult (normal and expanded \x mode)
    driving/completions.rs / completions/ — shell completion scripts (bash, zsh, fish)
```

**Dependency direction:** `cli` / `repl` → `ConnectionService` / `SchemaService` → port traits ← adapters. The core never imports from adapters.

**Generics over trait objects:** `Cli<R>` and `ConnectionService<R>` are generic over `R: ConnectionRepository`. The REPL takes `Box<dyn DbConnection>` because the concrete type isn't known at compile time in `app.rs`.

**CLI argument parsing:** No external arg-parsing library. Args are matched with `--key=value` prefix stripping via `optional_option` / `required_option` in `cli.rs`. Port defaults to 5432.

**`shell` vs `connect`:** `shell` opens the built-in pgrs REPL (reedline, tab-completion, `\x` expanded display). `connect` execs `psql` directly, replacing the process.

**Schema refresh:** After DDL queries (`CREATE/DROP/ALTER/TRUNCATE`) the REPL auto-refreshes `SchemaService`. Manual refresh via `\refresh`.
