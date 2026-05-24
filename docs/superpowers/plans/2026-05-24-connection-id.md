# Connection ID Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambah field `id` (8-char hex, read-only) ke `Connection` yang di-generate saat `add`, dan bisa dipakai sebagai pengganti `name` di semua perintah CLI.

**Architecture:** `id: Option<String>` di domain struct agar backward-compatible dengan koneksi lama. ID di-generate di service layer via `/dev/urandom`. Lookup baru `find_connection(input)` di service yang coba match ID dulu lalu fallback ke name — dipakai semua command handler.

**Tech Stack:** Rust, serde/serde_json, `/dev/urandom` (no new dependencies)

---

### Task 1: Add `id` field to `Connection` struct

**Files:**
- Modify: `src/core/domain/connection.rs`
- Modify: `src/core/ports/connection_repository.rs` (struct literal fix)
- Modify: `src/adapters/driven/file_connection_repository.rs` (struct literal fix)
- Modify: `src/core/services/connection/service.rs` (struct literal fix)

- [ ] **Step 1: Write failing deserialization tests**

Tambahkan di akhir `#[cfg(test)] mod tests` dalam `src/core/domain/connection.rs`:

```rust
#[test]
fn connection_without_id_field_deserializes_to_none() {
    let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db"}"#;
    let conn: Connection = serde_json::from_str(json).unwrap();
    assert_eq!(conn.id, None);
}

#[test]
fn connection_with_id_deserializes_correctly() {
    let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db","id":"a3f9c2d1"}"#;
    let conn: Connection = serde_json::from_str(json).unwrap();
    assert_eq!(conn.id, Some("a3f9c2d1".to_string()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test connection_without_id_field_deserializes_to_none connection_with_id_deserializes_correctly
```

Expected: compile error karena field `id` belum ada.

- [ ] **Step 3: Add `id` field to `Connection` struct**

Di `src/core/domain/connection.rs`, ubah struct `Connection`:

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Connection {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    #[serde(default)]
    pub tls: TlsMode,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}
```

- [ ] **Step 4: Fix struct literal in `service.rs`**

Di `src/core/services/connection/service.rs`, dalam `add_connection()`, tambah `id: None,`:

```rust
let connection = Connection {
    name: input.name,
    host: input.host,
    port: input.port,
    username: input.username,
    password: input.password,
    database: input.database,
    tls: input.tls,
    environment: input.environment,
    id: None,
};
```

- [ ] **Step 5: Fix struct literal in `connection_repository.rs`**

Di `src/core/ports/connection_repository.rs`, dalam `with_names()`:

```rust
Connection {
    name: n.to_string(),
    host: "localhost".to_string(),
    port: DEFAULT_PORT,
    username: "user".to_string(),
    password: "pass".to_string(),
    database: "db".to_string(),
    tls: TlsMode::Disable,
    environment: None,
    id: None,
}
```

- [ ] **Step 6: Fix struct literal in `file_connection_repository.rs`**

Di `src/adapters/driven/file_connection_repository.rs`, dalam `sample_connection()`:

```rust
fn sample_connection(name: &str) -> Connection {
    Connection {
        name: name.to_string(),
        host: "localhost".to_string(),
        port: 5432,
        username: "admin".to_string(),
        password: "secret".to_string(),
        database: "mydb".to_string(),
        tls: crate::core::domain::connection::TlsMode::Disable,
        environment: None,
        id: None,
    }
}
```

- [ ] **Step 7: Run all tests**

```
cargo test
```

Expected: semua pass, termasuk dua test baru di domain.

- [ ] **Step 8: Commit**

```bash
git add src/core/domain/connection.rs src/core/services/connection/service.rs src/core/ports/connection_repository.rs src/adapters/driven/file_connection_repository.rs
git commit -m "feat(domain): add optional id field to Connection"
```

---

### Task 2: Generate ID in `add_connection`

**Files:**
- Modify: `src/core/services/connection/service.rs`

- [ ] **Step 1: Write failing test**

Tambahkan dalam `#[cfg(test)] mod tests` di `src/core/services/connection/service.rs`:

```rust
#[test]
fn add_connection_assigns_non_none_id() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    let conn = svc.get_connection("prod").unwrap();
    assert!(conn.id.is_some(), "id should be assigned on add");
}

#[test]
fn add_connection_assigns_unique_ids() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    svc.add_connection(valid_input("staging")).unwrap();
    let id1 = svc.get_connection("prod").unwrap().id;
    let id2 = svc.get_connection("staging").unwrap().id;
    assert_ne!(id1, id2, "each connection should get a unique id");
}

#[test]
fn add_connection_assigns_8_char_hex_id() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    let id = svc.get_connection("prod").unwrap().id.unwrap();
    assert_eq!(id.len(), 8, "id should be 8 characters, got: {id}");
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "id should be hex, got: {id}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test add_connection_assigns_non_none_id add_connection_assigns_unique_ids add_connection_assigns_8_char_hex_id
```

Expected: FAIL — id masih `None`.

- [ ] **Step 3: Add `generate_id` function**

Tambahkan fungsi baru di `src/core/services/connection/service.rs`, sebelum `impl<R> ConnectionService<R>`:

```rust
fn generate_id() -> String {
    use std::io::Read;
    let mut file = std::fs::File::open("/dev/urandom").expect("failed to open /dev/urandom");
    let mut bytes = [0u8; 4];
    file.read_exact(&mut bytes).expect("failed to read /dev/urandom");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 4: Use `generate_id` in `add_connection`**

Ubah baris `id: None,` di `add_connection()` menjadi:

```rust
id: Some(generate_id()),
```

- [ ] **Step 5: Run tests**

```
cargo test
```

Expected: semua pass.

- [ ] **Step 6: Commit**

```bash
git add src/core/services/connection/service.rs
git commit -m "feat(service): generate 8-char hex id on add_connection"
```

---

### Task 3: Add `find_connection` to `ConnectionService`

**Files:**
- Modify: `src/core/services/connection/service.rs`

- [ ] **Step 1: Write failing tests**

Tambahkan dalam `#[cfg(test)] mod tests` di `src/core/services/connection/service.rs`:

```rust
#[test]
fn find_connection_by_name() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    let conn = svc.find_connection("prod").unwrap();
    assert_eq!(conn.name, "prod");
}

#[test]
fn find_connection_by_id() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    let id = svc.get_connection("prod").unwrap().id.unwrap();
    let conn = svc.find_connection(&id).unwrap();
    assert_eq!(conn.name, "prod");
}

#[test]
fn find_connection_returns_error_when_not_found() {
    let svc = service();
    let result = svc.find_connection("ghost");
    assert_eq!(result, Err("connection 'ghost' not found".to_string()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test find_connection_by_name find_connection_by_id find_connection_returns_error_when_not_found
```

Expected: compile error — method belum ada.

- [ ] **Step 3: Implement `find_connection`**

Tambahkan method baru di `impl<R> ConnectionService<R>` dalam `src/core/services/connection/service.rs`:

```rust
pub fn find_connection(&self, input: &str) -> Result<Connection, String> {
    let connections = self.repository.list()?;
    connections
        .into_iter()
        .find(|c| c.id.as_deref() == Some(input) || c.name == input)
        .ok_or_else(|| format!("connection '{}' not found", input))
}
```

- [ ] **Step 4: Run tests**

```
cargo test
```

Expected: semua pass.

- [ ] **Step 5: Commit**

```bash
git add src/core/services/connection/service.rs
git commit -m "feat(service): add find_connection lookup by id or name"
```

---

### Task 4: Wire `find_connection` in CLI commands

**Files:**
- Modify: `src/adapters/driving/cli.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Update `connect_to` in `cli.rs`**

Cari method `connect_to`. Ganti:

```rust
let connection = self.connection_service.get_connection(&name)?;
```

Menjadi:

```rust
let connection = self.connection_service.find_connection(&name)?;
```

- [ ] **Step 2: Update `edit_connection` in `cli.rs`**

Cari method yang memanggil `self.connection_service.edit_connection(&name, ...)`. Sebelum pemanggilan itu, tambahkan lookup untuk resolve name-or-id:

```rust
let resolved_name = self.connection_service.find_connection(&name)?.name;
```

Lalu ganti `&name` dengan `&resolved_name` pada pemanggilan `edit_connection`:

```rust
self.connection_service.edit_connection(&resolved_name, EditConnectionInput {
    ...
})?;
println!("connection '{resolved_name}' updated");
```

- [ ] **Step 3: Update `delete_connection` in `cli.rs`**

Cari method yang memanggil `self.connection_service.delete_connection(&name)`. Tambahkan resolve sebelumnya:

```rust
let resolved_name = self.connection_service.find_connection(&name)?.name;
self.connection_service.delete_connection(&resolved_name)?;
println!("connection '{resolved_name}' deleted");
```

- [ ] **Step 4: Update `rename_connection` in `cli.rs`**

Cari method yang memanggil `self.connection_service.rename_connection(&old_name, &new_name)`. Tambahkan resolve untuk `old_name`:

```rust
let resolved_old = self.connection_service.find_connection(&old_name)?.name;
self.connection_service.rename_connection(&resolved_old, &new_name)?;
println!("connection '{resolved_old}' renamed to '{new_name}'");
```

- [ ] **Step 5: Update `run_shell` in `app.rs`**

Ganti:

```rust
let conn = service.get_connection(name)?;
```

Menjadi:

```rust
let conn = service.find_connection(name)?;
```

- [ ] **Step 6: Update `run_test` in `app.rs`**

Ganti:

```rust
let conn = service.get_connection(name)?;
```

Menjadi:

```rust
let conn = service.find_connection(name)?;
```

- [ ] **Step 7: Run all tests**

```
cargo test
```

Expected: semua pass.

- [ ] **Step 8: Commit**

```bash
git add src/adapters/driving/cli.rs src/app.rs
git commit -m "feat(cli): accept connection id or name in all commands"
```

---

### Task 5: Show ID column in `list`

**Files:**
- Modify: `src/adapters/driving/cli.rs`

- [ ] **Step 1: Update header row**

Di method `list_connections`, ganti baris `println!` header:

```rust
println!(
    "{:<8}  {:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<env_w$}  {:<user_w$}  {:<tls_w$}  PASSWORD",
    "ID", "NAME", "HOST", "PORT", "DATABASE", "ENV", "USERNAME", "TLS",
);
```

- [ ] **Step 2: Update data rows**

Ganti baris `println!` di dalam loop `for c in &connections`:

```rust
println!(
    "{:<8}  {:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<env_w$}  {:<user_w$}  {:<tls_w$}  ****",
    c.id.as_deref().unwrap_or("-"),
    c.name, c.host, c.port, c.database,
    c.environment.as_deref().unwrap_or(""),
    c.username, c.tls,
);
```

- [ ] **Step 3: Build dan verifikasi output manual**

```
cargo build && cargo run -- list
```

Expected: kolom `ID` muncul paling kiri, lebar 8 karakter. Koneksi tanpa ID tampil `-`.

- [ ] **Step 4: Run all tests**

```
cargo test
```

Expected: semua pass.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/cli.rs
git commit -m "feat(cli): show id column in list output"
```
