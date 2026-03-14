# CLAUDE.md — airq

CLI air quality checker. Rust, async (tokio + reqwest).

## Stack
- clap (CLI), reqwest (HTTP), serde (JSON), colored (terminal colors), anyhow (errors)
- Open-Meteo API: air quality + geocoding (free, no key)
- Sensor.Community API: citizen science sensors (free, no key)

## Structure
- `src/lib.rs` — data types, fetch functions (Open-Meteo, Sensor.Community, geocoding), WHO thresholds
- `src/main.rs` — CLI (clap), display logic, subcommands

## Commands
```bash
cargo build              # build
cargo test               # run tests
cargo clippy             # lint
cargo run -- --city tokyo   # test
cargo install --path .   # install locally
cargo publish            # publish to crates.io
```

## Key APIs
- Air quality: `https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}&current=pm2_5,pm10,carbon_monoxide,nitrogen_dioxide`
- Geocoding: `https://geocoding-api.open-meteo.com/v1/search?name={city}&count=1`
- Sensor.Community: `https://data.sensor.community/airrohr/v1/sensor/{id}/`
- Nearby sensors: `https://data.sensor.community/airrohr/v1/filter/area={lat},{lon},{radius_km}`
