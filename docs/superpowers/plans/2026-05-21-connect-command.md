# Connect Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `pgrs connect <name>` yang membuka sesi interaktif `psql` menggunakan kredensial koneksi yang tersimpan.

**Architecture:** Tambah `get_connection` ke trait `ConnectionRepository`, implementasi di `FileConnectionRepository`, delegasi lewat `ConnectionService`, lalu method `connect_to` di `Cli` yang exec-replace process dengan `psql` dan `PGPASSWORD` env var.

**Tech Stack:** Rust std (`std::process::Command`, `std::os::unix::process::CommandExt`), tidak ada crate baru.

---

### Task 1: Tambah `get_connection` ke repository trait dan FileConnectionRepository

**Files:**
- Modify: `src/core/ports/connection_repository.rs`
- Modify: `src/adapters/driven/file_connection_repository.rs`

- [ ] **Step 1: Tulis failing test untuk `get_connection` di FileConnectionRepository**

Tambahkan di dalam `mod tests` di `src/adapters/driven/file_connection_repository.rs`:

```rust
#[test]
fn get_connection_returns_connection_by_name() {
    let (repo, _dir) = repo();
    repo.add(sample_connection("prod")).unwrap();
    let conn = repo.get_connection("prod").unwrap();
    assert_eq!(conn.name, "prod");
    assert_eq!(conn.host, "localhost");
}

#[test]
fn get_connection_returns_error_when_not_found() {
    let (repo, _dir) = repo();
    let result = repo.get_connection("nonexistent");
    assert_eq!(result, Err("connection 'nonexistent' not found".to_string()));
}
```

- [ ] **Step 2: Jalankan test — pastikan gagal (compile error)**

```bash
cargo test get_connection 2>&1 | head -20
```

Expected: compile error karena `get_connection` belum ada.

- [ ] **Step 3: Tambah `get_connection` ke trait**

Ganti isi `src/core/ports/connection_repository.rs` menjadi:

```rust
use crate::core::domain::connection::Connection;

pub trait ConnectionRepository {
    fn add(&self, connection: Connection) -> Result<(), String>;
    fn list(&self) -> Result<Vec<Connection>, String>;
    fn delete(&self, name: &str) -> Result<(), String>;
    fn get_connection(&self, name: &str) -> Result<Connection, String>;
}
```

- [ ] **Step 4: Implementasi `get_connection` di FileConnectionRepository**

Tambahkan method berikut di dalam `impl ConnectionRepository for FileConnectionRepository` di `src/adapters/driven/file_connection_repository.rs`:

```rust
fn get_connection(&self, name: &str) -> Result<Connection, String> {
    let connections = self.read_connections()?;
    connections
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| format!("connection '{}' not found", name))
}
```

- [ ] **Step 5: Jalankan test — pastikan lulus**

```bash
cargo test get_connection
```

Expected: 2 test lulus.

- [ ] **Step 6: Pastikan semua test masih lulus**

```bash
cargo test
```

Expected: semua test lulus (akan ada compile error di service karena StubRepository belum implement `get_connection` — fix di step ini juga).

Tambahkan ke `impl ConnectionRepository for StubRepository` di `src/core/services/connection/service.rs` (bagian `#[cfg(test)]`):

```rust
fn get_connection(&self, name: &str) -> Result<Connection, String> {
    self.connections
        .borrow()
        .iter()
        .find(|c| c.name == name)
        .cloned()
        .ok_or_else(|| format!("connection '{}' not found", name))
}
```

Jalankan lagi:

```bash
cargo test
```

Expected: semua test lulus.

- [ ] **Step 7: Commit**

```bash
git add src/core/ports/connection_repository.rs src/adapters/driven/file_connection_repository.rs src/core/services/connection/service.rs
git commit -m "feat: add get_connection to repository trait and implementations"
```

---

### Task 2: Tambah `get_connection` ke ConnectionService

**Files:**
- Modify: `src/core/services/connection/service.rs`

- [ ] **Step 1: Tulis failing test untuk `get_connection` di service**

Tambahkan di dalam `mod tests` di `src/core/services/connection/service.rs`:

```rust
#[test]
fn get_connection_returns_existing_connection() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    let conn = svc.get_connection("prod").unwrap();
    assert_eq!(conn.name, "prod");
}

#[test]
fn get_connection_returns_error_when_not_found() {
    let svc = service();
    let result = svc.get_connection("missing");
    assert_eq!(result, Err("connection 'missing' not found".to_string()));
}

#[test]
fn get_connection_rejects_empty_name() {
    let svc = service();
    let result = svc.get_connection("  ");
    assert_eq!(result, Err("connection name is required".to_string()));
}
```

- [ ] **Step 2: Jalankan test — pastikan gagal**

```bash
cargo test get_connection_returns_existing_connection 2>&1 | head -20
```

Expected: compile error karena `get_connection` belum ada di service.

- [ ] **Step 3: Implementasi `get_connection` di ConnectionService**

Tambahkan method berikut di dalam `impl<R> ConnectionService<R>` di `src/core/services/connection/service.rs`:

```rust
pub fn get_connection(&self, name: &str) -> Result<Connection, String> {
    if name.trim().is_empty() {
        return Err("connection name is required".to_string());
    }

    self.repository.get_connection(name)
}
```

- [ ] **Step 4: Jalankan test — pastikan lulus**

```bash
cargo test get_connection
```

Expected: 4 test lulus (2 dari Task 1 + 3 baru, minus duplikat nama).

- [ ] **Step 5: Pastikan semua test lulus**

```bash
cargo test
```

Expected: semua lulus.

- [ ] **Step 6: Commit**

```bash
git add src/core/services/connection/service.rs
git commit -m "feat: add get_connection to ConnectionService"
```

---

### Task 3: Tambah perintah `connect` ke CLI

**Files:**
- Modify: `src/adapters/driving/cli.rs`

- [ ] **Step 1: Tambah arm `connect` ke match dan method `connect_to`**

Di `src/adapters/driving/cli.rs`, tambahkan arm baru di `match`:

```rust
Some("connect") => self.connect_to(&args[1..]),
```

Sehingga block match menjadi:

```rust
match args.first().map(String::as_str) {
    None => {
        println!("{}", welcome());
        Ok(())
    }
    Some("add") => self.add_connection(&args[1..]),
    Some("list") => self.list_connections(),
    Some("delete") => self.delete_connection(&args[1..]),
    Some("connect") => self.connect_to(&args[1..]),
    _ => Err(usage().to_string()),
}
```

Lalu tambahkan method `connect_to` di dalam `impl<R> Cli<R>`:

```rust
fn connect_to(&self, args: &[String]) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    let name = args
        .first()
        .ok_or("usage: pgrs connect <connection-name>")?
        .trim()
        .to_string();

    let connection = self.connection_service.get_connection(&name)?;

    let error = std::process::Command::new("psql")
        .env("PGPASSWORD", &connection.password)
        .arg("-h")
        .arg(&connection.host)
        .arg("-p")
        .arg(connection.port.to_string())
        .arg("-U")
        .arg(&connection.username)
        .arg("-d")
        .arg(&connection.database)
        .exec();

    Err(if error.kind() == std::io::ErrorKind::NotFound {
        "psql not found — is it installed?".to_string()
    } else {
        error.to_string()
    })
}
```

- [ ] **Step 2: Update string `welcome()` untuk sertakan perintah connect**

Ganti fungsi `welcome()` di `src/adapters/driving/cli.rs`:

```rust
fn welcome() -> &'static str {
    "pgrs — PostgreSQL connection manager built with Rust\n\nManage and store named PostgreSQL connections locally.\n\nCommands:\n  add <name> --host=<host> --username=<user> --password=<pass> --database=<db> [--port=<port>]\n             Add a new named connection\n  list         List all saved connections\n  delete <name>\n             Delete a named connection\n  connect <name>\n             Open an interactive psql session using a saved connection\n\nRun `pgrs <command> --help` for more info on a specific command."
}
```

- [ ] **Step 3: Build dan pastikan compile bersih**

```bash
cargo build 2>&1
```

Expected: compile berhasil tanpa error.

- [ ] **Step 4: Jalankan semua test**

```bash
cargo test
```

Expected: semua test lulus.

- [ ] **Step 5: Smoke test manual**

```bash
# Tambah koneksi test
cargo run -- add testconn --host=localhost --username=postgres --password=secret --database=mydb

# Cek list
cargo run -- list

# Coba connect ke nama yang tidak ada — harus error
cargo run -- connect tidakada
```

Expected output untuk nama tidak ada:
```
connection 'tidakada' not found
```

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/cli.rs
git commit -m "feat: add connect command to launch psql with saved connection"
```
