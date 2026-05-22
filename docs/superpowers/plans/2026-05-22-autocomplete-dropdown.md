# Autocomplete Dropdown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrasi REPL dari rustyline ke reedline untuk mendapatkan ColumnarMenu dropdown otomatis dengan coloring per jenis entry dan qualified name (table.column) support.

**Architecture:** Swap library rustyline → reedline. `SqlCompleter` implement reedline `Completer` trait (bukan rustyline). `SqlHighlighter` baru implement reedline `Highlighter`. `PgrsPrompt` dan `SqlValidator` baru di `repl/mod.rs` untuk multi-line SQL. Logic `complete_input` dipertahankan, diperluas dengan qualified name detection.

**Tech Stack:** reedline 0.x, nu-ansi-term 0.50, unicode-width 0.2 (tetap), serde/postgres (tetap).

---

## File Map

| File | Perubahan |
|------|-----------|
| `Cargo.toml` | Hapus `rustyline`, tambah `reedline` dan `nu-ansi-term` |
| `src/adapters/driving/repl/mod.rs` | Rewrite: reedline REPL loop, `PgrsPrompt`, `SqlValidator` |
| `src/adapters/driving/repl/completer.rs` | Port: hapus rustyline traits, implement reedline `Completer` + `Highlighter`, tambah qualified name logic |
| `src/core/services/schema/service.rs` | Tambah `#[derive(Clone)]` pada `SchemaService` |

---

## Task 1: Tambah reedline ke Cargo.toml + derive Clone SchemaService

Kita **tambah** reedline dulu sambil biarkan rustyline tetap ada. rustyline dihapus di Task 3 setelah semua kode sudah dimigrasi, sehingga setiap commit tetap compilable.

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/core/services/schema/service.rs`

- [ ] **Step 1: Update Cargo.toml — tambah reedline, nu-ansi-term**

```toml
[dependencies]
dirs = "6"
postgres = "0.19"
rustyline = "14"
reedline = "0"
nu-ansi-term = "0.50"
unicode-width = "0.2"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.149"
```

- [ ] **Step 2: Derive Clone pada SchemaService**

Edit `src/core/services/schema/service.rs`, tambah derive Clone:

```rust
#[derive(Clone)]
pub struct SchemaService {
    pub tables: Vec<String>,
    pub columns: HashMap<String, Vec<String>>,
}
```

- [ ] **Step 3: Verifikasi masih compile**

```bash
cargo check 2>&1
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/core/services/schema/service.rs
git commit -m "chore: add reedline + nu-ansi-term deps, derive Clone on SchemaService"
```

---

## Task 2: Port completer.rs ke reedline (hapus rustyline traits)

Tasks 2 dan 3 harus selesai sebelum masing-masing bisa di-commit karena saling bergantung (`mod.rs` butuh `SqlHighlighter` dari `completer.rs`). Lakukan keduanya dulu, baru commit.

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Hapus semua rustyline imports di completer.rs, ganti dengan reedline**

Ganti bagian `use` paling atas:

```rust
use nu_ansi_term::{Color, Style};
use reedline::{Completer, Highlighter, Span, StyledText, Suggestion};

use crate::core::services::schema::service::SchemaService;
```

- [ ] **Step 2: Tambah method `label()` dan `style()` pada CompletionKind**

Tambahkan impl baru setelah definisi enum `CompletionKind`:

```rust
impl CompletionKind {
    fn label(&self) -> &'static str {
        match self {
            CompletionKind::Keyword => "[keyword]",
            CompletionKind::Table   => "[table]",
            CompletionKind::Column  => "[column]",
        }
    }

    fn style(&self) -> Style {
        match self {
            CompletionKind::Keyword => Style::new().fg(Color::Cyan).bold(),
            CompletionKind::Table   => Style::new().fg(Color::Yellow).bold(),
            CompletionKind::Column  => Style::new().fg(Color::Green),
        }
    }
}
```

- [ ] **Step 3: Tambah helper `word_start`**

Tambahkan free function sebelum `impl SqlCompleter`:

```rust
fn word_start(line: &str, pos: usize) -> usize {
    let input = &line[..pos];
    let last_ws = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
    let word = &input[last_ws..];
    if let Some(dot_pos) = word.rfind('.') {
        last_ws + dot_pos + 1
    } else {
        last_ws
    }
}
```

- [ ] **Step 4: Hapus rustyline trait impls, implement reedline `Completer`**

Hapus blok-blok berikut (versi rustyline):
- `impl Completer for SqlCompleter { ... }`
- `impl Hinter for SqlCompleter { ... }`
- `impl Highlighter for SqlCompleter { ... }`
- `impl Validator for SqlCompleter { ... }`
- `impl Helper for SqlCompleter { ... }`

Ganti dengan reedline `Completer`:

```rust
impl Completer for SqlCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let start = word_start(line, pos);
        self.complete_input(line, pos)
            .into_iter()
            .map(|(value, kind)| Suggestion {
                value,
                description: Some(kind.label().to_string()),
                style: Some(kind.style()),
                span: Span::new(start, pos),
                extra: None,
                append_whitespace: false,
            })
            .collect()
    }
}
```

- [ ] **Step 5: Tambah `SqlHighlighter` struct dan implement reedline `Highlighter`**

Tambahkan sebelum `#[cfg(test)]`:

```rust
pub struct SqlHighlighter {
    tables: Vec<String>,
    columns: Vec<String>,
}

impl SqlHighlighter {
    pub fn new(schema: SchemaService) -> Self {
        let tables = schema.tables().to_vec();
        let columns: Vec<String> = schema
            .tables()
            .iter()
            .flat_map(|t| schema.columns_for(t).iter().cloned())
            .collect();
        Self { tables, columns }
    }
}

impl Highlighter for SqlHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
                let start = i;
                while i < len && chars[i] != '\n' { i += 1; }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().dimmed(), span));
            } else if chars[i] == '\'' {
                let start = i;
                i += 1;
                loop {
                    if i >= len { break; }
                    if chars[i] == '\'' {
                        i += 1;
                        if i < len && chars[i] == '\'' { i += 1; } else { break; }
                    } else { i += 1; }
                }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().fg(Color::Yellow), span));
            } else if chars[i].is_ascii_digit() {
                let start = i;
                let mut has_dot = false;
                while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !has_dot && i + 1 < len && chars[i + 1].is_ascii_digit())) {
                    if chars[i] == '.' { has_dot = true; }
                    i += 1;
                }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().fg(Color::Magenta), span));
            } else if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
                let word: String = chars[start..i].iter().collect();
                let upper = word.to_uppercase();
                let style = if SQL_KEYWORDS.contains(&upper.as_str()) {
                    Style::new().fg(Color::Cyan).bold()
                } else if self.tables.iter().any(|t| t.eq_ignore_ascii_case(&word)) {
                    Style::new().fg(Color::Yellow).bold()
                } else if self.columns.iter().any(|c| c.eq_ignore_ascii_case(&word)) {
                    Style::new().fg(Color::Green)
                } else {
                    Style::new()
                };
                styled.push((style, word));
            } else {
                styled.push((Style::new(), chars[i].to_string()));
                i += 1;
            }
        }

        styled
    }
}
```

> **Jangan commit dulu** — lanjut ke Task 3 dulu agar mod.rs juga selesai, baru commit keduanya bersama.

---

## Task 3: Rewrite repl/mod.rs dengan reedline

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

- [ ] **Step 1: Tulis ulang mod.rs** (lanjutan dari Task 2 — commit keduanya setelah ini selesai)

Ganti seluruh isi `src/adapters/driving/repl/mod.rs`:

```rust
mod completer;
mod executor;

use std::borrow::Cow;

use reedline::{
    ColumnarMenu, Emacs, Prompt, PromptEditMode, PromptHistorySearch,
    PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    ValidationResult, Validator,
};
use reedline::keybindings::{default_emacs_keybindings, KeyCode, KeyModifiers};

use crate::core::ports::db_connection::DbConnection;
use crate::core::services::schema::service::SchemaService;

use completer::{SqlCompleter, SqlHighlighter};
use executor::print_result;

fn is_complete_statement(s: &str) -> bool {
    let s = s.trim_end();
    if !s.ends_with(';') {
        return false;
    }
    let mut in_string = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if in_string {
            if chars[i] == '\'' {
                if i + 1 < chars.len() && chars[i + 1] == '\'' {
                    i += 2;
                } else {
                    in_string = false;
                    i += 1;
                }
            } else {
                i += 1;
            }
        } else {
            if chars[i] == '\'' {
                in_string = true;
            }
            i += 1;
        }
    }
    !in_string
}

struct PgrsPrompt;

impl Prompt for PgrsPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        Cow::Borrowed("pgrs")
    }
    fn render_prompt_right(&self) -> Cow<str> {
        Cow::Borrowed("")
    }
    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<str> {
        Cow::Borrowed("> ")
    }
    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        Cow::Borrowed("   -> ")
    }
    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}

struct SqlValidator;

impl Validator for SqlValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        if line.trim().is_empty() || is_complete_statement(line) {
            ValidationResult::Complete
        } else {
            ValidationResult::Incomplete
        }
    }
}

pub fn run(conn: Box<dyn DbConnection>, db_name: &str) -> Result<(), String> {
    let schema = SchemaService::load(conn.as_ref())?;
    let tables_for_dt: Vec<String> = schema.tables().to_vec();

    let highlighter = SqlHighlighter::new(schema.clone());
    let completer = SqlCompleter::new(schema);

    let menu = ColumnarMenu::default().with_name("completion_menu");

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let mut rl = Reedline::create()
        .with_completer(Box::new(completer))
        .with_highlighter(Box::new(highlighter))
        .with_validator(Box::new(SqlValidator))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_edit_mode(Box::new(Emacs::new(keybindings)));

    let prompt = PgrsPrompt;

    println!(
        "Connected to '{}'. Type \\q or Ctrl+D to exit. \\dt to list tables.",
        db_name
    );

    loop {
        match rl.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let trimmed = line.trim();
                if trimmed == "\\q" || trimmed == "exit" {
                    break;
                }
                if trimmed == "\\dt" {
                    for table in &tables_for_dt {
                        println!(" {}", table);
                    }
                    continue;
                }
                if trimmed.is_empty() {
                    continue;
                }
                match conn.execute(trimmed) {
                    Ok(result) => print_result(&result),
                    Err(e) => eprintln!("ERROR:  {}", e),
                }
            }
            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => break,
            Err(e) => return Err(e.to_string()),
        }
    }

    println!("Bye.");
    Ok(())
}
```

- [ ] **Step 2: Hapus rustyline dari Cargo.toml**

Edit `Cargo.toml`, hapus baris `rustyline = "14"`.

- [ ] **Step 3: Verifikasi compile bersih**

```bash
cargo check 2>&1
```

Expected: no errors. Jika ada error terkait `PromptHistorySearch` atau `PromptHistorySearchStatus` tidak dikenal, implementasi `render_prompt_history_search_indicator` bisa disederhanakan:

```rust
fn render_prompt_history_search_indicator(&self, _: PromptHistorySearch) -> Cow<str> {
    Cow::Borrowed("(search) ")
}
```

- [ ] **Step 4: Jalankan tests**

```bash
cargo test
```

Expected: semua existing tests pass. Tests di `completer.rs` yang test `highlight_sql` tetap valid karena fungsi tersebut tidak dihapus.

- [ ] **Step 5: Commit Tasks 2 + 3 bersama**

```bash
git add Cargo.toml Cargo.lock src/adapters/driving/repl/mod.rs src/adapters/driving/repl/completer.rs
git commit -m "feat: migrate REPL to reedline with ColumnarMenu and colored completions"
```

---

## Task 3: Port SqlCompleter ke reedline Completer trait + colored Suggestions

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Hapus semua rustyline imports, ganti dengan reedline**

Di bagian atas `completer.rs`, ganti semua `use rustyline::...` dengan:

```rust
use nu_ansi_term::{Color, Style};
use reedline::{Completer, Highlighter, Span, StyledText, Suggestion};

use crate::core::services::schema::service::SchemaService;
```

- [ ] **Step 2: Tambah method `label()` dan `style()` pada CompletionKind**

Tambahkan `impl CompletionKind` baru (setelah definisi enum):

```rust
impl CompletionKind {
    fn label(&self) -> &'static str {
        match self {
            CompletionKind::Keyword => "[keyword]",
            CompletionKind::Table   => "[table]",
            CompletionKind::Column  => "[column]",
        }
    }

    fn style(&self) -> Style {
        match self {
            CompletionKind::Keyword => Style::new().fg(Color::Cyan).bold(),
            CompletionKind::Table   => Style::new().fg(Color::Yellow).bold(),
            CompletionKind::Column  => Style::new().fg(Color::Green),
        }
    }
}
```

- [ ] **Step 3: Tambah helper `word_start`**

Tambahkan free function sebelum `impl SqlCompleter`:

```rust
fn word_start(line: &str, pos: usize) -> usize {
    let input = &line[..pos];
    let last_ws = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
    let word = &input[last_ws..];
    if let Some(dot_pos) = word.rfind('.') {
        last_ws + dot_pos + 1
    } else {
        last_ws
    }
}
```

- [ ] **Step 4: Implement reedline `Completer` pada SqlCompleter**

Hapus seluruh blok `impl Completer for SqlCompleter { ... }` (rustyline version), ganti dengan:

```rust
impl Completer for SqlCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let start = word_start(line, pos);
        self.complete_input(line, pos)
            .into_iter()
            .map(|(value, kind)| Suggestion {
                value,
                description: Some(kind.label().to_string()),
                style: Some(kind.style()),
                span: Span::new(start, pos),
                extra: None,
                append_whitespace: false,
            })
            .collect()
    }
}
```

- [ ] **Step 5: Hapus Hinter, Highlighter (rustyline), Validator (rustyline), Helper impls**

Hapus semua blok ini yang masih pakai rustyline:
- `impl Hinter for SqlCompleter`
- `impl Highlighter for SqlCompleter` (versi rustyline)
- `impl Validator for SqlCompleter`
- `impl Helper for SqlCompleter`

- [ ] **Step 6: Tambah `SqlHighlighter` struct dan implement reedline `Highlighter`**

Tambahkan di bagian bawah file (sebelum `#[cfg(test)]`):

```rust
pub struct SqlHighlighter {
    tables: Vec<String>,
    columns: Vec<String>,
}

impl SqlHighlighter {
    pub fn new(schema: SchemaService) -> Self {
        let tables = schema.tables().to_vec();
        let columns: Vec<String> = schema
            .tables()
            .iter()
            .flat_map(|t| schema.columns_for(t).iter().cloned())
            .collect();
        Self { tables, columns }
    }
}

impl Highlighter for SqlHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
                let start = i;
                while i < len && chars[i] != '\n' {
                    i += 1;
                }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().dimmed(), span));
            } else if chars[i] == '\'' {
                let start = i;
                i += 1;
                loop {
                    if i >= len { break; }
                    if chars[i] == '\'' {
                        i += 1;
                        if i < len && chars[i] == '\'' { i += 1; } else { break; }
                    } else {
                        i += 1;
                    }
                }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().fg(Color::Yellow), span));
            } else if chars[i].is_ascii_digit() {
                let start = i;
                let mut has_dot = false;
                while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !has_dot && i + 1 < len && chars[i + 1].is_ascii_digit())) {
                    if chars[i] == '.' { has_dot = true; }
                    i += 1;
                }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().fg(Color::Magenta), span));
            } else if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let upper = word.to_uppercase();
                let style = if SQL_KEYWORDS.contains(&upper.as_str()) {
                    Style::new().fg(Color::Cyan).bold()
                } else if self.tables.iter().any(|t| t.eq_ignore_ascii_case(&word)) {
                    Style::new().fg(Color::Yellow).bold()
                } else if self.columns.iter().any(|c| c.eq_ignore_ascii_case(&word)) {
                    Style::new().fg(Color::Green)
                } else {
                    Style::new()
                };
                styled.push((style, word));
            } else {
                let ch = chars[i].to_string();
                styled.push((Style::new(), ch));
                i += 1;
            }
        }

        styled
    }
}
```

- [ ] **Step 7: Pastikan compile bersih**

```bash
cargo check 2>&1
```

Expected: no errors. Jika ada error terkait `StyledText::new()` atau `styled.push(...)`, periksa reedline docs untuk API yang benar — `StyledText` mungkin punya constructor atau method berbeda tergantung versi.

- [ ] **Step 8: Jalankan test**

```bash
cargo test
```

Expected: semua test yang ada sebelumnya pass. Test `highlight_*` di bagian bawah perlu di-update atau dihapus karena `highlight_sql` return type tidak berubah — test tersebut masih valid.

- [ ] **Step 9: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat: port SqlCompleter to reedline with colored ColumnarMenu suggestions"
```

---

## Task 4: Manual smoke test REPL (setelah Task 2+3 commit)

Tidak ada unit test untuk REPL loop (UI behavior), tapi kita verifikasi manual.

- [ ] **Step 1: Build dan jalankan**

```bash
cargo build 2>&1
```

Expected: build sukses tanpa warning.

- [ ] **Step 2: Test koneksi ke database PostgreSQL nyata**

```bash
./target/debug/pgrs connect <nama-koneksi-yang-ada>
```

Atau via `cargo run -- connect <nama>`.

- [ ] **Step 3: Verifikasi dropdown muncul**

Di REPL prompt `pgrs> `, ketik `SEL` lalu tekan Tab. Expected: ColumnarMenu muncul dengan suggestions berwarna cyan untuk keywords.

- [ ] **Step 4: Verifikasi warna entry**

- Keywords (SELECT, FROM, dll) → **bold cyan**
- Table names → **bold yellow**
- Column names → **green**

- [ ] **Step 5: Verifikasi multi-line prompt**

Ketik `SELECT * FROM users` (tanpa titik koma) lalu Enter. Expected: prompt berubah ke `   -> ` dan menunggu. Ketik `;` lalu Enter. Expected: query dieksekusi.

- [ ] **Step 6: Commit jika ada minor fix dari smoke test**

```bash
git add -p
git commit -m "fix: minor REPL wiring issues found during smoke test"
```

---

## Task 5: Qualified name completion (table.column, schema.table.column)

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Tulis failing tests**

Tambahkan test cases baru di dalam `#[cfg(test)] mod tests`:

```rust
#[test]
fn suggests_columns_after_table_dot() {
    let schema = schema_with(
        &["users"],
        &[("users", &["id", "email", "created_at"])],
    );
    let c = SqlCompleter::new(schema);
    let input = "SELECT users.";
    let results = c.complete_input(input, input.len());
    assert!(
        results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
        "expected id [column] in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
    assert!(results.iter().any(|(r, _)| r == "email"));
    assert!(results.iter().any(|(r, _)| r == "created_at"));
}

#[test]
fn filters_columns_after_table_dot_with_prefix() {
    let schema = schema_with(
        &["users"],
        &[("users", &["id", "email", "created_at"])],
    );
    let c = SqlCompleter::new(schema);
    let input = "SELECT users.em";
    let results = c.complete_input(input, input.len());
    assert!(results.iter().any(|(r, _)| r == "email"), "expected email");
    assert!(!results.iter().any(|(r, _)| r == "id"), "id should not appear");
}

#[test]
fn suggests_columns_after_schema_table_dot() {
    let schema = schema_with(
        &["users"],
        &[("users", &["id", "email"])],
    );
    let c = SqlCompleter::new(schema);
    let input = "SELECT public.users.";
    let results = c.complete_input(input, input.len());
    assert!(
        results.iter().any(|(r, _)| r == "id"),
        "expected id from public.users. in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
}

#[test]
fn word_start_returns_position_after_dot() {
    // "SELECT users." — word_start at pos=13 should be 13 (after the dot)
    assert_eq!(word_start("SELECT users.", 13), 13);
}

#[test]
fn word_start_returns_position_after_last_dot_in_schema_table() {
    // "SELECT public.users." — word_start at pos=20 should be 20
    assert_eq!(word_start("SELECT public.users.", 20), 20);
}
```

- [ ] **Step 2: Jalankan test, pastikan fail**

```bash
cargo test suggests_columns_after_table_dot 2>&1
cargo test word_start_returns_position_after_dot 2>&1
```

Expected: FAIL — `suggests_columns_after_table_dot` akan return keywords bukan columns.

- [ ] **Step 3: Implementasi qualified name detection di `complete_input`**

Di awal method `complete_input`, tambahkan early-return check sebelum blok `let table_triggers = ...`:

```rust
pub fn complete_input(&self, line: &str, pos: usize) -> Vec<(String, CompletionKind)> {
    let input = &line[..pos];

    // Qualified name detection: "table.col_prefix" atau "schema.table.col_prefix"
    {
        let last_ws = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
        let token = &input[last_ws..];
        if let Some(dot_pos) = token.rfind('.') {
            let prefix_part = &token[..dot_pos];
            let col_prefix = token[dot_pos + 1..].to_uppercase();
            // Ambil nama tabel: segment terakhir setelah titik terakhir (handle schema.table)
            let table_name = prefix_part
                .split('.')
                .last()
                .unwrap_or(prefix_part)
                .to_lowercase();
            let cols = self.schema.columns_for(&table_name);
            let candidates: Vec<(String, CompletionKind)> = if !cols.is_empty() {
                cols.iter()
                    .filter(|c| c.to_uppercase().starts_with(col_prefix.as_str()))
                    .map(|c| (c.to_string(), CompletionKind::Column))
                    .collect()
            } else {
                // Tabel tidak ditemukan: fallback ke semua kolom
                self.schema
                    .tables()
                    .iter()
                    .flat_map(|t| self.schema.columns_for(t).iter().cloned())
                    .filter(|c| c.to_uppercase().starts_with(col_prefix.as_str()))
                    .map(|c| (c, CompletionKind::Column))
                    .collect()
            };
            return candidates;
        }
    }

    // ... sisa kode existing tidak berubah ...
    let upper = input.to_uppercase();
    // ...
```

- [ ] **Step 4: Jalankan semua test**

```bash
cargo test
```

Expected: semua test pass termasuk yang baru.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat: add qualified name completion (table.column, schema.table.column)"
```

---

## Catatan Implementasi

**Jika `StyledText::new()` tidak ada:** Di beberapa versi reedline, `StyledText` langsung diinisialisasi dengan `StyledText { buffer: vec![] }`. Cek dengan `cargo doc --open` atau lihat reedline source.

**Jika `Suggestion { ..Default::default() }` tidak compile:** Pastikan semua field `Suggestion` diisi manual — beberapa versi tidak implement `Default`. Field lengkap: `value`, `description`, `style`, `extra`, `span`, `append_whitespace`.

**Jika `default_emacs_keybindings()` tidak ada di `reedline::keybindings`:** Import dari `reedline` langsung: `use reedline::default_emacs_keybindings;`.

**Jika `PromptHistorySearch` / `PromptHistorySearchStatus` tidak dikenal:** Implementasi `PgrsPrompt::render_prompt_history_search_indicator` bisa return `Cow::Borrowed("")` sebagai fallback.

**Untuk auto-show tanpa Tab:** Coba tambahkan `.with_quick_completions(true)` ke `Reedline::create()` builder chain. Jika method tidak ada di versi reedline yang diinstall, skip — Tab-triggered sudah jauh lebih baik dari experience sebelumnya.
