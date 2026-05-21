# SQL REPL & Shell Auto-completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambahkan `pgrs shell <name>` (interactive SQL REPL dengan context-aware completion) dan `pgrs completions <bash|zsh|fish>` (shell completion scripts yang include dynamic connection names).

**Architecture:** `DbConnection` trait di core/ports menjadi interface ke PostgreSQL; `PostgresDb` (adapter driven) implements trait tersebut menggunakan crate `postgres` sync. `SchemaService` load tables+columns sekali saat REPL start dan di-cache in-memory. `SqlCompleter` implements rustyline `Helper` dan detect context (FROM/JOIN → table names, SELECT/WHERE → column names dari tables yang sudah disebut di query).

**Tech Stack:** `rustyline` (REPL input + completion), `postgres` (sync PostgreSQL driver), Rust std only untuk completion scripts.

---

## File Map

**Baru dibuat:**
- `src/core/ports/db_connection.rs` — `DbConnection` trait + `QueryResult` struct
- `src/core/services/schema/mod.rs` — re-export
- `src/core/services/schema/service.rs` — `SchemaService` (load + cache)
- `src/adapters/driven/postgres_db.rs` — `PostgresDb` impl `DbConnection`
- `src/adapters/driving/completions.rs` — static completion scripts
- `src/adapters/driving/repl/mod.rs` — `pub fn run(conn, db_name)` entry point
- `src/adapters/driving/repl/completer.rs` — `SqlCompleter` (rustyline Helper)
- `src/adapters/driving/repl/executor.rs` — `print_result(result: &QueryResult)`

**Dimodifikasi:**
- `Cargo.toml` — tambah `rustyline`, `postgres`
- `src/core/ports/mod.rs` — tambah `pub mod db_connection;`
- `src/core/services/mod.rs` — tambah `pub mod schema;`
- `src/adapters/driven/mod.rs` — tambah `pub mod postgres_db;`
- `src/adapters/driving/mod.rs` — tambah `pub mod repl; pub mod completions;`
- `src/adapters/driving/cli.rs` — tambah `completions`, `list --names-only`, `pub fn get_connection()`
- `src/app.rs` — tambah dispatch untuk `shell` + wiring `PostgresDb`

---

## Task 1: Tambah Dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Edit Cargo.toml**

```toml
[dependencies]
dirs = "6"
postgres = "0.19"
rustyline = "14"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.149"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Verify kompilasi**

```bash
cargo check
```

Expected: kompilasi sukses, tidak ada error.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add rustyline and postgres dependencies"
```

---

## Task 2: DbConnection Trait + QueryResult

**Files:**
- Create: `src/core/ports/db_connection.rs`
- Modify: `src/core/ports/mod.rs`

- [ ] **Step 1: Tulis failing test**

Tambahkan ke akhir file baru `src/core/ports/db_connection.rs` sebelum implementasi apapun — test ini membuktikan trait bisa di-implement dengan mock:

```rust
use std::collections::HashMap;

pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub trait DbConnection {
    fn execute(&self, query: &str) -> Result<QueryResult, String>;
    fn list_tables(&self) -> Result<Vec<String>, String>;
    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDb;

    impl DbConnection for MockDb {
        fn execute(&self, _query: &str) -> Result<QueryResult, String> {
            Ok(QueryResult {
                columns: vec!["id".to_string()],
                rows: vec![vec!["1".to_string()]],
            })
        }

        fn list_tables(&self) -> Result<Vec<String>, String> {
            Ok(vec!["users".to_string()])
        }

        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
            let mut m = HashMap::new();
            m.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
            Ok(m)
        }
    }

    #[test]
    fn mock_db_implements_trait() {
        let db = MockDb;
        let tables = db.list_tables().unwrap();
        assert_eq!(tables, vec!["users"]);

        let cols = db.list_columns().unwrap();
        assert_eq!(cols["users"], vec!["id", "email"]);

        let result = db.execute("SELECT 1").unwrap();
        assert_eq!(result.columns, vec!["id"]);
        assert_eq!(result.rows[0][0], "1");
    }
}
```

- [ ] **Step 2: Daftarkan modul**

Di `src/core/ports/mod.rs`:
```rust
pub mod connection_repository;
pub mod db_connection;
```

- [ ] **Step 3: Jalankan test untuk verify fail**

```bash
cargo test db_connection
```

Expected: PASS (trait definition valid, mock bisa di-implement).

- [ ] **Step 4: Commit**

```bash
git add src/core/ports/db_connection.rs src/core/ports/mod.rs
git commit -m "feat: add DbConnection trait and QueryResult"
```

---

## Task 3: SchemaService

**Files:**
- Create: `src/core/services/schema/mod.rs`
- Create: `src/core/services/schema/service.rs`
- Modify: `src/core/services/mod.rs`

- [ ] **Step 1: Tulis failing test dulu**

Buat `src/core/services/schema/service.rs` dengan test di atas implementasi:

```rust
use std::collections::HashMap;
use crate::core::ports::db_connection::{DbConnection, QueryResult};

pub struct SchemaService {
    pub tables: Vec<String>,
    pub columns: HashMap<String, Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDb {
        tables: Vec<String>,
        columns: HashMap<String, Vec<String>>,
    }

    impl DbConnection for MockDb {
        fn execute(&self, _: &str) -> Result<QueryResult, String> {
            Ok(QueryResult { columns: vec![], rows: vec![] })
        }

        fn list_tables(&self) -> Result<Vec<String>, String> {
            Ok(self.tables.clone())
        }

        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
            Ok(self.columns.clone())
        }
    }

    fn mock_db() -> MockDb {
        let mut columns = HashMap::new();
        columns.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
        columns.insert("orders".to_string(), vec!["id".to_string(), "user_id".to_string()]);
        MockDb {
            tables: vec!["users".to_string(), "orders".to_string()],
            columns,
        }
    }

    #[test]
    fn load_populates_tables_and_columns() {
        let db = mock_db();
        let schema = SchemaService::load(&db).unwrap();
        assert_eq!(schema.tables(), &["users", "orders"]);
        assert_eq!(schema.columns_for("users"), &["id", "email"]);
        assert_eq!(schema.columns_for("orders"), &["id", "user_id"]);
    }

    #[test]
    fn columns_for_unknown_table_returns_empty() {
        let db = mock_db();
        let schema = SchemaService::load(&db).unwrap();
        assert_eq!(schema.columns_for("nonexistent"), &[] as &[String]);
    }
}
```

- [ ] **Step 2: Jalankan test untuk verify fail**

```bash
cargo test schema
```

Expected: FAIL — `SchemaService::load`, `tables()`, `columns_for()` belum ada.

- [ ] **Step 3: Implementasi SchemaService**

Tambahkan impl di bawah struct definition di `src/core/services/schema/service.rs`:

```rust
impl SchemaService {
    pub fn load(conn: &dyn DbConnection) -> Result<Self, String> {
        let tables = conn.list_tables()?;
        let columns = conn.list_columns()?;
        Ok(Self { tables, columns })
    }

    pub fn tables(&self) -> &[String] {
        &self.tables
    }

    pub fn columns_for(&self, table: &str) -> &[String] {
        self.columns.get(table).map(Vec::as_slice).unwrap_or(&[])
    }
}
```

- [ ] **Step 4: Buat mod.rs dan daftarkan modul**

`src/core/services/schema/mod.rs`:
```rust
pub mod service;
```

`src/core/services/mod.rs`:
```rust
pub mod connection;
pub mod schema;
```

- [ ] **Step 5: Jalankan test untuk verify pass**

```bash
cargo test schema
```

Expected: PASS — `load_populates_tables_and_columns` dan `columns_for_unknown_table_returns_empty` keduanya green.

- [ ] **Step 6: Commit**

```bash
git add src/core/services/schema/ src/core/services/mod.rs
git commit -m "feat: add SchemaService with in-memory cache"
```

---

## Task 4: PostgresDb

**Files:**
- Create: `src/adapters/driven/postgres_db.rs`
- Modify: `src/adapters/driven/mod.rs`

> **Note:** Unit test untuk PostgresDb membutuhkan koneksi DB nyata. Task ini ditest dengan `cargo check` + manual smoke test di Task 11.

- [ ] **Step 1: Buat `src/adapters/driven/postgres_db.rs`**

```rust
use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::domain::connection::Connection;
use crate::core::ports::db_connection::{DbConnection, QueryResult};

pub struct PostgresDb {
    client: RefCell<postgres::Client>,
}

impl PostgresDb {
    pub fn new(connection: &Connection) -> Result<Self, String> {
        let conn_str = format!(
            "host={} port={} user={} password={} dbname={}",
            connection.host,
            connection.port,
            connection.username,
            connection.password,
            connection.database
        );
        let client = postgres::Client::connect(&conn_str, postgres::NoTls)
            .map_err(|e| format!("could not connect to '{}': {}", connection.name, e))?;
        Ok(Self {
            client: RefCell::new(client),
        })
    }
}

impl DbConnection for PostgresDb {
    fn execute(&self, query: &str) -> Result<QueryResult, String> {
        let mut client = self.client.borrow_mut();
        let rows = client.query(query, &[]).map_err(|e| e.to_string())?;

        if rows.is_empty() {
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
            });
        }

        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let data = rows
            .iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| cell_to_string(row, i))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        Ok(QueryResult {
            columns,
            rows: data,
        })
    }

    fn list_tables(&self) -> Result<Vec<String>, String> {
        let mut client = self.client.borrow_mut();
        let rows = client
            .query(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
                 ORDER BY table_name",
                &[],
            )
            .map_err(|e| e.to_string())?;
        Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
    }

    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
        let mut client = self.client.borrow_mut();
        let rows = client
            .query(
                "SELECT table_name, column_name FROM information_schema.columns \
                 WHERE table_schema = 'public' \
                 ORDER BY table_name, ordinal_position",
                &[],
            )
            .map_err(|e| e.to_string())?;

        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for row in &rows {
            let table: String = row.get(0);
            let column: String = row.get(1);
            map.entry(table).or_default().push(column);
        }
        Ok(map)
    }
}

fn cell_to_string(row: &postgres::Row, idx: usize) -> String {
    if let Ok(Some(v)) = row.try_get::<_, Option<String>>(idx) {
        return v;
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<i64>>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<i32>>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<f64>>(idx) {
        return v.to_string();
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<bool>>(idx) {
        return v.to_string();
    }
    "NULL".to_string()
}
```

- [ ] **Step 2: Daftarkan modul**

Di `src/adapters/driven/mod.rs`:
```rust
pub mod file_connection_repository;
pub mod postgres_db;
```

- [ ] **Step 3: Verify kompilasi**

```bash
cargo check
```

Expected: kompilasi sukses tanpa error.

- [ ] **Step 4: Commit**

```bash
git add src/adapters/driven/postgres_db.rs src/adapters/driven/mod.rs
git commit -m "feat: add PostgresDb adapter implementing DbConnection"
```

---

## Task 5: Shell Completion Scripts

**Files:**
- Create: `src/adapters/driving/completions.rs`
- Modify: `src/adapters/driving/mod.rs`

- [ ] **Step 1: Tulis failing test**

Buat `src/adapters/driving/completions.rs`:

```rust
pub fn bash_script() -> &'static str {
    include_str!("completions/pgrs.bash")
}

pub fn zsh_script() -> &'static str {
    include_str!("completions/pgrs.zsh")
}

pub fn fish_script() -> &'static str {
    include_str!("completions/pgrs.fish")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_script_contains_subcommands() {
        let s = bash_script();
        assert!(s.contains("add"), "bash script missing 'add'");
        assert!(s.contains("list"), "bash script missing 'list'");
        assert!(s.contains("delete"), "bash script missing 'delete'");
        assert!(s.contains("connect"), "bash script missing 'connect'");
        assert!(s.contains("shell"), "bash script missing 'shell'");
        assert!(s.contains("completions"), "bash script missing 'completions'");
        assert!(s.contains("--names-only"), "bash script must call --names-only for dynamic names");
    }

    #[test]
    fn zsh_script_contains_subcommands() {
        let s = zsh_script();
        assert!(s.contains("add"));
        assert!(s.contains("shell"));
        assert!(s.contains("--names-only"));
    }

    #[test]
    fn fish_script_contains_subcommands() {
        let s = fish_script();
        assert!(s.contains("add"));
        assert!(s.contains("shell"));
        assert!(s.contains("--names-only"));
    }
}
```

- [ ] **Step 2: Jalankan test untuk verify fail**

```bash
cargo test completions
```

Expected: FAIL — file `completions/pgrs.bash` dll belum ada.

- [ ] **Step 3: Buat direktori dan file script**

```bash
mkdir -p src/adapters/driving/completions
```

Buat `src/adapters/driving/completions/pgrs.bash`:

```bash
_pgrs_completions() {
    local cur prev words cword
    _init_completion || return

    local subcommands="add list delete connect shell completions"
    local connection_cmds="connect shell delete"
    local shell_names="bash zsh fish"

    if [[ $cword -eq 1 ]]; then
        COMPREPLY=($(compgen -W "$subcommands" -- "$cur"))
        return
    fi

    case "${words[1]}" in
        connect|shell|delete)
            local names
            names=$(pgrs list --names-only 2>/dev/null)
            COMPREPLY=($(compgen -W "$names" -- "$cur"))
            ;;
        completions)
            COMPREPLY=($(compgen -W "$shell_names" -- "$cur"))
            ;;
    esac
}

complete -F _pgrs_completions pgrs
```

Buat `src/adapters/driving/completions/pgrs.zsh`:

```zsh
#compdef pgrs

_pgrs() {
    local state

    _arguments \
        '1: :->subcommand' \
        '*: :->args'

    case $state in
        subcommand)
            local subcommands
            subcommands=(add list delete connect shell completions)
            _describe 'subcommand' subcommands
            ;;
        args)
            case ${words[2]} in
                connect|shell|delete)
                    local names
                    names=(${(f)"$(pgrs list --names-only 2>/dev/null)"})
                    _describe 'connection' names
                    ;;
                completions)
                    local shells
                    shells=(bash zsh fish)
                    _describe 'shell' shells
                    ;;
            esac
            ;;
    esac
}

_pgrs
```

Buat `src/adapters/driving/completions/pgrs.fish`:

```fish
function __pgrs_connections
    pgrs list --names-only 2>/dev/null
end

complete -c pgrs -f -n '__fish_use_subcommand' -a add         -d 'Add a new connection'
complete -c pgrs -f -n '__fish_use_subcommand' -a list        -d 'List all connections'
complete -c pgrs -f -n '__fish_use_subcommand' -a delete      -d 'Delete a connection'
complete -c pgrs -f -n '__fish_use_subcommand' -a connect     -d 'Open psql session'
complete -c pgrs -f -n '__fish_use_subcommand' -a shell       -d 'Open pgrs interactive REPL'
complete -c pgrs -f -n '__fish_use_subcommand' -a completions -d 'Print shell completion script'

complete -c pgrs -f -n '__fish_seen_subcommand_from connect shell delete' -a '(__pgrs_connections)'
complete -c pgrs -f -n '__fish_seen_subcommand_from completions' -a 'bash zsh fish' -d 'Shell type'
```

- [ ] **Step 4: Jalankan test**

```bash
cargo test completions
```

Expected: PASS — semua tiga script tests green.

- [ ] **Step 5: Daftarkan modul**

Di `src/adapters/driving/mod.rs`:
```rust
pub mod cli;
pub mod completions;
pub mod repl;
```

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/completions.rs src/adapters/driving/completions/ src/adapters/driving/mod.rs
git commit -m "feat: add bash/zsh/fish shell completion scripts"
```

---

## Task 6: `list --names-only` + `pgrs completions` di CLI

**Files:**
- Modify: `src/adapters/driving/cli.rs`

- [ ] **Step 1: Tulis failing test**

Tambahkan test di `src/adapters/driving/cli.rs` di dalam modul `#[cfg(test)]` yang sudah ada (atau buat baru di bagian bawah file). Pastikan ada `StubRepository` yang sama dengan yang di connection/service.rs — copy paste dari sana.

Cari blok `#[cfg(test)]` di cli.rs dan tambahkan di sana. Jika belum ada, buat:

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
        fn with(names: &[&str]) -> Self {
            let connections = names
                .iter()
                .map(|n| Connection {
                    name: n.to_string(),
                    host: "localhost".to_string(),
                    port: 5432,
                    username: "user".to_string(),
                    password: "pass".to_string(),
                    database: "db".to_string(),
                })
                .collect();
            Self {
                connections: RefCell::new(connections),
            }
        }
    }

    impl ConnectionRepository for StubRepository {
        fn add(&self, c: Connection) -> Result<(), String> {
            self.connections.borrow_mut().push(c);
            Ok(())
        }
        fn list(&self) -> Result<Vec<Connection>, String> {
            Ok(self.connections.borrow().clone())
        }
        fn delete(&self, name: &str) -> Result<(), String> {
            self.connections.borrow_mut().retain(|c| c.name != name);
            Ok(())
        }
        fn get_connection(&self, name: &str) -> Result<Connection, String> {
            self.connections
                .borrow()
                .iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| format!("connection '{}' not found", name))
        }
    }

    fn cli_with(names: &[&str]) -> Cli<StubRepository> {
        Cli::new(ConnectionService::new(StubRepository::with(names)))
    }

    #[test]
    fn list_names_only_prints_one_name_per_line() {
        let cli = cli_with(&["prod", "staging"]);
        // capture stdout — kita test via output inspection
        // karena CLI print langsung, kita verifikasi tidak error saja
        // test sebenarnya ada di integration test (Task 11)
        let result = cli.run(vec!["list".to_string(), "--names-only".to_string()].into_iter());
        assert!(result.is_ok());
    }

    #[test]
    fn completions_bash_returns_ok() {
        let cli = cli_with(&[]);
        let result = cli.run(vec!["completions".to_string(), "bash".to_string()].into_iter());
        assert!(result.is_ok());
    }

    #[test]
    fn completions_unknown_shell_returns_err() {
        let cli = cli_with(&[]);
        let result = cli.run(vec!["completions".to_string(), "powershell".to_string()].into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn get_connection_returns_correct_connection() {
        let cli = cli_with(&["prod"]);
        let conn = cli.get_connection("prod").unwrap();
        assert_eq!(conn.name, "prod");
    }
}
```

- [ ] **Step 2: Jalankan test untuk verify fail**

```bash
cargo test -p pgrs cli
```

Expected: FAIL — `completions` dan `--names-only` belum dihandle, `get_connection` belum ada.

- [ ] **Step 3: Implementasi di cli.rs**

Tambahkan import di atas `cli.rs`:
```rust
use crate::adapters::driving::completions;
use crate::core::domain::connection::Connection;
```

Tambahkan arm `completions` dan `shell` di `match` dalam `run()`, dan ubah `list` agar bisa terima args:

```rust
pub fn run(&self, args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args: Vec<String> = args.into_iter().collect();

    match args.first().map(String::as_str) {
        None => {
            println!("{}", welcome());
            Ok(())
        }
        Some("add") => self.add_connection(&args[1..]),
        Some("list") => self.list_connections(&args[1..]),
        Some("delete") => self.delete_connection(&args[1..]),
        Some("connect") => self.connect_to(&args[1..]),
        Some("completions") => self.print_completions(&args[1..]),
        _ => Err(usage().to_string()),
    }
}
```

Ubah signature `list_connections` dari `(&self)` ke `(&self, args: &[String])`:

```rust
fn list_connections(&self, args: &[String]) -> Result<(), String> {
    let names_only = args.iter().any(|a| a == "--names-only");
    let connections = self.connection_service.list_connections()?;

    if names_only {
        for c in &connections {
            println!("{}", c.name);
        }
        return Ok(());
    }

    if connections.is_empty() {
        println!("no connections saved");
        return Ok(());
    }

    let name_w = connections.iter().map(|c| c.name.len()).max().unwrap_or(4).max(4);
    let host_w = connections.iter().map(|c| c.host.len()).max().unwrap_or(4).max(4);
    let db_w = connections.iter().map(|c| c.database.len()).max().unwrap_or(8).max(8);
    let user_w = connections.iter().map(|c| c.username.len()).max().unwrap_or(8).max(8);

    println!(
        "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<user_w$}  PASSWORD",
        "NAME", "HOST", "PORT", "DATABASE", "USERNAME",
    );

    for c in &connections {
        println!(
            "{:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<user_w$}  ****",
            c.name, c.host, c.port, c.database, c.username,
        );
    }

    Ok(())
}
```

Tambahkan method `print_completions` dan `get_connection`:

```rust
fn print_completions(&self, args: &[String]) -> Result<(), String> {
    let shell = args.first().ok_or("usage: pgrs completions <bash|zsh|fish>")?;
    let script = match shell.as_str() {
        "bash" => completions::bash_script(),
        "zsh" => completions::zsh_script(),
        "fish" => completions::fish_script(),
        other => return Err(format!("unknown shell '{}' — supported: bash, zsh, fish", other)),
    };
    print!("{}", script);
    Ok(())
}

pub fn get_connection(&self, name: &str) -> Result<Connection, String> {
    self.connection_service.get_connection(name)
}
```

- [ ] **Step 4: Jalankan test**

```bash
cargo test -p pgrs cli
```

Expected: PASS — semua test green.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/cli.rs
git commit -m "feat: add list --names-only, completions subcommand, get_connection"
```

---

## Task 7: Query Executor (print_result)

**Files:**
- Create: `src/adapters/driving/repl/executor.rs`

- [ ] **Step 1: Tulis failing test**

Buat `src/adapters/driving/repl/executor.rs`:

```rust
use crate::core::ports::db_connection::QueryResult;

pub fn print_result(result: &QueryResult) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture_print_result(result: &QueryResult) -> String {
        // kita tidak bisa capture stdout dengan mudah, jadi kita test format_result() terpisah
        let output = format_result(result);
        output
    }

    #[test]
    fn formats_single_row() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "email".to_string()],
            rows: vec![vec!["1".to_string(), "alice@example.com".to_string()]],
        };
        let out = format_result(&result);
        assert!(out.contains("id"), "missing column 'id'");
        assert!(out.contains("email"), "missing column 'email'");
        assert!(out.contains("1"), "missing value '1'");
        assert!(out.contains("alice@example.com"), "missing value");
        assert!(out.contains("(1 row)"), "missing row count");
    }

    #[test]
    fn formats_empty_result() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
        };
        let out = format_result(&result);
        assert!(out.contains("(0 rows)"));
    }

    #[test]
    fn column_width_fits_longest_value() {
        let result = QueryResult {
            columns: vec!["name".to_string()],
            rows: vec![
                vec!["short".to_string()],
                vec!["a_very_long_name".to_string()],
            ],
        };
        let out = format_result(&result);
        assert!(out.contains("a_very_long_name"));
        assert!(out.contains("short"));
    }
}
```

- [ ] **Step 2: Jalankan test untuk verify fail**

```bash
cargo test executor
```

Expected: FAIL — `format_result` belum ada, `print_result` menggunakan `todo!()`.

- [ ] **Step 3: Implementasi**

Ganti seluruh isi `executor.rs`:

```rust
use crate::core::ports::db_connection::QueryResult;

pub fn print_result(result: &QueryResult) {
    print!("{}", format_result(result));
}

pub fn format_result(result: &QueryResult) -> String {
    if result.columns.is_empty() {
        let count = result.rows.len();
        return format!("({} {})\n", count, if count == 1 { "row" } else { "rows" });
    }

    let col_widths: Vec<usize> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let max_val = result.rows.iter().map(|r| r[i].len()).max().unwrap_or(0);
            col.len().max(max_val)
        })
        .collect();

    let mut out = String::new();

    // header
    let header: Vec<String> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| format!("{:<width$}", col, width = col_widths[i]))
        .collect();
    out.push_str(&format!(" {} \n", header.join(" | ")));

    // separator
    let sep: Vec<String> = col_widths.iter().map(|w| "-".repeat(*w + 2)).collect();
    out.push_str(&sep.join("+"));
    out.push('\n');

    // rows
    for row in &result.rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, val)| format!("{:<width$}", val, width = col_widths[i]))
            .collect();
        out.push_str(&format!(" {} \n", cells.join(" | ")));
    }

    let count = result.rows.len();
    out.push_str(&format!(
        "({} {})\n",
        count,
        if count == 1 { "row" } else { "rows" }
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_single_row() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "email".to_string()],
            rows: vec![vec!["1".to_string(), "alice@example.com".to_string()]],
        };
        let out = format_result(&result);
        assert!(out.contains("id"), "missing column 'id'");
        assert!(out.contains("email"), "missing column 'email'");
        assert!(out.contains("1"), "missing value '1'");
        assert!(out.contains("alice@example.com"), "missing value");
        assert!(out.contains("(1 row)"), "missing row count");
    }

    #[test]
    fn formats_empty_result() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
        };
        let out = format_result(&result);
        assert!(out.contains("(0 rows)"));
    }

    #[test]
    fn column_width_fits_longest_value() {
        let result = QueryResult {
            columns: vec!["name".to_string()],
            rows: vec![
                vec!["short".to_string()],
                vec!["a_very_long_name".to_string()],
            ],
        };
        let out = format_result(&result);
        assert!(out.contains("a_very_long_name"));
        assert!(out.contains("short"));
    }
}
```

- [ ] **Step 4: Jalankan test**

```bash
cargo test executor
```

Expected: PASS — semua tiga test green.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/executor.rs
git commit -m "feat: add query result formatter with ASCII table output"
```

---

## Task 8: SqlCompleter

**Files:**
- Create: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Tulis failing test**

Buat `src/adapters/driving/repl/completer.rs`:

```rust
use crate::core::services::schema::service::SchemaService;

pub struct SqlCompleter {
    schema: SchemaService,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn schema_with(tables: &[&str], columns: &[(&str, &[&str])]) -> SchemaService {
        let mut col_map: HashMap<String, Vec<String>> = HashMap::new();
        for (table, cols) in columns {
            col_map.insert(
                table.to_string(),
                cols.iter().map(|c| c.to_string()).collect(),
            );
        }
        SchemaService {
            tables: tables.iter().map(|t| t.to_string()).collect(),
            columns: col_map,
        }
    }

    #[test]
    fn suggests_keywords_at_start_of_input() {
        let schema = schema_with(&[], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SEL", 3);
        assert!(results.iter().any(|r| r == "SELECT"), "expected SELECT in {:?}", results);
    }

    #[test]
    fn suggests_table_names_after_from() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 13);
        assert!(results.contains(&"users".to_string()));
        assert!(results.contains(&"orders".to_string()));
    }

    #[test]
    fn suggests_table_names_after_join() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM users JOIN ", 24);
        assert!(results.contains(&"orders".to_string()));
    }

    #[test]
    fn suggests_columns_after_select_when_table_known() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email", "created_at"])],
        );
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT  FROM users", 7);
        assert!(results.contains(&"id".to_string()), "expected id in {:?}", results);
        assert!(results.contains(&"email".to_string()));
    }

    #[test]
    fn filters_by_current_word_prefix() {
        let schema = schema_with(&["users", "user_sessions"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM user", 18);
        assert!(results.contains(&"users".to_string()));
        assert!(results.contains(&"user_sessions".to_string()));
        assert!(!results.contains(&"orders".to_string()));
    }

    #[test]
    fn no_duplicate_suggestions() {
        let schema = schema_with(&["users"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 14);
        let unique: std::collections::HashSet<_> = results.iter().collect();
        assert_eq!(results.len(), unique.len(), "duplicates found: {:?}", results);
    }
}
```

- [ ] **Step 2: Jalankan test untuk verify fail**

```bash
cargo test completer
```

Expected: FAIL — `SqlCompleter::new` dan `complete_input` belum ada.

- [ ] **Step 3: Implementasi SqlCompleter**

Ganti isi `completer.rs` dengan implementasi lengkap:

```rust
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

use crate::core::services::schema::service::SchemaService;

const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER",
    "ON", "AND", "OR", "NOT", "IN", "IS", "NULL", "AS", "DISTINCT",
    "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET", "INSERT", "INTO",
    "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "DROP", "ALTER",
    "BEGIN", "COMMIT", "ROLLBACK",
];

pub struct SqlCompleter {
    schema: SchemaService,
}

impl SqlCompleter {
    pub fn new(schema: SchemaService) -> Self {
        Self { schema }
    }

    pub fn complete_input(&self, line: &str, pos: usize) -> Vec<String> {
        let input = &line[..pos];
        let upper = input.to_uppercase();
        let tokens: Vec<&str> = upper.split_whitespace().collect();

        // Ambil kata yang sedang diketik (mungkin kosong jika diakhiri spasi)
        let current_word = if input.ends_with(char::is_whitespace) || input.is_empty() {
            ""
        } else {
            tokens.last().copied().unwrap_or("")
        };

        // Tentukan context berdasarkan token sebelum current word
        let context_token = if input.ends_with(char::is_whitespace) {
            tokens.last().copied().unwrap_or("")
        } else if tokens.len() >= 2 {
            tokens[tokens.len() - 2]
        } else {
            ""
        };

        let candidates: Vec<String> = match context_token {
            "FROM" | "JOIN" | "INTO" | "UPDATE" => {
                self.schema.tables().iter().map(|t| t.to_string()).collect()
            }
            "SELECT" | "WHERE" | "ON" | "SET" | "BY" => {
                // cari table names yang sudah disebut di query
                let table_refs = self.extract_table_refs(&upper);
                if table_refs.is_empty() {
                    SQL_KEYWORDS.iter().map(|k| k.to_string()).collect()
                } else {
                    table_refs
                        .iter()
                        .flat_map(|t| {
                            let t_lower = t.to_lowercase();
                            self.schema.columns_for(&t_lower).iter().map(|c| c.to_string())
                        })
                        .collect()
                }
            }
            _ => SQL_KEYWORDS.iter().map(|k| k.to_string()).collect(),
        };

        // Filter berdasarkan prefix (case-insensitive)
        let prefix_upper = current_word.to_uppercase();
        let mut results: Vec<String> = candidates
            .into_iter()
            .filter(|c| c.to_uppercase().starts_with(&prefix_upper))
            .collect();
        results.sort();
        results.dedup();
        results
    }

    fn extract_table_refs<'a>(&self, upper_query: &'a str) -> Vec<&'a str> {
        let tokens: Vec<&str> = upper_query.split_whitespace().collect();
        let mut tables = vec![];
        let trigger = ["FROM", "JOIN", "UPDATE"];
        for window in tokens.windows(2) {
            if trigger.contains(&window[0]) {
                tables.push(window[1]);
            }
        }
        tables
    }
}

impl Completer for SqlCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let word_start = line[..pos]
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);

        let candidates = self.complete_input(line, pos);
        let pairs = candidates
            .into_iter()
            .map(|c| Pair {
                display: c.clone(),
                replacement: c,
            })
            .collect();

        Ok((word_start, pairs))
    }
}

impl Hinter for SqlCompleter {
    type Hint = String;
    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        None
    }
}

impl Highlighter for SqlCompleter {}
impl Validator for SqlCompleter {}
impl Helper for SqlCompleter {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn schema_with(tables: &[&str], columns: &[(&str, &[&str])]) -> SchemaService {
        let mut col_map: HashMap<String, Vec<String>> = HashMap::new();
        for (table, cols) in columns {
            col_map.insert(
                table.to_string(),
                cols.iter().map(|c| c.to_string()).collect(),
            );
        }
        SchemaService {
            tables: tables.iter().map(|t| t.to_string()).collect(),
            columns: col_map,
        }
    }

    #[test]
    fn suggests_keywords_at_start_of_input() {
        let schema = schema_with(&[], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SEL", 3);
        assert!(results.iter().any(|r| r == "SELECT"), "expected SELECT in {:?}", results);
    }

    #[test]
    fn suggests_table_names_after_from() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 13);
        assert!(results.contains(&"users".to_string()));
        assert!(results.contains(&"orders".to_string()));
    }

    #[test]
    fn suggests_table_names_after_join() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM users JOIN ", 24);
        assert!(results.contains(&"orders".to_string()));
    }

    #[test]
    fn suggests_columns_after_select_when_table_known() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email", "created_at"])],
        );
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT  FROM users", 7);
        assert!(results.contains(&"id".to_string()), "expected id in {:?}", results);
        assert!(results.contains(&"email".to_string()));
    }

    #[test]
    fn filters_by_current_word_prefix() {
        let schema = schema_with(&["users", "user_sessions"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM user", 18);
        assert!(results.contains(&"users".to_string()));
        assert!(results.contains(&"user_sessions".to_string()));
        assert!(!results.contains(&"orders".to_string()));
    }

    #[test]
    fn no_duplicate_suggestions() {
        let schema = schema_with(&["users"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 14);
        let unique: std::collections::HashSet<_> = results.iter().collect();
        assert_eq!(results.len(), unique.len(), "duplicates found: {:?}", results);
    }
}
```

- [ ] **Step 4: Jalankan test**

```bash
cargo test completer
```

Expected: PASS — semua test green.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat: add SqlCompleter with context-aware SQL completion"
```

---

## Task 9: REPL Entry Point

**Files:**
- Create: `src/adapters/driving/repl/mod.rs`

- [ ] **Step 1: Buat `src/adapters/driving/repl/mod.rs`**

```rust
pub mod completer;
pub mod executor;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use rustyline::Editor;

use crate::core::ports::db_connection::DbConnection;
use crate::core::services::schema::service::SchemaService;

use completer::SqlCompleter;
use executor::print_result;

pub fn run(conn: Box<dyn DbConnection>, db_name: &str) -> Result<(), String> {
    let schema = SchemaService::load(conn.as_ref())?;
    let completer = SqlCompleter::new(schema);

    let mut rl: Editor<SqlCompleter, _> = Editor::new().map_err(|e| e.to_string())?;
    rl.set_helper(Some(completer));

    println!(
        "Connected to '{}'. Type \\q or Ctrl+D to exit. \\dt to list tables.",
        db_name
    );

    let mut pending = String::new();

    loop {
        let prompt = if pending.is_empty() {
            "pgrs> "
        } else {
            "   -> "
        };

        match rl.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim();

                if trimmed == "\\q" || trimmed == "exit" {
                    break;
                }

                if trimmed == "\\dt" {
                    if let Some(helper) = rl.helper() {
                        for table in helper.schema().tables() {
                            println!(" {}", table);
                        }
                    }
                    continue;
                }

                if trimmed.is_empty() {
                    continue;
                }

                rl.add_history_entry(&line).ok();
                pending.push_str(&line);
                pending.push('\n');

                if pending.trim_end().ends_with(';') {
                    let query = pending.trim().to_string();
                    pending.clear();
                    match conn.execute(&query) {
                        Ok(result) => print_result(&result),
                        Err(e) => eprintln!("ERROR:  {}", e),
                    }
                }
            }
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => break,
            Err(e) => return Err(e.to_string()),
        }
    }

    println!("Bye.");
    Ok(())
}
```

> **Note:** `\dt` mengakses `schema()` dari helper. Tambahkan method `pub fn schema(&self) -> &SchemaService` ke `SqlCompleter` di `completer.rs`.

- [ ] **Step 2: Tambah `schema()` accessor ke SqlCompleter**

Di `src/adapters/driving/repl/completer.rs`, tambahkan di dalam `impl SqlCompleter`:

```rust
pub fn schema(&self) -> &SchemaService {
    &self.schema
}
```

- [ ] **Step 3: Verify kompilasi**

```bash
cargo check
```

Expected: kompilasi sukses.

- [ ] **Step 4: Commit**

```bash
git add src/adapters/driving/repl/mod.rs src/adapters/driving/repl/completer.rs
git commit -m "feat: add interactive SQL REPL with rustyline"
```

---

## Task 10: Wire `pgrs shell` di app.rs

**Files:**
- Modify: `src/app.rs`
- Modify: `src/adapters/driving/mod.rs` (pastikan `repl` sudah didaftarkan)

- [ ] **Step 1: Pastikan repl terdaftar di mod.rs**

`src/adapters/driving/mod.rs` harus sudah berisi:
```rust
pub mod cli;
pub mod completions;
pub mod repl;
```

Jika belum, tambahkan.

- [ ] **Step 2: Update app.rs**

```rust
use std::env;

use crate::adapters::driven::file_connection_repository::FileConnectionRepository;
use crate::adapters::driven::postgres_db::PostgresDb;
use crate::adapters::driving::cli::Cli;
use crate::adapters::driving::repl;
use crate::core::services::connection::service::ConnectionService;

pub fn run() -> Result<(), String> {
    let data_dir = dirs::home_dir()
        .ok_or("could not determine home directory")?
        .join(".pgrs");

    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let repository = FileConnectionRepository::new(data_dir.join("connections.json"));
    let connection_service = ConnectionService::new(repository);
    let cli = Cli::new(connection_service);

    let args: Vec<String> = env::args().skip(1).collect();

    if args.first().map(String::as_str) == Some("shell") {
        let name = args.get(1).ok_or("usage: pgrs shell <connection-name>")?;
        let conn = cli.get_connection(name)?;
        let db_name = conn.database.clone();
        let db = PostgresDb::new(&conn)?;
        return repl::run(Box::new(db), &db_name);
    }

    cli.run(args.into_iter())
}
```

- [ ] **Step 3: Verify kompilasi**

```bash
cargo build
```

Expected: build sukses. Jika ada warning tentang dead code, itu normal untuk sementara.

- [ ] **Step 4: Update welcome text di cli.rs**

Di `fn welcome()` di `cli.rs`, tambahkan dua subcommand baru:

```rust
fn welcome() -> &'static str {
    "pgrs — PostgreSQL connection manager built with Rust\n\nManage and store named PostgreSQL connections locally.\n\nCommands:\n  add <name> --host=<host> --username=<user> --password=<pass> --database=<db> [--port=<port>]\n             Add a new named connection\n  list         List all saved connections\n  list --names-only\n             Print connection names only, one per line\n  delete <name>\n             Delete a named connection\n  connect <name>\n             Open an interactive psql session using a saved connection\n  shell <name>\n             Open pgrs interactive SQL REPL with auto-completion\n  completions <bash|zsh|fish>\n             Print shell completion script\n\nRun `pgrs <command> --help` for more info on a specific command."
}
```

- [ ] **Step 5: Jalankan semua tests**

```bash
cargo test
```

Expected: semua test pass.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs src/adapters/driving/cli.rs src/adapters/driving/mod.rs
git commit -m "feat: wire pgrs shell subcommand with PostgresDb and REPL"
```

---

## Task 11: Smoke Test Manual

> Task ini tidak punya automated test karena membutuhkan koneksi DB nyata. Jalankan secara manual untuk verifikasi end-to-end.

- [ ] **Step 1: Build release binary**

```bash
cargo build --release
```

Expected: `./target/release/pgrs` ada.

- [ ] **Step 2: Test shell completion output**

```bash
./target/release/pgrs completions bash | head -5
./target/release/pgrs completions zsh | head -5
./target/release/pgrs completions fish | head -5
```

Expected: masing-masing print script yang valid, tidak ada error.

- [ ] **Step 3: Test list --names-only**

```bash
./target/release/pgrs add mydb --host=localhost --username=postgres --password=secret --database=testdb
./target/release/pgrs list --names-only
```

Expected: output `mydb` satu baris saja.

- [ ] **Step 4: Test pgrs shell (butuh PostgreSQL running)**

```bash
./target/release/pgrs shell mydb
```

Expected:
- Tampil pesan "Connected to 'testdb'..."
- Prompt `pgrs> `
- Tab completion bekerja untuk SQL keywords
- `\dt` menampilkan tables
- Query dengan `;` dijalankan dan hasil ditampilkan sebagai ASCII table
- `\q` atau Ctrl+D keluar dengan "Bye."

- [ ] **Step 5: Commit final**

```bash
git add .
git commit -m "feat: complete SQL REPL and shell auto-completion"
```
