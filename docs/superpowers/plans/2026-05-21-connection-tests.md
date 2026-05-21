# Connection Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambah unit test untuk `ConnectionService` dan integration test untuk `FileConnectionRepository`.

**Architecture:** Service di-test dengan stub `ConnectionRepository` in-memory (tanpa I/O). Repository di-test dengan `tempfile::tempdir()` sehingga file temporer otomatis terhapus setelah tiap test. Test ditulis inline di masing-masing file menggunakan `#[cfg(test)]`.

**Tech Stack:** Rust built-in test framework (`#[test]`), `tempfile = "3"` (dev-dependency)

---

### Task 1: Tambah dev-dependency `tempfile`

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Tambah dev-dependency**

Edit `Cargo.toml`, tambah section berikut setelah `[dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Verifikasi compile**

```bash
cargo check
```

Expected: `Finished` tanpa error.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add tempfile dev-dependency for tests"
```

---

### Task 2: Unit test `ConnectionService`

**Files:**
- Modify: `src/core/services/connection/service.rs`

- [ ] **Step 1: Tulis semua test sekaligus (mereka akan fail karena belum ada)**

Tambah blok berikut di bagian paling bawah `src/core/services/connection/service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::connection::Connection;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use std::cell::RefCell;

    struct StubRepository {
        connections: RefCell<Vec<Connection>>,
    }

    impl StubRepository {
        fn new() -> Self {
            Self {
                connections: RefCell::new(vec![]),
            }
        }
    }

    impl ConnectionRepository for StubRepository {
        fn add(&self, connection: Connection) -> Result<(), String> {
            self.connections.borrow_mut().push(connection);
            Ok(())
        }

        fn list(&self) -> Result<Vec<Connection>, String> {
            Ok(self.connections.borrow().clone())
        }

        fn delete(&self, name: &str) -> Result<(), String> {
            let mut connections = self.connections.borrow_mut();
            let initial_len = connections.len();
            connections.retain(|c| c.name != name);
            if connections.len() == initial_len {
                return Err(format!("connection '{}' not found", name));
            }
            Ok(())
        }
    }

    fn valid_input(name: &str) -> AddConnectionInput {
        AddConnectionInput {
            name: name.to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: "admin".to_string(),
            password: "secret".to_string(),
            database: "mydb".to_string(),
        }
    }

    fn service() -> ConnectionService<StubRepository> {
        ConnectionService::new(StubRepository::new())
    }

    #[test]
    fn add_connection_succeeds() {
        let svc = service();
        assert!(svc.add_connection(valid_input("prod")).is_ok());
    }

    #[test]
    fn add_connection_rejects_empty_name() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            name: "  ".to_string(),
            ..valid_input("x")
        });
        assert_eq!(result, Err("connection name is required".to_string()));
    }

    #[test]
    fn add_connection_rejects_empty_host() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            host: "".to_string(),
            ..valid_input("prod")
        });
        assert_eq!(result, Err("host is required".to_string()));
    }

    #[test]
    fn add_connection_rejects_empty_database() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            database: "".to_string(),
            ..valid_input("prod")
        });
        assert_eq!(result, Err("database is required".to_string()));
    }

    #[test]
    fn add_connection_rejects_empty_username() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            username: "".to_string(),
            ..valid_input("prod")
        });
        assert_eq!(result, Err("username is required".to_string()));
    }

    #[test]
    fn add_connection_rejects_empty_password() {
        let svc = service();
        let result = svc.add_connection(AddConnectionInput {
            password: "".to_string(),
            ..valid_input("prod")
        });
        assert_eq!(result, Err("password is required".to_string()));
    }

    #[test]
    fn list_connections_returns_all() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        svc.add_connection(valid_input("staging")).unwrap();
        let list = svc.list_connections().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "prod");
        assert_eq!(list[1].name, "staging");
    }

    #[test]
    fn delete_connection_succeeds() {
        let svc = service();
        svc.add_connection(valid_input("prod")).unwrap();
        assert!(svc.delete_connection("prod").is_ok());
        assert!(svc.list_connections().unwrap().is_empty());
    }

    #[test]
    fn delete_connection_rejects_empty_name() {
        let svc = service();
        let result = svc.delete_connection("  ");
        assert_eq!(result, Err("connection name is required".to_string()));
    }
}
```

- [ ] **Step 2: Jalankan test**

```bash
cargo test connection::service::tests
```

Expected: semua 9 test `ok`.

- [ ] **Step 3: Commit**

```bash
git add src/core/services/connection/service.rs
git commit -m "test: add unit tests for ConnectionService"
```

---

### Task 3: Integration test `FileConnectionRepository`

**Files:**
- Modify: `src/adapters/driven/file_connection_repository.rs`

- [ ] **Step 1: Tulis semua test**

Tambah blok berikut di bagian paling bawah `src/adapters/driven/file_connection_repository.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use std::os::unix::fs::PermissionsExt;

    fn sample_connection(name: &str) -> Connection {
        Connection {
            name: name.to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: "admin".to_string(),
            password: "secret".to_string(),
            database: "mydb".to_string(),
        }
    }

    fn repo() -> (FileConnectionRepository, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let repo = FileConnectionRepository::new(dir.path().join("connections.json"));
        (repo, dir)
    }

    #[test]
    fn list_returns_empty_when_file_does_not_exist() {
        let (repo, _dir) = repo();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn add_persists_connection() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "prod");
    }

    #[test]
    fn add_returns_error_on_duplicate_name() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        let result = repo.add(sample_connection("prod"));
        assert_eq!(result, Err("connection 'prod' already exists".to_string()));
    }

    #[test]
    fn list_returns_all_connections() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        repo.add(sample_connection("staging")).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn delete_removes_connection() {
        let (repo, _dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        repo.delete("prod").unwrap();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn delete_returns_error_when_not_found() {
        let (repo, _dir) = repo();
        let result = repo.delete("nonexistent");
        assert_eq!(result, Err("connection 'nonexistent' not found".to_string()));
    }

    #[test]
    fn write_sets_file_permissions_to_0600() {
        let (repo, dir) = repo();
        repo.add(sample_connection("prod")).unwrap();
        let mode = std::fs::metadata(dir.path().join("connections.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
```

- [ ] **Step 2: Jalankan test**

```bash
cargo test driven::file_connection_repository::tests
```

Expected: semua 7 test `ok`.

- [ ] **Step 3: Commit**

```bash
git add src/adapters/driven/file_connection_repository.rs
git commit -m "test: add integration tests for FileConnectionRepository"
```
