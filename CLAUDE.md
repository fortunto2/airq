# CLAUDE.md — airq

CLI air quality checker with comfort index, pollution front detection, source attribution, and PDF reports.

## Workspace

```
airq/                    # CLI binary + network fetching
├── Cargo.toml           # workspace root
├── src/lib.rs           # re-exports airq-core + async fetch_* functions
├── src/main.rs          # CLI (clap), display logic, subcommands
├── airq-core/           # pure calculations, no IO (WASM-ready)
│   ├── Cargo.toml       # features: cli (default), wasm, storage
│   ├── src/lib.rs       # AQI, comfort, fronts, CPF, signal normalize, WASM bindings
│   └── src/matrix.rs    # SignalMatrix: macro-driven time-series + ML vectors (78 tests)
├── skills/airq/         # Agent skill for skills.sh + ClawHub
└── examples/            # Sample reports (Hamburg, Moscow)
```

## Stack
- **airq-core**: serde, petgraph, cities, colored (optional), wasm-bindgen (optional), bincode (optional, storage)
- **airq CLI**: airq-core + clap, reqwest, tokio, clap_complete

## Commands
```bash
cargo build                        # build
cargo test --workspace             # run all 78+ tests
cargo clippy --workspace           # lint
cargo run -- --city tokyo          # basic air quality
cargo run -- --city tokyo --full   # + pollen, earthquakes, Kp
cargo run -- comfort --city berlin # comfort index breakdown
cargo run -- front --city hamburg  # pollution front detection
cargo run -- blame --city moscow   # source attribution (CPF)
cargo run -- report --city hamburg --pdf  # HTML/PDF report
cargo run -- top --country france  # rank cities
cargo run -- completions zsh       # shell completions
cargo install --path .             # install locally
cargo publish -p airq-core         # publish core
cargo publish -p airq              # publish CLI
```

## Key APIs
- Air quality: Open-Meteo Air Quality API (PM2.5, PM10, CO, NO2, O3, SO2, pollen)
- Weather: Open-Meteo Weather API (wind, pressure, humidity, UV, temperature)
- Geocoding: Open-Meteo Geocoding API
- Sensors: Sensor.Community API (realtime) + Archive (historical CSV, cached)
- Earthquakes: USGS GeoJSON API
- Geomagnetic: NOAA SWPC Kp index
- Sources: OpenStreetMap Overpass API (factories, highways, power plants)

## Publishing
- crates.io: `airq` + `airq-core`
- GitHub Releases: Mac + Linux + Windows binaries (CI on tag push)
- Homebrew: `fortunto2/homebrew-tap` (auto-updated by CI)
- ClawHub + skills.sh: agent skill
- Overpass + sensor CSV cached in `~/.cache/airq/`

## SignalMatrix (airq-core/src/matrix.rs)

Macro-driven time-series matrix (pattern from video-analyzer `define_scoring_matrix!`):
- `define_signal_columns!` — single source of truth: column names, weights, indices
- `N_SIGNALS=11`, `SIGNAL_NAMES`, `SIGNAL_WEIGHTS`, `idx::air` etc.
- `SignalRow` — single measurement `[f64; 11]`, `weighted_score()`
- `SignalMatrix` — time-series: push, latest, slice, last_hours/days, compact
- Math: `deltas(window)`, `trends(window)` (OLS slope), `summary()` (min/max/mean/std)
- ML: `to_ml_vector()` → 35-dim vector (11 current + 11 delta + 11 trend + 2 meta)
- Storage: `save(path)`, `load(path)`, `append_and_save()` (bincode, feature `storage`)
- WASM: `wasm_matrix_push/latest/slice/ml_vector/summary`

To add a signal column: 1 line in macro + 1 normalize fn in `signal` module.

```bash
# WASM rebuild
cd airq-core && wasm-pack build --target web --features wasm --no-default-features
# Storage tests
cargo test --package airq-core --features storage
```

## Architecture decisions
- airq-core has no IO — pure functions, testable, WASM-compatible
- Dual-source: Open-Meteo model + Sensor.Community ground sensors merged with weighted confidence
- Front detection: Z-score spikes → cross-correlation with time-lag → haversine speed/direction
- Blame: CPF (Conditional Probability Function) — wind direction × PM2.5 threshold
- Reports: self-contained HTML (Leaflet.js + leaflet.heat + CartoDB tiles), PDF via Chrome headless
