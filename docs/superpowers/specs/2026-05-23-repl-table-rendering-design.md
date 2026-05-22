# REPL Table Rendering — Design

**Date:** 2026-05-23
**Status:** Approved

## Tujuan

Menampilkan hasil query di REPL dengan format tabel yang lebih modern, rapi, dan
mudah dibaca. Mengganti renderer psql-style (pipe `|`) saat ini dengan gaya
minimal, menambahkan expanded mode, dan middle-truncate untuk teks panjang.

## Konteks

Renderer saat ini ada di `src/adapters/driving/repl/executor.rs`
(`format_result` / `print_result`). Sudah mendukung:

- Auto column width (`col_widths`)
- Left-align dengan padding
- Colorize: `true` hijau, `false` merah, `null`/`NULL` dim
- `visible_len` yang ANSI- dan CJK-aware (via `unicode-width`)
- Normalisasi `t`/`f` → `true`/`false`

`QueryResult` (`src/core/ports/db_connection.rs`) berisi `columns`, `rows`
(`Vec<Vec<String>>`), `rows_affected`. NULL direpresentasikan adapter postgres
sebagai string `"NULL"`. Struktur ini tidak berubah.

REPL loop ada di `src/adapters/driving/repl/mod.rs` dan menangani perintah
backslash (`\dt`, `\help`, `\?`, `\q`).

## Pendekatan

Semua perubahan terlokalisir: logika rendering di `executor.rs`, state toggle
`\x` di REPL loop (`mod.rs`). Tidak ada perubahan arsitektur hexagonal.

## Detail Desain

### 1. Gaya minimal (default)

- Baris header diikuti garis bawah `─` (U+2500) selebar tiap kolom.
- Tanpa border vertikal. Kolom dipisah **2 spasi**.
- Auto-width per kolom (pertahankan logika `col_widths`), left-align.
- Pertahankan colorize untuk `true`/`false`/`null`/`NULL`.
- Lebar dihitung via `visible_len` agar ANSI escape & CJK tidak merusak alignment.

Contoh:

```
id   email               active
──   ─────               ──────
1    alice@example.com   true
2    bob@example.com     null
(2 rows)
```

Footer `(N row[s])` / `(N row[s] affected)` dan kasus `columns.is_empty()`
dipertahankan apa adanya.

### 2. Middle-truncate (mode minimal saja)

- Sel dengan panjang > **40 karakter** dipotong di tengah dengan `...` (ASCII).
- Komposisi total 40: prefix 19 + `...` (3) + suffix 18.
- Berbasis `char` (bukan byte) agar aman untuk multi-byte.
- Truncate diterapkan pada nilai (setelah normalisasi `t`/`f`) sebelum colorize,
  lalu `col_widths` dihitung dari nilai yang sudah ter-truncate.
- Nilai ≤ 40 karakter ditampilkan utuh (mis. `12345678910`).
- Nilai penuh tetap dapat dilihat lewat expanded mode.

### 3. Expanded mode — toggle manual `\x`

- Perintah `\x` di REPL loop membalik flag `bool` (default off).
- Saat di-toggle, cetak `Expanded display is on.` atau `Expanded display is off.`.
- Saat on, render gaya record psql, **tanpa truncate** (nilai penuh):

```
-[ RECORD 1 ]------
id     | 1
email  | alice@example.com
active | true
```

- Label kolom di-pad selebar nama kolom terpanjang, diikuti ` | ` lalu nilai.
- Header record: `-[ RECORD n ]` diikuti `-` hingga lebar yang konsisten.
- Colorize nilai tetap berlaku di expanded mode.
- Kasus `columns.is_empty()` (DML / 0 kolom): tetap cetak footer seperti mode
  minimal, tidak ada record.

### 4. Perubahan signature

- `format_result(result: &QueryResult, expanded: bool) -> String`
- `print_result(result: &QueryResult, expanded: bool)`
- REPL loop menyimpan `let mut expanded = false;` dan meneruskannya saat memanggil.

### 5. Help text

- Tambahkan baris `\x` (toggle expanded display) ke `repl_help_text()`.

## Testing

Unit test di `executor.rs`:

- Mode minimal: garis bawah memakai `─`, gap antar kolom 2 spasi.
- Middle-truncate: nilai > 40 char dipotong jadi `prefix...suffix` (total 40);
  nilai ≤ 40 utuh; truncate berbasis char untuk string multi-byte.
- Expanded: format `-[ RECORD n ]`, label di-pad, nilai tidak ter-truncate.
- Expanded dengan `columns` kosong → footer saja.
- Pertahankan semua test colorize & `visible_len` yang ada.

REPL loop: `\x` membalik flag dan tercantum di help text.

## Yang Tidak Termasuk (YAGNI)

- Auto expanded berdasarkan lebar terminal.
- Right-align untuk kolom numerik.
- Konfigurasi lebar truncate / gaya border yang dapat diatur pengguna.
- Pembedaan NULL asli vs string literal `"NULL"`.
