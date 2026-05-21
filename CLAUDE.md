# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`pgrs` is a CLI tool for managing named PostgreSQL connection configurations. Connections are stored locally in `connections.json` (JSON array of connection objects). There is no actual database driver — this tool only manages connection metadata.

## Commands

```bash
# Build
cargo build

# Build release binary
cargo build --release

# Run (debug)
cargo run -- add <name> --host=<host> --username=<user> --password=<pass> --database=<db> [--port=<port>]

# Run release binary
./target/release/pgrs add <name> --host=<host> --username=<user> --password=<pass> --database=<db>

# Check (fast compile check, no binary)
cargo check

# Lint
cargo clippy

# Test
cargo test
```

## Architecture

The codebase follows hexagonal architecture (ports and adapters):

```
src/
  main.rs               — entry point, calls app::run()
  app.rs                — dependency wiring: constructs repository → service → CLI and runs
  core/
    domain/connection.rs — Connection struct (name, host, port, username, password, database)
    ports/connection_repository.rs — ConnectionRepository trait (add, list, delete)
    services/connection/service.rs — ConnectionService<R>: business logic, validation
  adapters/
    driven/file_connection_repository.rs — FileConnectionRepository: reads/writes connections.json
    driving/cli.rs       — Cli<R>: parses argv, dispatches to ConnectionService
```

**Dependency direction:** `cli` → `ConnectionService` → `ConnectionRepository` trait ← `FileConnectionRepository`. The core never imports from adapters.

**Generics over trait objects:** Both `Cli<R>` and `ConnectionService<R>` are generic over `R: ConnectionRepository` rather than using `dyn`. Adding a new storage backend means implementing the trait and rewiring in `app.rs`.

**CLI argument parsing:** No external arg-parsing library. Args are matched manually using `--key=value` prefix stripping (see `optional_option` / `required_option` in `cli.rs`). Port defaults to 5432 if `--port` is omitted.

**~/.pgrs/connections.json:** Stored in the user's home directory under `.pgrs/`. The directory is created automatically on startup if it doesn't exist (`app.rs` uses the `dirs` crate to resolve `HOME`). Missing file is treated as empty list; duplicate names are rejected at the `FileConnectionRepository` level.
