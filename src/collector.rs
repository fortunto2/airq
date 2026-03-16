//! Collector: periodic poll of Sensor.Community for nearby sensors.

use crate::db::{Db, Reading};
use crate::detector::Baselines;
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// Fetch nearby sensors and their readings, insert into db.
/// Returns number of readings inserted.
pub async fn collect_once(db: &Db, city_name: &str, lat: f64, lon: f64, radius_km: f64) -> Result<usize> {
    // Fetch all sensor data in the area
    let url = format!(
        "https://data.sensor.community/airrohr/v1/filter/area={},{},{}",
        lat, lon, radius_km
    );
    let client = reqwest::Client::builder()
        .user_agent("airq-serve/1.0")
        .build()
        .context("build http client")?;

    let response: Vec<serde_json::Value> = client
        .get(&url)
        .send()
        .await
        .context("fetch sensor.community area data")?
        .json()
        .await
        .context("parse sensor.community JSON")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let mut count = 0;
    for entry in &response {
        let sensor_id = match entry
            .get("sensor")
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_i64())
        {
            Some(id) => id,
            None => continue,
        };

        let loc = entry.get("location");
        let slat = loc
            .and_then(|l| l.get("latitude"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok());
        let slon = loc
            .and_then(|l| l.get("longitude"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok());

        // Upsert sensor
        let _ = db.upsert_sensor(sensor_id, slat, slon, None, Some("community"));

        // Parse sensor data values
        let values = entry
            .get("sensordatavalues")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();

        let mut pm25 = None;
        let mut pm10 = None;
        let mut temp = None;
        let mut humidity = None;
        let mut pressure = None;

        for v in &values {
            let vtype = v.get("value_type").and_then(|t| t.as_str()).unwrap_or("");
            let val = v
                .get("value")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok());
            match vtype {
                "P2" | "SDS_P2" => pm25 = val,
                "P1" | "SDS_P1" => pm10 = val,
                "BME280_temperature" | "temperature" => temp = val,
                "BME280_humidity" | "humidity" => humidity = val,
                "BME280_pressure" | "pressure" => pressure = val,
                _ => {}
            }
        }

        // Only insert if we have at least PM data
        if pm25.is_some() || pm10.is_some() {
            let reading = Reading {
                ts: now,
                sensor: sensor_id,
                lat: slat,
                lon: slon,
                pm25,
                pm10,
                temp,
                humidity,
                pressure,
            };
            let _ = db.insert_reading(&reading);
            count += 1;
        }
    }

    log(&format!(
        "[collector] {} — {} readings from {} sensors",
        city_name,
        count,
        response.len()
    ));

    Ok(count)
}

/// Run collector loop: poll all cities every `interval`, then run event detection.
pub async fn run_collector(
    db: Arc<Db>,
    cities: Vec<(String, f64, f64, f64)>, // (name, lat, lon, radius_km)
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) {
    let baselines = crate::detector::new_baselines();

    log(&format!(
        "[collector] started — {} cities, interval {}s",
        cities.len(),
        interval.as_secs()
    ));

    // Collect + detect cycle
    let poll = |db: &Arc<Db>, baselines: &Baselines, cities: &[(String, f64, f64, f64)]| {
        let db = db.clone();
        let baselines = baselines.clone();
        let cities: Vec<_> = cities.to_vec();
        async move {
            for (name, lat, lon, radius) in &cities {
                if let Err(e) = collect_once(&db, name, *lat, *lon, *radius).await {
                    log(&format!("[collector] error collecting {}: {}", name, e));
                    continue;
                }
                // Run event detection after each city poll
                let city_id = db.upsert_city(name, *lat, *lon, *radius).unwrap_or(0);
                if let Err(e) = crate::detector::detect_for_city(&db, &baselines, city_id, name, *lat, *lon).await {
                    log(&format!("[detector] error for {}: {}", name, e));
                }
            }
        }
    };

    // Initial poll immediately
    poll(&db, &baselines, &cities).await;

    let mut tick = tokio::time::interval(interval);
    tick.tick().await; // consume first immediate tick

    loop {
        tokio::select! {
            _ = tick.tick() => {
                poll(&db, &baselines, &cities).await;
            }
            _ = shutdown.changed() => {
                log("[collector] shutting down");
                break;
            }
        }
    }
}

fn log(msg: &str) {
    let now = chrono::Local::now().format("%H:%M:%S");
    eprintln!("{} {}", now, msg);
}
