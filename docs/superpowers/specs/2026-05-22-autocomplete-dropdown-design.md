# Autocomplete Dropdown Design

**Date:** 2026-05-22  
**Status:** Approved

## Overview

Upgrade REPL autocomplete dari rustyline `CompletionType::List` (Tab-triggered flat list) ke reedline `ColumnarMenu` yang muncul otomatis saat mengetik, dengan coloring per jenis entry dan qualified name support.

## Goals

- Dropdown popup muncul otomatis saat mengetik (tanpa harus tekan Tab)
- Visual per jenis entry: keyword (cyan), table (yellow), column (green) — konsisten dengan syntax highlighting yang sudah ada
- Support qualified name notation: `table.column` dan `schema.table.column`
- Navigasi dengan arrow keys, select dengan Enter/Tab

## Non-Goals

- Fuzzy matching
- Column type descriptions dari `information_schema` (reserved untuk fase berikutnya)

## Architecture

### Library Migration

Ganti `rustyline` → `reedline` di `Cargo.toml`. Semua business logic di `SchemaService` dan `complete_input` dipertahankan — hanya wiring layer REPL yang berubah.

### Files Changed

| File | Perubahan |
|------|-----------|
| `Cargo.toml` | Hapus `rustyline`, tambah `reedline` |
| `src/adapters/driving/repl/mod.rs` | Rewrite REPL loop pakai `Reedline` + `DefaultPrompt` + `ColumnarMenu` |
| `src/adapters/driving/repl/completer.rs` | Implement reedline `Completer` trait, tambah qualified name logic di `complete_input` |

Files yang **tidak** berubah: `SchemaService`, `DbConnection`, `FileConnectionRepository`, `postgres_db.rs`.

### Dependency Direction

```
repl/mod.rs → SqlCompleter (reedline Completer) → SchemaService → DbConnection trait
                                                              ↑
                                                  FileConnectionRepository / postgres_db
```

Tidak ada perubahan dependency direction.

## Component Design

### REPL Loop (`repl/mod.rs`)

```rust
let completer = SqlCompleter::new(schema);
let menu = ColumnarMenu::default().with_name("completion_menu");
let keybindings = default_emacs_keybindings(); // Ctrl+Space / Tab trigger
let mut rl = Reedline::create()
    .with_completer(Box::new(completer))
    .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)));
```

Menu muncul otomatis saat ada suggestions, navigasi `↑↓`, select dengan `Enter` atau `Tab`.

### Completer (`completer.rs`)

Implement reedline `Completer` trait:

```rust
impl Completer for SqlCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        self.complete_input(line, pos)
            .into_iter()
            .map(|(value, kind)| Suggestion {
                value,
                description: Some(kind.label().to_string()),
                style: Some(kind.ansi_style()),
                ..Default::default()
            })
            .collect()
    }
}
```

`CompletionKind` diperluas dengan method:
- `label()` → `"[keyword]"` / `"[table]"` / `"[column]"`
- `ansi_style()` → warna ANSI konsisten dengan `highlight_sql`

### Qualified Name Logic

Di `complete_input`, cek apakah kata terakhir mengandung `.`:

```
input: "SELECT users."
→ last token: "users."
→ extract prefix "users", cari di schema.tables()
→ jika match → suggest columns dari "users"
→ word_start = posisi setelah dot (replacement hanya bagian setelah titik)

input: "SELECT public.users."
→ extract table "users" dari "public.users."
→ suggest columns dari "users"
```

Fallback: jika prefix sebelum dot tidak match tabel apapun, suggest semua columns.

## Testing

Unit tests yang sudah ada di `completer.rs` tetap valid — `complete_input` tidak berubah interface-nya, hanya tambah test cases untuk qualified name:

- `suggests_columns_after_table_dot` — `"SELECT users."` → columns dari users
- `suggests_columns_after_schema_table_dot` — `"SELECT public.users."` → columns dari users
- `word_start_after_dot_for_replacement` — verifikasi offset replacement benar
