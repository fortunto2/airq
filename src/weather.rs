//! Weather data per location with grid-based caching.
//!
//! Sensors within ~11km share the same weather grid cell.
//! One API call per cell, cached for 5 minutes.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cached wind data for a grid cell.
#[derive(Debug, Clone)]
pub struct WindPoint {
    pub speed_kmh: f64,
    pub direction_deg: f64,
    pub gusts_kmh: Option<f64>,
}

/// Grid-based weather cache. Key = (lat_grid, lon_grid).
static CACHE: Mutex<Option<WindCache>> = Mutex::new(None);

struct WindCache {
    entries: HashMap<(i32, i32), (Instant, WindPoint)>,
}

const CACHE_TTL: Duration = Duration::from_secs(300); // 5 min
const GRID_RESOLUTION: f64 = 10.0; // 0.1° ≈ 11km

fn grid_key(lat: f64, lon: f64) -> (i32, i32) {
    ((lat * GRID_RESOLUTION) as i32, (lon * GRID_RESOLUTION) as i32)
}

fn grid_center(key: (i32, i32)) -> (f64, f64) {
    (key.0 as f64 / GRID_RESOLUTION, key.1 as f64 / GRID_RESOLUTION)
}

/// Get wind for a specific location (uses grid cache).
pub async fn get_wind(lat: f64, lon: f64) -> Result<WindPoint> {
    let key = grid_key(lat, lon);

    // Check cache
    {
        let mut guard = CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(|| WindCache { entries: HashMap::new() });
        if let Some((ts, point)) = cache.entries.get(&key) {
            if ts.elapsed() < CACHE_TTL {
                return Ok(point.clone());
            }
        }
    }

    // Fetch from Open-Meteo
    let (clat, clon) = grid_center(key);
    let point = fetch_wind_point(clat, clon).await?;

    // Store in cache
    {
        let mut guard = CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(|| WindCache { entries: HashMap::new() });
        cache.entries.insert(key, (Instant::now(), point.clone()));
    }

    Ok(point)
}

/// Fetch wind for multiple sensors at once (batched by grid cell).
/// Returns map: sensor_id → WindPoint.
pub async fn get_wind_batch(sensors: &[(i64, f64, f64)]) -> HashMap<i64, WindPoint> {
    // Group sensors by grid cell
    let mut cells: HashMap<(i32, i32), Vec<i64>> = HashMap::new();
    for &(id, lat, lon) in sensors {
        cells.entry(grid_key(lat, lon)).or_default().push(id);
    }

    let mut result = HashMap::new();

    // Fetch each unique cell
    for (key, sensor_ids) in &cells {
        match get_wind(grid_center(*key).0, grid_center(*key).1).await {
            Ok(point) => {
                for &id in sensor_ids {
                    result.insert(id, point.clone());
                }
            }
            Err(e) => {
                eprintln!("[weather] Wind fetch failed for grid {:?}: {}", key, e);
            }
        }
    }

    result
}

async fn fetch_wind_point(lat: f64, lon: f64) -> Result<WindPoint> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=wind_speed_10m,wind_direction_10m,wind_gusts_10m&timezone=auto",
        lat, lon
    );
    let response = reqwest::get(&url)
        .await
        .context("fetch wind data")?
        .json::<serde_json::Value>()
        .await
        .context("parse wind JSON")?;

    let current = response.get("current");
    let speed = current.and_then(|c| c.get("wind_speed_10m")).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let dir = current.and_then(|c| c.get("wind_direction_10m")).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let gusts = current.and_then(|c| c.get("wind_gusts_10m")).and_then(|v| v.as_f64());

    Ok(WindPoint { speed_kmh: speed, direction_deg: dir, gusts_kmh: gusts })
}

/// Clear expired cache entries.
pub fn cleanup_cache() {
    let mut guard = CACHE.lock().unwrap();
    if let Some(cache) = guard.as_mut() {
        cache.entries.retain(|_, (ts, _)| ts.elapsed() < CACHE_TTL);
    }
}
