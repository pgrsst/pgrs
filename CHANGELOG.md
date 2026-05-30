# Changelog

## [0.6.0] - 2026-05-30

### Added
- Tab-completion suggestions kini diurutkan berdasarkan frekuensi akses — kolom/tabel yang sering digunakan muncul lebih dulu
- Completion kolom kini menyarankan tiga bentuk: `kolom`, `tabel.kolom`, dan `alias.kolom`

### Fixed
- Completion kolom kini muncul setelah `SELECT` meski belum ada klausa `FROM`

### Changed
- SQL parsing diganti dengan crate `sqlparser` — lebih robust untuk query kompleks
- Alias & tokenizer dipindah ke `core/query` (keluar dari repl adapter)
- `CompletionService` dipecah menjadi query dan command concerns
- ID koneksi diganti dari UUID ke SQLite autoincrement
- `Repl` struct dan `CommandHandler` diperkenalkan untuk konsistensi dengan pola CLI
- CLI dipecah per handler, `ConnectionSvc` trait diinjeksi ke `Cli`
- Berbagai service refactor untuk konsistensi trait (`AnalyticsSvc`, `SchemaSvc`)

## [0.5.0] - 2026-05-26

### Added
- `\export` command: export hasil query ke file CSV dari dalam REPL, dengan dukungan quoted paths dan tilde expansion (`~/file.csv`)
- SQLite backend: koneksi kini disimpan di `~/.pgrs/pgrs.db` (SQLite), menggantikan `connections.json`
- Analytics: `\history` menampilkan riwayat query dengan ID dan timestamp lokal; `\stats` menampilkan tabel dan query yang paling sering digunakan
- Schema caching: skema tabel/kolom di-cache di SQLite untuk performa tab-completion yang lebih cepat
- `DomainError` enum: error handling end-to-end diganti dari `String` ke typed enum
- UUID v4 sebagai generator ID koneksi (menggantikan `DefaultHasher`)

### Fixed
- `\export`: blokir DDL dalam export, cleanup file parsial jika write gagal
- `\export`: tampilkan usage jika diketik tanpa argumen
- CSV quoting: handle `\r` dengan benar
- SQLite: tambahkan `ROLLBACK` pada error `save_schema`, transaction di `record_query`
- Install script: deteksi OS dan arsitektur dengan benar untuk binary yang sesuai
- UUID: ganti `DefaultHasher` dengan UUID v4 untuk ID yang lebih aman

### Changed
- Pemisahan arsitektur heksagonal: repository, service, dan port traits dipecah ke modul yang lebih fokus
- REPL commands dipecah ke modul terpisah (`commands.rs`, `ui.rs`, `csv.rs`, `sql_utils.rs`)

## [0.4.0] - 2026-05-24

### Added
- Connection IDs: setiap koneksi mendapat ID hex 8-karakter saat ditambahkan
- Semua perintah kini menerima ID atau nama koneksi
- Kolom ID ditampilkan di output `list`
- Alias baru: `ls` (list), `del` / `rm` (delete)
- Label environment opsional via `--env`; kolom ENV di `list`, label di prompt REPL
- REPL `\d <table>` dan `\d+ <table>`: describe table dengan kolom, indeks, FK, constraint, trigger
- Tab-completion nama tabel setelah `\d` dan `\d+`
- REPL `\l`: tampilkan semua database di server
- Multi-platform CI release builds dengan version injection

### Fixed
- Decode karakter percent-encoded di kredensial URL koneksi
- Pesan error menggunakan nama koneksi yang sudah di-resolve
- Semicolon di dalam identifier double-quoted tidak lagi mengakhiri statement
- Tolak nama tabel kosong pada perintah describe

### Changed
- Refactor REPL: tokenizer, alias resolver, dan schema port dipisah menjadi modul terstruktur
- Teks bantuan distrukturkan sebagai data array
