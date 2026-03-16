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

        (best_lag, best_corr)
    }

    /// Edge data in the pollution propagation graph.
    #[derive(Debug, Clone)]
    pub struct PropagationEdge {
        pub distance_km: f64,
        pub bearing_deg: f64,
        pub lag_hours: i32,
        pub correlation: f64,
        pub speed_kmh: f64,
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
    pub fn build_graph(
        target_name: &str,
        target_lat: f64,
        target_lon: f64,
        neighbors: Vec<(String, f64, f64, f64, Vec<String>, Vec<Option<f64>>)>,
        target_times: &[String],
        target_pm25: &[Option<f64>],
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
                let (lag, corr) = cross_correlate(pm25_a, pm25_b, 24);

                if corr > 0.5 && lag != 0 {
                    let speed = dist / (lag.unsigned_abs() as f64);
                    let (from, to, actual_bearing) = if lag > 0 {
                        (*node_a, *node_b, brng)
                    } else {
                        (*node_b, *node_a, (brng + 180.0) % 360.0)
                    };

                    let edge = PropagationEdge {
                        distance_km: dist,
                        bearing_deg: actual_bearing,
                        lag_hours: lag.abs(),
                        correlation: corr,
                        speed_kmh: speed,
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
                                lag_hours: lag.abs(),
                                correlation: corr,
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
