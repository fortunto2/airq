// Re-export everything from airq-core so main.rs and external users see a flat API.
pub use airq_core::*;

pub mod db;

use anyhow::{Context, Result};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Provider enum (CLI-specific, uses clap)
// ---------------------------------------------------------------------------

#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq)]
pub enum Provider {
    #[default]
    All,
    OpenMeteo,
    SensorCommunity,
}

// ---------------------------------------------------------------------------
// Geocoding (network)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GeocodeResponse {
    results: Option<Vec<GeocodeResult>>,
}

#[derive(Debug, Deserialize)]
struct GeocodeResult {
    latitude: f64,
    longitude: f64,
    name: String,
    country: Option<String>,
}

pub async fn geocode(city: &str) -> Result<(f64, f64, String)> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1",
        city
    );
    let response = reqwest::get(&url)
        .await
        .context("Failed to send request to Geocoding API")?
        .json::<GeocodeResponse>()
        .await
        .context("Failed to parse JSON response")?;

    let result = response
        .results
        .and_then(|mut r| r.pop())
        .context(format!("City not found: {}", city))?;

    let location_name = if let Some(country) = result.country {
        format!("{}, {}", result.name, country)
    } else {
        result.name
    };

    Ok((result.latitude, result.longitude, location_name))
}

// ---------------------------------------------------------------------------
// Sensor.Community internal types (not public)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SensorCommunityResponse {
    sensordatavalues: Vec<SensorDataValue>,
    location: SensorLocation,
}

#[derive(Debug, Deserialize)]
struct SensorDataValue {
    value: String,
    value_type: String,
}

#[derive(Debug, Deserialize)]
struct SensorLocation {
    latitude: String,
    longitude: String,
}

// ---------------------------------------------------------------------------
// Fetch functions (all async, all network)
// ---------------------------------------------------------------------------

pub async fn fetch_wind(lat: f64, lon: f64) -> Result<WindData> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=wind_speed_10m,wind_direction_10m,wind_gusts_10m&timezone=auto",
        lat, lon
    );
    let response = reqwest::get(&url)
        .await
        .context("Failed to fetch wind data")?
        .json::<WindResponse>()
        .await
        .context("Failed to parse wind JSON")?;
    Ok(response.current)
}

pub async fn fetch_open_meteo(lat: f64, lon: f64) -> Result<AirQualityResponse> {
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}&current=pm2_5,pm10,carbon_monoxide,nitrogen_dioxide,ozone,sulphur_dioxide,uv_index,us_aqi,european_aqi&timezone=auto",
        lat, lon
    );

    let response = reqwest::get(&url)
        .await
        .context("Failed to send request to Open-Meteo API")?
        .json::<AirQualityResponse>()
        .await
        .context("Failed to parse JSON response")?;

    Ok(response)
}

pub async fn fetch_sensor_community(sensor_id: u64) -> Result<AirQualityResponse> {
    let url = format!(
        "https://data.sensor.community/airrohr/v1/sensor/{}/",
        sensor_id
    );

    let response = reqwest::get(&url)
        .await
        .context("Failed to send request to Sensor.Community API")?
        .json::<Vec<SensorCommunityResponse>>()
        .await
        .context("Failed to parse JSON response")?;

    let latest = response
        .into_iter()
        .next()
        .context("No data found for sensor")?;

    let mut pm2_5 = None;
    let mut pm10 = None;

    for val in latest.sensordatavalues {
        if val.value_type == "P1" {
            pm10 = val.value.parse::<f64>().ok();
        } else if val.value_type == "P2" {
            pm2_5 = val.value.parse::<f64>().ok();
        }
    }

    let lat = latest.location.latitude.parse::<f64>().unwrap_or(0.0);
    let lon = latest.location.longitude.parse::<f64>().unwrap_or(0.0);

    Ok(AirQualityResponse {
        latitude: lat,
        longitude: lon,
        current: CurrentData {
            pm2_5,
            pm10,
            carbon_monoxide: None,
            nitrogen_dioxide: None,
            ozone: None,
            sulphur_dioxide: None,
            uv_index: None,
            us_aqi: None,
            european_aqi: None,
        },
        current_units: CurrentUnits {
            pm2_5: "\u{00b5}g/m\u{00b3}".to_string(),
            pm10: "\u{00b5}g/m\u{00b3}".to_string(),
            carbon_monoxide: "".to_string(),
            nitrogen_dioxide: "".to_string(),
            ozone: "".to_string(),
            sulphur_dioxide: "".to_string(),
            uv_index: "".to_string(),
        },
    })
}

pub async fn fetch_sensor_community_nearby(
    lat: f64,
    lon: f64,
    radius: f64,
) -> Result<Vec<SensorInfo>> {
    let url = format!(
        "https://data.sensor.community/airrohr/v1/filter/area={},{},{}",
        lat, lon, radius
    );

    let response = reqwest::Client::builder()
        .user_agent("airq/0.5")
        .build()
        .context("client")?
        .get(&url)
        .send()
        .await
        .context("Failed to send request to Sensor.Community API")?
        .json::<Vec<serde_json::Value>>()
        .await
        .context("Failed to parse JSON response")?;

    let mut sensors = std::collections::HashSet::new();
    for item in response {
        if let Some(sensor) = item.get("sensor")
            && let Some(id) = sensor.get("id").and_then(|id| id.as_u64())
        {
            sensors.insert(id);
        }
    }

    Ok(sensors.into_iter().map(|id| SensorInfo { id }).collect())
}

/// Area average from Sensor.Community -- aggregates all sensors within radius.
/// Uses median to filter outliers (broken sensors, indoor sensors).
pub async fn fetch_area_average(lat: f64, lon: f64, radius_km: f64) -> Result<AreaAverage> {
    // sensor.community uses = in path segment, reqwest URL-encodes it.
    // Use Client::get(String) which doesn't re-parse the URL.
    let url = format!(
        "https://data.sensor.community/airrohr/v1/filter/area={},{},{}",
        lat, lon, radius_km
    );
    let text = reqwest::Client::builder()
        .user_agent("airq/0.5")
        .build()
        .context("client")?
        .get(&url)
        .send()
        .await
        .context("Sensor.Community area API failed")?
        .text()
        .await
        .context("Read response")?;
    let response: Vec<serde_json::Value> =
        serde_json::from_str(&text).context("Parse area JSON")?;

    let mut pm25_vals = Vec::new();
    let mut pm10_vals = Vec::new();
    let mut sensors = std::collections::HashSet::new();

    for entry in &response {
        if let Some(sid) = entry.get("sensor").and_then(|s| s.get("id")).and_then(|v| v.as_u64()) {
            sensors.insert(sid);
        }
        for v in entry
            .get("sensordatavalues")
            .and_then(|a| a.as_array())
            .unwrap_or(&Vec::new())
        {
            let vtype = v.get("value_type").and_then(|t| t.as_str()).unwrap_or("");
            let val = v
                .get("value")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok());
            if let Some(val) = val {
                if val > 0.0 && val < 500.0 {
                    // filter obvious outliers
                    match vtype {
                        "P2" => pm25_vals.push(val),
                        "P1" => pm10_vals.push(val),
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(AreaAverage {
        sensor_count: sensors.len(),
        pm2_5_median: median(&mut pm25_vals),
        pm10_median: median(&mut pm10_vals),
        pm2_5_readings: pm25_vals.len(),
        pm10_readings: pm10_vals.len(),
    })
}

pub async fn fetch_history(lat: f64, lon: f64, days: u32) -> Result<HistoryResponse> {
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}&hourly=pm2_5,pm10,us_aqi&past_days={}&forecast_days=0&timezone=auto",
        lat, lon, days
    );

    let response = reqwest::get(&url)
        .await
        .context("Failed to send request to Open-Meteo API")?
        .json::<HistoryResponse>()
        .await
        .context("Failed to parse JSON response")?;

    Ok(response)
}

// ---------------------------------------------------------------------------
// Sensor archive fetch (network + caching)
// ---------------------------------------------------------------------------

/// Cache directory for sensor CSV files.
fn sensor_cache_dir() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".cache"))
        .join("airq")
        .join("sensors")
}

pub async fn fetch_sensor_archive(
    sensor_id: u64,
    days: u32,
) -> Result<Vec<(String, f64)>> {
    let client = reqwest::Client::builder()
        .user_agent("airq/1.0")
        .build()
        .context("client")?;

    let cache_dir = sensor_cache_dir();
    let _ = std::fs::create_dir_all(&cache_dir);

    let mut all_readings: Vec<(String, f64)> = Vec::new();
    let today = chrono_date_now();

    for d in 0..days {
        let date = date_minus_days(&today, d);
        let cache_file = cache_dir.join(format!("{}_sds011_{}.csv", date, sensor_id));

        // Try cache first
        if let Ok(text) = std::fs::read_to_string(&cache_file) {
            parse_sensor_csv(&text, &mut all_readings);
            continue;
        }

        // Fetch from archive
        let url = format!(
            "https://archive.sensor.community/{}/{}_sds011_sensor_{}.csv",
            date, date, sensor_id
        );
        let resp = client.get(&url).send().await;
        if let Ok(r) = resp {
            if r.status().is_success() {
                if let Ok(text) = r.text().await {
                    // Cache if not today (today's data is still updating)
                    if d > 0 {
                        let _ = std::fs::write(&cache_file, &text);
                    }
                    parse_sensor_csv(&text, &mut all_readings);
                }
            }
        }
    }

    // Aggregate to hourly medians
    let hourly = aggregate_sensor_to_hourly(&all_readings);
    Ok(hourly)
}

/// Fetch nearby SDS011 sensor IDs with their locations from Sensor.Community.
pub async fn fetch_nearby_dust_sensors(
    lat: f64,
    lon: f64,
    radius_km: f64,
) -> Result<Vec<(u64, f64, f64)>> {
    let url = format!(
        "https://data.sensor.community/airrohr/v1/filter/area={},{},{}&type=SDS011",
        lat, lon, radius_km
    );
    let response = reqwest::Client::builder()
        .user_agent("airq/1.0")
        .build()
        .context("client")?
        .get(&url)
        .send()
        .await
        .context("Sensor.Community area API")?
        .json::<Vec<serde_json::Value>>()
        .await
        .context("Parse sensor JSON")?;

    let mut sensors = std::collections::HashMap::new();
    for item in &response {
        if let (Some(sid), Some(loc)) = (
            item.get("sensor").and_then(|s| s.get("id")).and_then(|v| v.as_u64()),
            item.get("location"),
        ) {
            let lat = loc.get("latitude").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
            let lon = loc.get("longitude").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok());
            if let (Some(lat), Some(lon)) = (lat, lon) {
                sensors.insert(sid, (lat, lon));
            }
        }
    }

    Ok(sensors.into_iter().map(|(id, (lat, lon))| (id, lat, lon)).collect())
}

// ---------------------------------------------------------------------------
// Wind history (network)
// ---------------------------------------------------------------------------

pub async fn fetch_wind_history(lat: f64, lon: f64, days: u32) -> Result<WindHistoryResponse> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&hourly=wind_speed_10m,wind_direction_10m&past_days={}&forecast_days=0&timezone=auto",
        lat, lon, days
    );
    let response = reqwest::get(&url)
        .await
        .context("Failed to fetch wind history")?
        .json::<WindHistoryResponse>()
        .await
        .context("Failed to parse wind history JSON")?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Pollution sources (network — Overpass / OpenStreetMap)
// ---------------------------------------------------------------------------

/// Cache directory for Overpass API responses.
fn overpass_cache_path(lat: f64, lon: f64, radius_km: f64) -> std::path::PathBuf {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".cache"))
        .join("airq")
        .join("overpass");
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join(format!("{:.2}_{:.2}_{:.0}km.json", lat, lon, radius_km))
}

fn urlencoding(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
            format!("{}", b as char)
        }
        _ => format!("%{:02X}", b),
    }).collect()
}

pub async fn fetch_pollution_sources(lat: f64, lon: f64, radius_km: f64) -> Result<Vec<PollutionSource>> {
    // Check cache first (valid for 7 days)
    let cache_path = overpass_cache_path(lat, lon, radius_km);
    if let Ok(meta) = std::fs::metadata(&cache_path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = modified.elapsed() {
                if age.as_secs() < 7 * 86400 {
                    if let Ok(text) = std::fs::read_to_string(&cache_path) {
                        if let Ok(cached) = serde_json::from_str::<Vec<PollutionSource>>(&text) {
                            return Ok(cached);
                        }
                    }
                }
            }
        }
    }

    let radius_m = (radius_km * 1000.0) as u32;
    let query = format!(
        r#"[out:json][timeout:25];
(
  nwr["power"="plant"](around:{},{},{});
  nwr["man_made"="works"](around:{},{},{});
  nwr["landuse"="industrial"](around:{},{},{});
  way["highway"="motorway"](around:{},{},{});
  way["highway"="trunk"](around:{},{},{});
);
out center tags 50;"#,
        radius_m, lat, lon,
        radius_m, lat, lon,
        radius_m, lat, lon,
        radius_m, lat, lon,
        radius_m, lat, lon,
    );

    let client = reqwest::Client::builder()
        .user_agent("airq/1.1")
        .build()
        .context("client")?;

    let overpass_servers = [
        "https://overpass-api.de/api/interpreter",
        "https://overpass.kumi.systems/api/interpreter",
    ];

    let mut text = String::new();
    for server in &overpass_servers {
        let result = client
            .post(*server)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format!("data={}", urlencoding(&query)))
            .send()
            .await;
        if let Ok(r) = result {
            if let Ok(t) = r.text().await {
                if t.starts_with('{') {
                    text = t;
                    break;
                }
            }
        }
    }
    if text.is_empty() {
        anyhow::bail!("All Overpass API servers busy or unavailable. Try again later.");
    }

    let resp: serde_json::Value = serde_json::from_str(&text)
        .context("Parse Overpass JSON")?;

    let elements = resp
        .get("elements")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    let mut sources: Vec<PollutionSource> = Vec::new();

    for el in &elements {
        let tags = el.get("tags");

        // Determine source type
        let source_type = if tags.and_then(|t| t.get("power")).and_then(|v| v.as_str()) == Some("plant") {
            "power_plant"
        } else if tags.and_then(|t| t.get("man_made")).and_then(|v| v.as_str()) == Some("works") {
            "factory"
        } else if tags.and_then(|t| t.get("landuse")).and_then(|v| v.as_str()) == Some("industrial") {
            "industrial"
        } else if let Some(hw) = tags.and_then(|t| t.get("highway")).and_then(|v| v.as_str()) {
            if hw == "motorway" || hw == "trunk" {
                "highway"
            } else {
                continue;
            }
        } else {
            continue;
        };

        // Get coordinates: node -> lat/lon, way/relation -> center.lat/center.lon
        let (elat, elon) = {
            let el_type = el.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if el_type == "node" {
                let la = el.get("lat").and_then(|v| v.as_f64());
                let lo = el.get("lon").and_then(|v| v.as_f64());
                match (la, lo) {
                    (Some(a), Some(o)) => (a, o),
                    _ => continue,
                }
            } else {
                let center = el.get("center");
                let la = center.and_then(|c| c.get("lat")).and_then(|v| v.as_f64());
                let lo = center.and_then(|c| c.get("lon")).and_then(|v| v.as_f64());
                match (la, lo) {
                    (Some(a), Some(o)) => (a, o),
                    _ => continue,
                }
            }
        };

        // Name from tags
        let name = tags
            .and_then(|t| {
                t.get("name")
                    .or_else(|| t.get("ref"))
                    .and_then(|v| v.as_str())
            })
            .map(|s| s.to_string());

        let distance_km = front::haversine(lat, lon, elat, elon);

        let name = name.unwrap_or_else(|| {
            format!(
                "{} ({:.1}km)",
                match source_type {
                    "power_plant" => "Power plant",
                    "factory" => "Factory",
                    "industrial" => "Industrial zone",
                    "highway" => "Highway segment",
                    _ => "Unknown",
                },
                distance_km
            )
        });

        sources.push(PollutionSource {
            name,
            lat: elat,
            lon: elon,
            source_type: source_type.to_string(),
            distance_km,
        });
    }

    // Deduplicate: if two sources within 1km, keep the one with a name (non-generated)
    sources.sort_by(|a, b| a.distance_km.partial_cmp(&b.distance_km).unwrap());
    let mut deduped: Vec<PollutionSource> = Vec::new();
    for src in sources {
        let dominated = deduped.iter().any(|existing| {
            front::haversine(existing.lat, existing.lon, src.lat, src.lon) < 1.0
                && existing.source_type == src.source_type
        });
        if !dominated {
            deduped.push(src);
        }
    }

    // Limit to 30 closest
    deduped.truncate(30);

    // Cache for 7 days
    if let Ok(json) = serde_json::to_string_pretty(&deduped) {
        let _ = std::fs::write(&cache_path, json);
    }

    Ok(deduped)
}

// ---------------------------------------------------------------------------
// Weather data fetch (network)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct WeatherResponse {
    current: WeatherCurrentRaw,
}

#[derive(Debug, Deserialize)]
struct WeatherCurrentRaw {
    surface_pressure: Option<f64>,
    relative_humidity_2m: Option<f64>,
    apparent_temperature: Option<f64>,
    precipitation: Option<f64>,
    cloud_cover: Option<f64>,
}

pub async fn fetch_weather(lat: f64, lon: f64) -> Result<WeatherData> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=surface_pressure,relative_humidity_2m,apparent_temperature,precipitation,cloud_cover&timezone=auto",
        lat, lon
    );
    let response = reqwest::get(&url)
        .await
        .context("Failed to fetch weather data")?
        .json::<WeatherResponse>()
        .await
        .context("Failed to parse weather JSON")?;
    Ok(WeatherData {
        pressure_hpa: response.current.surface_pressure,
        humidity_pct: response.current.relative_humidity_2m,
        apparent_temp_c: response.current.apparent_temperature,
        precipitation_mm: response.current.precipitation,
        cloud_cover_pct: response.current.cloud_cover,
    })
}

// ---------------------------------------------------------------------------
// Pollen data fetch (network)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PollenResponse {
    current: PollenCurrentRaw,
}

#[derive(Debug, Deserialize)]
struct PollenCurrentRaw {
    grass_pollen: Option<f64>,
    birch_pollen: Option<f64>,
    alder_pollen: Option<f64>,
    ragweed_pollen: Option<f64>,
}

pub async fn fetch_pollen(lat: f64, lon: f64) -> Result<PollenData> {
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}&current=alder_pollen,birch_pollen,grass_pollen,ragweed_pollen&timezone=auto",
        lat, lon
    );
    let response = reqwest::get(&url)
        .await
        .context("Failed to fetch pollen data")?
        .json::<PollenResponse>()
        .await
        .context("Failed to parse pollen JSON")?;
    Ok(PollenData {
        grass_pollen: response.current.grass_pollen,
        birch_pollen: response.current.birch_pollen,
        alder_pollen: response.current.alder_pollen,
        ragweed_pollen: response.current.ragweed_pollen,
    })
}

// ---------------------------------------------------------------------------
// Earthquake data fetch (network)
// ---------------------------------------------------------------------------

pub async fn fetch_nearby_earthquakes(
    lat: f64,
    lon: f64,
    radius_km: f64,
    days: u32,
) -> Result<Vec<EarthquakeInfo>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let start_epoch = now - (days as u64 * 86400);
    let start_date = epoch_days_to_date(start_epoch / 86400);

    let url = format!(
        "https://earthquake.usgs.gov/fdsnws/event/1/query?format=geojson&latitude={}&longitude={}&maxradiuskm={}&starttime={}&minmagnitude=3",
        lat, lon, radius_km, start_date
    );
    let response = reqwest::get(&url)
        .await
        .context("Failed to fetch earthquake data")?
        .json::<serde_json::Value>()
        .await
        .context("Failed to parse earthquake JSON")?;

    let features = response
        .get("features")
        .and_then(|f| f.as_array())
        .cloned()
        .unwrap_or_default();

    let mut quakes: Vec<EarthquakeInfo> = Vec::new();
    for feature in &features {
        let props = match feature.get("properties") {
            Some(p) => p,
            None => continue,
        };
        let geom = match feature.get("geometry") {
            Some(g) => g,
            None => continue,
        };
        let coords = geom.get("coordinates").and_then(|c| c.as_array());
        let (qlon, qlat) = match coords {
            Some(c) if c.len() >= 2 => {
                let lo = c[0].as_f64().unwrap_or(0.0);
                let la = c[1].as_f64().unwrap_or(0.0);
                (lo, la)
            }
            _ => continue,
        };

        let magnitude = props.get("mag").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let place = props
            .get("place")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let time_ms = props.get("time").and_then(|v| v.as_u64()).unwrap_or(0);
        let time_secs = time_ms / 1000;
        let time_str = epoch_days_to_date(time_secs / 86400);
        let distance_km = front::haversine(lat, lon, qlat, qlon);

        quakes.push(EarthquakeInfo {
            magnitude,
            place,
            distance_km,
            time: time_str,
        });
    }

    quakes.sort_by(|a, b| b.magnitude.partial_cmp(&a.magnitude).unwrap_or(std::cmp::Ordering::Equal));
    Ok(quakes)
}

// ---------------------------------------------------------------------------
// Geomagnetic Kp index fetch (network)
// ---------------------------------------------------------------------------

pub async fn fetch_geomagnetic() -> Result<GeomagneticData> {
    let url = "https://services.swpc.noaa.gov/products/noaa-planetary-k-index.json";
    let response = reqwest::get(url)
        .await
        .context("Failed to fetch geomagnetic data")?
        .json::<Vec<Vec<String>>>()
        .await
        .context("Failed to parse geomagnetic JSON")?;

    // Last entry is current, format: [time_tag, Kp, a_running, station_count]
    // Skip header row (index 0)
    let last = response
        .last()
        .context("No geomagnetic data")?;
    let kp: f64 = last
        .get(1)
        .context("No Kp value")?
        .parse()
        .context("Invalid Kp value")?;

    Ok(GeomagneticData::from_kp(kp))
}
