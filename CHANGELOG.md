# Changelog

## [0.8.0] - 2026-06-09

### Added
- Saved queries: simpan SQL favorit per-koneksi di `~/.pgrs/pgrs.db` (`\save <nama> <id>`, `\saved`, `\run <nama>`, `\unsave <nama>`) — `\run` lewat jalur eksekusi yang sama sehingga DML guard, analytics, dan auto-refresh skema tetap berlaku
- `\explain` / `\explain+` merender plan EXPLAIN sebagai pohon ASCII (core menjalankan `EXPLAIN (FORMAT JSON)`); `\explain+` menambah `ANALYZE` sehingga tunduk pada DML transaction guard
- Auto pager: output panjang REPL dialirkan ke `$PAGER` (fallback `less -SR`) hanya saat melebihi tinggi terminal di TTY; `\pager` untuk toggle
- REPL transaction-aware: alias `\begin`/`\commit`/`\rollback`, indikator status transaksi di prompt, dan konfirmasi-rollback saat keluar dengan transaksi terbuka

### Changed
- DML transaction guard: `INSERT`/`UPDATE`/`DELETE` (termasuk CTE-wrapped) ditolak di REPL kecuali ada transaksi terbuka — jalankan `BEGIN`/`\begin` dulu (`connect`/`psql` tidak terpengaruh)

## [0.7.0] - 2026-06-07

### Added
- Postgres query errors kini menampilkan SQLSTATE code, detail/hint, dan penanda caret (`^`) pada posisi error

### Fixed
- `\stats <table>` tidak lagi kosong — penggunaan kolom nyata kini direkam dengan benar
- Path data dir yang bukan UTF-8 kini memunculkan error eksplisit, bukan fallback diam-diam

### Changed
- **BREAKING (internal):** kode dipecah menjadi Cargo workspace `pgrs-core` + `pgrs-cli`; boundary hexagonal ditegakkan compiler (perilaku CLI tidak berubah)
- `pg_catalog` lookups & klasifikasi SQL (DDL/DML) dipindah ke core
- Error port diseragamkan ke `DomainError`; service membubble-up error alih-alih `eprintln!`
- DI analytics/schema dipindah ke composition root; REPL dispatch diratakan

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
