# Autocomplete Dropdown dengan Label Tipe — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tampilkan autocomplete kandidat sebagai dropdown list dengan label tipe (`[keyword]`, `[table]`, `[column]`) di SQL REPL, mirip autocomplete VSCode/Zed.

**Architecture:** Tambah `CompletionKind` enum ke `completer.rs`, refactor `complete_input` return type ke `Vec<(String, CompletionKind)>`, update `Completer::complete` untuk build `Pair` dengan `display` berisi label dan `replacement` bersih. Aktifkan `CompletionType::List` di `Editor` config di `mod.rs`.

**Tech Stack:** Rust, rustyline 14 (`CompletionType::List`, `Pair`)

---

## File yang Diubah

| File | Perubahan |
|------|-----------|
| `src/adapters/driving/repl/completer.rs` | Tambah `CompletionKind` enum, refactor `complete_input`, update `Completer::complete`, update tests |
| `src/adapters/driving/repl/mod.rs` | Ganti `Editor::new()` dengan `Editor::with_config(config)` pakai `CompletionType::List` |

---

### Task 1: Tambah `CompletionKind` dan refactor `complete_input`

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Tulis failing test untuk `CompletionKind` tagging**

Tambah test berikut di dalam blok `#[cfg(test)]` di `src/adapters/driving/repl/completer.rs`, di bawah test `no_duplicate_suggestions` yang sudah ada:

```rust
#[test]
fn tags_keywords_with_keyword_kind() {
    let schema = schema_with(&[], &[]);
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("SEL", 3);
    assert!(
        results.iter().any(|(r, k)| r == "SELECT" && matches!(k, CompletionKind::Keyword)),
        "expected SELECT [keyword] in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
}

#[test]
fn tags_tables_with_table_kind() {
    let schema = schema_with(&["users", "orders"], &[]);
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("SELECT * FROM ", 13);
    assert!(
        results.iter().any(|(r, k)| r == "users" && matches!(k, CompletionKind::Table)),
        "expected users [table]"
    );
}

#[test]
fn tags_columns_with_column_kind() {
    let schema = schema_with(
        &["users"],
        &[("users", &["id", "email"])],
    );
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("SELECT  FROM users", 7);
    assert!(
        results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
        "expected id [column]"
    );
}
```

- [ ] **Step 2: Jalankan test untuk verifikasi gagal**

```bash
cargo test tags_keywords_with_keyword_kind tags_tables_with_table_kind tags_columns_with_column_kind 2>&1 | tail -20
```

Expected: compile error karena `complete_input` masih return `Vec<String>` dan `CompletionKind` belum ada.

- [ ] **Step 3: Tambah `CompletionKind` enum**

Di `src/adapters/driving/repl/completer.rs`, tambah enum tepat setelah baris `const SQL_KEYWORDS`:

```rust
pub enum CompletionKind {
    Keyword,
    Table,
    Column,
}
```

- [ ] **Step 4: Refactor `complete_input` return type**

Ganti seluruh method `complete_input` dengan implementasi berikut (logic filtering tidak berubah, hanya wrapping dengan `CompletionKind`):

```rust
pub fn complete_input(&self, line: &str, pos: usize) -> Vec<(String, CompletionKind)> {
    let input = &line[..pos];
    let upper = input.to_uppercase();
    let tokens: Vec<&str> = upper.split_whitespace().collect();

    let current_word = if input.ends_with(char::is_whitespace) || input.is_empty() {
        ""
    } else {
        tokens.last().copied().unwrap_or("")
    };

    let table_triggers = ["FROM", "JOIN", "INTO", "UPDATE"];
    let col_triggers = ["SELECT", "WHERE", "ON", "SET", "BY"];

    let effective_trigger = if table_triggers.contains(&current_word) || col_triggers.contains(&current_word) {
        current_word
    } else if input.ends_with(char::is_whitespace) {
        tokens.last().copied().unwrap_or("")
    } else if tokens.len() >= 2 {
        tokens[tokens.len() - 2]
    } else {
        ""
    };

    let full_upper = line.to_uppercase();

    let candidates: Vec<(String, CompletionKind)> = match effective_trigger {
        "FROM" | "JOIN" | "INTO" | "UPDATE" => self
            .schema
            .tables()
            .iter()
            .map(|t| (t.to_string(), CompletionKind::Table))
            .collect(),
        "SELECT" | "WHERE" | "ON" | "SET" | "BY" => {
            let table_refs = self.extract_table_refs(&full_upper);
            if table_refs.is_empty() {
                SQL_KEYWORDS
                    .iter()
                    .map(|k| (k.to_string(), CompletionKind::Keyword))
                    .collect()
            } else {
                table_refs
                    .iter()
                    .flat_map(|t| {
                        let t_lower = t.to_lowercase();
                        self.schema
                            .columns_for(&t_lower)
                            .iter()
                            .map(|c| (c.to_string(), CompletionKind::Column))
                    })
                    .collect()
            }
        }
        _ => SQL_KEYWORDS
            .iter()
            .map(|k| (k.to_string(), CompletionKind::Keyword))
            .collect(),
    };

    let is_trigger = table_triggers.contains(&current_word) || col_triggers.contains(&current_word);
    let prefix_upper = if is_trigger { "".to_string() } else { current_word.to_uppercase() };

    let mut results: Vec<(String, CompletionKind)> = candidates
        .into_iter()
        .filter(|(c, _)| c.to_uppercase().starts_with(&prefix_upper))
        .collect();

    results.sort_by(|a, b| a.0.cmp(&b.0));
    results.dedup_by_key(|item| item.0.clone());
    results
}
```

- [ ] **Step 5: Update existing tests yang menggunakan `complete_input`**

Ganti semua assertion di existing tests yang mengakses hasil `complete_input` sebagai `Vec<String>` menjadi tuple destructuring. Berikut versi lengkap seluruh blok `#[cfg(test)]` setelah update (termasuk 3 test baru dari Step 1):

```rust
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
        assert!(
            results.iter().any(|(r, _)| r == "SELECT"),
            "expected SELECT in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn suggests_table_names_after_from() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 13);
        assert!(results.iter().any(|(r, _)| r == "users"));
        assert!(results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn suggests_table_names_after_join() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM users JOIN ", 24);
        assert!(results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn suggests_columns_after_select_when_table_known() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email", "created_at"])],
        );
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT  FROM users", 7);
        assert!(results.iter().any(|(r, _)| r == "id"), "expected id in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>());
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn filters_by_current_word_prefix() {
        let schema = schema_with(&["users", "user_sessions"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM user", 18);
        assert!(results.iter().any(|(r, _)| r == "users"));
        assert!(results.iter().any(|(r, _)| r == "user_sessions"));
        assert!(!results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn no_duplicate_suggestions() {
        let schema = schema_with(&["users"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 14);
        let names: Vec<&str> = results.iter().map(|(r, _)| r.as_str()).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "duplicates found: {:?}", names);
    }

    #[test]
    fn tags_keywords_with_keyword_kind() {
        let schema = schema_with(&[], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SEL", 3);
        assert!(
            results.iter().any(|(r, k)| r == "SELECT" && matches!(k, CompletionKind::Keyword)),
            "expected SELECT [keyword] in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tags_tables_with_table_kind() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 13);
        assert!(
            results.iter().any(|(r, k)| r == "users" && matches!(k, CompletionKind::Table)),
            "expected users [table]"
        );
    }

    #[test]
    fn tags_columns_with_column_kind() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email"])],
        );
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT  FROM users", 7);
        assert!(
            results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
            "expected id [column]"
        );
    }
}
```

- [ ] **Step 6: Jalankan semua tests untuk verifikasi pass**

```bash
cargo test 2>&1 | tail -20
```

Expected output: semua test pass, tidak ada error kompilasi.

- [ ] **Step 7: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat: add CompletionKind enum and refactor complete_input to return typed candidates"
```

---

### Task 2: Update `Completer::complete` dengan label display dan aktifkan `CompletionType::List`

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs` (method `complete`)
- Modify: `src/adapters/driving/repl/mod.rs` (Editor config)

- [ ] **Step 1: Update method `complete` di `impl Completer for SqlCompleter`**

Di `src/adapters/driving/repl/completer.rs`, ganti implementasi `fn complete` yang ada:

```rust
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
            .map(|(c, kind)| {
                let label = match kind {
                    CompletionKind::Keyword => "[keyword]",
                    CompletionKind::Table   => "[table]",
                    CompletionKind::Column  => "[column]",
                };
                Pair {
                    display:     format!("{:<20} {}", c, label),
                    replacement: c,
                }
            })
            .collect();

        Ok((word_start, pairs))
    }
}
```

- [ ] **Step 2: Aktifkan `CompletionType::List` di `repl/mod.rs`**

Di `src/adapters/driving/repl/mod.rs`, ganti baris import rustyline dan baris `Editor::new()`:

Tambah import di bagian atas (setelah `use rustyline::Editor;`):
```rust
use rustyline::config::{Builder, CompletionType};
```

Ganti:
```rust
let mut rl: Editor<SqlCompleter, DefaultHistory> =
    Editor::new().map_err(|e| e.to_string())?;
```

Dengan:
```rust
let config = Builder::new()
    .completion_type(CompletionType::List)
    .build();
let mut rl: Editor<SqlCompleter, DefaultHistory> =
    Editor::with_config(config).map_err(|e| e.to_string())?;
```

- [ ] **Step 3: Build dan jalankan semua tests**

```bash
cargo build 2>&1 | tail -20
```

Expected: build sukses tanpa error atau warning.

```bash
cargo test 2>&1 | tail -20
```

Expected: semua test pass.

- [ ] **Step 4: Commit**

```bash
git add src/adapters/driving/repl/completer.rs src/adapters/driving/repl/mod.rs
git commit -m "feat: show autocomplete as dropdown list with [keyword/table/column] labels"
```
