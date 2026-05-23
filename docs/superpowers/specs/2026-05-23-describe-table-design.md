# REPL `\d` / `\d+` — Describe Table Design

**Date:** 2026-05-23
**Status:** Approved

## Tujuan

Tambah perintah `\d <table>` dan `\d+ <table>` ke REPL pgrs dengan full psql
parity — menampilkan kolom, tipe, nullable, default, indexes, foreign-key
constraints, check constraints, dan (untuk `\d+`) triggers, column storage,
statistics target, dan column comments.

## Konteks

REPL sudah punya backslash commands (`\dt`, `\l`, `\x`, `\refresh`, `\help`,
`\q`) yang di-dispatch di `src/adapters/driving/repl/mod.rs`. Tab-completion
sudah terintegrasi di `completer.rs` via `SchemaService`. Format output
menggunakan pgrs minimal style (`executor.rs`).

## Pendekatan

Semua query menggunakan `pg_catalog` langsung — satu-satunya cara mendapat full
psql parity (storage mode, statistics target, trigger detail, OID tidak tersedia
di `information_schema`).

## Arsitektur

```
src/adapters/driving/repl/
  describe.rs     ← NEW: describe_table(db, name, extended)
  mod.rs          ← tambah \d / \d+ ke command dispatch
  completer.rs    ← tambah tab-completion untuk argumen \d / \d+
```

Tidak ada perubahan di core, ports, domain, atau executor.

`describe_table` signature:
```rust
pub fn describe_table(db: &dyn DbConnection, table: &str, extended: bool) -> Result<(), String>
```

## Sanitasi Input

Sebelum interpolasi nama tabel ke query, validasi: hanya izinkan
`[a-zA-Z0-9_.]`. Nama yang mengandung karakter lain ditolak dengan:
`"invalid table name: only letters, digits, underscores, and dots are allowed"`

## SQL Queries

Semua query untuk `\d` dan `\d+`:

### Columns
```sql
SELECT
    a.attname,
    pg_catalog.format_type(a.atttypid, a.atttypmod),
    CASE WHEN a.attnotnull THEN 'not null' ELSE '' END,
    COALESCE(pg_catalog.pg_get_expr(d.adbin, d.adrelid), '')
FROM pg_catalog.pg_attribute a
LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
WHERE a.attrelid = '<table>'::regclass
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY a.attnum
```

### Indexes
```sql
SELECT indexname, indexdef
FROM pg_indexes
WHERE tablename = '<table>'
ORDER BY indexname
```

### Foreign-key constraints
```sql
SELECT conname, pg_catalog.pg_get_constraintdef(oid, true)
FROM pg_catalog.pg_constraint
WHERE conrelid = '<table>'::regclass AND contype = 'f'
ORDER BY conname
```

### Check constraints
```sql
SELECT conname, pg_catalog.pg_get_constraintdef(oid, true)
FROM pg_catalog.pg_constraint
WHERE conrelid = '<table>'::regclass AND contype = 'c'
ORDER BY conname
```

### Tambahan untuk `\d+` saja

**Triggers:**
```sql
SELECT tgname, pg_catalog.pg_get_triggerdef(oid, true)
FROM pg_catalog.pg_trigger
WHERE tgrelid = '<table>'::regclass AND NOT tgisinternal
ORDER BY tgname
```

**Column extras** (storage, stats target, description) — join tambahan ke query
columns:
```sql
SELECT
    a.attname,
    pg_catalog.format_type(a.atttypid, a.atttypmod),
    CASE WHEN a.attnotnull THEN 'not null' ELSE '' END,
    COALESCE(pg_catalog.pg_get_expr(d.adbin, d.adrelid), ''),
    CASE a.attstorage
        WHEN 'p' THEN 'plain'
        WHEN 'e' THEN 'external'
        WHEN 'm' THEN 'main'
        WHEN 'x' THEN 'extended'
        ELSE ''
    END,
    CASE WHEN a.attstattarget = -1 THEN '-' ELSE a.attstattarget::text END,
    COALESCE(pg_catalog.col_description(a.attrelid, a.attnum), '')
FROM pg_catalog.pg_attribute a
LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
WHERE a.attrelid = '<table>'::regclass
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY a.attnum
```

## Output Format

Menggunakan pgrs minimal style (header + `─` underline, 2-space separator, tanpa
border vertikal). Section non-tabular (indexes, FK, constraints, triggers)
ditampilkan sebagai indented text list.

### Contoh `\d users`
```
Table "public.users"

 Column      Type                     Nullable  Default
 ──────────  ───────────────────────  ────────  ──────────────────────────────
 id          integer                  not null  nextval('users_id_seq'::regclass)
 email       character varying(255)   not null
 created_at  timestamp with time zone           now()

Indexes:
    "users_pkey" PRIMARY KEY, btree (id)
    "users_email_key" UNIQUE CONSTRAINT, btree (email)

Foreign-key constraints:
    "users_role_id_fkey" FOREIGN KEY (role_id) REFERENCES roles(id)

Check constraints:
    "users_email_check" CHECK ((email ~* '^[^@]+'::text))
```

### Contoh `\d+ users` — semua di atas, ditambah
```
Triggers:
    audit_users AFTER INSERT OR UPDATE ON users FOR EACH ROW EXECUTE ...

 Column      Storage   Stats target  Description
 ──────────  ────────  ────────────  ─────────────────
 id          plain     -
 email       extended  -             User email address
 created_at  plain     -
```

### Error cases
- Tabel tidak ditemukan: `Did not find any relation named "foo".`
- Nama tabel invalid: `invalid table name: only letters, digits, underscores, and dots are allowed`

## Tab-completion

Di `completer.rs`, deteksi prefix backslash command sebelum SQL token analysis:

```
"\d+ ord<TAB>"  → strip "\d+ " → filter tables starts_with("ord")
"\d use<TAB>"   → strip "\d "  → filter tables starts_with("use")
```

Cek `starts_with("\\d+ ")` dulu (lebih spesifik), lalu `starts_with("\\d ")`.
Source: `SchemaService::table_names()` — tidak perlu perubahan di SchemaService.

## Command Dispatch (mod.rs)

```
"\d <name>"  → describe_table(db, name, false)
"\d+ <name>" → describe_table(db, name, true)
"\d"         → tampilkan "Usage: \d <table>"
"\d+"        → tampilkan "Usage: \d+ <table>"
```

Parsing: `strip_prefix("\\d+ ").or(strip_prefix("\\d "))` untuk extract nama tabel.
