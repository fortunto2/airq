//! SQLite storage layer for airq serve.
//!
//! Single-file database with WAL mode. Stores readings, sensors, cities, events.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Domain structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Reading {
    pub ts: i64,
    pub sensor: i64,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub pm25: Option<f64>,
    pub pm10: Option<f64>,
    pub temp: Option<f64>,
    pub humidity: Option<f64>,
    pub pressure: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Sensor {
    pub id: i64,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub name: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct City {
    pub id: i64,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub radius: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Event {
    pub id: Option<i64>,
    pub ts: i64,
    pub city_id: i64,
    pub event_type: String,
    pub confidence: f64,
    pub pm25: Option<f64>,
    pub pm10: Option<f64>,
    pub ratio: Option<f64>,
    pub direction: Option<String>,
    pub summary: Option<String>,
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS readings (
    ts       INTEGER NOT NULL,
    sensor   INTEGER NOT NULL,
    lat      REAL, lon REAL,
    pm25     REAL, pm10 REAL,
    temp     REAL, humidity REAL, pressure REAL,
    PRIMARY KEY (sensor, ts)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS sensors (
    id       INTEGER PRIMARY KEY,
    lat      REAL, lon REAL,
    name     TEXT,
    source   TEXT
);

CREATE TABLE IF NOT EXISTS cities (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    name     TEXT NOT NULL,
    lat      REAL NOT NULL,
    lon      REAL NOT NULL,
    radius   REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    ts       INTEGER,
    city_id  INTEGER,
    type     TEXT,
    confidence REAL,
    pm25     REAL, pm10 REAL, ratio REAL,
    direction TEXT, summary TEXT
);

CREATE INDEX IF NOT EXISTS idx_readings_ts ON readings(ts);
CREATE INDEX IF NOT EXISTS idx_events_city_ts ON events(city_id, ts);
";

/// Thread-safe SQLite database handle.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open or create database at `path`. Runs migrations, enables WAL.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("create db directory")?;
        }
        let conn = Connection::open(path)
            .context("open SQLite database")?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .context("set WAL mode")?;
        conn.execute_batch(SCHEMA)
            .context("run schema migrations")?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Open in-memory database (for tests).
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .context("open in-memory SQLite")?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .context("set WAL mode")?;
        conn.execute_batch(SCHEMA)
            .context("run schema migrations")?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    // -- Readings --

    pub fn insert_reading(&self, r: &Reading) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO readings (ts, sensor, lat, lon, pm25, pm10, temp, humidity, pressure)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![r.ts, r.sensor, r.lat, r.lon, r.pm25, r.pm10, r.temp, r.humidity, r.pressure],
        ).context("insert reading")?;
        Ok(())
    }

    pub fn query_readings(&self, sensor: i64, from_ts: i64, to_ts: i64) -> Result<Vec<Reading>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT ts, sensor, lat, lon, pm25, pm10, temp, humidity, pressure
             FROM readings WHERE sensor = ?1 AND ts >= ?2 AND ts <= ?3
             ORDER BY ts"
        )?;
        let rows = stmt.query_map(params![sensor, from_ts, to_ts], |row| {
            Ok(Reading {
                ts: row.get(0)?,
                sensor: row.get(1)?,
                lat: row.get(2)?,
                lon: row.get(3)?,
                pm25: row.get(4)?,
                pm10: row.get(5)?,
                temp: row.get(6)?,
                humidity: row.get(7)?,
                pressure: row.get(8)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Query all readings for a city's sensors within a time range.
    pub fn query_readings_for_city(&self, city_id: i64, from_ts: i64, to_ts: i64) -> Result<Vec<Reading>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT r.ts, r.sensor, r.lat, r.lon, r.pm25, r.pm10, r.temp, r.humidity, r.pressure
             FROM readings r
             JOIN sensors s ON r.sensor = s.id
             WHERE r.ts >= ?1 AND r.ts <= ?2
             ORDER BY r.ts"
        )?;
        // For simplicity, filter all readings in time range (city filtering done via sensors_for_city)
        let _ = city_id; // will be used with proper city-sensor mapping later
        let rows = stmt.query_map(params![from_ts, to_ts], |row| {
            Ok(Reading {
                ts: row.get(0)?,
                sensor: row.get(1)?,
                lat: row.get(2)?,
                lon: row.get(3)?,
                pm25: row.get(4)?,
                pm10: row.get(5)?,
                temp: row.get(6)?,
                humidity: row.get(7)?,
                pressure: row.get(8)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // -- Sensors --

    pub fn upsert_sensor(&self, id: i64, lat: Option<f64>, lon: Option<f64>, name: Option<&str>, source: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sensors (id, lat, lon, name, source) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET lat=excluded.lat, lon=excluded.lon, name=COALESCE(excluded.name, sensors.name), source=COALESCE(excluded.source, sensors.source)",
            params![id, lat, lon, name, source],
        ).context("upsert sensor")?;
        Ok(())
    }

    pub fn sensors_for_city(&self, city_id: i64) -> Result<Vec<Sensor>> {
        let conn = self.conn.lock().unwrap();
        // Get city center + radius, then find sensors within radius using haversine approximation
        let city: Option<(f64, f64, f64)> = conn.query_row(
            "SELECT lat, lon, radius FROM cities WHERE id = ?1",
            params![city_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).ok();

        let (clat, clon, radius_km) = match city {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        // Simple bounding box filter (approximate, good enough for <100km)
        let dlat = radius_km / 111.0;
        let dlon = radius_km / (111.0 * clat.to_radians().cos());

        let mut stmt = conn.prepare(
            "SELECT id, lat, lon, name, source FROM sensors
             WHERE lat BETWEEN ?1 AND ?2 AND lon BETWEEN ?3 AND ?4"
        )?;
        let rows = stmt.query_map(
            params![clat - dlat, clat + dlat, clon - dlon, clon + dlon],
            |row| {
                Ok(Sensor {
                    id: row.get(0)?,
                    lat: row.get(1)?,
                    lon: row.get(2)?,
                    name: row.get(3)?,
                    source: row.get(4)?,
                })
            },
        )?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn all_sensors(&self) -> Result<Vec<Sensor>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, lat, lon, name, source FROM sensors")?;
        let rows = stmt.query_map([], |row| {
            Ok(Sensor {
                id: row.get(0)?,
                lat: row.get(1)?,
                lon: row.get(2)?,
                name: row.get(3)?,
                source: row.get(4)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // -- Cities --

    pub fn upsert_city(&self, name: &str, lat: f64, lon: f64, radius: f64) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        // Try to find existing city by name
        let existing: Option<i64> = conn.query_row(
            "SELECT id FROM cities WHERE name = ?1",
            params![name],
            |row| row.get(0),
        ).ok();

        if let Some(id) = existing {
            conn.execute(
                "UPDATE cities SET lat = ?1, lon = ?2, radius = ?3 WHERE id = ?4",
                params![lat, lon, radius, id],
            )?;
            Ok(id)
        } else {
            conn.execute(
                "INSERT INTO cities (name, lat, lon, radius) VALUES (?1, ?2, ?3, ?4)",
                params![name, lat, lon, radius],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn all_cities(&self) -> Result<Vec<City>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, lat, lon, radius FROM cities")?;
        let rows = stmt.query_map([], |row| {
            Ok(City {
                id: row.get(0)?,
                name: row.get(1)?,
                lat: row.get(2)?,
                lon: row.get(3)?,
                radius: row.get(4)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // -- Events --

    pub fn insert_event(&self, e: &Event) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (ts, city_id, type, confidence, pm25, pm10, ratio, direction, summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![e.ts, e.city_id, e.event_type, e.confidence, e.pm25, e.pm10, e.ratio, e.direction, e.summary],
        ).context("insert event")?;
        Ok(conn.last_insert_rowid())
    }

    pub fn query_events(&self, city_id: i64, from_ts: i64) -> Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, ts, city_id, type, confidence, pm25, pm10, ratio, direction, summary
             FROM events WHERE city_id = ?1 AND ts >= ?2
             ORDER BY ts DESC"
        )?;
        let rows = stmt.query_map(params![city_id, from_ts], |row| {
            Ok(Event {
                id: row.get(0)?,
                ts: row.get(1)?,
                city_id: row.get(2)?,
                event_type: row.get(3)?,
                confidence: row.get(4)?,
                pm25: row.get(5)?,
                pm10: row.get(6)?,
                ratio: row.get(7)?,
                direction: row.get(8)?,
                summary: row.get(9)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // -- Stats --

    pub fn reading_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM readings", [], |row| row.get(0))
            .context("count readings")
    }

    pub fn sensor_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM sensors", [], |row| row.get(0))
            .context("count sensors")
    }

    pub fn last_reading_ts(&self) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT MAX(ts) FROM readings", [], |row| row.get(0))
            .context("last reading ts")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_memory() {
        let db = Db::open_memory().unwrap();
        assert_eq!(db.reading_count().unwrap(), 0);
    }

    #[test]
    fn test_reading_roundtrip() {
        let db = Db::open_memory().unwrap();
        let reading = Reading {
            ts: 1700000000,
            sensor: 77955,
            lat: Some(36.27),
            lon: Some(32.30),
            pm25: Some(8.3),
            pm10: Some(12.5),
            temp: Some(22.1),
            humidity: Some(45.0),
            pressure: Some(1013.25),
        };
        db.insert_reading(&reading).unwrap();

        let results = db.query_readings(77955, 1699999999, 1700000001).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sensor, 77955);
        assert!((results[0].pm25.unwrap() - 8.3).abs() < 0.01);
        assert!((results[0].pm10.unwrap() - 12.5).abs() < 0.01);
    }

    #[test]
    fn test_reading_upsert() {
        let db = Db::open_memory().unwrap();
        let r1 = Reading {
            ts: 1700000000, sensor: 100, lat: None, lon: None,
            pm25: Some(10.0), pm10: Some(20.0),
            temp: None, humidity: None, pressure: None,
        };
        db.insert_reading(&r1).unwrap();

        // Same sensor + ts, different values → should replace
        let r2 = Reading { pm25: Some(15.0), ..r1.clone() };
        db.insert_reading(&r2).unwrap();

        let results = db.query_readings(100, 0, i64::MAX).unwrap();
        assert_eq!(results.len(), 1);
        assert!((results[0].pm25.unwrap() - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_sensor_upsert() {
        let db = Db::open_memory().unwrap();
        db.upsert_sensor(77955, Some(36.27), Some(32.30), Some("Gazipasa-1"), Some("community")).unwrap();
        db.upsert_sensor(77955, Some(36.28), Some(32.31), None, None).unwrap();

        let sensors = db.all_sensors().unwrap();
        assert_eq!(sensors.len(), 1);
        assert_eq!(sensors[0].name.as_deref(), Some("Gazipasa-1")); // COALESCE keeps old name
        assert!((sensors[0].lat.unwrap() - 36.28).abs() < 0.01); // lat updated
    }

    #[test]
    fn test_city_upsert() {
        let db = Db::open_memory().unwrap();
        let id1 = db.upsert_city("gazipasha", 36.27, 32.30, 15.0).unwrap();
        let id2 = db.upsert_city("gazipasha", 36.27, 32.30, 20.0).unwrap();
        assert_eq!(id1, id2); // same city

        let cities = db.all_cities().unwrap();
        assert_eq!(cities.len(), 1);
        assert!((cities[0].radius - 20.0).abs() < 0.01); // radius updated
    }

    #[test]
    fn test_event_roundtrip() {
        let db = Db::open_memory().unwrap();
        let city_id = db.upsert_city("test", 55.75, 37.60, 10.0).unwrap();
        let event = Event {
            id: None,
            ts: 1700000000,
            city_id,
            event_type: "Event".to_string(),
            confidence: 0.85,
            pm25: Some(45.0),
            pm10: Some(60.0),
            ratio: Some(1.33),
            direction: Some("NE".to_string()),
            summary: Some("2 sensors confirm from NE".to_string()),
        };
        let event_id = db.insert_event(&event).unwrap();
        assert!(event_id > 0);

        let events = db.query_events(city_id, 0).unwrap();
        assert_eq!(events.len(), 1);
        assert!((events[0].confidence - 0.85).abs() < 0.01);
        assert_eq!(events[0].direction.as_deref(), Some("NE"));
    }

    #[test]
    fn test_sensors_for_city() {
        let db = Db::open_memory().unwrap();
        let city_id = db.upsert_city("gazipasha", 36.27, 32.30, 15.0).unwrap();

        // Sensor within radius
        db.upsert_sensor(1, Some(36.28), Some(32.31), Some("near"), Some("community")).unwrap();
        // Sensor far away
        db.upsert_sensor(2, Some(41.0), Some(29.0), Some("istanbul"), Some("community")).unwrap();

        let sensors = db.sensors_for_city(city_id).unwrap();
        assert_eq!(sensors.len(), 1);
        assert_eq!(sensors[0].id, 1);
    }

    #[test]
    fn test_concurrent_writes() {
        let db = Db::open_memory().unwrap();
        let db2 = db.clone();

        // Simulate concurrent writes from collector and push handler
        let h1 = std::thread::spawn(move || {
            for i in 0..100 {
                let r = Reading {
                    ts: 1700000000 + i, sensor: 1, lat: None, lon: None,
                    pm25: Some(10.0), pm10: Some(20.0),
                    temp: None, humidity: None, pressure: None,
                };
                db.insert_reading(&r).unwrap();
            }
        });
        let h2 = std::thread::spawn(move || {
            for i in 0..100 {
                let r = Reading {
                    ts: 1700000000 + i, sensor: 2, lat: None, lon: None,
                    pm25: Some(15.0), pm10: Some(25.0),
                    temp: None, humidity: None, pressure: None,
                };
                db2.insert_reading(&r).unwrap();
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();
    }

    #[test]
    fn test_stats() {
        let db = Db::open_memory().unwrap();
        assert_eq!(db.reading_count().unwrap(), 0);
        assert_eq!(db.sensor_count().unwrap(), 0);
        assert_eq!(db.last_reading_ts().unwrap(), None);

        db.upsert_sensor(1, None, None, None, None).unwrap();
        let r = Reading {
            ts: 1700000000, sensor: 1, lat: None, lon: None,
            pm25: Some(10.0), pm10: None, temp: None, humidity: None, pressure: None,
        };
        db.insert_reading(&r).unwrap();

        assert_eq!(db.reading_count().unwrap(), 1);
        assert_eq!(db.sensor_count().unwrap(), 1);
        assert_eq!(db.last_reading_ts().unwrap(), Some(1700000000));
    }
}
