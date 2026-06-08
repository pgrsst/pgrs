# Spec B — Saved Queries (per-connection, save-from-history-id)

Date: 2026-06-08
Status: Approved (design)

## Goal

Let users name and re-run frequently used queries from the `shell` REPL — the
CLI analogue of DataGrip "favorites". Saved queries are **per-connection**,
persisted in `~/.pgrs/pgrs.db`, and executed immediately on `\run`.

## Data model & core changes

Mirrors the `query_history` vertical slice exactly (store + port + service + api),
using the established `connection_id` foreign-key pattern with the
`SqliteRepository::connection_id_for` lookup helper.

- **Migration v3** (`adapters/driven/sqlite/migrations.rs`, `if version < 3`):
  ```sql
  CREATE TABLE saved_queries (
      id            INTEGER PRIMARY KEY,
      connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
      name          TEXT NOT NULL,
      sql           TEXT NOT NULL,
      created_at    INTEGER NOT NULL,
      UNIQUE (connection_id, name)
  );
  CREATE INDEX IF NOT EXISTS idx_saved_queries_conn ON saved_queries(connection_id);
  ```
  Additive migration — no data loss, no warning needed (unlike v2).
- **domain** `domain/saved_query.rs`: `SavedQuery { id, name, sql, created_at }`.
- **port** `ports/saved_query_repository.rs`:
  ```rust
  trait SavedQueryRepository {
      fn save(&self, connection_name: &str, name: &str, sql: &str) -> Result<(), DomainError>;
      fn list_by_connection(&self, connection_name: &str) -> Vec<SavedQuery>;
      fn find_by_name(&self, connection_name: &str, name: &str) -> Option<SavedQuery>;
      fn delete(&self, connection_name: &str, name: &str) -> Result<(), DomainError>;
  }
  ```
  `save` returns an error when the name already exists for that connection
  (conservative, consistent with `\export`'s "file already exists" stance — no
  silent overwrite).
- **adapter** `adapters/driven/sqlite/saved_query_store.rs`; register it on
  `SqliteRepository` (`sqlite/mod.rs`).
- **service** `services/saved_query/`.
- **api** `api/saved_query.rs`:
  ```rust
  SavedQueryApi {
      save(connection_name, name, sql),
      list(connection_name) -> Vec<SavedQuery>,
      get(connection_name, name) -> Option<SavedQuery>,
      delete(connection_name, name),
  }
  ```
  Exposed via `Core` (e.g. `core.saved_query_api()`); re-export `SavedQueryApi`
  and `SavedQuery` from the public facade.

## REPL commands (CLI)

`SavedQueryApi` is wired in `app.rs` (like `analytics_api()`) and passed into
`Repl::new`.

| Command           | Action |
|-------------------|--------|
| `\save <name> <id>` | Look up history entry `<id>` (active connection), save its SQL under `<name>`. Errors if the id doesn't exist or the name is taken. Arg parsing mirrors `parse_export_args` (name + integer id). |
| `\saved`          | List saved queries for the active connection (name + SQL preview), `\history`-style table. |
| `\run <name>`     | Fetch the saved SQL and execute it through the existing `handle_sql` path — analytics recorded, DDL auto-refresh, DML transaction guard all apply. |
| `\unsave <name>`  | Delete a saved query. |

Flow: run a query → check `\history` for its id → `\save myquery 42` → later
`\run myquery`. No `last_query` tracking needed (the id comes from history),
which is consistent with how `\export <id> <path>` already works.

`\run` reuses `handle_sql`, so a saved DML statement is still rejected unless a
transaction is open — the guard is not bypassed.

Update REPL help text (`repl/ui.rs`) and `CLAUDE.md` (REPL command list + the
new SQLite store / migration in the architecture section).

## Testing

- **store** (`saved_query_store.rs`): in-memory `SqliteRepository` — save/list/
  find/delete round-trips; duplicate-name `save` errors; per-connection
  isolation; `ON DELETE CASCADE` when a connection is removed.
- **api/service**: behavior over an in-memory `Core`; add a `test-support`
  constructor if the CLI tests need one.
- **CLI** (`repl/`): `ReplCommand::parse` for `\save`/`\saved`/`\run`/`\unsave`
  (with and without args → usage). Arg parser for `\save <name> <id>` (valid,
  missing id, non-integer id). Handler tests using stub history + in-memory
  saved-query store: `\save` from a real history id persists; `\save` with
  unknown id errors; `\run` of a saved SELECT produces result output; `\run` of
  saved DML with no open transaction is rejected.

## Out of scope

- Loading a saved query into the editor buffer for editing (`\edit`) — user chose
  execute-immediately only.
- Global / cross-connection snippets — per-connection only.
- Named parameters / templates in saved SQL.
