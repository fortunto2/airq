//! Application state — shared between UI and background collector.

use airq::db::{City, Db, Event, Reading, Sensor};
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// CityData — live API data for comfort matrix display
// ---------------------------------------------------------------------------

/// Live API data for the active city (fetched on demand).
/// Stores extracted numeric values to avoid PartialEq issues with API structs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CityData {
    /// Comfort score breakdown (6 signals used in calculate_comfort)
    pub comfort_total: u32,
    pub comfort_label: String,
    pub air_score: u32,
    pub temperature_score: u32,
    pub wind_score: u32,
    pub uv_score: u32,
    pub pressure_score: u32,
    pub humidity_score: u32,
    /// Raw values for display
    pub aqi: u32,
    pub temperature_c: Option<f64>,
    pub wind_kmh: Option<f64>,
    pub uv_index: Option<f64>,
    pub pressure_hpa: Option<f64>,
    pub humidity_pct: Option<f64>,
    /// Whether data has been loaded
    pub loaded: bool,
}

/// Fetch live API data and compute comfort for a city.
pub async fn fetch_city_data(lat: f64, lon: f64) -> CityData {
    let air = airq::fetch_open_meteo(lat, lon).await.ok();
    let weather = airq::fetch_weather(lat, lon).await.ok();
    let wind = airq::fetch_wind(lat, lon).await.ok();

    let default_current = airq_core::CurrentData {
        pm2_5: None, pm10: None, carbon_monoxide: None,
        nitrogen_dioxide: None, ozone: None, sulphur_dioxide: None,
        uv_index: None, us_aqi: None, european_aqi: None,
    };
    let default_weather = airq_core::WeatherData {
        pressure_hpa: None, humidity_pct: None, apparent_temp_c: None,
        precipitation_mm: None, cloud_cover_pct: None,
    };
    let default_wind = airq_core::WindData {
        wind_speed_10m: None, wind_direction_10m: None, wind_gusts_10m: None,
    };

    let current = air.as_ref().map(|a| &a.current).unwrap_or(&default_current);
    let w = weather.as_ref().unwrap_or(&default_weather);
    let wi = wind.as_ref().unwrap_or(&default_wind);

    let comfort = airq_core::calculate_comfort(current, w, wi);
    let aqi = airq_core::overall_aqi(current).unwrap_or(0);

    CityData {
        comfort_total: comfort.total,
        comfort_label: comfort.label().to_string(),
        air_score: comfort.air,
        temperature_score: comfort.temperature,
        wind_score: comfort.wind,
        uv_score: comfort.uv,
        pressure_score: comfort.pressure,
        humidity_score: comfort.humidity,
        aqi,
        temperature_c: w.apparent_temp_c,
        wind_kmh: wi.wind_speed_10m,
        uv_index: current.uv_index,
        pressure_hpa: w.pressure_hpa,
        humidity_pct: w.humidity_pct,
        loaded: true,
    }
}

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
    // Count readings only for this city's sensors (not global)
    let reading_count = sensors_with_readings.iter()
        .filter(|sr| sr.latest.is_some())
        .count() as i64;
    let last_poll = sensors_with_readings.iter()
        .filter_map(|sr| sr.latest.as_ref().map(|r| r.ts))
        .max();

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
