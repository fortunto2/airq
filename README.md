# airq

A command-line tool to check air quality (AQI, PM2.5, PM10) from Open-Meteo and Sensor.Community.

## Installation

```bash
cargo install airq
```

## Usage

Check air quality using Open-Meteo (default):

```bash
airq --lat 52.52 --lon 13.41
```

Check air quality using Sensor.Community:

```bash
airq --lat 52.52 --lon 13.41 --provider sensor-community
```

## WHO Air Quality Guidelines

| Pollutant | 24-hour mean | Annual mean |
|-----------|--------------|-------------|
| PM2.5     | 15 µg/m³     | 5 µg/m³     |
| PM10      | 45 µg/m³     | 15 µg/m³    |
| NO2       | 25 µg/m³     | 10 µg/m³    |
| O3        | 100 µg/m³    | -           |
| SO2       | 40 µg/m³     | -           |
| CO        | 4 mg/m³      | -           |

## License

MIT License
