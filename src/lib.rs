use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct AirQualityResponse {
    pub latitude: f64,
    pub longitude: f64,
    pub current: CurrentData,
    pub current_units: CurrentUnits,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CurrentData {
    pub pm2_5: Option<f64>,
    pub pm10: Option<f64>,
    pub carbon_monoxide: Option<f64>,
    pub nitrogen_dioxide: Option<f64>,
    pub ozone: Option<f64>,
    pub sulphur_dioxide: Option<f64>,
    pub uv_index: Option<f64>,
    pub us_aqi: Option<f64>,
    pub european_aqi: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SensorInfo {
    pub id: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CurrentUnits {
    pub pm2_5: String,
    pub pm10: String,
    pub carbon_monoxide: String,
    pub nitrogen_dioxide: String,
    pub ozone: String,
    pub sulphur_dioxide: String,
    pub uv_index: String,
}

// ---------------------------------------------------------------------------
// Wind data (from Open-Meteo Weather API)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize)]
pub struct WindResponse {
    pub current: WindData,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WindData {
    pub wind_speed_10m: Option<f64>,
    pub wind_direction_10m: Option<f64>,
    pub wind_gusts_10m: Option<f64>,
}

impl WindData {
    /// Wind direction in degrees → compass direction string.
    pub fn direction_label(&self) -> Option<&'static str> {
        self.wind_direction_10m.map(|deg| {
            let dirs = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
                        "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];
            let idx = ((deg + 11.25) % 360.0 / 22.5) as usize;
            dirs[idx.min(15)]
        })
    }

    /// Wind direction → arrow emoji.
    pub fn direction_arrow(&self) -> Option<&'static str> {
        self.wind_direction_10m.map(|deg| {
            let arrows = ["↓", "↙", "←", "↖", "↑", "↗", "→", "↘"];
            let idx = ((deg + 22.5) % 360.0 / 45.0) as usize;
            arrows[idx.min(7)]
        })
    }
}

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

#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq)]
pub enum Provider {
    #[default]
    All,
    OpenMeteo,
    SensorCommunity,
}

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

#[derive(Debug, Deserialize)]
pub struct HistoryResponse {
    pub hourly: HourlyData,
}

#[derive(Debug, Deserialize)]
pub struct HourlyData {
    pub time: Vec<String>,
    pub pm2_5: Vec<Option<f64>>,
    pub pm10: Vec<Option<f64>>,
    pub us_aqi: Option<Vec<Option<f64>>>,
}

#[derive(Debug, PartialEq)]
pub struct DailyAverage {
    pub date: String,
    pub pm2_5: Option<f64>,
    pub pm10: Option<f64>,
    pub us_aqi: Option<f64>,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct AppConfig {
    pub default_city: Option<String>,
    pub cities: Option<Vec<String>>,
    pub sensor_id: Option<u64>,
    pub radius: Option<f64>,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = Self::path();
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn path() -> PathBuf {
        dirs::config_dir()
            .map(|p| p.join("airq").join("config.toml"))
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".airq.toml"))
    }
}

// ---------------------------------------------------------------------------
// AQI calculation (US EPA formula)
// ---------------------------------------------------------------------------

/// AQI breakpoints: (C_low, C_high, AQI_low, AQI_high)
const PM25_BREAKPOINTS: &[(f64, f64, u32, u32)] = &[
    (0.0, 12.0, 0, 50),       // Good
    (12.1, 35.4, 51, 100),    // Moderate
    (35.5, 55.4, 101, 150),   // Unhealthy for Sensitive
    (55.5, 150.4, 151, 200),  // Unhealthy
    (150.5, 250.4, 201, 300), // Very Unhealthy
    (250.5, 500.4, 301, 500), // Hazardous
];

const PM10_BREAKPOINTS: &[(f64, f64, u32, u32)] = &[
    (0.0, 54.0, 0, 50),
    (55.0, 154.0, 51, 100),
    (155.0, 254.0, 101, 150),
    (255.0, 354.0, 151, 200),
    (355.0, 424.0, 201, 300),
    (425.0, 604.0, 301, 500),
];

/// Calculate AQI from concentration using EPA linear interpolation.
/// Clamped: negative → 0, beyond max breakpoint → last bracket's AQI_high.
pub fn calculate_aqi(value: f64, breakpoints: &[(f64, f64, u32, u32)]) -> u32 {
    if value <= 0.0 {
        return 0;
    }
    for &(c_low, c_high, aqi_low, aqi_high) in breakpoints {
        if value <= c_high {
            let aqi = ((aqi_high as f64 - aqi_low as f64) / (c_high - c_low))
                * (value - c_low).max(0.0)
                + aqi_low as f64;
            return (aqi.round() as u32).min(500);
        }
    }
    // Beyond max breakpoint — cap at 500
    500
}

/// Calculate PM2.5 AQI.
pub fn pm25_aqi(value: f64) -> u32 {
    calculate_aqi(value, PM25_BREAKPOINTS)
}

/// Calculate PM10 AQI.
pub fn pm10_aqi(value: f64) -> u32 {
    calculate_aqi(value, PM10_BREAKPOINTS)
}

/// Overall AQI = max of individual pollutant AQIs (EPA standard).
pub fn overall_aqi(data: &CurrentData) -> Option<u32> {
    let mut max_aqi = None;
    if let Some(pm25) = data.pm2_5 {
        max_aqi = Some(pm25_aqi(pm25));
    }
    if let Some(pm10) = data.pm10 {
        let aqi = pm10_aqi(pm10);
        max_aqi = Some(max_aqi.map_or(aqi, |m: u32| m.max(aqi)));
    }
    max_aqi
}

// ---------------------------------------------------------------------------
// AQI categories + colors
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum AqiCategory {
    Good,               // 0–50
    Moderate,           // 51–100
    UnhealthySensitive, // 101–150
    Unhealthy,          // 151–200
    VeryUnhealthy,      // 201–300
    Hazardous,          // 301–500
}

impl AqiCategory {
    pub fn from_aqi(aqi: u32) -> Self {
        match aqi {
            0..=50 => Self::Good,
            51..=100 => Self::Moderate,
            101..=150 => Self::UnhealthySensitive,
            151..=200 => Self::Unhealthy,
            201..=300 => Self::VeryUnhealthy,
            _ => Self::Hazardous,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Good => "Good",
            Self::Moderate => "Moderate",
            Self::UnhealthySensitive => "Unhealthy for Sensitive Groups",
            Self::Unhealthy => "Unhealthy",
            Self::VeryUnhealthy => "Very Unhealthy",
            Self::Hazardous => "Hazardous",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Good => "🟢",
            Self::Moderate => "🟡",
            Self::UnhealthySensitive => "🟠",
            Self::Unhealthy => "🔴",
            Self::VeryUnhealthy => "🟣",
            Self::Hazardous => "🟤",
        }
    }

    pub fn colorize(&self, text: &str) -> colored::ColoredString {
        use colored::Colorize;
        match self {
            Self::Good => text.green(),
            Self::Moderate => text.yellow(),
            Self::UnhealthySensitive => text.truecolor(255, 165, 0), // orange
            Self::Unhealthy => text.red(),
            Self::VeryUnhealthy => text.purple(),
            Self::Hazardous => text.truecolor(128, 0, 0), // dark red
        }
    }
}

/// Legacy status functions (now based on AQI)
pub fn get_pm25_status(value: f64) -> AqiCategory {
    AqiCategory::from_aqi(pm25_aqi(value))
}

pub fn get_pm10_status(value: f64) -> AqiCategory {
    AqiCategory::from_aqi(pm10_aqi(value))
}

pub fn get_co_status(value: f64) -> AqiCategory {
    // CO: WHO 24h guideline 4mg/m³ = 4000 µg/m³
    if value <= 4000.0 {
        AqiCategory::Good
    } else if value <= 10000.0 {
        AqiCategory::Moderate
    } else {
        AqiCategory::Unhealthy
    }
}

pub fn get_no2_status(value: f64) -> AqiCategory {
    // NO2: WHO 24h guideline 25 µg/m³
    if value <= 25.0 {
        AqiCategory::Good
    } else if value <= 50.0 {
        AqiCategory::Moderate
    } else {
        AqiCategory::Unhealthy
    }
}

pub fn get_so2_status(value: f64) -> AqiCategory {
    // SO2: WHO 24h guideline 40 µg/m³
    if value <= 40.0 {
        AqiCategory::Good
    } else if value <= 80.0 {
        AqiCategory::Moderate
    } else {
        AqiCategory::Unhealthy
    }
}

pub fn get_o3_status(value: f64) -> AqiCategory {
    // O3: WHO 8h guideline 100 µg/m³
    if value <= 100.0 {
        AqiCategory::Good
    } else if value <= 160.0 {
        AqiCategory::Moderate
    } else {
        AqiCategory::Unhealthy
    }
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
            pm2_5: "µg/m³".to_string(),
            pm10: "µg/m³".to_string(),
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

/// Area average from Sensor.Community — aggregates all sensors within radius.
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
                if val > 0.0 && val < 1000.0 {
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

#[derive(Debug, Serialize)]
pub struct AreaAverage {
    pub sensor_count: usize,
    pub pm2_5_median: Option<f64>,
    pub pm10_median: Option<f64>,
    pub pm2_5_readings: usize,
    pub pm10_readings: usize,
}

fn median(vals: &mut Vec<f64>) -> Option<f64> {
    if vals.is_empty() {
        return None;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = vals.len() / 2;
    if vals.len() % 2 == 0 {
        Some((vals[mid - 1] + vals[mid]) / 2.0)
    } else {
        Some(vals[mid])
    }
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

/// Fetch sensor history from archive.sensor.community CSV.
/// Returns hourly-aggregated PM2.5 values aligned with Open-Meteo timestamps.
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

/// Simple date helpers (avoid chrono dependency).
fn chrono_date_now() -> String {
    // Use system time to get current date
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days_since_epoch = now / 86400;
    epoch_days_to_date(days_since_epoch)
}

fn epoch_days_to_date(days: u64) -> String {
    // Simple Gregorian calendar conversion
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let days_in_months: [i64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0;
    for (i, &dim) in days_in_months.iter().enumerate() {
        if remaining < dim {
            m = i;
            break;
        }
        remaining -= dim;
    }
    format!("{:04}-{:02}-{:02}", y, m + 1, remaining + 1)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn date_minus_days(date: &str, days: u32) -> String {
    // Parse date, subtract days
    let parts: Vec<u64> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return date.to_string();
    }
    let (y, m, d) = (parts[0], parts[1], parts[2]);
    // Convert to epoch days, subtract, convert back
    let mut epoch_days = 0u64;
    for yr in 1970..y {
        epoch_days += if is_leap(yr as i64) { 366 } else { 365 };
    }
    let dims: [u64; 12] = if is_leap(y as i64) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    for i in 0..(m - 1) as usize {
        epoch_days += dims[i];
    }
    epoch_days += d - 1;
    epoch_days_to_date(epoch_days.saturating_sub(days as u64))
}

fn parse_sensor_csv(text: &str, out: &mut Vec<(String, f64)>) {
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split(';').collect();
        // Format: sensor_id;sensor_type;location;lat;lon;timestamp;P1;durP1;ratioP1;P2;...
        if cols.len() >= 10 {
            let timestamp = cols[5]; // e.g. 2026-03-14T00:02:12
            let p2 = cols[9]; // PM2.5
            if let Ok(val) = p2.parse::<f64>() {
                if val > 0.0 && val < 1000.0 {
                    out.push((timestamp.to_string(), val));
                }
            }
        }
    }
}

fn aggregate_sensor_to_hourly(readings: &[(String, f64)]) -> Vec<(String, f64)> {
    let mut hourly: std::collections::BTreeMap<String, Vec<f64>> =
        std::collections::BTreeMap::new();
    for (ts, val) in readings {
        // "2026-03-14T00:02:12" → "2026-03-14T00:00"
        let hour_key = if ts.len() >= 13 {
            format!("{}:00", &ts[..13])
        } else {
            continue;
        };
        hourly.entry(hour_key).or_default().push(*val);
    }

    hourly
        .into_iter()
        .map(|(hour, mut vals)| {
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mid = vals.len() / 2;
            let median = if vals.len() % 2 == 0 && vals.len() >= 2 {
                (vals[mid - 1] + vals[mid]) / 2.0
            } else {
                vals[mid]
            };
            (hour, median)
        })
        .collect()
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

pub fn aggregate_history(hourly: &HourlyData) -> Vec<DailyAverage> {
    let mut daily_map: std::collections::BTreeMap<String, (f64, usize, f64, usize, f64, usize)> =
        std::collections::BTreeMap::new();

    for (i, time) in hourly.time.iter().enumerate() {
        let date = time.split('T').next().unwrap_or(time).to_string();
        let entry = daily_map.entry(date).or_insert((0.0, 0, 0.0, 0, 0.0, 0));

        if let Some(pm25) = hourly.pm2_5.get(i).and_then(|v| *v) {
            entry.0 += pm25;
            entry.1 += 1;
        }
        if let Some(pm10) = hourly.pm10.get(i).and_then(|v| *v) {
            entry.2 += pm10;
            entry.3 += 1;
        }
        if let Some(us_aqi_vec) = &hourly.us_aqi {
            if let Some(us_aqi) = us_aqi_vec.get(i).and_then(|v| *v) {
                entry.4 += us_aqi;
                entry.5 += 1;
            }
        }
    }

    daily_map
        .into_iter()
        .map(
            |(date, (pm25_sum, pm25_count, pm10_sum, pm10_count, us_aqi_sum, us_aqi_count))| DailyAverage {
                date,
                pm2_5: if pm25_count > 0 {
                    Some(pm25_sum / pm25_count as f64)
                } else {
                    None
                },
                pm10: if pm10_count > 0 {
                    Some(pm10_sum / pm10_count as f64)
                } else {
                    None
                },
                us_aqi: if us_aqi_count > 0 {
                    Some(us_aqi_sum / us_aqi_count as f64)
                } else {
                    None
                },
            },
        )
        .collect()
}

/// City with pre-resolved coordinates (from `cities` crate).
#[derive(Debug)]
pub struct CityInfo {
    pub name: &'static str,
    pub country: &'static str,
    pub lat: f64,
    pub lon: f64,
}

/// Get major cities for any country. Returns up to `limit` cities.
/// Country name is case-insensitive, supports common aliases (e.g., "usa" → "United States").
pub fn get_major_cities(country: &str, limit: usize) -> Vec<CityInfo> {
    let normalized = normalize_country(country);
    cities::all()
        .iter()
        .filter(|c| c.country.to_lowercase() == normalized)
        .take(limit)
        .map(|c| CityInfo {
            name: c.city,
            country: c.country,
            lat: c.latitude,
            lon: c.longitude,
        })
        .collect()
}

/// List all available countries.
pub fn list_countries() -> Vec<&'static str> {
    let mut countries: Vec<&str> = cities::all()
        .iter()
        .map(|c| c.country)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    countries.sort();
    countries
}

fn normalize_country(input: &str) -> String {
    match input.to_lowercase().as_str() {
        "usa" | "us" | "united states" | "america" => "united states".into(),
        "uk" | "england" | "britain" | "great britain" => "united kingdom".into(),
        "turkey" | "türkiye" | "turkiye" => "turkey".into(),
        "russia" | "rf" => "russia".into(),
        "south korea" | "korea" => "south korea".into(),
        "uae" | "emirates" => "united arab emirates".into(),
        other => other.into(),
    }
}

// ---------------------------------------------------------------------------
// Front analysis — pollution front detection and tracking
// ---------------------------------------------------------------------------

pub mod front {
    use petgraph::graph::{Graph, NodeIndex};

    /// Haversine distance in km between two points.
    pub fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        let r = 6371.0;
        let dlat = (lat2 - lat1).to_radians();
        let dlon = (lon2 - lon1).to_radians();
        let a = (dlat / 2.0).sin().powi(2)
            + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
        r * 2.0 * a.sqrt().asin()
    }

    /// Bearing in degrees (clockwise from N) from point A to point B.
    pub fn bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        let lat1 = lat1.to_radians();
        let lat2 = lat2.to_radians();
        let dlon = (lon2 - lon1).to_radians();
        let y = dlon.sin() * lat2.cos();
        let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
        (y.atan2(x).to_degrees() + 360.0) % 360.0
    }

    /// Compass label from bearing degrees.
    pub fn bearing_label(deg: f64) -> &'static str {
        let dirs = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
        let idx = ((deg + 22.5) % 360.0 / 45.0) as usize;
        dirs[idx.min(7)]
    }

    /// Arrow from bearing degrees (direction pollution is moving TO).
    pub fn bearing_arrow(deg: f64) -> &'static str {
        let arrows = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"];
        let idx = ((deg + 22.5) % 360.0 / 45.0) as usize;
        arrows[idx.min(7)]
    }

    /// Find cities within `radius_km` of a given point from the cities crate.
    pub fn nearby_cities(lat: f64, lon: f64, radius_km: f64, max: usize) -> Vec<super::CityInfo> {
        let mut nearby: Vec<(f64, super::CityInfo)> = cities::all()
            .iter()
            .filter_map(|c| {
                let d = haversine(lat, lon, c.latitude, c.longitude);
                if d > 5.0 && d <= radius_km {
                    Some((d, super::CityInfo {
                        name: c.city,
                        country: c.country,
                        lat: c.latitude,
                        lon: c.longitude,
                    }))
                } else {
                    None
                }
            })
            .collect();
        nearby.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        nearby.into_iter().take(max).map(|(_, c)| c).collect()
    }

    /// Spike detected in time-series data.
    #[derive(Debug, Clone)]
    pub struct Spike {
        /// Hour index in the time-series
        pub hour: usize,
        /// Timestamp string
        pub time: String,
        /// PM2.5 value at spike
        pub value: f64,
        /// Change from previous hour
        pub delta: f64,
        /// Z-score of the change
        pub z_score: f64,
    }

    /// Detect spikes using Z-score on hourly differences.
    /// Returns spikes where |z| > threshold (default 2.0).
    pub fn detect_spikes(times: &[String], values: &[Option<f64>], threshold: f64) -> Vec<Spike> {
        // Compute hourly differences
        let mut deltas = Vec::new();
        for i in 1..values.len() {
            if let (Some(prev), Some(curr)) = (values[i - 1], values[i]) {
                deltas.push((i, curr - prev, curr));
            }
        }
        if deltas.len() < 3 {
            return Vec::new();
        }

        let mean: f64 = deltas.iter().map(|(_, d, _)| d).sum::<f64>() / deltas.len() as f64;
        let std: f64 = (deltas.iter().map(|(_, d, _)| (d - mean).powi(2)).sum::<f64>()
            / deltas.len() as f64)
            .sqrt();

        if std < 0.1 {
            return Vec::new(); // no variation
        }

        deltas
            .iter()
            .filter_map(|(i, delta, value)| {
                let z = (delta - mean) / std;
                if z > threshold {
                    Some(Spike {
                        hour: *i,
                        time: times.get(*i).cloned().unwrap_or_default(),
                        value: *value,
                        delta: *delta,
                        z_score: z,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Cross-correlation between two time-series with lag search.
    /// Returns (best_lag, correlation) where lag is in hours.
    /// Positive lag means series_a leads series_b.
    pub fn cross_correlate(
        series_a: &[Option<f64>],
        series_b: &[Option<f64>],
        max_lag: i32,
    ) -> (i32, f64) {
        let n = series_a.len().min(series_b.len());
        if n < 6 {
            return (0, 0.0);
        }

        // Extract valid values and compute stats
        let mean_a = series_a.iter().filter_map(|v| *v).sum::<f64>()
            / series_a.iter().filter(|v| v.is_some()).count().max(1) as f64;
        let mean_b = series_b.iter().filter_map(|v| *v).sum::<f64>()
            / series_b.iter().filter(|v| v.is_some()).count().max(1) as f64;

        let std_a = (series_a
            .iter()
            .filter_map(|v| *v)
            .map(|v| (v - mean_a).powi(2))
            .sum::<f64>()
            / series_a.iter().filter(|v| v.is_some()).count().max(1) as f64)
            .sqrt();
        let std_b = (series_b
            .iter()
            .filter_map(|v| *v)
            .map(|v| (v - mean_b).powi(2))
            .sum::<f64>()
            / series_b.iter().filter(|v| v.is_some()).count().max(1) as f64)
            .sqrt();

        if std_a < 0.1 || std_b < 0.1 {
            return (0, 0.0);
        }

        let mut best_lag = 0i32;
        let mut best_corr = f64::NEG_INFINITY;

        for lag in -max_lag..=max_lag {
            let mut sum = 0.0;
            let mut count = 0;

            for i in 0..n {
                let j = i as i32 + lag;
                if j >= 0 && (j as usize) < n {
                    if let (Some(a), Some(b)) = (series_a[i], series_b[j as usize]) {
                        sum += (a - mean_a) * (b - mean_b);
                        count += 1;
                    }
                }
            }

            if count > 3 {
                let corr = sum / (count as f64 * std_a * std_b);
                if corr > best_corr {
                    best_corr = corr;
                    best_lag = lag;
                }
            }
        }

        (best_lag, best_corr.clamp(-1.0, 1.0))
    }

    /// Edge data in the pollution propagation graph.
    #[derive(Debug, Clone)]
    pub struct PropagationEdge {
        pub distance_km: f64,
        pub bearing_deg: f64,
        // Merged values (weighted)
        pub lag_hours: i32,
        pub correlation: f64,
        pub speed_kmh: f64,
        // Per-source (None if source unavailable)
        pub om_lag: Option<i32>,
        pub om_correlation: Option<f64>,
        pub sc_lag: Option<i32>,
        pub sc_correlation: Option<f64>,
        /// Confidence: higher when both sources agree
        pub confidence: f64,
    }

    /// Station node in the graph.
    #[derive(Debug, Clone)]
    pub struct StationNode {
        pub name: String,
        pub lat: f64,
        pub lon: f64,
        pub distance_from_target: f64,
    }

    /// Result of front analysis.
    #[derive(Debug)]
    pub struct FrontAnalysis {
        pub graph: Graph<StationNode, PropagationEdge>,
        pub target: NodeIndex,
        pub spikes: Vec<(NodeIndex, Vec<Spike>)>,
        pub fronts: Vec<FrontEvent>,
    }

    /// A detected pollution front.
    #[derive(Debug)]
    pub struct FrontEvent {
        pub from_city: String,
        pub to_city: String,
        pub speed_kmh: f64,
        pub bearing_deg: f64,
        pub lag_hours: i32,
        pub correlation: f64,
        pub from_spike_time: String,
        pub to_spike_time: String,
    }

    /// Build propagation graph from target city + nearby cities with history data.
    /// `histories` is Vec of (city_name, lat, lon, distance_km, hourly_times, hourly_pm25).
    /// Sensor.Community hourly data for a node.
    /// Key = hour timestamp ("2026-03-14T10:00"), Value = median PM2.5.
    pub type SensorHourlyData = std::collections::BTreeMap<String, f64>;

    /// Build propagation graph with dual-source analysis.
    /// `sensor_data`: optional map of node_name → hourly sensor readings.
    pub fn build_graph(
        target_name: &str,
        target_lat: f64,
        target_lon: f64,
        neighbors: Vec<(String, f64, f64, f64, Vec<String>, Vec<Option<f64>>)>,
        target_times: &[String],
        target_pm25: &[Option<f64>],
    ) -> FrontAnalysis {
        build_graph_dual(
            target_name, target_lat, target_lon,
            neighbors, target_times, target_pm25,
            &std::collections::HashMap::new(), // no sensor data
        )
    }

    /// Build graph with optional Sensor.Community data for dual-source edges.
    pub fn build_graph_dual(
        target_name: &str,
        target_lat: f64,
        target_lon: f64,
        neighbors: Vec<(String, f64, f64, f64, Vec<String>, Vec<Option<f64>>)>,
        target_times: &[String],
        target_pm25: &[Option<f64>],
        sensor_data: &std::collections::HashMap<String, SensorHourlyData>,
    ) -> FrontAnalysis {
        let mut graph = Graph::new();

        let target_node = graph.add_node(StationNode {
            name: target_name.to_string(),
            lat: target_lat,
            lon: target_lon,
            distance_from_target: 0.0,
        });

        let mut nodes = vec![(target_node, target_times.to_vec(), target_pm25.to_vec())];
        let mut all_spikes = vec![(target_node, detect_spikes(target_times, target_pm25, 2.0))];

        for (name, lat, lon, dist, times, pm25) in &neighbors {
            let node = graph.add_node(StationNode {
                name: name.clone(),
                lat: *lat,
                lon: *lon,
                distance_from_target: *dist,
            });
            all_spikes.push((node, detect_spikes(times, pm25, 2.0)));
            nodes.push((node, times.clone(), pm25.clone()));
        }

        // Build sensor time-series aligned with Open-Meteo timestamps
        let sc_series: Vec<Option<Vec<Option<f64>>>> = nodes.iter().map(|(node, times, _)| {
            let name = &graph[*node].name;
            sensor_data.get(name).map(|sd| {
                times.iter().map(|t| sd.get(t).copied()).collect()
            })
        }).collect();

        // Pairwise cross-correlation → edges
        let mut fronts = Vec::new();
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let (node_a, _, pm25_a) = &nodes[i];
                let (node_b, _, pm25_b) = &nodes[j];
                let a = &graph[*node_a];
                let b = &graph[*node_b];

                let dist = haversine(a.lat, a.lon, b.lat, b.lon);
                let brng = bearing(a.lat, a.lon, b.lat, b.lon);

                // Open-Meteo correlation
                let (om_lag, om_corr) = cross_correlate(pm25_a, pm25_b, 24);

                // Sensor.Community correlation (if available)
                let (sc_lag, sc_corr) = match (&sc_series[i], &sc_series[j]) {
                    (Some(sa), Some(sb)) => {
                        let (l, c) = cross_correlate(sa, sb, 24);
                        (Some(l), Some(c))
                    }
                    _ => (None, None),
                };

                // Merge: weighted average if both available, otherwise use whichever exists
                let (merged_lag, merged_corr, confidence) = match (sc_lag, sc_corr) {
                    (Some(sl), Some(sc)) if sc > 0.3 && om_corr > 0.3 => {
                        // Both sources available — weight by correlation strength
                        let w_om = om_corr.abs();
                        let w_sc = sc.abs();
                        let total_w = w_om + w_sc;
                        let merged_lag = ((om_lag as f64 * w_om + sl as f64 * w_sc) / total_w).round() as i32;
                        let merged_corr = (om_corr * w_om + sc * w_sc) / total_w;
                        // Confidence bonus if both agree on direction
                        let agree = (om_lag > 0 && sl > 0) || (om_lag < 0 && sl < 0);
                        let conf = if agree {
                            (merged_corr * 1.2).min(1.0)
                        } else {
                            merged_corr * 0.5 // disagree = low confidence
                        };
                        (merged_lag, merged_corr, conf)
                    }
                    _ => {
                        // Only Open-Meteo available
                        (om_lag, om_corr, om_corr * 0.8)
                    }
                };

                if merged_corr > 0.5 && merged_lag != 0 {
                    let speed = dist / (merged_lag.unsigned_abs() as f64);
                    let (from, to, actual_bearing) = if merged_lag > 0 {
                        (*node_a, *node_b, brng)
                    } else {
                        (*node_b, *node_a, (brng + 180.0) % 360.0)
                    };

                    let edge = PropagationEdge {
                        distance_km: dist,
                        bearing_deg: actual_bearing,
                        lag_hours: merged_lag.abs(),
                        correlation: merged_corr,
                        speed_kmh: speed,
                        om_lag: Some(om_lag),
                        om_correlation: Some(om_corr),
                        sc_lag,
                        sc_correlation: sc_corr,
                        confidence,
                    };
                    graph.add_edge(from, to, edge.clone());

                    // Find matching spikes
                    let from_spikes = all_spikes.iter().find(|(n, _)| *n == from);
                    let to_spikes = all_spikes.iter().find(|(n, _)| *n == to);
                    if let (Some((_, fs)), Some((_, ts))) = (from_spikes, to_spikes) {
                        if let (Some(f), Some(t)) = (fs.first(), ts.first()) {
                            fronts.push(FrontEvent {
                                from_city: graph[from].name.clone(),
                                to_city: graph[to].name.clone(),
                                speed_kmh: speed,
                                bearing_deg: actual_bearing,
                                lag_hours: merged_lag.abs(),
                                correlation: merged_corr,
                                from_spike_time: f.time.clone(),
                                to_spike_time: t.time.clone(),
                            });
                        }
                    }
                }
            }
        }

        // Sort fronts by correlation (strongest first)
        fronts.sort_by(|a, b| b.correlation.partial_cmp(&a.correlation).unwrap());

        FrontAnalysis {
            graph,
            target: target_node,
            spikes: all_spikes,
            fronts,
        }
    }

    /// Generate a self-contained HTML report with Leaflet.js map and pollution analysis.
    pub fn generate_report(
        target_name: &str,
        target_lat: f64,
        target_lon: f64,
        analysis: &FrontAnalysis,
        wind: Option<&super::WindData>,
        days: u32,
    ) -> String {
        // Build markers JS
        let mut markers_js = String::new();

        // Target marker (red)
        markers_js.push_str(&format!(
            "L.circleMarker([{}, {}], {{radius: 10, color: '#f44336', fillColor: '#f44336', fillOpacity: 0.8}}).addTo(map).bindPopup('<b>{}</b><br>Target city');\n",
            target_lat, target_lon,
            html_escape(target_name),
        ));

        // Neighbor markers (blue)
        for node_idx in analysis.graph.node_indices() {
            let node = &analysis.graph[node_idx];
            if node.distance_from_target > 0.0 {
                markers_js.push_str(&format!(
                    "L.circleMarker([{}, {}], {{radius: 7, color: '#2196f3', fillColor: '#2196f3', fillOpacity: 0.7}}).addTo(map).bindPopup('<b>{}</b><br>{:.0} km from target');\n",
                    node.lat, node.lon,
                    html_escape(&node.name),
                    node.distance_from_target,
                ));
            }
        }

        // Front polylines with arrow markers
        let mut lines_js = String::new();
        for front in &analysis.fronts {
            if front.correlation < 0.6 {
                continue;
            }
            let from_node = analysis.graph.node_indices()
                .find(|n| analysis.graph[*n].name == front.from_city);
            let to_node = analysis.graph.node_indices()
                .find(|n| analysis.graph[*n].name == front.to_city);
            if let (Some(fi), Some(ti)) = (from_node, to_node) {
                let from = &analysis.graph[fi];
                let to = &analysis.graph[ti];
                let color = if front.correlation > 0.85 {
                    "#00c853"
                } else if front.correlation > 0.70 {
                    "#ffc107"
                } else {
                    "#ff9800"
                };

                // Polyline
                lines_js.push_str(&format!(
                    "L.polyline([[{},{}],[{},{}]], {{color:'{}', weight:3, opacity:0.8}}).addTo(map).bindPopup('{} &rarr; {} | {:.0} km/h | corr {:.0}%');\n",
                    from.lat, from.lon, to.lat, to.lon, color,
                    html_escape(&front.from_city), html_escape(&front.to_city),
                    front.speed_kmh, front.correlation * 100.0,
                ));

                // Arrow head at destination using rotated divIcon
                let brng = front.bearing_deg;
                lines_js.push_str(&format!(
                    "L.marker([{},{}], {{icon: L.divIcon({{className:'arrow-icon', html:'<div style=\"transform:rotate({:.0}deg);font-size:20px;color:{};line-height:1\">&uarr;</div>', iconSize:[20,20], iconAnchor:[10,10]}}) }}).addTo(map);\n",
                    to.lat, to.lon, brng, color,
                ));
            }
        }

        // Wind info
        let wind_html = if let Some(w) = wind {
            let speed = w.wind_speed_10m.map(|s| format!("{:.1} km/h", s)).unwrap_or_else(|| "N/A".into());
            let dir = w.direction_label().unwrap_or("N/A");
            let arrow = w.direction_arrow().unwrap_or("");
            format!("<p><b>Wind:</b> {} {} {}</p>", speed, arrow, dir)
        } else {
            "<p><b>Wind:</b> N/A</p>".to_string()
        };

        // Spikes table rows
        let mut spikes_rows = String::new();
        for (node_idx, spikes) in &analysis.spikes {
            let node = &analysis.graph[*node_idx];
            for spike in spikes.iter().take(5) {
                let aqi = super::pm25_aqi(spike.value);
                let cat = super::AqiCategory::from_aqi(aqi);
                let css_class = match cat {
                    super::AqiCategory::Good => "good",
                    super::AqiCategory::Moderate => "moderate",
                    super::AqiCategory::UnhealthySensitive => "unhealthy-sensitive",
                    super::AqiCategory::Unhealthy => "unhealthy",
                    super::AqiCategory::VeryUnhealthy => "very-unhealthy",
                    super::AqiCategory::Hazardous => "hazardous",
                };
                let time_display = spike.time.replace('T', " ");
                spikes_rows.push_str(&format!(
                    "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{:.1}</td><td>+{:.1}</td><td>{:.1}</td></tr>\n",
                    css_class,
                    html_escape(&node.name),
                    time_display,
                    spike.value,
                    spike.delta,
                    spike.z_score,
                ));
            }
        }
        if spikes_rows.is_empty() {
            spikes_rows = "<tr><td colspan=\"5\">No significant spikes detected</td></tr>".to_string();
        }

        // Fronts table rows (correlation > 70%)
        let mut fronts_rows = String::new();
        for front in analysis.fronts.iter().filter(|f| f.correlation > 0.7) {
            let dir_label = bearing_label(front.bearing_deg);
            fronts_rows.push_str(&format!(
                "<tr><td>{} &rarr; {}</td><td>{:.0}</td><td>{}</td><td>{}</td><td>{:.0}%</td></tr>\n",
                html_escape(&front.from_city),
                html_escape(&front.to_city),
                front.speed_kmh,
                dir_label,
                front.lag_hours,
                front.correlation * 100.0,
            ));
        }
        if fronts_rows.is_empty() {
            fronts_rows = "<tr><td colspan=\"5\">No significant fronts detected</td></tr>".to_string();
        }

        // Compute radius from max neighbor distance
        let max_dist = analysis.graph.node_indices()
            .map(|n| analysis.graph[n].distance_from_target)
            .fold(0.0_f64, f64::max);
        let radius_display = if max_dist > 0.0 { format!("{:.0} km", max_dist) } else { "N/A".to_string() };

        format!(r##"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Air Quality Report — {target_name}</title>
    <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
    <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
    <style>
        body {{ font-family: -apple-system, system-ui, sans-serif; margin: 0; }}
        #map {{ height: 500px; width: 100%; }}
        .map-wrap {{ position: relative; }}
        .info-panel {{ position: absolute; top: 10px; right: 10px; z-index: 1000;
                      background: white; padding: 15px; border-radius: 8px;
                      box-shadow: 0 2px 8px rgba(0,0,0,0.2); max-width: 300px; font-size: 14px; }}
        .info-panel h3 {{ margin: 0 0 8px; }}
        .info-panel p {{ margin: 4px 0; }}
        table {{ width: 100%; border-collapse: collapse; margin: 20px 0; }}
        th, td {{ padding: 8px 12px; text-align: left; border-bottom: 1px solid #eee; }}
        th {{ background: #f5f5f5; font-weight: 600; }}
        .container {{ max-width: 1200px; margin: 0 auto; padding: 20px; }}
        h1, h2 {{ color: #333; }}
        h1 {{ border-bottom: 2px solid #eee; padding-bottom: 10px; }}
        .good td {{ color: #00c853; }}
        .moderate td {{ color: #ffc107; }}
        .unhealthy-sensitive td {{ color: #ff9800; }}
        .unhealthy td {{ color: #f44336; }}
        .very-unhealthy td {{ color: #9c27b0; }}
        .hazardous td {{ color: #795548; }}
        .arrow-icon {{ background: none !important; border: none !important; }}
        .footer {{ color: #999; font-size: 12px; margin-top: 30px; padding-top: 10px; border-top: 1px solid #eee; }}
    </style>
</head>
<body>
    <div class="map-wrap">
        <div id="map"></div>
        <div class="info-panel">
            <h3>{target_name}</h3>
            <p><b>Coords:</b> {target_lat:.4}, {target_lon:.4}</p>
            {wind_html}
            <p><b>Period:</b> {days} days</p>
            <p><b>Radius:</b> {radius_display}</p>
        </div>
    </div>
    <div class="container">
        <h1>Air Quality Report — {target_name}</h1>

        <h2>Spikes</h2>
        <table>
            <thead>
                <tr><th>City</th><th>Time</th><th>PM2.5</th><th>Delta</th><th>Z-score</th></tr>
            </thead>
            <tbody>
                {spikes_rows}
            </tbody>
        </table>

        <h2>Pollution Fronts</h2>
        <table>
            <thead>
                <tr><th>From → To</th><th>Speed (km/h)</th><th>Direction</th><th>Lag (h)</th><th>Correlation</th></tr>
            </thead>
            <tbody>
                {fronts_rows}
            </tbody>
        </table>

        <p class="footer">Generated by airq</p>
    </div>
    <script>
        var map = L.map('map').setView([{target_lat}, {target_lon}], 8);
        L.tileLayer('https://{{s}}.tile.openstreetmap.org/{{z}}/{{x}}/{{y}}.png', {{
            attribution: '&copy; OpenStreetMap contributors'
        }}).addTo(map);

        {markers_js}
        {lines_js}
    </script>
</body>
</html>"##,
            target_name = html_escape(target_name),
            target_lat = target_lat,
            target_lon = target_lon,
            wind_html = wind_html,
            days = days,
            radius_display = radius_display,
            spikes_rows = spikes_rows,
            fronts_rows = fronts_rows,
            markers_js = markers_js,
            lines_js = lines_js,
        )
    }

    /// Minimal HTML escaping for user-provided strings.
    fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;")
         .replace('<', "&lt;")
         .replace('>', "&gt;")
         .replace('"', "&quot;")
         .replace('\'', "&#39;")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- AQI calculation tests ---

    #[test]
    fn test_pm25_aqi_good() {
        assert_eq!(pm25_aqi(0.0), 0);
        assert_eq!(pm25_aqi(12.0), 50);
    }

    #[test]
    fn test_pm25_aqi_moderate() {
        assert_eq!(pm25_aqi(12.1), 51);
        assert_eq!(pm25_aqi(35.4), 100);
    }

    #[test]
    fn test_pm25_aqi_unhealthy() {
        assert_eq!(pm25_aqi(55.5), 151);
        assert!(pm25_aqi(150.4) <= 200);
    }

    #[test]
    fn test_pm25_aqi_beyond_max() {
        assert_eq!(pm25_aqi(999.0), 500);
    }

    #[test]
    fn test_pm10_aqi() {
        assert_eq!(pm10_aqi(0.0), 0);
        assert_eq!(pm10_aqi(54.0), 50);
        assert_eq!(pm10_aqi(55.0), 51);
    }

    #[test]
    fn test_overall_aqi() {
        let data = CurrentData {
            pm2_5: Some(35.4), // AQI 100
            pm10: Some(54.0),  // AQI 50
            carbon_monoxide: None,
            nitrogen_dioxide: None,
            ozone: None,
            sulphur_dioxide: None,
            uv_index: None,
            us_aqi: None,
            european_aqi: None,
        };
        assert_eq!(overall_aqi(&data), Some(100)); // max of 100, 50
    }

    #[test]
    fn test_overall_aqi_none() {
        let data = CurrentData {
            pm2_5: None,
            pm10: None,
            carbon_monoxide: Some(100.0),
            nitrogen_dioxide: None,
            ozone: None,
            sulphur_dioxide: None,
            uv_index: None,
            us_aqi: None,
            european_aqi: None,
        };
        assert_eq!(overall_aqi(&data), None); // no PM data = no AQI
    }

    #[test]
    fn test_aqi_category() {
        assert_eq!(AqiCategory::from_aqi(25), AqiCategory::Good);
        assert_eq!(AqiCategory::from_aqi(75), AqiCategory::Moderate);
        assert_eq!(AqiCategory::from_aqi(125), AqiCategory::UnhealthySensitive);
        assert_eq!(AqiCategory::from_aqi(175), AqiCategory::Unhealthy);
        assert_eq!(AqiCategory::from_aqi(250), AqiCategory::VeryUnhealthy);
        assert_eq!(AqiCategory::from_aqi(400), AqiCategory::Hazardous);
    }

    #[test]
    fn test_aqi_category_labels() {
        assert_eq!(AqiCategory::Good.label(), "Good");
        assert_eq!(AqiCategory::Hazardous.label(), "Hazardous");
    }

    // --- Legacy status tests ---

    #[test]
    fn test_pm25_status() {
        assert!(matches!(get_pm25_status(10.0), AqiCategory::Good));
        assert!(matches!(get_pm25_status(20.0), AqiCategory::Moderate));
        assert!(matches!(
            get_pm25_status(40.0),
            AqiCategory::Unhealthy | AqiCategory::UnhealthySensitive
        ));
    }

    #[test]
    fn test_pm10_status() {
        assert!(matches!(get_pm10_status(30.0), AqiCategory::Good));
        assert!(matches!(get_pm10_status(60.0), AqiCategory::Moderate));
        assert!(matches!(
            get_pm10_status(200.0),
            AqiCategory::UnhealthySensitive
        ));
    }

    #[test]
    fn test_co_status() {
        assert!(matches!(get_co_status(2000.0), AqiCategory::Good));
        assert!(matches!(get_co_status(6000.0), AqiCategory::Moderate));
        assert!(matches!(get_co_status(12000.0), AqiCategory::Unhealthy));
    }

    #[test]
    fn test_no2_status() {
        assert!(matches!(get_no2_status(15.0), AqiCategory::Good));
        assert!(matches!(get_no2_status(35.0), AqiCategory::Moderate));
        assert!(matches!(get_no2_status(60.0), AqiCategory::Unhealthy));
    }

    #[test]
    fn test_json_serialization() {
        let data = AirQualityResponse {
            latitude: 52.52,
            longitude: 13.41,
            current: CurrentData {
                pm2_5: Some(10.0),
                pm10: Some(20.0),
                carbon_monoxide: Some(300.0),
                nitrogen_dioxide: Some(15.0),
                ozone: None,
                sulphur_dioxide: None,
                uv_index: None,
                us_aqi: None,
                european_aqi: None,
            },
            current_units: CurrentUnits {
                pm2_5: "ug/m3".to_string(),
                pm10: "ug/m3".to_string(),
                carbon_monoxide: "ug/m3".to_string(),
                nitrogen_dioxide: "ug/m3".to_string(),
                ozone: "ug/m3".to_string(),
                sulphur_dioxide: "ug/m3".to_string(),
                uv_index: "".to_string(),
            },
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"latitude\":52.52"));
        assert!(json.contains("\"pm2_5\":10.0"));
    }

    #[test]
    fn test_aggregate_history() {
        let hourly = HourlyData {
            time: vec![
                "2026-03-07T00:00".to_string(),
                "2026-03-07T01:00".to_string(),
                "2026-03-08T00:00".to_string(),
            ],
            pm2_5: vec![Some(10.0), Some(20.0), Some(5.0)],
            pm10: vec![Some(15.0), None, Some(10.0)],
            us_aqi: None,
        };

        let daily = aggregate_history(&hourly);
        assert_eq!(daily.len(), 2);

        assert_eq!(daily[0].date, "2026-03-07");
        assert_eq!(daily[0].pm2_5, Some(15.0));
        assert_eq!(daily[0].pm10, Some(15.0));

        assert_eq!(daily[1].date, "2026-03-08");
        assert_eq!(daily[1].pm2_5, Some(5.0));
        assert_eq!(daily[1].pm10, Some(10.0));
    }

    #[test]
    fn test_get_major_cities() {
        let cities = get_major_cities("turkey", 10);
        assert!(!cities.is_empty());
        assert!(cities.iter().any(|c| c.name == "Istanbul"));

        // Case insensitive
        assert!(!get_major_cities("Turkey", 10).is_empty());
        assert!(!get_major_cities("TURKEY", 10).is_empty());

        // Aliases
        assert!(!get_major_cities("usa", 10).is_empty());
        assert!(!get_major_cities("uk", 10).is_empty());

        // Any country works
        assert!(!get_major_cities("france", 10).is_empty());
        assert!(!get_major_cities("brazil", 10).is_empty());
        assert!(!get_major_cities("india", 10).is_empty());

        // Unknown returns empty
        assert!(get_major_cities("zzzzz", 10).is_empty());
    }

    #[test]
    fn test_list_countries() {
        let countries = list_countries();
        assert!(countries.len() > 100);
        assert!(countries.contains(&"France"));
        assert!(countries.contains(&"Japan"));
    }
}
