# Core Module Review — SRP & Separation of Concerns

Hasil review modul `pgrs-core` (`modules/core`) dengan fokus Single Responsibility
Principle dan Separation of Concerns. Daftar issue beserta lokasi file & baris,
diurutkan sesuai rencana implementasi (risiko rendah → tinggi).

---

## Fase 1 — Quick wins, zero-risk (mekanis murni)

### [1] 🟢 Timestamp dihitung inline, padahal `unix_now()` sudah ada
Inkonsistensi/duplikasi. `SchemaCacheService::save` menghitung `SystemTime::now()...`
manual, sementara helper `utils::unix_now()` tersedia.

| File | Baris |
|------|-------|
| `modules/core/src/services/schema_cache/service.rs` | 27-30 |
| (helper yang seharusnya dipakai) `modules/core/src/utils.rs` | 1-6 |
| (import yang bisa dihapus) `modules/core/src/services/schema_cache/service.rs` | 3 |

**Aksi:** ganti perhitungan inline dengan `crate::utils::unix_now()`.

---

### [2] 🟡 Duplikasi pola "resolve connection_id + unix_now"
Pola identik (`connection_repo.get_connection(name)?.id.ok_or(StorageError("no id"))?`
lalu `unix_now()`) diulang verbatim di 3 service. Tanggung jawab "resolve id" tersebar.

| File | Baris |
|------|-------|
| `modules/core/src/services/table_access/service.rs` | 37-41 |
| `modules/core/src/services/column_access/service.rs` | 38-42 |
| `modules/core/src/services/query_history/service.rs` | 35-38 |

**Aksi:** ekstrak helper `resolve_connection_id(repo, name) -> Result<i64, DomainError>`
(mis. di `services/mod.rs` atau extension trait), pakai di ketiga service.

---

## Fase 2 — Pindahkan logika yang salah tempat (SoC)

### [3] 🔴 Parsing SQL bocor ke API facade
`extract_column_refs` (sqlparser: AST, projection, identifier) hidup di dalam
`api/analytics.rs`. Facade seharusnya tipis (delegasi saja, bandingkan dengan
`api/connection.rs`). Tidak konsisten pula: `extract_referenced_tables` berada di
`query/alias.rs` (benar), tapi `extract_column_refs` malah di layer API.

| File | Baris |
|------|-------|
| `modules/core/src/api/analytics.rs` (fungsi `extract_column_refs`) | 79-118 |
| `modules/core/src/api/analytics.rs` (pemanggilan di `record_query`) | 48-52 |
| (lokasi tujuan, sebelah extractor sejenis) `modules/core/src/query/alias.rs` | — |
| (referensi pola benar) `extract_referenced_tables` dipanggil di `api/analytics.rs` | 9, 49 |

**Aksi:** pindahkan `extract_column_refs` ke modul `query/`. **Penting:** jangan
bergantung pada `api::SchemaApi` (itu mundur ke layer atas) — terima data schema
lewat `&dyn SchemaPort` atau `&[(table, columns)]`. Keputusan ini memengaruhi Fase 3 #4.

---

## Fase 3 — Perubahan arsitektur (signature/wiring berubah, lintas-crate)

### [4] 🟡 Wiring DI bercampur di dalam facade
`AnalyticsApi::from_sqlite` membangun 4 service + casting `Arc<dyn ...>`;
`SchemaApi::from_sqlite` membangun 3. Komposisi idealnya tanggung jawab composition
root (`Core`/`lib.rs`), bukan facade.

| File | Baris |
|------|-------|
| `modules/core/src/api/analytics.rs` (`from_sqlite`, rakit 4 service) | 24-44 |
| `modules/core/src/api/schema.rs` (`from_sqlite`, rakit 3 service) | 22-39 |
| (composition root tujuan) `modules/core/src/lib.rs` | 52-83 |
| (pemanggil hilir yang mungkin terdampak) `modules/cli/src/app.rs` | — |

**Aksi:** pindahkan perakitan service ke `Core::init`; facade menerima service jadi.

---

### [5] 🟢 Dua strategi error berdampingan di port
`ConnectionRepository` → `DomainError`, tetapi `DbConnection`/`SchemaPort` →
`Result<_, String>` (stringly-typed). Batas port tidak seragam. Dampak paling luas
(merambat ke `pgrs-cli`), jadi dikerjakan paling akhir di antara perubahan struktural.

| File | Baris |
|------|-------|
| `modules/core/src/ports/db_connection.rs` (`execute -> Result<_, String>`) | 8-10 |
| `modules/core/src/ports/schema_port.rs` (`list_columns -> Result<_, String>`) | — |
| (pembanding yang sudah benar) `modules/core/src/ports/connection_repository.rs` | 4-11 |
| (adapter terdampak) `modules/core/src/adapters/driven/postgres_db.rs` | 56-117 |
| (facade terdampak) `modules/core/src/api/query.rs` | 25-42 |

**Aksi:** seragamkan ke `DomainError` (atau varian baru) untuk port query/schema.

---

## Fase 4 — Opsional / butuh keputusan desain

### [6] 🟡 Concern logging/presentasi bocor ke core
`eprintln!("pgrs: ...")` dipakai untuk menelan error di service core (fire-and-forget).
Core tidak seharusnya menulis ke stderr — itu keputusan UI.

| File | Baris |
|------|-------|
| `modules/core/src/services/analytics/service.rs` | 57, 68, 72 |
| `modules/core/src/services/schema_cache/service.rs` | 33, 38, 47, 53, 58, 79, 81 |

**Aksi:** bubble-up `Result`, atau inject logging port (`dyn Logger`); biarkan caller
(CLI) memutuskan cara menampilkan. Mengubah kontrak `record_query`.

---

### [7] 🟡 `QueryCompletionService` memikul terlalu banyak tanggung jawab
Satu struct (~265 baris) menangani: deteksi trigger, ekstraksi table refs, generate
kandidat, filter prefix, **dan** ranking berbasis frekuensi. Ranking adalah concern
terpisah dari pembangkitan kandidat.

| File | Baris |
|------|-------|
| `modules/core/src/services/query/query_completion.rs` (keseluruhan) | 1-265 |
| — `filter_and_sort` (filter + sort frekuensi bercampur) | 188-245 |
| — `candidates_for_trigger` (generate kandidat) | 121-186 |
| — `resolve_trigger_and_word` (deteksi trigger) | 267-290 |

**Aksi:** pisahkan strategi ranking (frequency sort) ke unit sendiri, mis.
`CompletionRanker`; pisahkan filter dari sort.

---

## Ringkasan Urutan Implementasi

| Urutan | Item | Severity | Risiko | Lintas-crate? |
|--------|------|----------|--------|---------------|
| 1 | `unix_now()` di schema_cache | 🟢 | Sangat rendah | Tidak |
| 2 | Helper `resolve_connection_id` | 🟡 | Rendah | Tidak |
| 3 | Pindah `extract_column_refs` ke `query/` | 🔴 | Sedang | Tidak (core) |
| 4 | Wiring DI → `Core::init` | 🟡 | Sedang | Ya (CLI) |
| 5 | Seragamkan error port (`String`→`DomainError`) | 🟢 | Tinggi | Ya (CLI) |
| 6 | Hilangkan `eprintln!` (logging port) | 🟡 | Tinggi | Sebagian |
| 7 | Pisah ranking dari `QueryCompletionService` | 🟡 | Tinggi | Tidak |

**Prinsip:** commit per item, jalankan `cargo test --workspace` di antara setiap langkah.
Item 1–3 idealnya tidak mengubah test sama sekali (bukti behavior-preserving);
item 4–7 baru boleh menyentuh test.
