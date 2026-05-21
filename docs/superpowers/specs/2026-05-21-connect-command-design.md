# Design: `pgrs connect` Command

**Date:** 2026-05-21  
**Status:** Approved

## Overview

Add a `connect` command to `pgrs` that launches an interactive `psql` session using a named stored connection. The concept mirrors pgcli's DSN alias feature — store once, connect by name.

## Command

```
pgrs connect <name>
```

Looks up the connection named `<name>` and exec-replaces the current process with `psql`, passing credentials via the `PGPASSWORD` environment variable and host/user/database as CLI arguments.

## Architecture

### CLI (`src/adapters/driving/cli.rs`)

Add a new match arm:

```rust
Some("connect") => self.connect_to(&args[1..])
```

New method `connect_to`:
1. Extract `name` from `args[0]`
2. Call `connection_service.get_connection(&name)`
3. Build `std::process::Command` for `psql`:
   - env: `PGPASSWORD=<password>`
   - args: `-h <host> -p <port> -U <username> -d <database>`
4. On Unix: use `Command::exec` to replace the current process so `psql` becomes the foreground terminal directly (no dangling `pgrs` parent)

### Service (`src/core/services/connection/service.rs`)

Add method:

```rust
pub fn get_connection(&self, name: &str) -> Result<Connection, String>
```

Delegates to `self.repository.get_connection(name)`.

### Repository trait (`src/core/ports/connection_repository.rs`)

Add method to `ConnectionRepository` trait:

```rust
fn get_connection(&self, name: &str) -> Result<Connection, String>;
```

### File repository (`src/adapters/driven/file_connection_repository.rs`)

Implement `get_connection`: read the list, find by name, return `Err("connection '<name>' not found")` if not found.

## Error Handling

| Scenario | Error message |
|---|---|
| Name not provided | `usage: pgrs connect <connection-name>` |
| Connection not found | `connection '<name>' not found` |
| `psql` not in PATH | `psql not found — is it installed?` |

## Credentials Approach

Use `PGPASSWORD` env var + separate CLI arguments. Password is not visible in the process argument list (`ps aux`). This is the standard approach used by most PostgreSQL tooling.

## No external dependencies

Uses only `std::process::Command` from Rust's standard library. No new crates required.
