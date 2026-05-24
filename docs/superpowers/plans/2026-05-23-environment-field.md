# Environment Field Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambah field `environment` opsional ke `Connection` dan tampilkan di REPL prompt (`pgrs(db:env)>`) serta kolom `list`.

**Architecture:** Field `Option<String>` di domain model dengan `#[serde(default)]` agar backward-compatible. Service menerima `Option<Option<String>>` di `EditConnectionInput` untuk membedakan "tidak diubah" vs "dihapus". REPL prompt membaca environment dari parameter baru di `repl::run`.

**Tech Stack:** Rust, serde_json, reedline

---

## File Map

| File | Perubahan |
|---|---|
| `src/core/domain/connection.rs` | Tambah `environment: Option<String>` ke `Connection` |
| `src/core/ports/connection_repository.rs` | Update `StubConnectionRepository::with_names` |
| `src/core/services/connection/service.rs` | Tambah field ke `AddConnectionInput`/`EditConnectionInput`, update logic |
| `src/adapters/driving/cli.rs` | Parse `--env`, tambah kolom ENV di `list` |
| `src/adapters/driving/repl/mod.rs` | Update `PgrsPrompt` dan signature `run` |
| `src/app.rs` | Teruskan `conn.environment.as_deref()` ke `repl::run` |

---

### Task 1: Tambah `environment` ke `Connection` dan perbaiki semua construction site

**Files:**
- Modify: `src/core/domain/connection.rs`
- Modify: `src/core/ports/connection_repository.rs`
- Modify: `src/core/services/connection/service.rs` (construction site only)

- [ ] **Step 1: Tulis failing tests di `connection.rs`**

Tambahkan dua test baru di dalam `mod tests` di `src/core/domain/connection.rs`:

```rust
#[test]
fn connection_without_environment_field_deserializes_to_none() {
    let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db"}"#;
    let conn: Connection = serde_json::from_str(json).unwrap();
    assert_eq!(conn.environment, None);
}

#[test]
fn connection_with_environment_deserializes_correctly() {
    let json = r#"{"name":"prod","host":"localhost","port":5432,"username":"u","password":"p","database":"db","environment":"production"}"#;
    let conn: Connection = serde_json::from_str(json).unwrap();
    assert_eq!(conn.environment, Some("production".to_string()));
}
```

- [ ] **Step 2: Jalankan test, pastikan GAGAL**

```bash
cargo test connection_without_environment_field_deserializes_to_none
```

Expected: `error[E0609]: no field 'environment' on type 'Connection'` atau compile error.

- [ ] **Step 3: Tambah field `environment` ke `Connection` struct**

Di `src/core/domain/connection.rs`, ubah struct `Connection` menjadi:

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
}
```

- [ ] **Step 4: Update `StubConnectionRepository::with_names` di `connection_repository.rs`**

Di `src/core/ports/connection_repository.rs`, dalam fungsi `with_names`, ubah struct literal `Connection { ... }` menjadi:

```rust
.map(|&n| Connection {
    name: n.to_string(),
    host: "localhost".to_string(),
    port: DEFAULT_PORT,
    username: "user".to_string(),
    password: "pass".to_string(),
    database: "db".to_string(),
    tls: TlsMode::Disable,
    environment: None,
})
```

- [ ] **Step 5: Update construction site di `service.rs`**

Di `src/core/services/connection/service.rs`, dalam `add_connection`, ubah `Connection { ... }` menjadi (sementara pakai `None`, akan diganti di Task 2):

```rust
let connection = Connection {
    name: input.name,
    host: input.host,
    port: input.port,
    username: input.username,
    password: input.password,
    database: input.database,
    tls: input.tls,
    environment: None,
};
```

- [ ] **Step 6: Jalankan semua tests, pastikan PASS**

```bash
cargo test
```

Expected: semua test pass, termasuk dua test baru.

- [ ] **Step 7: Commit**

```bash
git add src/core/domain/connection.rs src/core/ports/connection_repository.rs src/core/services/connection/service.rs
git commit -m "feat(domain): add optional environment field to Connection"
```

---

### Task 2: Tambah `environment` ke service layer inputs dan logic

**Files:**
- Modify: `src/core/services/connection/service.rs`
- Modify: `src/adapters/driving/cli.rs` (placeholder saja agar compile)

- [ ] **Step 1: Tulis failing tests di `service.rs`**

Tambahkan test berikut di dalam `mod tests` di `src/core/services/connection/service.rs`:

```rust
#[test]
fn add_connection_saves_environment() {
    let svc = service();
    svc.add_connection(AddConnectionInput {
        environment: Some("production".to_string()),
        ..valid_input("prod")
    }).unwrap();
    assert_eq!(
        svc.get_connection("prod").unwrap().environment,
        Some("production".to_string())
    );
}

#[test]
fn add_connection_without_environment_saves_none() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    assert_eq!(svc.get_connection("prod").unwrap().environment, None);
}

#[test]
fn edit_connection_sets_environment() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    svc.edit_connection("prod", EditConnectionInput {
        environment: Some(Some("staging".to_string())),
        ..edit_input()
    }).unwrap();
    assert_eq!(
        svc.get_connection("prod").unwrap().environment,
        Some("staging".to_string())
    );
}

#[test]
fn edit_connection_clears_environment() {
    let svc = service();
    svc.add_connection(AddConnectionInput {
        environment: Some("production".to_string()),
        ..valid_input("prod")
    }).unwrap();
    svc.edit_connection("prod", EditConnectionInput {
        environment: Some(None),
        ..edit_input()
    }).unwrap();
    assert_eq!(svc.get_connection("prod").unwrap().environment, None);
}

#[test]
fn edit_connection_with_only_environment_succeeds() {
    let svc = service();
    svc.add_connection(valid_input("prod")).unwrap();
    let result = svc.edit_connection("prod", EditConnectionInput {
        environment: Some(Some("dev".to_string())),
        ..edit_input()
    });
    assert!(result.is_ok());
}

#[test]
fn edit_connection_without_environment_does_not_change_it() {
    let svc = service();
    svc.add_connection(AddConnectionInput {
        environment: Some("prod".to_string()),
        ..valid_input("prod")
    }).unwrap();
    svc.edit_connection("prod", EditConnectionInput {
        database: Some("otherdb".to_string()),
        ..edit_input()
    }).unwrap();
    assert_eq!(
        svc.get_connection("prod").unwrap().environment,
        Some("prod".to_string())
    );
}
```

- [ ] **Step 2: Jalankan tests, pastikan GAGAL**

```bash
cargo test add_connection_saves_environment
```

Expected: compile error karena `environment` belum ada di `AddConnectionInput`.

- [ ] **Step 3: Tambah `environment` ke `AddConnectionInput`**

```rust
pub struct AddConnectionInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub tls: TlsMode,
    pub environment: Option<String>,
}
```

- [ ] **Step 4: Tambah `environment` ke `EditConnectionInput`**

```rust
pub struct EditConnectionInput {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub tls: Option<TlsMode>,
    pub environment: Option<Option<String>>,
}
```

Semantik: `None` = tidak diubah, `Some(None)` = hapus, `Some(Some("x"))` = set ke "x".

- [ ] **Step 5: Update `add_connection` untuk menyimpan `input.environment`**

Di `add_connection`, ubah baris `environment: None` menjadi:

```rust
environment: input.environment,
```

- [ ] **Step 6: Update `edit_connection` — empty-field check**

Di `edit_connection`, tambah `&& input.environment.is_none()` ke kondisi guard:

```rust
if input.host.is_none()
    && input.port.is_none()
    && input.username.is_none()
    && input.password.is_none()
    && input.database.is_none()
    && input.tls.is_none()
    && input.environment.is_none()
{
    return Err("at least one field must be specified".to_string());
}
```

- [ ] **Step 7: Update `edit_connection` — apply environment update**

Tambahkan baris berikut setelah baris `if let Some(v) = input.tls { conn.tls = v; }`:

```rust
if let Some(v) = input.environment { conn.environment = v; }
```

- [ ] **Step 8: Update `valid_input` dan `edit_input` di test helpers**

Di `mod tests` dalam `service.rs`, update kedua fungsi helper:

```rust
fn valid_input(name: &str) -> AddConnectionInput {
    AddConnectionInput {
        name: name.to_string(),
        host: "localhost".to_string(),
        port: DEFAULT_PORT,
        username: "admin".to_string(),
        password: "secret".to_string(),
        database: "mydb".to_string(),
        tls: TlsMode::Disable,
        environment: None,
    }
}

fn edit_input() -> EditConnectionInput {
    EditConnectionInput {
        host: None,
        port: None,
        username: None,
        password: None,
        database: None,
        tls: None,
        environment: None,
    }
}
```

- [ ] **Step 9: Tambah placeholder `environment: None` di `cli.rs` agar compile**

Di `src/adapters/driving/cli.rs`, method `add_connection`, tambahkan `environment: None` ke `AddConnectionInput { ... }` yang dikirim ke `connection_service.add_connection`:

```rust
self.connection_service.add_connection(AddConnectionInput {
    name: name.clone(),
    host,
    port,
    username,
    password,
    database,
    tls,
    environment: None,  // placeholder — akan diganti di Task 3
})?;
```

Dan di method `edit_connection`, tambahkan `environment: None` ke `EditConnectionInput { ... }`:

```rust
self.connection_service.edit_connection(&name, EditConnectionInput {
    host: optional_option(args, "--host"),
    port,
    username: optional_option(args, "--username"),
    password: optional_option(args, "--password"),
    database: optional_option(args, "--database"),
    tls,
    environment: None,  // placeholder — akan diganti di Task 3
})?;
```

- [ ] **Step 10: Jalankan semua tests, pastikan PASS**

```bash
cargo test
```

Expected: semua test pass termasuk enam test baru.

- [ ] **Step 11: Commit**

```bash
git add src/core/services/connection/service.rs src/adapters/driving/cli.rs
git commit -m "feat(service): add environment field to Add/EditConnectionInput"
```

---

### Task 3: CLI — flag `--env` dan kolom ENV di `list`

**Files:**
- Modify: `src/adapters/driving/cli.rs`

- [ ] **Step 1: Tulis failing tests di `cli.rs`**

Tambahkan test berikut di dalam `mod tests` di `src/adapters/driving/cli.rs`:

```rust
#[test]
fn add_with_env_flag_saves_environment() {
    let cli = cli_with(&[]);
    cli.run(add_args("prod", &["--env=production"])).unwrap();
    assert_eq!(
        cli.get_connection("prod").unwrap().environment,
        Some("production".to_string())
    );
}

#[test]
fn add_without_env_flag_leaves_environment_none() {
    let cli = cli_with(&[]);
    cli.run(add_args("prod", &[])).unwrap();
    assert_eq!(cli.get_connection("prod").unwrap().environment, None);
}

#[test]
fn edit_env_flag_sets_environment() {
    let cli = cli_with(&["prod"]);
    cli.run(edit_args("prod", &["--env=staging"])).unwrap();
    assert_eq!(
        cli.get_connection("prod").unwrap().environment,
        Some("staging".to_string())
    );
}

#[test]
fn edit_empty_env_flag_clears_environment() {
    let cli = cli_with(&["prod"]);
    // set dulu
    cli.run(edit_args("prod", &["--env=prod"])).unwrap();
    // lalu clear
    cli.run(edit_args("prod", &["--env="])).unwrap();
    assert_eq!(cli.get_connection("prod").unwrap().environment, None);
}
```

- [ ] **Step 2: Jalankan tests, pastikan GAGAL**

```bash
cargo test add_with_env_flag_saves_environment
```

Expected: FAIL karena `--env` belum diparsing (masih placeholder `None`).

- [ ] **Step 3: Ganti placeholder di `add_connection` dengan parsing `--env`**

Di method `add_connection` di `cli.rs`, ganti `environment: None` dengan:

```rust
self.connection_service.add_connection(AddConnectionInput {
    name: name.clone(),
    host,
    port,
    username,
    password,
    database,
    tls,
    environment: optional_option(args, "--env"),
})?;
```

- [ ] **Step 4: Ganti placeholder di `edit_connection` dengan parsing `--env`**

Di method `edit_connection` di `cli.rs`, tambahkan parsing environment sebelum konstruksi `EditConnectionInput`, lalu ganti placeholder:

```rust
let environment = args.iter()
    .find(|a| a.starts_with("--env="))
    .map(|arg| {
        let val = arg.strip_prefix("--env=").unwrap();
        if val.is_empty() { None } else { Some(val.to_string()) }
    });

self.connection_service.edit_connection(&name, EditConnectionInput {
    host: optional_option(args, "--host"),
    port,
    username: optional_option(args, "--username"),
    password: optional_option(args, "--password"),
    database: optional_option(args, "--database"),
    tls,
    environment,
})?;
```

- [ ] **Step 5: Tambah kolom ENV di `list_connections`**

Di method `list_connections`, setelah baris `let tls_w = ...`, tambahkan:

```rust
let env_w = connections
    .iter()
    .map(|c| c.environment.as_deref().unwrap_or("").len())
    .max()
    .unwrap_or(3)
    .max(3);
```

Ubah header print menjadi:

```rust
println!(
    "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<env_w$}  {:<user_w$}  {:<tls_w$}  PASSWORD",
    "NAME", "HOST", "PORT", "DATABASE", "ENV", "USERNAME", "TLS",
);
```

Ubah row print menjadi:

```rust
for c in &connections {
    println!(
        "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<env_w$}  {:<user_w$}  {:<tls_w$}  ****",
        c.name, c.host, c.port, c.database,
        c.environment.as_deref().unwrap_or(""),
        c.username, c.tls,
    );
}
```

- [ ] **Step 6: Jalankan semua tests, pastikan PASS**

```bash
cargo test
```

Expected: semua test pass termasuk empat test baru.

- [ ] **Step 7: Commit**

```bash
git add src/adapters/driving/cli.rs
git commit -m "feat(cli): add --env flag to add/edit and ENV column to list"
```

---

### Task 4: REPL prompt dengan environment

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Tulis failing tests di `repl/mod.rs`**

Tambahkan test berikut di dalam `mod tests` di `src/adapters/driving/repl/mod.rs`:

```rust
#[test]
fn prompt_left_with_environment_shows_env() {
    let prompt = PgrsPrompt {
        db_name: "mydb".to_string(),
        environment: Some("production".to_string()),
    };
    let left = prompt.render_prompt_left();
    assert_eq!(left.as_ref(), "pgrs(mydb:production)");
}

#[test]
fn prompt_left_without_environment_omits_env() {
    let prompt = PgrsPrompt {
        db_name: "mydb".to_string(),
        environment: None,
    };
    let left = prompt.render_prompt_left();
    assert_eq!(left.as_ref(), "pgrs(mydb)");
}
```

Juga update test lama yang akan patah karena format berubah. Cari test `prompt_left_format_is_pgrs_parens_name` dan ubah expected value-nya:

```rust
#[test]
fn prompt_left_format_is_pgrs_parens_name() {
    let prompt = PgrsPrompt {
        db_name: "production".to_string(),
        environment: None,
    };
    let left = prompt.render_prompt_left();
    assert_eq!(left.as_ref(), "pgrs(production)");
}
```

Dan update `prompt_left_includes_database_name`:

```rust
#[test]
fn prompt_left_includes_database_name() {
    let prompt = PgrsPrompt {
        db_name: "mydb".to_string(),
        environment: None,
    };
    let left = prompt.render_prompt_left();
    assert!(
        left.contains("mydb"),
        "prompt should include db name, got: {left}"
    );
}
```

- [ ] **Step 2: Jalankan tests, pastikan GAGAL**

```bash
cargo test prompt_left_with_environment_shows_env
```

Expected: compile error karena `PgrsPrompt` belum punya field `environment`.

- [ ] **Step 3: Update `PgrsPrompt` struct**

Di `src/adapters/driving/repl/mod.rs`, ubah struct `PgrsPrompt`:

```rust
struct PgrsPrompt {
    db_name: String,
    environment: Option<String>,
}
```

- [ ] **Step 4: Update `render_prompt_left`**

```rust
fn render_prompt_left(&self) -> Cow<'_, str> {
    match &self.environment {
        Some(env) => Cow::Owned(format!("pgrs({}:{})", self.db_name, env)),
        None => Cow::Owned(format!("pgrs({})", self.db_name)),
    }
}
```

- [ ] **Step 5: Update signature `repl::run`**

Ubah signature fungsi `run` dari:

```rust
pub fn run(conn: Box<dyn ReplPort>, db_name: &str) -> Result<(), String> {
```

menjadi:

```rust
pub fn run(conn: Box<dyn ReplPort>, db_name: &str, environment: Option<&str>) -> Result<(), String> {
```

Dan ubah konstruksi `PgrsPrompt` di dalam fungsi `run`:

```rust
let prompt = PgrsPrompt {
    db_name: db_name.to_string(),
    environment: environment.map(|s| s.to_string()),
};
```

- [ ] **Step 6: Update `app.rs` untuk meneruskan environment**

Di `src/app.rs`, dalam fungsi `run_shell`, ubah baris pemanggilan `repl::run`:

```rust
repl::run(Box::new(db), &conn.database, conn.environment.as_deref())
```

- [ ] **Step 7: Jalankan semua tests, pastikan PASS**

```bash
cargo test
```

Expected: semua test pass, tidak ada compile error.

- [ ] **Step 8: Jalankan clippy**

```bash
cargo clippy
```

Expected: tidak ada warning baru.

- [ ] **Step 9: Commit**

```bash
git add src/adapters/driving/repl/mod.rs src/app.rs
git commit -m "feat(repl): show environment in REPL prompt"
```
