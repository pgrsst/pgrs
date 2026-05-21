# Design: Autocomplete Dropdown dengan Label Tipe

**Date:** 2026-05-21  
**Status:** Approved

## Summary

Tambahkan autocomplete dropdown list ke SQL REPL — mirip model autocomplete di VSCode/Zed/Sublime — menggunakan `CompletionType::List` dari rustyline dan label tipe (`[keyword]`, `[table]`, `[column]`) pada setiap kandidat.

## Background

Saat ini `SqlCompleter` menggunakan behavior default rustyline (`CompletionType::Circular`): menekan Tab bersiklus satu per satu melalui kandidat tanpa menampilkan daftar. User tidak bisa melihat semua pilihan sekaligus.

## Perubahan

### 1. `CompletionKind` enum (`completer.rs`)

Tambah enum untuk membawa informasi tipe kandidat:

```rust
pub enum CompletionKind {
    Keyword,
    Table,
    Column,
}
```

`complete_input` diubah return type-nya dari `Vec<String>` ke `Vec<(String, CompletionKind)>`. Logic filtering, sorting, dan dedup tidak berubah — hanya hasil di-wrap dengan tipe yang sesuai:
- Token dari `SQL_KEYWORDS` → `CompletionKind::Keyword`
- Nama tabel dari `schema.tables()` → `CompletionKind::Table`
- Nama kolom dari `schema.columns_for()` → `CompletionKind::Column`

### 2. `Pair.display` vs `Pair.replacement` (`completer.rs`)

Di `impl Completer for SqlCompleter`, build `Pair` dengan:

```rust
let label = match kind {
    CompletionKind::Keyword => "[keyword]",
    CompletionKind::Table   => "[table]",
    CompletionKind::Column  => "[column]",
};
Pair {
    display:     format!("{:<20} {}", candidate, label),
    replacement: candidate,
}
```

- `display`: teks yang ditampilkan di daftar dropdown, berisi nama + label rata kanan
- `replacement`: teks yang diinsert ke input, bersih tanpa label

### 3. `CompletionType::List` di Editor (`repl/mod.rs`)

Ganti inisialisasi `Editor`:

```rust
use rustyline::config::{Builder, CompletionType};

let config = Builder::new()
    .completion_type(CompletionType::List)
    .build();
let mut rl: Editor<SqlCompleter, DefaultHistory> = Editor::with_config(config)?;
```

Menekan Tab menampilkan semua kandidat sebagai daftar di bawah kursor. User navigasi dengan arrow key, Enter untuk memilih.

## File yang Diubah

| File | Perubahan |
|------|-----------|
| `src/adapters/driving/repl/completer.rs` | Tambah `CompletionKind`, refactor `complete_input`, update `Completer::complete` |
| `src/adapters/driving/repl/mod.rs` | Ganti `Editor::new()` dengan `Editor::with_config(config)` |

## Tidak Berubah

- Tidak ada dependency baru — fitur sudah ada di rustyline 14
- Logic context-aware completion (trigger keyword, extract table refs, filter by prefix) tidak berubah
- Semua existing tests tetap valid; hanya perlu update return type assertion

## Trade-offs

- `CompletionType::List` mengambil alih rendering dari rustyline — tidak ada kontrol visual lebih lanjut (warna, ikon) tanpa mengganti library
- Padding `:<20` bisa terlihat terlalu lebar untuk nama pendek; bisa disesuaikan jika perlu
