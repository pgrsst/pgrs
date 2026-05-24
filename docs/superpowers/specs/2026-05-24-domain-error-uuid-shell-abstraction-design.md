# Design: DomainError, UUID ID Generation, run_shell Abstraction

**Date:** 2026-05-24
**Issues:** #1, #2, #3 dari docs/issue.md

---

## Issue 1 — DomainError enum (Full end-to-end)

### Motivasi

Semua port dan service saat ini return `Result<_, String>`. Caller yang ingin bereaksi berbeda berdasarkan jenis error harus cek teks string (fragile, typo = bug). Dengan `DomainError` enum, caller bisa pattern match pada variant.

### Design

**File baru:** `src/core/domain/error.rs`

```rust
#[derive(Debug, PartialEq)]
pub enum DomainError {
    NotFound(String),
    AlreadyExists(String),
    ValidationError(String),
    StorageError(String),
}

impl std::fmt::Display for DomainError { ... }
```

**Mapping error lama ke variant:**

| Pesan lama | Variant |
|---|---|
| `"connection 'x' not found"` | `NotFound` |
| `"connection 'x' already exists"` | `AlreadyExists` |
| `"host is required"`, `"at least one field..."` | `ValidationError` |
| Error dari SQLite/I/O | `StorageError` |

**Files yang berubah:**
- `src/core/domain/error.rs` — baru
- `src/core/domain/mod.rs` — tambah `pub mod error`
- `src/core/ports/connection_repository.rs` — semua `String` → `DomainError`
- `src/core/services/connection/service.rs` — return types + error construction
- `src/adapters/driven/sqlite_repository.rs` — impl trait pakai `DomainError`
- `src/adapters/driving/cli.rs` — format `DomainError` via `Display` untuk output

**Tests:** `assert_eq!(result, Err("...".to_string()))` diganti ke `assert!(matches!(result, Err(DomainError::NotFound(_))))`.

---

## Issue 2 — generate_id() pakai UUID

### Motivasi

`DefaultHasher` adalah implementasi unstable — Rust bisa menggantinya kapan saja. ID yang sama bisa menghasilkan hash berbeda di versi Rust berbeda.

### Design

Tambah dependency: `uuid = { version = "1", features = ["v4"] }`

```rust
fn generate_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}
```

- Format tetap 8-char hex — tidak ada perubahan di database schema atau existing test expectations
- Return type disederhanakan: `String` (bukan `Result<String, String>`) karena UUID tidak bisa gagal
- Sumber randomness: cryptographically random UUID v4, stable lintas versi Rust

---

## Issue 3 — run_shell tanpa Arc\<SqliteRepository\>

### Motivasi

`run_shell` menerima `Arc<SqliteRepository>` (concrete type) padahal hanya butuh `AnalyticsPort` dan `SchemaCachePort`. Ini melanggar hexagonal architecture — fungsi di layer app tidak seharusnya tahu storage-nya SQLite.

### Design

**Signature baru:**

```rust
fn run_shell<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
    analytics: Option<Arc<dyn AnalyticsPort>>,
    schema_cache: Option<Arc<dyn SchemaCachePort>>,
) -> Result<(), String>
```

**Caller di `run_with_dir`:**

```rust
Some("shell") => run_shell(
    &args[1..],
    &connection_service,
    Some(Arc::clone(&sqlite) as Arc<dyn AnalyticsPort>),
    Some(Arc::clone(&sqlite) as Arc<dyn SchemaCachePort>),
),
```

`run_shell` tidak lagi import atau tahu tentang `SqliteRepository`.
