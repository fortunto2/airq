//! Application state — shared between UI and background collector.

use airq::db::{City, Db, Event, Reading, Sensor};
use std::path::PathBuf;
use std::sync::Arc;

/// Snapshot of current monitoring data for the ACTIVE CITY.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonitorSnapshot {
    pub active_city: Option<City>,
    pub cities: Vec<City>,
    pub sensors: Vec<SensorWithReading>,
    pub events: Vec<Event>,
    pub reading_count: i64,
    pub sensor_count: i64,
    pub last_poll: Option<i64>,
    pub avg_pm25: Option<f64>,
    pub avg_pm10: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorWithReading {
    pub sensor: Sensor,
    pub latest: Option<Reading>,
}

/// Open the shared database (singleton path).
pub fn open_db() -> anyhow::Result<Arc<Db>> {
    let db_path = default_db_path();
    tracing::info!("Opening database: {}", db_path.display());
    let db = Db::open(&db_path)?;
    Ok(Arc::new(db))
}

pub fn default_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("airq")
        .join("airq.db")
}

/// Build a snapshot filtered by active city name.
/// If city_name is None, returns all data.
pub fn build_snapshot(db: &Db, city_name: Option<&str>) -> MonitorSnapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let all_cities = db.all_cities().unwrap_or_default();

    // Find active city
    let active_city = city_name
        .and_then(|name| all_cities.iter().find(|c| c.name == name))
        .cloned();

    // Get sensors for active city (filtered by radius) or all sensors
    let sensors = if let Some(ref city) = active_city {
        db.sensors_for_city(city.id).unwrap_or_default()
    } else {
        db.all_sensors().unwrap_or_default()
    };

    // Get latest reading per sensor (last 10 min)
    let from = now - 600;
    let mut sensors_with_readings = Vec::new();
    let mut total_pm25 = 0.0;
    let mut total_pm10 = 0.0;
    let mut pm_count = 0;

    for s in sensors {
        let readings = db.query_readings(s.id, from, now).unwrap_or_default();
        let latest = readings.last().cloned();

        if let Some(ref r) = latest {
            if let Some(pm25) = r.pm25 {
                total_pm25 += pm25;
                pm_count += 1;
            }
            if let Some(pm10) = r.pm10 {
                total_pm10 += pm10;
            }
        }

        sensors_with_readings.push(SensorWithReading { sensor: s, latest });
    }

    // Events for active city (last 24h) or all cities
    let events_from = now - 86400;
    let mut events = Vec::new();
    if let Some(ref city) = active_city {
        if let Ok(city_events) = db.query_events(city.id, events_from) {
            events.extend(city_events);
        }
    } else {
        for city in &all_cities {
            if let Ok(city_events) = db.query_events(city.id, events_from) {
                events.extend(city_events);
            }
        }
    }
    events.sort_by(|a, b| b.ts.cmp(&a.ts));

    let sensor_count = sensors_with_readings.len() as i64;
    let reading_count = db.reading_count().unwrap_or(0);
    let last_poll = db.last_reading_ts().unwrap_or(None);

    MonitorSnapshot {
        active_city,
        cities: all_cities,
        sensors: sensors_with_readings,
        events,
        reading_count,
        sensor_count,
        last_poll,
        avg_pm25: if pm_count > 0 { Some(total_pm25 / pm_count as f64) } else { None },
        avg_pm10: if pm_count > 0 { Some(total_pm10 / pm_count as f64) } else { None },
    }
}
