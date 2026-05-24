# Changelog

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
