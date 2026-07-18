# Production Hardening — Batch 1 Progress Report

**Date:** 2026-07-09  
**Batch 1 Scope:** panic removal, unwrap/expect cleanup (runtime/event-bus/scheduler, cdp/browser/page), unsafe review  

---

## Alt Görev 1: ✅ MessageEnvelopeBuilder panic → Result

| Değişiklik | Durum |
|-----------|-------|
| `BuilderError` enum eklendi (MissingSource, MissingContentType, MissingPayload) | ✅ |
| `build()` artık `Result<MessageEnvelope, BuilderError>` döndürüyor | ✅ |
| 3 adet `panic!()` production koddan kaldırıldı | ✅ |
| Testler güncellendi (panic test → Result test) | ✅ |
| `cargo test -p browseros-types` — 178+19 test geçti | ✅ |

**Kalan production panic! sayısı: 1** (browseros-types/src/identifiers.rs:24 — `ModuleId::default()` panic, `unreachable!()` mantığında)

---

## Alt Görev 2: ✅ Runtime/EventBus/Scheduler unwrap/expect

**browseros-runtime/src/lib.rs:** Production kodunda unwrap/expect yok. Sadece testlerde `.unwrap()` var (kabul edilebilir).

**browseros-event-bus/src/lib.rs:** Production unwrap/expect yok.

**browseros-scheduler/src/lib.rs:** Production unwrap/expect yok.

---

## Alt Görev 3: ✅ CDP/Browser/Page unwrap/expect

### browseros-cdp (3 files modified)

| Dosya | Değişiklik |
|-------|-----------|
| `connection.rs` | 13 adet `lock().expect(...)` dönüştürüldü: Result döndüren metodlarda `map_err(?)` (Category A), thread-safe olmayan yerlerde `unwrap_or_else(\|e\| e.into_inner())` (Category B) |
| `transport_ws.rs` | 1 adet `lock().unwrap()` → `lock().map_err(?)` (Category A) |
| `traits.rs` | ~20 adet `lock().unwrap()` dönüştürüldü: Result döndüren metodlarda `map_err(?)` (Category A), getter/event-handler'larda `unwrap_or_else(\|e\| e.into_inner())` (Category B) |

### browseros-browser — 0 production unwrap/expect (değişiklik yok)

### browseros-page — 0 production unwrap/expect (değişiklik yok)

**Toplam:** ~34 production unwrap/expect kaldırıldı, 0 yeni expect eklendi.

**Kalan:** `browseros-cdp`'de tüm `lock().unwrap()`/`lock().expect()` ler kaldırıldı. Geriye sadece test `.unwrap()`/`.expect()` leri kaldı (kabul edilebilir).

---

## Alt Görev 4: ⏳ Unsafe review (browseros-dom)

Henüz başlanmadı.

---

## Validation

| Adım | Sonuç |
|------|-------|
| `cargo fmt --all` | ✅ Temiz |
| `cargo clippy --workspace` | ✅ 0 yeni uyarı |
| `cargo test --workspace` (unit) | ✅ Tüm unit testler geçti |
| `cargo test --workspace` (entegrasyon) | 🟡 Uzun süren real browser testleri (önceden var olan) |
