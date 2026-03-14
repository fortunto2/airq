# airq

[![Crates.io](https://img.shields.io/crates/v/airq)](https://crates.io/crates/airq)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Check air quality from your terminal. Any city in the world, no API key needed.

Two data providers:
- **Open-Meteo** — global coverage, PM2.5, PM10, CO, NO2 (default)
- **Sensor.Community** — citizen science sensors, real-time PM2.5/PM10

## Install

Homebrew (macOS/Linux):

```bash
brew install fortunto2/tap/airq
```

From crates.io:

```bash
cargo install airq
```

From source:

```bash
git clone https://github.com/fortunto2/airq
cd airq && cargo install --path .
```

## Usage

### By city name (geocoding via Open-Meteo)

```bash
airq --city tokyo
airq --city "new york"
airq --city gazipasa
airq --city berlin
airq --city анталья     # unicode works
```

Any city, town, or village — resolved automatically via Open-Meteo geocoding API:

```
$ airq --city gazipasa
Resolved city: Gazipaşa, Türkiye
Air Quality for Coordinates: 36.3, 32.3
--------------------------------------------------
PM2.5: 12.8 μg/m³    (green = good)
PM10: 16.6 μg/m³     (green = good)
CO: 160 μg/m³        (green = good)
NO2: 2 μg/m³         (green = good)
```

### By coordinates

```bash
airq --lat 55.75 --lon 37.62          # Moscow
airq --lat 36.27 --lon 32.30          # Gazipasa
```

### Sensor.Community (citizen science sensors)

Real-time data from [sensor.community](https://sensor.community) network — 15,000+ sensors worldwide:

```bash
# Use specific sensor by ID
airq --city gazipasa --provider sensor-community --sensor-id 77955

# Find nearby sensors
airq nearby --lat 36.27 --lon 32.30
```

### History (last N days)

```bash
airq history --city gazipasa --days 7
```

```
Gazipaşa, Türkiye — last 7 days
2026-03-07: ██░░░ 7.5 µg/m³ (AQI 31 🟢)
2026-03-08: █░░░░ 3.5 µg/m³ (AQI 15 🟢)
2026-03-09: █░░░░ 4.0 µg/m³ (AQI 16 🟢)
...
```

### Top cities by AQI

```bash
airq top --country turkey
airq top --country russia --count 10
airq top --country usa --json
```

```
# City              AQI  PM2.5
1 Bursa             132  🟠 48.1
2 Izmir             111  🟠 39.5
3 Ankara            99   🟡 35.1
4 Istanbul          95   🟡 33.0
5 Gazipasa          27   🟢 6.4
```

Supported countries: turkey, russia, usa, germany, japan.

### JSON output

All commands support `--json` for machine-readable output:

```bash
airq --city berlin --json
airq history --city tokyo --days 7 --json
airq top --country usa --json
```

```json
{
  "latitude": 52.5,
  "longitude": 13.4,
  "current": {
    "pm2_5": 4.8,
    "pm10": 5.9,
    "carbon_monoxide": 192.0,
    "nitrogen_dioxide": 14.9
  }
}
```

## Color coding (WHO thresholds)

Output is color-coded based on WHO Air Quality Guidelines (2021):

| Pollutant | Green (Good) | Yellow (Moderate) | Red (Poor) |
|-----------|-------------|-------------------|------------|
| PM2.5     | ≤ 15 µg/m³ | 15–35 µg/m³       | > 35 µg/m³ |
| PM10      | ≤ 45 µg/m³ | 45–100 µg/m³      | > 100 µg/m³|
| CO        | ≤ 4 mg/m³  | 4–10 mg/m³        | > 10 mg/m³ |
| NO2       | ≤ 25 µg/m³ | 25–50 µg/m³       | > 50 µg/m³ |

## Data sources

- [Open-Meteo Air Quality API](https://open-meteo.com/en/docs/air-quality-api) — free, no key, global coverage
- [Open-Meteo Geocoding API](https://open-meteo.com/en/docs/geocoding-api) — city name → coordinates
- [Sensor.Community](https://sensor.community/) — citizen science, real-time sensors

## Built with

This project was created by [rust-code](https://github.com/fortunto2/rust-code) AI agent in autonomous BigHead mode — from `cargo init` to `cargo publish` without human edits.

## License

MIT
