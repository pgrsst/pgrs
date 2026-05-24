# DomainError, UUID, run_shell Abstraction — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `String` error types with a typed `DomainError` enum throughout the core, stabilize ID generation using UUID v4, and remove the `Arc<SqliteRepository>` hardcode from `run_shell`.

**Architecture:** Three independent changes landed in sequence. DomainError is defined in `core/domain/error.rs`, propagated through the port trait and all implementors atomically (Rust requires all trait implementors to be updated together), then consumed by the service and converted to `String` at the CLI boundary via `From<DomainError> for String`. UUID replaces DefaultHasher in `generate_id()`. `run_shell` receives trait objects instead of a concrete type.

**Tech Stack:** Rust, uuid 1.x crate (v4 feature), rusqlite, existing hexagonal architecture.

---

## File Map

| File | Change |
|---|---|
| `src/core/domain/error.rs` | **Create** — DomainError enum, Display, From for String |
| `src/core/domain/mod.rs` | Add `pub mod error` |
| `Cargo.toml` | Add `uuid = { version = "1", features = ["v4"] }` |
| `src/core/services/connection/service.rs` | `generate_id()` → uuid, all methods return `DomainError`, tests updated |
| `src/core/ports/connection_repository.rs` | Trait + Arc blanket impl + StubRepository → `DomainError` |
| `src/adapters/driven/sqlite_repository.rs` | ConnectionRepository impl → `DomainError`, tests updated |
| `src/adapters/driving/cli.rs` | `get_connection` test helper: add `.map_err(|e| e.to_string())` |
| `src/app.rs` | `run_shell` signature: drop `Arc<SqliteRepository>`, accept trait objects |

> **Note on compilation:** Tasks 1 and 2 compile independently. Task 3 (DomainError migration) touches the port trait and all its implementors atomically — intermediate steps within Task 3 will not compile until Step 3.4 is complete. Commit only after the full task passes `cargo test`.

---

## Task 1: Add DomainError enum + UUID dependency

**Files:**
- Create: `src/core/domain/error.rs`
- Modify: `src/core/domain/mod.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1.1: Create `src/core/domain/error.rs`**

```rust
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum DomainError {
    NotFound(String),
    AlreadyExists(String),
    ValidationError(String),
    StorageError(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::NotFound(msg)
            | DomainError::AlreadyExists(msg)
            | DomainError::ValidationError(msg)
            | DomainError::StorageError(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<DomainError> for String {
    fn from(e: DomainError) -> Self {
        e.to_string()
    }
}
```

- [ ] **Step 1.2: Export from `src/core/domain/mod.rs`**

Replace the file content with:

```rust
pub mod analytics;
pub mod connection;
pub mod error;
```

- [ ] **Step 1.3: Add uuid to `Cargo.toml`**

Add to `[dependencies]`:

```toml
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 1.4: Verify it compiles**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 1.5: Commit**

```bash
git add src/core/domain/error.rs src/core/domain/mod.rs Cargo.toml Cargo.lock
git commit -m "feat(domain): add DomainError enum and uuid dependency"
```

---

## Task 2: Replace DefaultHasher with UUID in generate_id()

**Files:**
- Modify: `src/core/services/connection/service.rs` (only `generate_id` function)

- [ ] **Step 2.1: Replace `generate_id()` in `service.rs`**

Find and replace the entire `generate_id` function (lines 40–57):

```rust
fn generate_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}
```

- [ ] **Step 2.2: Update the call site in `add_connection`**

The old call was `id: Some(generate_id()?)` because it returned `Result`. Change to:

```rust
id: Some(generate_id()),
```

- [ ] **Step 2.3: Run tests to verify existing ID tests still pass**

```bash
cargo test add_connection_assigns
```

Expected output: 3 tests pass — `add_connection_assigns_non_none_id`, `add_connection_assigns_unique_ids`, `add_connection_assigns_8_char_hex_id`.

- [ ] **Step 2.4: Commit**

```bash
git add src/core/services/connection/service.rs
git commit -m "fix(service): replace DefaultHasher with UUID v4 in generate_id"
```

---

## Task 3: Migrate ConnectionRepository port + all implementors to DomainError

> This task must be completed in full before any intermediate step compiles. Steps 3.1–3.4 change interconnected code; only commit after Step 3.6 passes.

**Files:**
- Modify: `src/core/ports/connection_repository.rs`
- Modify: `src/core/services/connection/service.rs`
- Modify: `src/adapters/driven/sqlite_repository.rs`
- Modify: `src/adapters/driving/cli.rs`

### Step 3.1: Update ConnectionRepository trait and Arc blanket impl

Replace the entire content of `src/core/ports/connection_repository.rs` up to (but not including) the `#[cfg(test)]` block:

```rust
use crate::core::domain::connection::Connection;
use crate::core::domain::error::DomainError;

pub trait ConnectionRepository {
    fn add(&self, connection: Connection) -> Result<(), DomainError>;
    fn list(&self) -> Result<Vec<Connection>, DomainError>;
    fn delete(&self, name: &str) -> Result<(), DomainError>;
    fn get_connection(&self, name: &str) -> Result<Connection, DomainError>;
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError>;
    fn update(&self, connection: Connection) -> Result<(), DomainError>;
}

impl<T: ConnectionRepository> ConnectionRepository for std::sync::Arc<T> {
    fn add(&self, connection: Connection) -> Result<(), DomainError> {
        (**self).add(connection)
    }
    fn list(&self) -> Result<Vec<Connection>, DomainError> {
        (**self).list()
    }
    fn delete(&self, name: &str) -> Result<(), DomainError> {
        (**self).delete(name)
    }
    fn get_connection(&self, name: &str) -> Result<Connection, DomainError> {
        (**self).get_connection(name)
    }
    fn update(&self, connection: Connection) -> Result<(), DomainError> {
        (**self).update(connection)
    }
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError> {
        (**self).rename(old_name, new_name)
    }
}
```

- [ ] **Step 3.2: Update StubConnectionRepository in the same file**

Replace the `impl ConnectionRepository for StubConnectionRepository` block (within the `#[cfg(test)]` module):

```rust
impl ConnectionRepository for StubConnectionRepository {
    fn add(&self, connection: Connection) -> Result<(), DomainError> {
        let mut connections = self.connections.borrow_mut();
        if connections.iter().any(|c| c.name == connection.name) {
            return Err(DomainError::AlreadyExists(
                format!("connection '{}' already exists", connection.name)
            ));
        }
        connections.push(connection);
        Ok(())
    }

    fn list(&self) -> Result<Vec<Connection>, DomainError> {
        Ok(self.connections.borrow().clone())
    }

    fn delete(&self, name: &str) -> Result<(), DomainError> {
        let mut connections = self.connections.borrow_mut();
        let initial_len = connections.len();
        connections.retain(|c| c.name != name);
        if connections.len() == initial_len {
            return Err(DomainError::NotFound(format!("connection '{}' not found", name)));
        }
        Ok(())
    }

    fn get_connection(&self, name: &str) -> Result<Connection, DomainError> {
        self.connections
            .borrow()
            .iter()
            .find(|c| c.name == name)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(format!("connection '{}' not found", name)))
    }

    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError> {
        let mut connections = self.connections.borrow_mut();
        if connections.iter().any(|c| c.name == new_name) {
            return Err(DomainError::AlreadyExists(
                format!("connection '{}' already exists", new_name)
            ));
        }
        let conn = connections
            .iter_mut()
            .find(|c| c.name == old_name)
            .ok_or_else(|| DomainError::NotFound(format!("connection '{}' not found", old_name)))?;
        conn.name = new_name.to_string();
        Ok(())
    }

    fn update(&self, connection: Connection) -> Result<(), DomainError> {
        let mut connections = self.connections.borrow_mut();
        let pos = connections
            .iter()
            .position(|c| c.name == connection.name)
            .ok_or_else(|| DomainError::NotFound(
                format!("connection '{}' not found", connection.name)
            ))?;
        connections[pos] = connection;
        Ok(())
    }
}
```

Also add the import at the top of the `test_support` module:

```rust
use crate::core::domain::error::DomainError;
```

- [ ] **Step 3.3: Update SqliteRepository ConnectionRepository impl**

In `src/adapters/driven/sqlite_repository.rs`, replace the `impl ConnectionRepository for SqliteRepository` block with:

```rust
impl crate::core::ports::connection_repository::ConnectionRepository for SqliteRepository {
    fn add(&self, connection: crate::core::domain::connection::Connection) -> Result<(), crate::core::domain::error::DomainError> {
        use crate::core::domain::error::DomainError;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO connections (name, host, port, username, password, database, tls, environment, uuid)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                connection.name,
                connection.host,
                connection.port as i64,
                connection.username,
                connection.password,
                connection.database,
                connection.tls.to_string(),
                connection.environment,
                connection.id,
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                DomainError::AlreadyExists(format!("connection '{}' already exists", connection.name))
            } else {
                DomainError::StorageError(e.to_string())
            }
        })?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<crate::core::domain::connection::Connection>, crate::core::domain::error::DomainError> {
        use crate::core::domain::connection::Connection;
        use crate::core::domain::error::DomainError;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name, host, port, username, password, database, tls, environment, uuid
                 FROM connections ORDER BY name",
            )
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                let tls_str: String = r.get(6)?;
                Ok(Connection {
                    name: r.get(0)?,
                    host: r.get(1)?,
                    port: r.get::<_, i64>(2)? as u16,
                    username: r.get(3)?,
                    password: r.get(4)?,
                    database: r.get(5)?,
                    tls: SqliteRepository::tls_from_str(&tls_str),
                    environment: r.get(7)?,
                    id: r.get(8)?,
                })
            })
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| DomainError::StorageError(e.to_string()))
    }

    fn delete(&self, name: &str) -> Result<(), crate::core::domain::error::DomainError> {
        use crate::core::domain::error::DomainError;
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute("DELETE FROM connections WHERE name = ?1", rusqlite::params![name])
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        if n == 0 {
            return Err(DomainError::NotFound(format!("connection '{}' not found", name)));
        }
        Ok(())
    }

    fn get_connection(&self, name: &str) -> Result<crate::core::domain::connection::Connection, crate::core::domain::error::DomainError> {
        use crate::core::domain::connection::Connection;
        use crate::core::domain::error::DomainError;
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT name, host, port, username, password, database, tls, environment, uuid
             FROM connections WHERE name = ?1",
            rusqlite::params![name],
            |r| {
                let tls_str: String = r.get(6)?;
                Ok(Connection {
                    name: r.get(0)?,
                    host: r.get(1)?,
                    port: r.get::<_, i64>(2)? as u16,
                    username: r.get(3)?,
                    password: r.get(4)?,
                    database: r.get(5)?,
                    tls: SqliteRepository::tls_from_str(&tls_str),
                    environment: r.get(7)?,
                    id: r.get(8)?,
                })
            },
        )
        .map_err(|e| {
            if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                DomainError::NotFound(format!("connection '{}' not found", name))
            } else {
                DomainError::StorageError(e.to_string())
            }
        })
    }

    fn update(&self, connection: crate::core::domain::connection::Connection) -> Result<(), crate::core::domain::error::DomainError> {
        use crate::core::domain::error::DomainError;
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute(
                "UPDATE connections SET host=?1, port=?2, username=?3, password=?4,
                 database=?5, tls=?6, environment=?7, uuid=?8 WHERE name=?9",
                rusqlite::params![
                    connection.host,
                    connection.port as i64,
                    connection.username,
                    connection.password,
                    connection.database,
                    connection.tls.to_string(),
                    connection.environment,
                    connection.id,
                    connection.name,
                ],
            )
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        if n == 0 {
            return Err(DomainError::NotFound(format!("connection '{}' not found", connection.name)));
        }
        Ok(())
    }

    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), crate::core::domain::error::DomainError> {
        use crate::core::domain::error::DomainError;
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute(
                "UPDATE connections SET name = ?1 WHERE name = ?2",
                rusqlite::params![new_name, old_name],
            )
            .map_err(|e| {
                if e.to_string().contains("UNIQUE constraint failed") {
                    DomainError::AlreadyExists(format!("connection '{}' already exists", new_name))
                } else {
                    DomainError::StorageError(e.to_string())
                }
            })?;
        if n == 0 {
            return Err(DomainError::NotFound(format!("connection '{}' not found", old_name)));
        }
        Ok(())
    }
}
```

- [ ] **Step 3.4: Update ConnectionService in `service.rs`**

Replace `require_field`:

```rust
fn require_field(label: &str, value: &str) -> Result<(), crate::core::domain::error::DomainError> {
    use crate::core::domain::error::DomainError;
    if value.trim().is_empty() {
        Err(DomainError::ValidationError(format!("{label} is required")))
    } else {
        Ok(())
    }
}
```

Update the `impl<R> ConnectionService<R>` method signatures. Change every method that currently returns `Result<_, String>` to return `Result<_, DomainError>`. Add `use crate::core::domain::error::DomainError;` at the top of the file. Update `find_connection`:

```rust
pub fn find_connection(&self, input: &str) -> Result<Connection, DomainError> {
    let connections = self.repository.list()?;
    connections
        .into_iter()
        .find(|c| c.id.as_deref() == Some(input) || c.name == input)
        .ok_or_else(|| DomainError::NotFound(format!("connection '{}' not found", input)))
}
```

Update `edit_connection` validation error:

```rust
return Err(DomainError::ValidationError("at least one field must be specified".to_string()));
```

The full updated import section and method signatures for `service.rs`:

```rust
use crate::core::domain::connection::{Connection, TlsMode};
use crate::core::domain::error::DomainError;
use crate::core::ports::connection_repository::ConnectionRepository;

// ... (structs unchanged) ...

fn require_field(label: &str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        Err(DomainError::ValidationError(format!("{label} is required")))
    } else {
        Ok(())
    }
}

fn generate_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

impl<R> ConnectionService<R>
where
    R: ConnectionRepository,
{
    pub fn new(repository: R) -> Self { Self { repository } }

    pub fn add_connection(&self, input: AddConnectionInput) -> Result<(), DomainError> { ... }
    pub fn list_connections(&self) -> Result<Vec<Connection>, DomainError> { ... }
    pub fn delete_connection(&self, name: &str) -> Result<(), DomainError> { ... }
    #[cfg(test)]
    pub fn get_connection(&self, name: &str) -> Result<Connection, DomainError> { ... }
    pub fn find_connection(&self, input: &str) -> Result<Connection, DomainError> { ... }
    pub fn edit_connection(&self, name: &str, input: EditConnectionInput) -> Result<(), DomainError> { ... }
    pub fn rename_connection(&self, old_name: &str, new_name: &str) -> Result<(), DomainError> { ... }
}
```

(All method bodies remain the same — only the return type annotation changes from `String` to `DomainError`. The `?` operator on repository calls works automatically since repository now returns `DomainError`.)

- [ ] **Step 3.5: Fix CLI `get_connection` test helper in `cli.rs`**

Find the test helper `get_connection` (near line 306) and add `.map_err`:

```rust
#[cfg(test)]
pub(crate) fn get_connection(
    &self,
    name: &str,
) -> Result<crate::core::domain::connection::Connection, String> {
    self.connection_service.get_connection(name).map_err(|e| e.to_string())
}
```

- [ ] **Step 3.6: Verify compilation**

```bash
cargo check
```

Expected: no errors. If you see errors, they will be in one of the four files above — re-check that all method signatures match `DomainError`.

- [ ] **Step 3.7: Update service.rs tests to use DomainError pattern matching**

In the `#[cfg(test)]` block of `service.rs`, add this import:

```rust
use crate::core::domain::error::DomainError;
```

Replace all `assert_eq!(result, Err("...".to_string()))` with `assert!(matches!(...))`. Full list:

```rust
// add_connection_returns_error_on_duplicate_name
assert!(matches!(result, Err(DomainError::AlreadyExists(_))));

// add_connection_rejects_empty_name
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// add_connection_rejects_empty_host
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// add_connection_rejects_empty_database
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// add_connection_rejects_empty_username
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// add_connection_rejects_empty_password
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// delete_connection_rejects_empty_name
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// get_connection_returns_error_when_not_found
assert!(matches!(result, Err(DomainError::NotFound(_))));

// get_connection_rejects_empty_name
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// edit_connection_rejects_no_fields
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// edit_connection_rejects_empty_name
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// edit_connection_rejects_empty_host_value
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// edit_connection_returns_error_when_not_found
assert!(matches!(result, Err(DomainError::NotFound(_))));

// rename_connection_returns_error_when_not_found
assert!(matches!(result, Err(DomainError::NotFound(_))));

// rename_connection_returns_error_when_new_name_exists
assert!(matches!(result, Err(DomainError::AlreadyExists(_))));

// rename_connection_rejects_empty_old_name
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// rename_connection_rejects_empty_new_name
assert!(matches!(result, Err(DomainError::ValidationError(_))));

// find_connection_returns_error_when_not_found
assert!(matches!(result, Err(DomainError::NotFound(_))));
```

- [ ] **Step 3.8: Update sqlite_repository.rs tests**

In the `#[cfg(test)]` block of `sqlite_repository.rs`, add:

```rust
use crate::core::domain::error::DomainError;
```

Replace all `assert_eq!(err, "...")` on repository errors:

```rust
// add_duplicate_returns_error
let err = repo.add(sample_conn("prod")).unwrap_err();
assert!(matches!(err, DomainError::AlreadyExists(_)));

// delete_returns_error_when_not_found
let err = repo.delete("ghost").unwrap_err();
assert!(matches!(err, DomainError::NotFound(_)));

// get_connection_returns_error_when_not_found
let err = repo.get_connection("ghost").unwrap_err();
assert!(matches!(err, DomainError::NotFound(_)));

// update_returns_error_when_not_found
let err = repo.update(sample_conn("ghost")).unwrap_err();
assert!(matches!(err, DomainError::NotFound(_)));

// rename_returns_error_when_not_found
let err = repo.rename("ghost", "new").unwrap_err();
assert!(matches!(err, DomainError::NotFound(_)));

// rename_returns_error_when_new_name_exists
let err = repo.rename("prod", "staging").unwrap_err();
assert!(matches!(err, DomainError::AlreadyExists(_)));

// rename_connection (in test rename_connection)
assert!(matches!(
    repo.get_connection("prod").unwrap_err(),
    DomainError::NotFound(_)
));
```

- [ ] **Step 3.9: Run all tests**

```bash
cargo test
```

Expected: all tests pass. If any fail, check that DomainError Display output still contains the expected strings (e.g. "not found", "already exists") — these are used by CLI tests which check `.contains("not found")`.

- [ ] **Step 3.10: Commit**

```bash
git add src/core/domain/error.rs src/core/domain/mod.rs \
        src/core/ports/connection_repository.rs \
        src/core/services/connection/service.rs \
        src/adapters/driven/sqlite_repository.rs \
        src/adapters/driving/cli.rs
git commit -m "feat(core): replace String errors with DomainError enum end-to-end"
```

---

## Task 4: run_shell abstraction — remove Arc\<SqliteRepository\> hardcode

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 4.1: Update `run_shell` signature in `app.rs`**

Replace:

```rust
fn run_shell<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
    sqlite: Arc<SqliteRepository>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs shell <connection-name>")?;
    let conn = service.find_connection(name)?;
    let db = PostgresDb::new(&conn)?;

    let analytics: Option<Arc<dyn AnalyticsPort>> =
        Some(Arc::clone(&sqlite) as Arc<dyn AnalyticsPort>);
    let schema_cache: Option<Arc<dyn SchemaCachePort>> =
        Some(Arc::clone(&sqlite) as Arc<dyn SchemaCachePort>);

    repl::run(
        Box::new(db),
        &conn.database,
        &conn.name,
        conn.environment.as_deref(),
        analytics,
        schema_cache,
    )
}
```

With:

```rust
fn run_shell<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
    analytics: Option<Arc<dyn AnalyticsPort>>,
    schema_cache: Option<Arc<dyn SchemaCachePort>>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs shell <connection-name>")?;
    let conn = service.find_connection(name)?;
    let db = PostgresDb::new(&conn)?;

    repl::run(
        Box::new(db),
        &conn.database,
        &conn.name,
        conn.environment.as_deref(),
        analytics,
        schema_cache,
    )
}
```

- [ ] **Step 4.2: Update the call site in `run_with_dir`**

Replace:

```rust
Some("shell") => run_shell(&args[1..], &connection_service, Arc::clone(&sqlite)),
```

With:

```rust
Some("shell") => run_shell(
    &args[1..],
    &connection_service,
    Some(Arc::clone(&sqlite) as Arc<dyn AnalyticsPort>),
    Some(Arc::clone(&sqlite) as Arc<dyn SchemaCachePort>),
),
```

- [ ] **Step 4.3: Remove unused import of `SqliteRepository` in `run_shell` scope**

`run_shell` no longer references `SqliteRepository` directly. Check that the import at the top of `app.rs` is still needed for `run_with_dir` (it is, for `SqliteRepository::open`). No change needed to imports.

- [ ] **Step 4.4: Run tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 4.5: Commit**

```bash
git add src/app.rs
git commit -m "refactor(app): run_shell accepts trait objects instead of Arc<SqliteRepository>"
```

---

## Self-review

**Spec coverage:**
- Issue 1 (DomainError full end-to-end): Tasks 1 + 3 ✓
- Issue 2 (UUID generate_id): Task 2 ✓
- Issue 3 (run_shell abstraction): Task 4 ✓

**Placeholder scan:** No TBD or TODO in any step. All code blocks are complete.

**Type consistency:**
- `DomainError` is defined in Task 1, used in Tasks 3 and 4 consistently.
- `generate_id()` return type changes from `Result<String, String>` to `String` in Task 2; call site `id: Some(generate_id())` is updated in the same task.
- `run_shell` signature in Task 4 matches the call site update in the same task.
