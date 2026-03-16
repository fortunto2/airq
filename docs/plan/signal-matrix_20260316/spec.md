# Specification: SignalMatrix — Unified Time-Series + Single-Point Data Structure

**Track ID:** signal-matrix_20260316
**Type:** Feature
**Created:** 2026-03-16
**Status:** Draft

## Summary

Архитектурное расширение `airq-core`: единая матричная структура `SignalMatrix` для хранения, агрегации и анализа экологических данных во времени. Матрица — это struct-of-arrays (SoA) из 11 signal-колонок × N строк (timestamps). Один и тот же объект обслуживает single-point запросы (последняя строка), time-series слайсы (24h/7d/30d), дельты, тренды и ML-ready feature vectors.

Polars отброшен: ломается при компиляции в WASM, бинарник 3-10MB. Parquet отброшен: +456KB минимум, overkill для 11×8760. Выбрано: чистый `Vec<f64>` + `serde` + `bincode` (для файлового формата). +0KB к WASM, cache-friendly SoA layout.

## Acceptance Criteria

- [ ] `SignalMatrix` хранит N строк × 11 signal колонок + timestamps + optional sensor_count + optional front_detected
- [ ] `matrix.latest()` → single-point `SignalComfort` (backward compat)
- [ ] `matrix.slice(start, end)` → sub-matrix за период
- [ ] `matrix.deltas(window)` → разница current vs N-hours-ago для каждой колонки
- [ ] `matrix.trends(window)` → линейная регрессия slope per column (растёт/падает/стабильно)
- [ ] `matrix.to_ml_vector()` → `[11 current + 11 deltas_24h + 11 trends_7d + sensor_count + front_flag]` = 36 features
- [ ] Файловое хранение: bincode per city (`{city_slug}.bin`), append-friendly
- [ ] WASM bindings: `wasm_matrix_from_json`, `wasm_matrix_latest`, `wasm_matrix_slice`, `wasm_matrix_ml_vector`
- [ ] Sensor.Community `AreaAverage` интегрирована как доп. колонка
- [ ] Front detection (`front::FrontEvent`) — bool flag в матрице
- [ ] 46 существующих тестов не ломаются + ≥15 новых тестов на matrix ops

## Dependencies

- `bincode` crate (2.x, ~5KB WASM overhead) — бинарная сериализация файлов
- Существующие: `serde`, `serde_json`, `petgraph` (уже в Cargo.toml)
- НЕ нужны: polars, arrow, parquet

## Out of Scope

- Cron/scheduler для автоматического сбора (это в Air Signal Next.js, не в core)
- Network fetch (остаётся в main airq crate)
- Database (Supabase/ClickHouse) — файлы first, DB later
- Visualization/charts — это в Air Signal frontend

## Technical Notes

### Математическая модель (Knuth-style)

Пусть **M** — матрица размерности _T × (C + K)_ где:
- _T_ = количество timestamps (строк)
- _C_ = 11 (signal columns: air, temperature, uv, sea, earthquake, fire, pollen, pressure, geomagnetic, daylight, moon)
- _K_ = meta-columns (sensor_count, front_detected, wind_speed, wind_dir)

Каждый элемент **M[t][c]** ∈ [0, 100] (normalized score) или NaN (missing).

**Single-point:** `M[T-1][*]` — последняя строка.

**Delta:** `Δ(t, w) = M[t] - M[t-w]` для window _w_ hours.

**Trend (OLS slope):**
Для колонки _c_ на окне [t-w, t]:
```
β_c = Σ(x_i - x̄)(y_i - ȳ) / Σ(x_i - x̄)²
```
где x_i = i (hours), y_i = M[t-w+i][c].
β > 0 → improving, β < 0 → degrading.

**ML vector:** `v = [M[T-1], Δ(T-1, 24), β(T-1, 168), sensor_count, front_flag]`
Dim = 11 + 11 + 11 + 1 + 1 = **35 features**, все float64.

### Struct-of-Arrays (SoA) layout

```rust
struct SignalMatrix {
    timestamps: Vec<f64>,      // epoch seconds
    air: Vec<f64>,             // normalized 0-100
    temperature: Vec<f64>,
    uv: Vec<f64>,
    sea: Vec<f64>,
    earthquake: Vec<f64>,
    fire: Vec<f64>,
    pollen: Vec<f64>,
    pressure: Vec<f64>,
    geomagnetic: Vec<f64>,
    daylight: Vec<f64>,
    moon: Vec<f64>,
    // meta
    sensor_count: Vec<u32>,
    front_detected: Vec<bool>,
}
```

Почему SoA а не AoS: колоночные операции (mean, delta, trend) сканируют одну `Vec<f64>` — кеш-линия 64 bytes = 8 float64 = 8 часов данных за один cache miss. AoS (`Vec<Row>`) scattered.

### Файловый формат

bincode (v2) — zero-copy десериализация, ~3-5x компактнее JSON.
- 1 год hourly: 11 × 8760 × 8 bytes ≈ 770 KB raw, ~200-300 KB bincode
- Файл per city: `data/{city_slug}.bin`
- Append: deserialize → push row → serialize. Для MVP достаточно.
