# Implementation Plan: SignalMatrix

**Track ID:** signal-matrix_20260316
**Spec:** [spec.md](./spec.md)
**Created:** 2026-03-16
**Status:** [ ] Not Started

## Overview

Macro-driven `SignalMatrix` в `airq-core` — по паттерну `define_scoring_matrix!` из video-analyzer. Single source of truth для колонок, compile-time `N_SIGNALS` constant, fixed-size row arrays `[f64; N]`, name-based column access. Без Polars/ndarray — чистый Rust + serde + bincode.

## Phase 1: Macro + Core Structure <!-- checkpoint:57cb817 -->

Фундамент: macro для колонок, SoA matrix, базовые операции.

### Tasks
- [x] Task 1.1: Создать `airq-core/src/matrix.rs` с макросом `define_signal_columns!` <!-- sha:57cb817 -->
  - Макрос генерирует:
    - `N_SIGNALS: usize` — compile-time constant (11)
    - `SIGNAL_NAMES: [&str; N_SIGNALS]` — ordered names
    - `SIGNAL_WEIGHTS: [f64; N_SIGNALS]` — weights for comfort index
    - `SignalRow { scores: [f64; N_SIGNALS] }` — single measurement
    - `SignalMatrix` struct (SoA: `timestamps: Vec<f64>`, `data: Vec<[f64; N_SIGNALS]>`, meta Vecs)
  - Определение колонок (одно место, всё остальное derived):
    ```
    define_signal_columns! {
        air         0.22,
        temperature 0.18,
        sea         0.12,
        uv          0.08,
        earthquake  0.08,
        fire        0.05,
        pollen      0.05,
        pressure    0.05,
        geomagnetic 0.03,
        daylight    0.02,
        moon        0.00,  // no weight, informational
    }
    ```
- [x] Task 1.2: <!-- sha:57cb817 --> `SignalMatrix` core methods:
  - `new() -> Self`, `len()`, `is_empty()`
  - `push(&mut self, ts: f64, row: SignalRow)` — append
  - `push_with_meta(&mut self, ts, row, sensor_count, front_detected)` — с метаданными
  - `latest() -> Option<(f64, &SignalRow)>` — last row
  - `to_comfort(&self) -> Option<SignalComfort>` — backward compat
  - `column(name: &str) -> Option<Vec<f64>>` — extract column by name
  - `column_idx(idx: usize) -> Vec<f64>` — extract by index
- [x] Task 1.3: `slice(&self, start_ts: f64, end_ts: f64) -> SignalMatrix` <!-- sha:57cb817 --> — binary search на timestamps
  - `last_hours(h: u32) -> SignalMatrix`
  - `last_days(d: u32) -> SignalMatrix`
- [x] Task 1.4: `Serialize`/`Deserialize` <!-- sha:57cb817 --> derive на `SignalRow`, `SignalMatrix`
- [x] Task 1.5: `pub mod matrix;` <!-- sha:57cb817 --> в `lib.rs`, re-export ключевых типов
- [x] Task 1.6: 16 тестов (62 total) <!-- sha:57cb817 -->: macro generates correct constants, push/latest, slice, column access, empty matrix edge cases

### Verification
- [x] `N_SIGNALS == 11`, `SIGNAL_NAMES[0] == "air"`, `SIGNAL_WEIGHTS` sum ≈ 0.88
- [x] `cargo test --package airq-core` — 46 old + 16 new = 62 pass

## Phase 2: Math Operations (Deltas, Trends, ML Vector)

Аналитика на матрице — по аналогии с video-analyzer `score_all(&Weights)`.

### Tasks
- [ ] Task 2.1: `deltas(&self, window_hours: usize) -> Option<[f64; N_SIGNALS]>`
  - Разница last row vs row at t-window. NaN если window > len.
- [ ] Task 2.2: `trends(&self, window_hours: usize) -> [f64; N_SIGNALS]`
  - OLS slope per column. Clamped [-1, 1].
  - Helper: `fn ols_slope(values: &[f64]) -> f64`
- [ ] Task 2.3: `summary(&self) -> MatrixSummary` — per-column min/max/mean/std
  - Аналог video-analyzer `usable_mean_std()` (исключая missing data)
- [ ] Task 2.4: `to_ml_vector(&self) -> MlVector`
  - `features: [f64; 35]` = [11 current/100] + [11 delta_24h/100] + [11 trend_7d clamped] + [sensor_count/50] + [front_flag]
  - `names: [&str; 35]` — generated from `SIGNAL_NAMES` + suffixes
  - `comfort: f64` — weighted dot product (как video-analyzer `score_all`)
  - `label: &str` — classification
- [ ] Task 2.5: `weighted_score(&self) -> f64` — vectorized dot product `row.scores * SIGNAL_WEIGHTS`
  - Единая точка расчёта comfort score, заменяет `calculate_signal_comfort`
- [ ] Task 2.6: 10+ тестов: deltas, trends (linear data → known slope), summary, ML vector dim=35, weighted score matches old comfort

### Verification
- [ ] `MlVector.features.len() == 35`
- [ ] `weighted_score` даёт те же результаты что `calculate_signal_comfort` (backward compat test)

## Phase 3: Storage (bincode per city)

### Tasks
- [ ] Task 3.1: Добавить `bincode = { version = "2", optional = true }` в Cargo.toml, feature `storage`
- [ ] Task 3.2: `save(path: &Path) -> Result<()>`, `load(path: &Path) -> Result<SignalMatrix>` — bincode
- [ ] Task 3.3: `append_and_save(path, ts, row)` — load → push → save
- [ ] Task 3.4: `compact(&mut self, max_rows: usize)` — trim oldest, keep last N
- [ ] Task 3.5: 5+ тестов: save/load roundtrip, append, compact, corrupt file → error

### Verification
- [ ] 8760 rows roundtrip: data identical, file < 500KB

## Phase 4: WASM Bindings

### Tasks
- [ ] Task 4.1: WASM functions в `wasm` module:
  - `wasm_matrix_push(matrix_json, ts, row_json) -> String` — append + return updated
  - `wasm_matrix_latest(json) -> String` — SignalComfort JSON
  - `wasm_matrix_slice(json, hours) -> String` — sub-matrix
  - `wasm_matrix_ml_vector(json) -> String` — 35-dim vector
  - `wasm_matrix_summary(json) -> String` — per-column stats
  - `wasm_signal_names() -> String` — from macro constant
  - `wasm_signal_weights() -> String` — from macro constant
- [ ] Task 4.2: Rebuild WASM: `wasm-pack build --target web --features wasm --no-default-features`
- [ ] Task 4.3: Update `air-signal/src/lib/airq-core.ts` — TypeScript types for matrix ops
- [ ] Task 4.4: Update `air-signal/src/lib/comfort-index.ts` — use matrix if available

### Verification
- [ ] WASM builds, size < 250KB
- [ ] TypeScript compiles clean

## Phase 5: Docs & Cleanup

### Tasks
- [ ] Task 5.1: Update `airq/CLAUDE.md` — matrix module, storage feature, macro pattern
- [ ] Task 5.2: Update `air-signal/CLAUDE.md` — matrix WASM integration
- [ ] Task 5.3: Migrate existing `signal::SignalComfort` → delegate to matrix `weighted_score`
  - Keep `SignalComfort` struct (backward compat), but internal calc goes through matrix
- [ ] Task 5.4: Remove dead code — old `signal::calculate_signal_comfort`, duplicated weight constants

### Verification
- [ ] CLAUDE.md up to date
- [ ] `cargo clippy` clean
- [ ] All tests pass (46 old + ~27 new)

## Final Verification
- [ ] All acceptance criteria from spec met
- [ ] 70+ tests pass
- [ ] Clippy + cargo check clean
- [ ] WASM build < 250KB
- [ ] Adding new signal column = 1 line in macro + 1 normalize fn
- [ ] Backward compat: existing Air Signal modules work unchanged

## Context Handoff

### Session Intent
Add macro-driven `SignalMatrix` to airq-core: unified time-series + single-point with ML vectors, inspired by video-analyzer's `define_scoring_matrix!` pattern.

### Key Files
- `airq-core/src/matrix.rs` — **NEW** — macro + SignalMatrix + math ops
- `airq-core/src/lib.rs` — add `pub mod matrix;`
- `airq-core/Cargo.toml` — add bincode (optional)
- `air-signal/src/lib/airq-core.ts` — TypeScript matrix bindings

### Decisions Made
1. **Macro-driven** (from video-analyzer) — `define_signal_columns!` generates all infrastructure
2. **Fixed array rows** `[f64; N_SIGNALS]` — cache-friendly, compile-time size
3. **No Polars/ndarray** — overkill for 11 cols, broken WASM
4. **bincode** for storage — optional feature, CLI-only
5. **35-dim ML vector** — 11 current + 11 deltas + 11 trends + 2 meta
6. **Backward compat** — `SignalComfort` stays, delegates to matrix internally

### Risks
- Macro complexity — keep it simple, no proc-macro (declarative `macro_rules!` only)
- JSON matrix transfer to WASM — send slices, not full year
- Adding column later = recompile everything (N_SIGNALS changes) — acceptable tradeoff for type safety

### Prior Art
- `life2film/video-analyzer/crates/va-domain/src/features.rs` — `define_scoring_matrix!` macro
- `life2film/video-analyzer/src/matrix/mod.rs` — matrix ops, `score_all`, `column(name)`

---
_Generated by /plan. Tasks marked [~] in progress and [x] complete by /build._
