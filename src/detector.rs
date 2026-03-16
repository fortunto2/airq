//! Event detection loop: runs after each collector poll.
//!
//! Maintains per-sensor EWMA baselines in memory,
//! calls airq_core::event::detect_event() per city.

use crate::db::{Db, Event};
use airq_core::event::{self, DualBaseline, EventType, SensorReading};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// In-memory baselines per sensor.
pub type Baselines = Arc<Mutex<HashMap<u64, DualBaseline>>>;

pub fn new_baselines() -> Baselines {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Run event detection for a city after a collector poll.
///
/// 1. Query latest readings for city sensors
/// 2. Update EWMA baselines
/// 3. Run detect_event()
/// 4. If event detected → insert into events table
pub async fn detect_for_city(
    db: &Db,
    baselines: &Baselines,
    city_id: i64,
    city_name: &str,
    city_lat: f64,
    city_lon: f64,
) -> anyhow::Result<()> {
    // Get sensors for this city
    let sensors = db.sensors_for_city(city_id)?;
    if sensors.is_empty() {
        return Ok(());
    }

    // Get latest reading per sensor (last 10 minutes)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let from = now - 600;

    let mut readings = Vec::new();
    for s in &sensors {
        let rs = db.query_readings(s.id, from, now)?;
        if let Some(r) = rs.last() {
            readings.push(SensorReading {
                sensor_id: s.id as u64,
                lat: s.lat.unwrap_or(0.0),
                lon: s.lon.unwrap_or(0.0),
                pm25: r.pm25.unwrap_or(0.0),
                pm10: r.pm10.unwrap_or(0.0),
            });
        }
    }

    if readings.is_empty() {
        return Ok(());
    }

    // Update baselines
    let mut bl = baselines.lock().await;
    for r in &readings {
        let entry = bl.entry(r.sensor_id).or_insert_with(DualBaseline::new);
        entry.pm25.update(r.pm25);
        entry.pm10.update(r.pm10);
    }

    // Run detection
    let analysis = event::detect_event(city_lat, city_lon, &readings, &bl, 3.0);

    // Only store if event or widespread
    if analysis.concordance.event_type == EventType::Event
        || analysis.concordance.event_type == EventType::Widespread
    {
        let evt = Event {
            id: None,
            ts: now,
            city_id,
            event_type: format!("{:?}", analysis.concordance.event_type),
            confidence: analysis.confidence,
            pm25: Some(analysis.median_pm25),
            pm10: Some(analysis.median_pm10),
            ratio: Some(analysis.pm10_pm25_ratio),
            direction: analysis.directional.as_ref().map(|d| d.bearing_label.clone()),
            summary: Some(analysis.summary.clone()),
        };
        db.insert_event(&evt)?;
        eprintln!(
            "[detector] {} — {} (confidence {:.0}%)",
            city_name, analysis.summary, analysis.confidence * 100.0
        );
    }

    drop(bl);
    Ok(())
}
