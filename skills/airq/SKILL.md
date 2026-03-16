---
name: airq
description: Check air quality, AQI, PM2.5, PM10, pollution levels for any city from the terminal using airq CLI. Installs, configures, and runs air quality queries. Use when the user asks about air quality, pollution, AQI scores, or wants to monitor air in their city.
---

# airq — CLI Air Quality Checker

Check air quality for any city from the terminal. Sensors primary, model as reference. Event detection, pollution fronts, source attribution. No API keys needed.

## Installation

First check if `airq` is already installed:

```bash
airq --version
```

If not installed, detect the user's platform:

### macOS (Homebrew)
```bash
brew tap fortunto2/tap && brew install airq
```

### Linux (prebuilt binary)
```bash
curl -LO https://github.com/fortunto2/airq/releases/latest/download/airq-linux-x86_64.tar.gz
tar xzf airq-linux-x86_64.tar.gz
sudo mv airq /usr/local/bin/
```

### Any platform (Rust/cargo)
```bash
cargo install airq
```

## Configuration

```bash
airq init --city <city-name>
```

Config: `~/.config/airq/config.toml`

```toml
default_city = "berlin"
cities = ["berlin", "tokyo", "istanbul"]
```

## Commands

### Current air quality
```bash
airq                              # default city
airq --city tokyo                 # any city
airq --city tokyo --full          # + pollen, earthquakes, geomagnetic
airq --lat 55.75 --lon 37.62     # coordinates
```

Output: PM2.5, PM10, CO, NO2, O3, SO2, UV, humidity, pressure, wind, comfort (0-100).

**Data merge:** Sensor.Community (real sensors) primary. Open-Meteo (CAMS model) as fallback. Dynamic weight by divergence — if model differs >5x from sensors, model is ignored.

### Comfort index (14 signals, sigmoid/gaussian)
```bash
airq comfort --city berlin
```

Signals: air, temperature, wind, sea, UV, earthquake, fire, pollen, pressure, geomagnetic, humidity, daylight, noise, moon. All normalized with smooth sigmoid curves.

### History
```bash
airq history --city istanbul --days 7
```

### Rank cities
```bash
airq top --country turkey
airq top --country russia --count 10
```

10,000+ cities built-in.

### Pollution front detection
```bash
airq front --city hamburg --radius 150 --days 3
```

Z-score spikes → cross-correlation → haversine speed/direction. Dual-source: model + sensors.

### Source attribution (blame)
```bash
airq blame --city moscow --radius 20 --days 7
```

CPF (Conditional Probability Function): wind direction × PM2.5 threshold. Auto-discovers factories/plants from OpenStreetMap.

### Event detection
```bash
cargo run --example detect_events
```

Three-layer detection:
1. **EWMA baseline** — adaptive threshold per sensor (α=0.1)
2. **Concordance** — 2+ sensors confirm = event (not noise)
3. **Directional** — anomaly sensors in same wind sector = point source

Dual-channel PM2.5 + PM10. Source classification by ratio:
- ratio >4 → dust/sand storm
- ratio 2.5-4 → construction dust
- ratio <1.5, PM2.5 >55 → smoke/wildfire
- ratio ~1, PM2.5 >35 → combustion/traffic

### HTML/PDF report
```bash
airq report --city hamburg --radius 150 --pdf
```

Leaflet map + heatmap + front arrows + CPF table + source markers.

### JSON output
```bash
airq --city tokyo --json
airq top --country usa --json
```

## Core library (airq-core)

Use in your own project:

```toml
airq-core = "1.3"
```

4 modules:
- `matrix` — SignalMatrix (macro-driven, 14 signals, time-series, ML vector 44-dim)
- `event` — EWMA + concordance + directional event detection
- `merge` — sensor/model dynamic weighting
- `signal` — sigmoid/gaussian normalize functions

WASM-ready: `wasm-pack build --target web --features wasm --no-default-features`

108 tests.

## How it works

Sensor.Community (real sensors, ground truth) → primary. Open-Meteo CAMS model → fallback with dynamic weight. When sources diverge (model says 130, sensors say 7), sensors win.
