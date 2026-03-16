# Specification: airq serve — Local Collector Daemon

**Track ID:** airq-serve_20260316
**Type:** Feature
**Created:** 2026-03-16
**Status:** Draft

## Summary

`airq serve` превращает любой компьютер в станцию мониторинга воздуха. Один бинарник — collector + web UI + event detector + REST API. Собирает данные с Sensor.Community (remote) и локальных ESP8266 (push), хранит в SQLite, детектит события в реальном времени, отдаёт через phone-first web dashboard.

Заменяет BashAir (Django + InfluxDB + Docker) одним Rust бинарником без зависимостей.

## Acceptance Criteria

- [ ] `airq serve --city gazipasha --radius 15` запускается и начинает сбор
- [ ] SQLite файл `~/.local/share/airq/airq.db` создаётся автоматически
- [ ] Collector: каждые 5 мин poll Sensor.Community, insert readings
- [ ] POST `/api/push` принимает ESP8266 JSON (Sensor.Community формат)
- [ ] GET `/api/readings?sensor=X&from=Y&to=Z` — JSON с readings
- [ ] GET `/api/events` — список обнаруженных событий
- [ ] GET `/` — web dashboard с картой, графиком, event alerts (phone-first)
- [ ] Event detection: EWMA+concordance на каждом poll cycle, events в БД
- [ ] `airq serve --city moscow --city istanbul` — multi-city одним процессом
- [ ] Ctrl+C — graceful shutdown, данные не теряются

## Dependencies

- `axum` — web framework (async, tokio-native)
- `rusqlite` — SQLite with bundled (zero system deps)
- `tower-http` — CORS, static files
- Existing: `airq-core` (event, merge, matrix), `reqwest`, `tokio`, `serde`

## Out of Scope

- Telegram alerts (Phase 2, separate track)
- Mobile app (use web dashboard)
- Docker (single binary, no container)
- Auth/login (LAN-first, trust local network)
- SSL/HTTPS (reverse proxy or tunnel handles this)

## Technical Notes

### SQLite Schema
```sql
CREATE TABLE readings (
    ts       INTEGER NOT NULL,
    sensor   INTEGER NOT NULL,
    lat      REAL, lon REAL,
    pm25     REAL, pm10 REAL,
    temp     REAL, humidity REAL, pressure REAL,
    PRIMARY KEY (sensor, ts)
) WITHOUT ROWID;

CREATE TABLE sensors (
    id       INTEGER PRIMARY KEY,
    lat      REAL, lon REAL,
    name     TEXT,
    source   TEXT  -- 'community' | 'local' | 'manual'
);

CREATE TABLE cities (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    name     TEXT, lat REAL, lon REAL, radius REAL
);

CREATE TABLE events (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    ts       INTEGER, city_id INTEGER,
    type     TEXT, confidence REAL,
    pm25     REAL, pm10 REAL, ratio REAL,
    direction TEXT, summary TEXT
);
```

### ESP8266 Push Format (from BashAir)
```json
{
  "sensordatavalues": [
    {"value_type": "SDS_P1", "value": "12.5"},
    {"value_type": "SDS_P2", "value": "8.3"},
    {"value_type": "BME280_temperature", "value": "22.1"},
    {"value_type": "BME280_humidity", "value": "45.0"},
    {"value_type": "BME280_pressure", "value": "1013.25"}
  ],
  "esp8266id": "15072310",
  "software_version": "NRZ-2020-133"
}
```

### Reuse from airq-core
- `event::detect_event()` — concordance + directional
- `event::EwmaBaseline` / `DualBaseline` — per-sensor state (kept in memory)
- `event::classify_source()` — PM10/PM2.5 ratio → category
- `merge::merge()` — if also fetching Open-Meteo model
- `front::haversine()` — distance calculations
