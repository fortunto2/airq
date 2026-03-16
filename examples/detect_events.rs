/// Event detection demo: Open-Meteo grid + concordance analysis.
/// Creates virtual sensor grid around city, fetches AQ data, detects anomalies.
use airq_core::event::*;
use airq_core::front;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cities = vec![
        ("Hamburg", 53.55, 10.0),
        ("Moscow", 55.76, 37.62),
        ("Berlin", 52.52, 13.41),
        ("Istanbul", 41.01, 28.98),
        ("Gazipasha", 36.27, 32.32),
    ];

    for (name, lat, lon) in &cities {
        println!("\n{}", "=".repeat(60));
        println!("  {} ({}, {})", name, lat, lon);
        println!("{}", "=".repeat(60));

        // Create grid of 9 points around city (3x3, ~10km spacing)
        let delta = 0.05; // ~5km
        let mut grid_lats = Vec::new();
        let mut grid_lons = Vec::new();
        for dy in [-1.0, 0.0, 1.0] {
            for dx in [-1.0, 0.0, 1.0] {
                grid_lats.push(lat + dy * delta);
                grid_lons.push(lon + dx * delta);
            }
        }

        let lats_str = grid_lats.iter().map(|l| format!("{:.4}", l)).collect::<Vec<_>>().join(",");
        let lons_str = grid_lons.iter().map(|l| format!("{:.4}", l)).collect::<Vec<_>>().join(",");

        // Fetch PM2.5 for all grid points
        let url = format!(
            "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}&current=pm2_5,pm10",
            lats_str, lons_str
        );

        let response = reqwest::get(&url).await?;
        let data: serde_json::Value = response.json().await?;

        // Parse grid results
        let items = if data.is_array() { data.as_array().unwrap().clone() } else { vec![data] };

        let mut readings = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let pm25 = item["current"]["pm2_5"].as_f64().unwrap_or(0.0);
            let pm10 = item["current"]["pm10"].as_f64().unwrap_or(0.0);
            if pm25 > 0.0 {
                readings.push(SensorReading {
                    sensor_id: (i + 1) as u64,
                    lat: grid_lats[i],
                    lon: grid_lons[i],
                    pm25,
                });
                // Also print PM10
                let dist = front::haversine(*lat, *lon, grid_lats[i], grid_lons[i]);
                let bearing = front::bearing(*lat, *lon, grid_lats[i], grid_lons[i]);
                let dir = front::bearing_label(bearing);
                println!("  Grid #{}: PM2.5={:.1} PM10={:.1} ({:.0}km {}, {:.3},{:.3})",
                    i+1, pm25, pm10, dist, dir, grid_lats[i], grid_lons[i]);
            }
        }

        if readings.len() < 3 {
            println!("  Not enough grid points with data");
            continue;
        }

        // Build baselines from grid median
        let mut pm_vals: Vec<f64> = readings.iter().map(|r| r.pm25).collect();
        pm_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = pm_vals[pm_vals.len() / 2];
        let variance = pm_vals.iter().map(|v| (v - median).powi(2)).sum::<f64>() / pm_vals.len() as f64;

        let baselines: HashMap<u64, EwmaBaseline> = readings.iter()
            .map(|r| (r.sensor_id, EwmaBaseline::with_baseline(median, variance)))
            .collect();

        // Detect event
        let result = detect_event(*lat, *lon, &readings, &baselines, 2.0);

        println!("\n  --- Event Analysis ---");
        println!("  Grid points: {}", readings.len());
        println!("  Median PM2.5: {:.1} ug/m3", result.median_pm25);
        if let Some(am) = result.anomaly_median_pm25 {
            println!("  Anomaly PM2.5: {:.1} ug/m3", am);
        }
        println!("  Concordance: {:.0}% ({}/{})",
            result.concordance.concordance * 100.0,
            result.concordance.anomaly_count,
            result.concordance.total_sensors);
        println!("  Event type: {:?}", result.concordance.event_type);
        if let Some(ref dir) = result.directional {
            println!("  Direction: {} (spread {:.0}deg, directional: {})",
                dir.bearing_label, dir.spread_deg, dir.is_directional);
        }
        println!("  Confidence: {:.0}%", result.confidence * 100.0);
        println!("  >> {}", result.summary);
    }

    Ok(())
}
