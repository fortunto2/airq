# Implementation Plan: airq serve

**Track ID:** airq-serve_20260316
**Spec:** [spec.md](./spec.md)
**Created:** 2026-03-16
**Status:** [~] In Progress

## Overview

Add `serve` subcommand to airq CLI. Single Rust binary: Axum web server + SQLite storage + collector loop + event detector. No Docker, no separate processes.

## Phase 1: SQLite Storage Layer

Foundation: database, schema, CRUD operations.

### Tasks
- [x] Task 1.1: Add `rusqlite` (bundled feature) + `axum` + `tower-http` to `Cargo.toml`
- [x] Task 1.2: Create `src/db.rs` — SQLite connection pool + schema init
  - `Db::open(path)` — open or create, run migrations
  - `Db::insert_reading(ts, sensor, lat, lon, pm25, pm10, temp, humidity, pressure)`
  - `Db::query_readings(sensor, from_ts, to_ts) -> Vec<Reading>`
  - `Db::insert_event(event)` / `Db::query_events(city_id, from_ts) -> Vec<Event>`
  - `Db::upsert_sensor(id, lat, lon, name, source)`
  - `Db::upsert_city(name, lat, lon, radius) -> city_id`
  - `Db::sensors_for_city(city_id) -> Vec<Sensor>`
  - WAL mode enabled by default
- [x] Task 1.3: Create `src/db.rs` tests — insert/query roundtrip, WAL concurrent, schema migration
- [x] Task 1.4: `Reading` + `Event` + `Sensor` structs in `src/db.rs` with serde

### Verification
- [x] `cargo test` — db roundtrip tests pass (9/9)
- [x] SQLite file created, schema visible via `sqlite3 airq.db .schema`

## Phase 2: Collector + Push Receiver

Data ingestion from Sensor.Community API + local ESP8266 push.

### Tasks
- [x] Task 2.1: Create `src/collector.rs` — poll loop
  - `async fn collect_once(db, city) -> Result<usize>` — fetch nearby sensors, insert readings
  - Uses existing `fetch_sensor_community_nearby` + individual sensor fetch
  - `async fn run_collector(db, cities, interval)` — tokio interval loop
- [x] Task 2.2: Create `src/push.rs` — ESP8266 receiver (Axum handler)
  - `POST /api/push` — parse Sensor.Community JSON format
  - Map `SDS_P1→pm10`, `SDS_P2→pm25`, `BME280_*→temp/humidity/pressure`
  - Insert into db, return 200 OK
  - Compatible with ESP8266 "Send to own API" config
- [x] Task 2.3: Wire collector + push into `src/serve.rs` entry point
  - `async fn run_serve(config) -> Result<()>` — spawn collector task + start Axum server
- [x] Task 2.4: Tests — collector mock, push handler parse, concurrent db writes

### Verification
- [x] `cargo run -- serve --city gazipasha --radius 15` collects data
- [x] `curl -X POST localhost:8080/api/push -d '{"sensordatavalues":[...]}' ` → 200
- [x] Readings appear in SQLite

## Phase 3: REST API + Event Detection

JSON API for data access + real-time event detection on each poll.

### Tasks
- [x] Task 3.1: Create `src/api.rs` — Axum REST routes
  - `GET /api/readings?sensor=X&from=Y&to=Z` — paginated readings
  - `GET /api/sensors?city=X` — sensors for a city
  - `GET /api/events?city=X&from=Y` — detected events
  - `GET /api/status` — uptime, cities, sensor count, last poll time
  - `GET /api/cities` — configured cities
- [x] Task 3.2: Create `src/detector.rs` — event detection loop
  - Runs after each collector poll
  - Maintains `HashMap<u64, DualBaseline>` in memory (per-sensor EWMA)
  - Calls `airq_core::event::detect_event()` per city
  - If event detected → insert into events table
  - Uses `classify_source()` for PM10/PM2.5 ratio classification
- [x] Task 3.3: Tests — API endpoints, event detection integration

### Verification
- [x] `curl localhost:8080/api/readings?sensor=77955&from=0&to=9999999999` → JSON
- [x] `curl localhost:8080/api/status` → JSON with stats
- [x] Events appear in `/api/events` when anomaly detected

## Phase 4: Web Dashboard

Phone-first HTML dashboard served by Axum.

### Tasks
- [x] Task 4.1: Create `src/web.rs` — embedded HTML/CSS/JS dashboard
  - Single-page: Leaflet map + chart + event list + sensor table
  - `GET /` serves HTML (embedded in binary, no separate files)
  - Fetches data from REST API via fetch()
  - Dark theme, mobile-first, minimal JS
  - Chart: last 24h PM2.5 line (Canvas or lightweight chart lib)
  - Map: sensors colored by PM2.5, city radius circle
  - Events: colored badges with source classification + advice
- [x] Task 4.2: Add `serve` subcommand to `src/main.rs`
  - `airq serve --city gazipasha --radius 15 --port 8080`
  - Multiple `--city` flags for multi-city
  - `--db-path` optional (default `~/.local/share/airq/airq.db`)
  - `--interval` poll interval (default 300s)
  - `--push-port` for ESP8266 receiver (default same as --port)
  - Graceful shutdown on Ctrl+C (save baselines)

### Verification
- [x] `airq serve --city gazipasha` → opens http://localhost:8080
- [x] Dashboard works on mobile (phone-first)
- [x] Chart shows real data after first poll cycle

## Phase 5: Docs & Cleanup

### Tasks
- [ ] Task 5.1: Update `CLAUDE.md` — serve command, db schema, architecture
- [ ] Task 5.2: Update `README.md` — serve section with examples
- [ ] Task 5.3: Update `skills/airq/SKILL.md` — serve command docs
- [ ] Task 5.4: `cargo clippy --workspace` clean, all tests pass

### Verification
- [ ] CLAUDE.md documents serve command
- [ ] README has serve quick-start
- [ ] `cargo test --workspace` all pass
- [ ] `cargo clippy --workspace` clean

## Final Verification
- [ ] `airq serve --city gazipasha --radius 15` runs daemon
- [ ] Collects data every 5 min from Sensor.Community
- [ ] ESP8266 push works (POST /api/push)
- [ ] Dashboard shows map + chart + events on phone
- [ ] REST API returns JSON for all endpoints
- [ ] Event detection runs on each poll, saves to events table
- [ ] Graceful shutdown preserves data
- [ ] Single binary, no Docker, no external dependencies

## Context Handoff

### Session Intent
Add `airq serve` — local collector daemon with SQLite, web dashboard, event detection. Replaces BashAir (Django+InfluxDB+Docker) with single Rust binary.

### Key Files
- `src/db.rs` — **NEW** — SQLite storage layer
- `src/collector.rs` — **NEW** — Sensor.Community poll loop
- `src/push.rs` — **NEW** — ESP8266 receiver (POST /api/push)
- `src/api.rs` — **NEW** — REST API routes
- `src/detector.rs` — **NEW** — real-time event detection
- `src/web.rs` — **NEW** — embedded HTML dashboard
- `src/serve.rs` — **NEW** — entry point, wires everything
- `src/main.rs` — add `Serve` variant to Commands enum
- `Cargo.toml` — add axum, rusqlite, tower-http

### Decisions Made
1. **SQLite** (not InfluxDB) — single file, cross-platform, zero-config, good enough for 100M rows
2. **Axum** (not Actix) — tokio-native, lighter, same team as tokio
3. **Embedded HTML** (not SPA) — no build step, no node_modules, works offline
4. **Same binary** (not separate crate) — `airq serve` is a subcommand, not a new project
5. **ESP8266 format** — same JSON as Sensor.Community, one parser for both
6. **No auth** — LAN-first, trust local network. Remote via tunnel (optional)

### Risks
- Sensor.Community API rate limits during aggressive polling (mitigate: poll only changed sensors)
- SQLite concurrent write lock (mitigate: WAL mode, single writer thread)
- Embedded HTML may be hard to maintain (mitigate: keep it minimal, use REST API)
- Event baselines lost on restart (mitigate: serialize to SQLite or separate file)

### Prior Art
- BashAir (`~/startups/old/bashair/`) — Django + InfluxDB, sensor push format, signal model
- airq-core `event.rs` — EWMA, concordance, directional, SourceClassification
- airq-core `merge.rs` — sensor/model weighting

---
_Generated by /plan. Tasks marked [~] in progress and [x] complete by /build._
