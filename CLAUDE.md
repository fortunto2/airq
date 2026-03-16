# CLAUDE.md — airq

CLI air quality checker + Rust core library. Comfort index, event detection, pollution fronts, source attribution.

## Workspace

```
airq/
├── Cargo.toml              # workspace root
├── src/lib.rs              # re-exports airq-core + async fetch_* (reqwest/tokio)
├── src/main.rs             # CLI (clap): city, comfort, front, blame, report, top, history
├── airq-core/              # pure calculations, no IO — WASM-ready
│   ├── Cargo.toml          # features: cli (default), wasm, storage
│   ├── src/lib.rs          # AQI (EPA), sigmoid normalize (14 signals), ComfortScore, fronts, CPF, WASM bindings
│   ├── src/matrix.rs       # SignalMatrix: macro-driven time-series, ML vector, bincode storage
│   ├── src/event.rs        # Event detection: EWMA + concordance + directional (dual PM2.5+PM10)
│   └── src/merge.rs        # Model+sensor dynamic weighting by divergence
├── examples/               # detect_events.rs, reports (Hamburg, Moscow)
└── skills/airq/            # Agent skill for ClawHub
```

## Stack
- **airq-core**: serde, serde_json, petgraph, cities (~40K embedded), bincode (optional), wasm-bindgen (optional), colored (optional)
- **airq CLI**: airq-core + clap, reqwest, tokio, clap_complete

## Commands
```bash
cargo test --workspace               # 101 tests
cargo test --package airq-core --features storage  # + storage tests
cargo clippy --workspace             # lint
cargo run -- --city tokyo            # basic air quality
cargo run -- --city tokyo --full     # + pollen, earthquakes, Kp
cargo run -- comfort --city berlin   # comfort index (14 signals, sigmoid)
cargo run -- front --city hamburg    # pollution front detection
cargo run -- blame --city moscow     # source attribution (CPF)
cargo run -- report --city hamburg --pdf  # HTML/PDF report
cargo run -- top --country france    # rank cities by AQI
cargo run --example detect_events    # live event detection grid

# WASM (for Air Signal web)
cd airq-core && wasm-pack build --target web --features wasm --no-default-features
```

## Core Modules

### matrix.rs — SignalMatrix
Macro-driven: `define_signal_columns!` is single source of truth.
- 14 signals: air, temperature, wind, sea, uv, earthquake, fire, pollen, pressure, geomagnetic, humidity, daylight, noise, moon
- `N_SIGNALS=14`, `SIGNAL_NAMES`, `SIGNAL_WEIGHTS` (sum=1.0), `idx::air` etc.
- `SignalRow` — `[f64; 14]`, `weighted_score()` (dot product)
- `SignalMatrix` — time-series: push, latest, slice, last_hours/days, compact
- Math: `deltas(window)`, `trends(window)` OLS slope, `summary()` min/max/mean/σ
- ML: `to_ml_vector()` → 44-dim (14 current + 14 delta + 14 trend + 2 meta)
- Storage: `save/load/append_and_save` (bincode, feature `storage`)
- To add signal: 1 line in macro + 1 sigmoid fn in `signal` module

### event.rs — Event Detection
- `EwmaBaseline` — per-sensor adaptive threshold (α=0.1, min σ=1.0)
- `DualBaseline` — PM2.5 + PM10 channels. Anomaly = EITHER channel
- `concordance()` — fraction of sensors confirming (Normal → Noise → Event → Widespread)
- `directional_cluster()` — anomaly sensors in same 90° wind sector?
- `detect_event()` → confidence 0-100%, summary, source_hint
- PM10/PM2.5 ratio → source type: >4 dust, >2.5 construction, ~1 combustion/smoke

### merge.rs — Source Merging
- Sensors (Sensor.Community) = ground truth. Model (Open-Meteo CAMS) = fallback.
- Dynamic weight by divergence: div=1→30% model, div=5→~0% model
- Sensor count discount: more sensors → less model weight
- Moscow fix: model=130 sensor=6.7 → divergence 19x → merged=6.7 (not 73)
- `from_sensors()`, `from_model()` convenience constructors

### signal (in lib.rs) — Normalize Functions
All sigmoid/gaussian, no piecewise linear:
- `sigmoid_desc(x, mid, k)` — monotone descending (air, uv, marine, earthquake, ...)
- `sigmoid_asc(x, mid, k)` — monotone ascending (fire distance, daylight)
- `gaussian(x, center, σ)` — bell curve (temperature c=23 σ=12, humidity c=50 σ=25, pressure c=1013 σ=10)
- `cos(2πφ)` — moon phase

## Key APIs (all free)
- Open-Meteo: AQ (CAMS model), weather, marine, pollen, geocoding
- Sensor.Community: real sensors (15K+ worldwide) + archive CSV
- USGS: earthquakes. NOAA SWPC: geomagnetic Kp. OSM Overpass: pollution sources.

## Architecture
- airq-core: pure functions, no IO, WASM-compatible, 101 tests
- Merge: sensors primary, model fallback by divergence
- Front detection: Z-score → cross-correlation → haversine speed/direction
- Blame/CPF: wind direction × PM2.5 threshold, Overpass for sources
- SignalComfort: HashMap<String, u32> (auto-derived from matrix macro, no manual field sync)
