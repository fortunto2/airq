use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub mod matrix;
pub mod event;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

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
// Wind data types (structs only, no fetch)
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
    /// Wind direction in degrees -> compass direction string.
    pub fn direction_label(&self) -> Option<&'static str> {
        self.wind_direction_10m.map(|deg| {
            let dirs = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
                        "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];
            let idx = ((deg + 11.25) % 360.0 / 22.5) as usize;
            dirs[idx.min(15)]
        })
    }

    /// Wind direction -> arrow emoji.
    pub fn direction_arrow(&self) -> Option<&'static str> {
        self.wind_direction_10m.map(|deg| {
            let arrows = ["\u{2193}", "\u{2199}", "\u{2190}", "\u{2196}", "\u{2191}", "\u{2197}", "\u{2192}", "\u{2198}"];
            let idx = ((deg + 22.5) % 360.0 / 45.0) as usize;
            arrows[idx.min(7)]
        })
    }
}

// ---------------------------------------------------------------------------
// History types
// ---------------------------------------------------------------------------

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
// Area average type
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AreaAverage {
    pub sensor_count: usize,
    pub pm2_5_median: Option<f64>,
    pub pm10_median: Option<f64>,
    pub pm2_5_readings: usize,
    pub pm10_readings: usize,
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct AppConfig {
    pub default_city: Option<String>,
    pub cities: Option<Vec<String>>,
    pub sensor_id: Option<u64>,
    pub radius: Option<f64>,
    pub sources: Option<Vec<ConfigSource>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConfigSource {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default = "default_source_type")]
    pub source_type: String,
    #[serde(default)]
    pub height: f64,
}

pub fn default_source_type() -> String {
    "custom".to_string()
}

#[cfg(feature = "cli")]
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
// Wind history types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WindHistoryResponse {
    pub hourly: WindHourlyData,
}

#[derive(Debug, Deserialize)]
pub struct WindHourlyData {
    pub time: Vec<String>,
    pub wind_speed_10m: Vec<Option<f64>>,
    pub wind_direction_10m: Vec<Option<f64>>,
}

// ---------------------------------------------------------------------------
// Pollution sources type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollutionSource {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub source_type: String,
    pub distance_km: f64,
}

impl PollutionSource {
    /// Create from config source entry with distance from a reference point.
    pub fn from_config(src: &ConfigSource, ref_lat: f64, ref_lon: f64) -> Self {
        Self {
            name: src.name.clone(),
            lat: src.lat,
            lon: src.lon,
            source_type: src.source_type.clone(),
            distance_km: front::haversine(ref_lat, ref_lon, src.lat, src.lon),
        }
    }
}

// ---------------------------------------------------------------------------
// Weather data type (struct only, no fetch)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherData {
    pub pressure_hpa: Option<f64>,
    pub humidity_pct: Option<f64>,
    pub apparent_temp_c: Option<f64>,
    pub precipitation_mm: Option<f64>,
    pub cloud_cover_pct: Option<f64>,
}

// ---------------------------------------------------------------------------
// Pollen data type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollenData {
    pub grass_pollen: Option<f64>,
    pub birch_pollen: Option<f64>,
    pub alder_pollen: Option<f64>,
    pub ragweed_pollen: Option<f64>,
}

impl PollenData {
    /// Returns true if any pollen level is significant (> 10).
    pub fn is_significant(&self) -> bool {
        [self.grass_pollen, self.birch_pollen, self.alder_pollen, self.ragweed_pollen]
            .iter()
            .any(|v| v.map_or(false, |x| x > 10.0))
    }

    /// Label for a pollen level.
    pub fn pollen_label(val: f64) -> &'static str {
        if val < 10.0 {
            "Low"
        } else if val < 30.0 {
            "Moderate"
        } else if val < 60.0 {
            "High"
        } else {
            "Very High"
        }
    }
}

// ---------------------------------------------------------------------------
// Earthquake data type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct EarthquakeInfo {
    pub magnitude: f64,
    pub place: String,
    pub distance_km: f64,
    pub time: String,
}

// ---------------------------------------------------------------------------
// Geomagnetic data type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct GeomagneticData {
    pub kp_index: f64,
    pub label: String,
}

impl GeomagneticData {
    pub fn from_kp(kp: f64) -> Self {
        let label = if kp < 3.0 {
            "Quiet"
        } else if kp < 5.0 {
            "Unsettled"
        } else {
            "Storm"
        };
        Self {
            kp_index: kp,
            label: label.to_string(),
        }
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
/// Clamped: negative -> 0, beyond max breakpoint -> last bracket's AQI_high.
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
    // Beyond max breakpoint -- cap at 500
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
    Good,               // 0-50
    Moderate,           // 51-100
    UnhealthySensitive, // 101-150
    Unhealthy,          // 151-200
    VeryUnhealthy,      // 201-300
    Hazardous,          // 301-500
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
            Self::Good => "\u{1f7e2}",
            Self::Moderate => "\u{1f7e1}",
            Self::UnhealthySensitive => "\u{1f7e0}",
            Self::Unhealthy => "\u{1f534}",
            Self::VeryUnhealthy => "\u{1f7e3}",
            Self::Hazardous => "\u{1f7e4}",
        }
    }

    #[cfg(feature = "cli")]
    pub fn colorize(&self, text: &str) -> colored::ColoredString {
        use colored::Colorize;
        match self {
            Self::Good => text.green(),
            Self::Moderate => text.yellow(),
            Self::UnhealthySensitive => text.truecolor(255, 165, 0),
            Self::Unhealthy => text.red(),
            Self::VeryUnhealthy => text.purple(),
            Self::Hazardous => text.truecolor(128, 0, 0),
        }
    }

    /// Color as hex string (for web/WASM).
    pub fn color_hex(&self) -> &'static str {
        match self {
            Self::Good => "#00c853",
            Self::Moderate => "#ffc107",
            Self::UnhealthySensitive => "#ff9800",
            Self::Unhealthy => "#f44336",
            Self::VeryUnhealthy => "#9c27b0",
            Self::Hazardous => "#800000",
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
    // CO: WHO 24h guideline 4mg/m3 = 4000 ug/m3
    if value <= 4000.0 {
        AqiCategory::Good
    } else if value <= 10000.0 {
        AqiCategory::Moderate
    } else {
        AqiCategory::Unhealthy
    }
}

pub fn get_no2_status(value: f64) -> AqiCategory {
    // NO2: WHO 24h guideline 25 ug/m3
    if value <= 25.0 {
        AqiCategory::Good
    } else if value <= 50.0 {
        AqiCategory::Moderate
    } else {
        AqiCategory::Unhealthy
    }
}

pub fn get_so2_status(value: f64) -> AqiCategory {
    // SO2: WHO 24h guideline 40 ug/m3
    if value <= 40.0 {
        AqiCategory::Good
    } else if value <= 80.0 {
        AqiCategory::Moderate
    } else {
        AqiCategory::Unhealthy
    }
}

pub fn get_o3_status(value: f64) -> AqiCategory {
    // O3: WHO 8h guideline 100 ug/m3
    if value <= 100.0 {
        AqiCategory::Good
    } else if value <= 160.0 {
        AqiCategory::Moderate
    } else {
        AqiCategory::Unhealthy
    }
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

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

pub fn median(vals: &mut Vec<f64>) -> Option<f64> {
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

// ---------------------------------------------------------------------------
// Country/city helpers
// ---------------------------------------------------------------------------

/// City with pre-resolved coordinates (from `cities` crate).
#[derive(Debug, Serialize)]
pub struct CityInfo {
    pub name: &'static str,
    pub country: &'static str,
    pub lat: f64,
    pub lon: f64,
}

/// Get major cities for any country. Returns up to `limit` cities.
/// Country name is case-insensitive, supports common aliases (e.g., "usa" -> "United States").
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

pub fn normalize_country(input: &str) -> String {
    match input.to_lowercase().as_str() {
        "usa" | "us" | "united states" | "america" => "united states".into(),
        "uk" | "england" | "britain" | "great britain" => "united kingdom".into(),
        "turkey" | "t\u{00fc}rkiye" | "turkiye" => "turkey".into(),
        "russia" | "rf" => "russia".into(),
        "south korea" | "korea" => "south korea".into(),
        "uae" | "emirates" => "united arab emirates".into(),
        other => other.into(),
    }
}

// ---------------------------------------------------------------------------
// Date helpers
// ---------------------------------------------------------------------------

/// Simple date helpers (avoid chrono dependency).
pub fn chrono_date_now() -> String {
    // Use system time to get current date
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days_since_epoch = now / 86400;
    epoch_days_to_date(days_since_epoch)
}

pub fn epoch_days_to_date(days: u64) -> String {
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

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn date_minus_days(date: &str, days: u32) -> String {
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

// ---------------------------------------------------------------------------
// CSV parsing
// ---------------------------------------------------------------------------

pub fn parse_sensor_csv(text: &str, out: &mut Vec<(String, f64)>) {
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split(';').collect();
        // Format: sensor_id;sensor_type;location;lat;lon;timestamp;P1;durP1;ratioP1;P2;...
        if cols.len() >= 10 {
            let timestamp = cols[5]; // e.g. 2026-03-14T00:02:12
            let p2 = cols[9]; // PM2.5
            if let Ok(val) = p2.parse::<f64>() {
                if val > 0.0 && val < 500.0 {
                    out.push((timestamp.to_string(), val));
                }
            }
        }
    }
}

pub fn aggregate_sensor_to_hourly(readings: &[(String, f64)]) -> Vec<(String, f64)> {
    let mut hourly: std::collections::BTreeMap<String, Vec<f64>> =
        std::collections::BTreeMap::new();
    for (ts, val) in readings {
        // "2026-03-14T00:02:12" -> "2026-03-14T00:00"
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

// ---------------------------------------------------------------------------
// Comfort Index
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ComfortScore {
    pub total: u32,
    pub air: u32,
    pub temperature: u32,
    pub wind: u32,
    pub uv: u32,
    pub pressure: u32,
    pub humidity: u32,
}

impl ComfortScore {
    pub fn label(&self) -> &'static str {
        match self.total {
            80..=100 => "Excellent",
            60..=79 => "Good",
            40..=59 => "Fair",
            20..=39 => "Poor",
            _ => "Bad",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self.total {
            80..=100 => "\u{1f7e2}", // green circle
            60..=79 => "\u{1f7e1}",  // yellow circle
            40..=59 => "\u{1f7e0}",  // orange circle
            20..=39 => "\u{1f534}",  // red circle
            _ => "\u{1f7e4}",        // brown circle
        }
    }
}

pub fn calculate_comfort(
    air: &CurrentData,
    weather: &WeatherData,
    wind: &WindData,
) -> ComfortScore {
    // Air: based on AQI
    let aqi = overall_aqi(air).unwrap_or(0);
    let air_score = match aqi {
        0..=25 => 100,
        26..=50 => 80,
        51..=100 => 50,
        101..=200 => 20,
        _ => 0,
    };

    // Temperature: 18-26C = 100, linear decrease outside
    let temp_score = if let Some(t) = weather.apparent_temp_c {
        if (18.0..=26.0).contains(&t) {
            100
        } else if t < 18.0 {
            (100.0 - (18.0 - t) * 5.0).max(0.0) as u32
        } else {
            (100.0 - (t - 26.0) * 5.0).max(0.0) as u32
        }
    } else {
        50 // unknown
    };

    // Wind: lower is better for comfort
    let wind_score = if let Some(s) = wind.wind_speed_10m {
        match s {
            v if v < 10.0 => 100,
            v if v < 20.0 => 80,
            v if v < 40.0 => 50,
            _ => 20,
        }
    } else {
        50
    };

    // UV: lower is better
    let uv_score = if let Some(uv) = air.uv_index {
        match uv {
            v if v < 3.0 => 100,
            v if v < 6.0 => 80,
            v if v < 8.0 => 60,
            v if v < 11.0 => 30,
            _ => 10,
        }
    } else {
        50
    };

    // Pressure: 1010-1020 optimal
    let pressure_score = if let Some(p) = weather.pressure_hpa {
        if (1010.0..=1020.0).contains(&p) {
            100
        } else {
            let diff = if p < 1010.0 { 1010.0 - p } else { p - 1020.0 };
            (100.0 - diff * 3.0).max(0.0) as u32
        }
    } else {
        50
    };

    // Humidity: 30-60% optimal
    let humidity_score = if let Some(h) = weather.humidity_pct {
        if (30.0..=60.0).contains(&h) {
            100
        } else if h < 20.0 || h > 80.0 {
            50
        } else {
            75
        }
    } else {
        50
    };

    // Weighted average: air 30%, temp 25%, wind 10%, uv 10%, pressure 15%, humidity 10%
    let total = (air_score as f64 * 0.30
        + temp_score as f64 * 0.25
        + wind_score as f64 * 0.10
        + uv_score as f64 * 0.10
        + pressure_score as f64 * 0.15
        + humidity_score as f64 * 0.10)
        .round() as u32;

    ComfortScore {
        total: total.min(100),
        air: air_score,
        temperature: temp_score,
        wind: wind_score,
        uv: uv_score,
        pressure: pressure_score,
        humidity: humidity_score,
    }
}

/// Render a progress bar: filled blocks + empty blocks (10 chars total).
pub fn progress_bar(score: u32) -> String {
    let filled = (score / 10).min(10) as usize;
    let empty = 10 - filled;
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

// ---------------------------------------------------------------------------
// Front analysis -- pollution front detection and tracking
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
        let arrows = ["\u{2191}", "\u{2197}", "\u{2192}", "\u{2198}", "\u{2193}", "\u{2199}", "\u{2190}", "\u{2196}"];
        let idx = ((deg + 22.5) % 360.0 / 45.0) as usize;
        arrows[idx.min(7)]
    }

    /// A sensor cluster -- group of nearby sensors aggregated into one node.
    #[derive(Debug, Clone)]
    pub struct SensorCluster {
        pub id: String,           // "cluster_0" or "sensor_12345"
        pub lat: f64,
        pub lon: f64,
        pub sensor_ids: Vec<u64>,
        pub sensor_count: usize,
    }

    /// Cluster sensors by proximity (grid-based, ~5km cells).
    /// Returns clusters with centroid coordinates and member sensor IDs.
    pub fn cluster_sensors(sensors: &[(u64, f64, f64)], cell_km: f64) -> Vec<SensorCluster> {
        // Grid-based clustering: round lat/lon to grid cells
        let cell_lat = cell_km / 111.0; // ~111km per degree latitude
        let mut grid: std::collections::HashMap<(i32, i32), Vec<(u64, f64, f64)>> =
            std::collections::HashMap::new();

        for &(id, lat, lon) in sensors {
            let cell_lon = cell_km / (111.0 * lat.to_radians().cos().max(0.01));
            let gx = (lon / cell_lon).floor() as i32;
            let gy = (lat / cell_lat).floor() as i32;
            grid.entry((gx, gy)).or_default().push((id, lat, lon));
        }

        let mut used_names: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        grid.into_iter()
            .map(|((_, _), members)| {
                let n = members.len() as f64;
                let lat = members.iter().map(|(_, la, _)| la).sum::<f64>() / n;
                let lon = members.iter().map(|(_, _, lo)| lo).sum::<f64>() / n;
                let ids: Vec<u64> = members.iter().map(|(id, _, _)| *id).collect();

                // Name by nearest city from cities crate
                let name = nearest_city_name(lat, lon);
                let count = used_names.entry(name.clone()).or_insert(0);
                *count += 1;
                let id = if *count > 1 {
                    format!("{}-{}", name, count)
                } else {
                    name
                };

                SensorCluster {
                    id,
                    lat,
                    lon,
                    sensor_count: ids.len(),
                    sensor_ids: ids,
                }
            })
            .collect()
    }

    /// Find the nearest city name from the cities crate for a coordinate.
    fn nearest_city_name(lat: f64, lon: f64) -> String {
        cities::all()
            .iter()
            .map(|c| {
                let d = haversine(lat, lon, c.latitude, c.longitude);
                (d, c.city)
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .map(|(_, name)| name.to_string())
            .unwrap_or_else(|| format!("{:.2},{:.2}", lat, lon))
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
    /// `sensor_data`: optional map of node_name -> hourly sensor readings.
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

        // Pairwise cross-correlation -> edges
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
                        // Both sources available -- weight by correlation strength
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

    /// Build graph from sensor clusters with archive history data.
    pub fn build_sensor_graph(
        target_name: &str,
        target_lat: f64,
        target_lon: f64,
        clusters: &[SensorCluster],
        cluster_data: &std::collections::HashMap<String, Vec<(String, f64)>>,
    ) -> FrontAnalysis {
        let mut graph = Graph::new();

        let target_node = graph.add_node(StationNode {
            name: target_name.to_string(),
            lat: target_lat,
            lon: target_lon,
            distance_from_target: 0.0,
        });

        // Find target cluster (closest to target point)
        let target_cluster_id = clusters.iter()
            .min_by(|a, b| {
                haversine(target_lat, target_lon, a.lat, a.lon)
                    .partial_cmp(&haversine(target_lat, target_lon, b.lat, b.lon))
                    .unwrap()
            })
            .map(|c| c.id.clone());

        // Add cluster nodes + their hourly data
        struct NodeData {
            idx: NodeIndex,
            times: Vec<String>,
            pm25: Vec<Option<f64>>,
        }

        let mut nodes: Vec<NodeData> = Vec::new();
        let mut all_spikes = Vec::new();

        // Add target with its cluster data if available
        let target_hourly = target_cluster_id.as_ref()
            .and_then(|id| cluster_data.get(id));
        let (target_times, target_pm25): (Vec<String>, Vec<Option<f64>>) =
            if let Some(data) = target_hourly {
                (data.iter().map(|(t, _)| t.clone()).collect(),
                 data.iter().map(|(_, v)| Some(*v)).collect())
            } else {
                (Vec::new(), Vec::new())
            };
        all_spikes.push((target_node, detect_spikes(&target_times, &target_pm25, 2.0)));
        nodes.push(NodeData { idx: target_node, times: target_times, pm25: target_pm25 });

        for cluster in clusters {
            // Skip target cluster (already added)
            if Some(&cluster.id) == target_cluster_id.as_ref() {
                continue;
            }
            let dist = haversine(target_lat, target_lon, cluster.lat, cluster.lon);
            let node = graph.add_node(StationNode {
                name: if cluster.sensor_count > 1 {
                    format!("{} ({} sensors)", cluster.id, cluster.sensor_count)
                } else {
                    cluster.id.clone()
                },
                lat: cluster.lat,
                lon: cluster.lon,
                distance_from_target: dist,
            });

            let (times, pm25): (Vec<String>, Vec<Option<f64>>) =
                if let Some(data) = cluster_data.get(&cluster.id) {
                    (data.iter().map(|(t, _)| t.clone()).collect(),
                     data.iter().map(|(_, v)| Some(*v)).collect())
                } else {
                    (Vec::new(), Vec::new())
                };
            all_spikes.push((node, detect_spikes(&times, &pm25, 2.0)));
            nodes.push(NodeData { idx: node, times, pm25 });
        }

        // Pairwise cross-correlation (only between nearby clusters, <80km)
        let mut fronts = Vec::new();
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let a = &graph[nodes[i].idx];
                let b = &graph[nodes[j].idx];
                let dist = haversine(a.lat, a.lon, b.lat, b.lon);
                if dist > 80.0 { continue; } // skip distant pairs

                let brng = bearing(a.lat, a.lon, b.lat, b.lon);
                let (lag, corr) = cross_correlate(&nodes[i].pm25, &nodes[j].pm25, 24);

                if corr > 0.5 && lag != 0 {
                    let speed = dist / (lag.unsigned_abs() as f64);
                    if speed > 100.0 { continue; } // unrealistic
                    let (from, to, actual_bearing) = if lag > 0 {
                        (nodes[i].idx, nodes[j].idx, brng)
                    } else {
                        (nodes[j].idx, nodes[i].idx, (brng + 180.0) % 360.0)
                    };

                    let edge = PropagationEdge {
                        distance_km: dist,
                        bearing_deg: actual_bearing,
                        lag_hours: lag.abs(),
                        correlation: corr,
                        speed_kmh: speed,
                        om_lag: None,
                        om_correlation: None,
                        sc_lag: Some(lag),
                        sc_correlation: Some(corr),
                        confidence: corr,
                    };
                    graph.add_edge(from, to, edge);

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
        generate_report_with_sensors(
            target_name, target_lat, target_lon,
            analysis, wind, days, &[],
        )
    }

    /// Generate report with individual sensor locations shown on map.
    pub fn generate_report_with_sensors(
        target_name: &str,
        target_lat: f64,
        target_lon: f64,
        analysis: &FrontAnalysis,
        wind: Option<&super::WindData>,
        days: u32,
        raw_sensors: &[(u64, f64, f64)], // (id, lat, lon)
    ) -> String {
        generate_report_with_sensor_values(
            target_name, target_lat, target_lon,
            analysis, wind, days, raw_sensors, &[],
        )
    }

    pub fn generate_report_with_sensor_values(
        target_name: &str,
        target_lat: f64,
        target_lon: f64,
        analysis: &FrontAnalysis,
        wind: Option<&super::WindData>,
        days: u32,
        raw_sensors: &[(u64, f64, f64)],
        sensor_values: &[(u64, f64)],
    ) -> String {
        generate_report_full(
            target_name, target_lat, target_lon,
            analysis, wind, days, raw_sensors, sensor_values,
            &[], &[],
        )
    }

    /// Full report with fronts + blame sources + CPF.
    pub fn generate_report_full(
        target_name: &str,
        target_lat: f64,
        target_lon: f64,
        analysis: &FrontAnalysis,
        wind: Option<&super::WindData>,
        days: u32,
        raw_sensors: &[(u64, f64, f64)],
        sensor_values: &[(u64, f64)],
        pollution_sources: &[super::PollutionSource],
        cpf_results: &[CpfResult],
    ) -> String {
        // Build markers JS
        let mut markers_js = String::new();
        let mut heatmap_js = String::from("var heat = L.heatLayer([");
        let mut has_heat = false;

        // Build sensor value lookup
        let val_map: std::collections::HashMap<u64, f64> = sensor_values.iter().copied().collect();

        // Individual sensor markers -- colored by PM2.5 value
        for (sid, slat, slon) in raw_sensors {
            let (color, pm_text) = if let Some(&pm) = val_map.get(sid) {
                let c = if pm < 12.0 { "#00c853" }
                    else if pm < 35.5 { "#ffc107" }
                    else if pm < 55.5 { "#ff9800" }
                    else if pm < 150.5 { "#f44336" }
                    else { "#9c27b0" };
                // Add to heatmap
                heatmap_js.push_str(&format!("[{},{},{:.1}],", slat, slon, pm));
                has_heat = true;
                (c, format!("<br>PM2.5: {:.1}", pm))
            } else {
                ("#90a4ae", String::new())
            };
            markers_js.push_str(&format!(
                "L.circleMarker([{}, {}], {{radius: 4, color: '{}', fillColor: '{}', fillOpacity: 0.7, weight: 1}}).addTo(map).bindPopup('Sensor #{}{}');\n",
                slat, slon, color, color, sid, pm_text,
            ));
        }

        heatmap_js.push_str("], {radius: 25, blur: 15, maxZoom: 12, max: 100, gradient: {0.2: '#00c853', 0.4: '#ffc107', 0.6: '#ff9800', 0.8: '#f44336', 1.0: '#9c27b0'}}).addTo(map);\n");
        if !has_heat {
            heatmap_js.clear();
        }

        // Target marker (red, on top)
        markers_js.push_str(&format!(
            "L.circleMarker([{}, {}], {{radius: 10, color: '#f44336', fillColor: '#f44336', fillOpacity: 0.8}}).addTo(map).bindPopup('<b>{}</b><br>Target city');\n",
            target_lat, target_lon,
            html_escape(target_name),
        ));

        // Only label top 5 nodes: target + top fronts endpoints
        let important_nodes: std::collections::HashSet<String> = {
            let mut set = std::collections::HashSet::new();
            set.insert(target_name.to_string());
            for f in analysis.fronts.iter().filter(|f| f.correlation > 0.7).take(5) {
                set.insert(f.from_city.clone());
                set.insert(f.to_city.clone());
            }
            set
        };

        // Neighbor markers (blue) -- labels only for important nodes
        for node_idx in analysis.graph.node_indices() {
            let node = &analysis.graph[node_idx];
            if node.distance_from_target > 0.0 {
                let is_important = important_nodes.contains(&node.name);
                // Short label: "Zelenograd" or "#5" for duplicates
                let short = node.name.split('(').next().unwrap_or(&node.name).trim();
                let label = if short.contains('-') {
                    // "Moscow-58" -> "#58"
                    let num = short.rsplit('-').next().unwrap_or(short);
                    format!("#{}", num)
                } else {
                    short.to_string()
                };

                let tooltip = if is_important {
                    format!(".bindTooltip('{}', {{permanent: true, direction: 'top', className: 'node-label'}})", html_escape(&label))
                } else {
                    String::new()
                };

                markers_js.push_str(&format!(
                    "L.circleMarker([{}, {}], {{radius: {}, color: '#2196f3', fillColor: '#2196f3', fillOpacity: 0.7}}).addTo(map).bindPopup('<b>{}</b><br>{:.0} km from target'){};\n",
                    node.lat, node.lon,
                    if is_important { 8 } else { 5 },
                    html_escape(&node.name),
                    node.distance_from_target,
                    tooltip,
                ));
            }
        }

        // Pollution source markers (triangles -- factories/power plants, diamonds -- highways)
        for src in pollution_sources {
            let (icon, size) = match src.source_type.as_str() {
                "power_plant" => ("\u{26a1}", 16),
                "factory" | "industrial" => ("\u{1f3ed}", 14),
                "highway" => ("\u{1f6e3}", 12),
                _ => ("\u{26a0}", 12),
            };
            let cpf_info = cpf_results.iter()
                .find(|r| r.source.name == src.name)
                .map(|r| format!("<br>CPF: {:.0}%", r.cpf_score * 100.0))
                .unwrap_or_default();
            let s = size + 4;
            let h = s / 2;
            markers_js.push_str(&format!(
                "L.marker([{},{}], {{icon: L.divIcon({{className:'src-icon', html:'<div style=\"font-size:{}px\">{}</div>', iconSize:[{},{}], iconAnchor:[{},{}]}}) }}).addTo(map).bindPopup('<b>{}</b><br>{} ({:.0}km){}');\n",
                src.lat, src.lon, size, icon, s, s, h, h,
                html_escape(&src.name),
                src.source_type.replace('_', " "),
                src.distance_km,
                cpf_info,
            ));
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

        // Sensor count
        let sensor_count = raw_sensors.len();
        let node_count = analysis.graph.node_count();
        let front_count = analysis.fronts.iter().filter(|f| f.correlation > 0.7).count();
        let _ = front_count; // used in template below

        // Generate insights
        let mut insights = Vec::new();

        // Worst spike
        let worst_spike = analysis.spikes.iter()
            .flat_map(|(n, spikes)| spikes.iter().map(move |s| (n, s)))
            .max_by(|a, b| a.1.value.partial_cmp(&b.1.value).unwrap());
        if let Some((node_idx, spike)) = worst_spike {
            let name = &analysis.graph[*node_idx].name;
            let aqi = super::pm25_aqi(spike.value);
            let cat = super::AqiCategory::from_aqi(aqi);
            insights.push(format!(
                "<li><b>Highest PM2.5:</b> {:.1} \u{00b5}g/m\u{00b3} (AQI {}, {}) at {} \u{2014} {}</li>",
                spike.value, aqi, cat.label(),
                html_escape(name),
                spike.time.replace('T', " "),
            ));
        }

        // Strongest front
        if let Some(front) = analysis.fronts.first() {
            insights.push(format!(
                "<li><b>Strongest front:</b> {} \u{2192} {} at {:.0} km/h {} (correlation {:.0}%, lag {}h)</li>",
                html_escape(&front.from_city),
                html_escape(&front.to_city),
                front.speed_kmh,
                bearing_label(front.bearing_deg),
                front.correlation * 100.0,
                front.lag_hours,
            ));
        }

        // Wind context
        if let Some(w) = wind {
            if let (Some(speed), Some(dir)) = (w.wind_speed_10m, w.direction_label()) {
                let condition = if speed < 5.0 {
                    "calm \u{2014} pollutants tend to accumulate locally"
                } else if speed < 15.0 {
                    "moderate \u{2014} pollution disperses gradually"
                } else {
                    "strong \u{2014} good ventilation, rapid dispersal"
                };
                insights.push(format!(
                    "<li><b>Wind:</b> {:.1} km/h {} \u{2014} {}</li>",
                    speed, dir, condition,
                ));
            }
        }

        // Overall assessment
        let all_pm25: Vec<f64> = analysis.spikes.iter()
            .flat_map(|(_, spikes)| spikes.iter().map(|s| s.value))
            .collect();
        if !all_pm25.is_empty() {
            let max_pm = all_pm25.iter().cloned().fold(0.0_f64, f64::max);
            let assessment = if max_pm < 12.0 {
                "Excellent air quality across the region. Safe for all outdoor activities."
            } else if max_pm < 35.5 {
                "Moderate air quality. Generally safe, sensitive individuals may want to limit prolonged outdoor exertion."
            } else if max_pm < 55.5 {
                "Unhealthy for sensitive groups. Consider reducing prolonged outdoor activity."
            } else if max_pm < 150.5 {
                "Unhealthy. Everyone should reduce outdoor exertion."
            } else {
                "Very unhealthy to hazardous. Avoid outdoor activity. Close windows."
            };
            insights.push(format!("<li><b>Assessment:</b> {}</li>", assessment));
        }

        let insights_html = if insights.is_empty() {
            "<p>Insufficient data for insights.</p>".to_string()
        } else {
            format!("<ul>{}</ul>", insights.join("\n"))
        };

        // CPF section (if blame data available)
        let cpf_section = if cpf_results.is_empty() {
            String::new()
        } else {
            let mut rows = String::new();
            for r in cpf_results.iter().filter(|r| r.hours_in_sector > 0).take(15) {
                let css = if r.cpf_score >= 0.6 { "unhealthy" }
                    else if r.cpf_score >= 0.3 { "moderate" }
                    else { "" };
                let icon = match r.source.source_type.as_str() {
                    "power_plant" => "\u{26a1}",
                    "factory" | "industrial" => "\u{1f3ed}",
                    "highway" => "\u{1f6e3}",
                    _ => "\u{1f4cd}",
                };
                rows.push_str(&format!(
                    "<tr class=\"{}\"><td>{} {}</td><td>{}</td><td>{:.0}km</td><td><b>{:.0}%</b></td><td>{:.1}</td><td>{:.1}</td></tr>\n",
                    css, icon, html_escape(&r.source.name),
                    r.source.source_type.replace('_', " "),
                    r.source.distance_km,
                    r.cpf_score * 100.0,
                    r.avg_pm25_in_sector,
                    r.avg_pm25_other,
                ));
            }
            format!(r#"
        <h2>Source Attribution (CPF)</h2>
        <p>Conditional Probability Function -- likelihood that high PM2.5 occurs when wind blows from each source direction.</p>
        <table>
            <thead>
                <tr><th>Source</th><th>Type</th><th>Distance</th><th>CPF</th><th>Avg PM2.5 (from src)</th><th>Background</th></tr>
            </thead>
            <tbody>
                {}
            </tbody>
        </table>"#, rows)
        };

        format!(r##"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Air Quality Report -- {target_name}</title>
    <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
    <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
    <script src="https://unpkg.com/leaflet.heat@0.2.0/dist/leaflet-heat.js"></script>
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
        .src-icon {{ background: none !important; border: none !important; }}
        .node-label {{ background: rgba(255,255,255,0.8) !important; border: none !important; box-shadow: none !important; font-size: 11px; font-weight: 600; padding: 1px 4px !important; }}
        .methodology {{ font-size: 13px; color: #555; line-height: 1.6; }}
        .methodology p {{ margin: 6px 0; }}
        .footer {{ color: #999; font-size: 12px; margin-top: 30px; padding-top: 10px; border-top: 1px solid #eee; }}
        .footer a {{ color: #999; }}
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
        <h1>Air Quality Report -- {target_name}</h1>

        <h2>Key Insights</h2>
        {insights_html}

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
                <tr><th>From -> To</th><th>Speed (km/h)</th><th>Direction</th><th>Lag (h)</th><th>Correlation</th></tr>
            </thead>
            <tbody>
                {fronts_rows}
            </tbody>
        </table>

        {cpf_section}

        <h2>Methodology</h2>
        <div class="methodology">
        <p><b>Data sources:</b> Open-Meteo atmospheric model (~11km grid) + Sensor.Community citizen science network ({sensor_count} sensors, {node_count} analysis nodes).</p>
        <p><b>Spike detection:</b> Z-score on hourly PM2.5 differences. A spike is flagged when the rate of change exceeds 2 standard deviations above the rolling mean -- indicating a sudden pollution event rather than normal variation.</p>
        <p><b>Front tracking:</b> Cross-correlation analysis between city pairs with time lags from -24h to +24h. The lag at peak correlation reveals how many hours pollution takes to travel between two points. Combined with haversine distance, this gives front speed and direction.</p>
        <p><b>Confidence:</b> When both Open-Meteo (model) and Sensor.Community (ground sensors) agree on lag direction, confidence is boosted. Disagreement reduces confidence, highlighting uncertain fronts.</p>
        </div>

        <h2>AQI Reference</h2>
        <table>
            <thead><tr><th>AQI</th><th>Category</th><th>Health Guidance</th></tr></thead>
            <tbody>
                <tr class="good"><td>0-50</td><td>Good</td><td>No restrictions</td></tr>
                <tr class="moderate"><td>51-100</td><td>Moderate</td><td>Sensitive individuals: limit prolonged outdoor exertion</td></tr>
                <tr class="unhealthy-sensitive"><td>101-150</td><td>Unhealthy for Sensitive</td><td>Active children and adults: reduce prolonged outdoor exertion</td></tr>
                <tr class="unhealthy"><td>151-200</td><td>Unhealthy</td><td>Everyone: reduce outdoor exertion</td></tr>
                <tr class="very-unhealthy"><td>201-300</td><td>Very Unhealthy</td><td>Everyone: avoid outdoor activity</td></tr>
                <tr class="hazardous"><td>301-500</td><td>Hazardous</td><td>Stay indoors. Close windows.</td></tr>
            </tbody>
        </table>

        <p class="footer">Generated by <a href="https://github.com/fortunto2/airq">airq</a> -- open source CLI air quality analyzer</p>
    </div>
    <script>
        var map = L.map('map').setView([{target_lat}, {target_lon}], 8);
        L.tileLayer('https://{{s}}.basemaps.cartocdn.com/light_all/{{z}}/{{x}}/{{y}}@2x.png', {{
            attribution: '&copy; OpenStreetMap &amp; CartoDB'
        }}).addTo(map);

        {heatmap_js}
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
            heatmap_js = heatmap_js,
            spikes_rows = spikes_rows,
            fronts_rows = fronts_rows,
            markers_js = markers_js,
            lines_js = lines_js,
            insights_html = insights_html,
            sensor_count = sensor_count,
            node_count = node_count,
            cpf_section = cpf_section,
        )
    }

    // -----------------------------------------------------------------------
    // CPF (Conditional Probability Function) for blame analysis
    // -----------------------------------------------------------------------

    /// CPF result for a single pollution source.
    #[derive(Debug, serde::Serialize)]
    pub struct CpfResult {
        pub source: super::PollutionSource,
        /// CPF score 0.0-1.0
        pub cpf_score: f64,
        /// Bearing from sensor to source (degrees)
        pub bearing_deg: f64,
        /// Total hours where wind came from source sector (and speed > threshold)
        pub hours_in_sector: usize,
        /// Hours in sector where PM2.5 exceeded threshold
        pub high_hours_in_sector: usize,
        /// Average PM2.5 when wind is from source direction
        pub avg_pm25_in_sector: f64,
        /// Average PM2.5 from other directions (background)
        pub avg_pm25_other: f64,
    }

    /// Calculate CPF for each source given hourly observations.
    ///
    /// `wind_dirs` and `wind_speeds` must be same length as `pm25_values`.
    /// `percentile` -- fraction (0.75 or 0.90) to compute the high-pollution threshold.
    pub fn calculate_cpf(
        sensor_lat: f64,
        sensor_lon: f64,
        sources: &[super::PollutionSource],
        pm25_values: &[f64],
        wind_dirs: &[f64],
        wind_speeds: &[f64],
        percentile: f64,
    ) -> Vec<CpfResult> {
        if pm25_values.is_empty() {
            return Vec::new();
        }

        // 1. Calculate threshold = percentile of pm25_values
        let mut sorted_pm = pm25_values.to_vec();
        sorted_pm.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((sorted_pm.len() as f64 * percentile) as usize).min(sorted_pm.len() - 1);
        let threshold = sorted_pm[idx];

        let half_sector = 15.0_f64; // +/-15 degree sector width
        let min_wind_speed = 5.0; // km/h -- filter calm winds

        let mut results: Vec<CpfResult> = Vec::new();

        for source in sources {
            let brng = bearing(sensor_lat, sensor_lon, source.lat, source.lon);

            let mut sector_count = 0usize;
            let mut high_count = 0usize;
            let mut sector_pm_sum = 0.0;
            let mut other_pm_sum = 0.0;
            let mut other_count = 0usize;

            for i in 0..pm25_values.len() {
                if wind_speeds[i] < min_wind_speed {
                    continue; // calm wind -- unreliable direction
                }

                // Angular difference (wind comes FROM wind_dir, source is at brng)
                let diff = ((wind_dirs[i] - brng + 540.0) % 360.0) - 180.0;
                let in_sector = diff.abs() <= half_sector;

                if in_sector {
                    sector_count += 1;
                    sector_pm_sum += pm25_values[i];
                    if pm25_values[i] > threshold {
                        high_count += 1;
                    }
                } else {
                    other_pm_sum += pm25_values[i];
                    other_count += 1;
                }
            }

            let cpf = if sector_count > 0 {
                high_count as f64 / sector_count as f64
            } else {
                0.0
            };

            let avg_in = if sector_count > 0 {
                sector_pm_sum / sector_count as f64
            } else {
                0.0
            };

            let avg_other = if other_count > 0 {
                other_pm_sum / other_count as f64
            } else {
                0.0
            };

            results.push(CpfResult {
                source: source.clone(),
                cpf_score: cpf,
                bearing_deg: brng,
                hours_in_sector: sector_count,
                high_hours_in_sector: high_count,
                avg_pm25_in_sector: avg_in,
                avg_pm25_other: avg_other,
            });
        }

        // Sort by CPF descending
        results.sort_by(|a, b| b.cpf_score.partial_cmp(&a.cpf_score).unwrap());
        results
    }

    /// Minimal HTML escaping for user-provided strings.
    pub fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;")
         .replace('<', "&lt;")
         .replace('>', "&gt;")
         .replace('"', "&quot;")
         .replace('\'', "&#39;")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    // --- CPF tests ---

    #[test]
    fn test_cpf_high_correlation() {
        // Source is due north (bearing ~0 deg), wind from north on high-PM hours
        let source = PollutionSource {
            name: "Factory North".to_string(),
            lat: 56.0,
            lon: 37.0,
            source_type: "factory".to_string(),
            distance_km: 111.0,
        };
        // 10 observations, use 50th percentile so threshold is median
        // sorted: [5,6,7,8,10,55,60,65,70,80], p50 idx=5 -> threshold=55
        let pm25: Vec<f64> = vec![5.0, 8.0, 55.0, 60.0, 10.0, 65.0, 70.0, 80.0, 6.0, 7.0];
        // Wind from north (0 deg) on the high-PM hours, south (180 deg) on low
        let wind_dirs: Vec<f64> = vec![180.0, 180.0, 0.0, 5.0, 180.0, 355.0, 10.0, 0.0, 180.0, 180.0];
        let wind_speeds: Vec<f64> = vec![10.0; 10];

        let results = front::calculate_cpf(55.0, 37.0, &[source], &pm25, &wind_dirs, &wind_speeds, 0.50);
        assert_eq!(results.len(), 1);
        // 5 hours in sector (indices 2,3,5,6,7 with PM 55,60,65,70,80), all > 55 threshold
        // CPF = 4/5 = 0.8 (60,65,70,80 > 55)
        assert!(results[0].cpf_score > 0.5, "CPF should be high, got {}", results[0].cpf_score);
        assert!(results[0].avg_pm25_in_sector > results[0].avg_pm25_other);
    }

    #[test]
    fn test_cpf_calm_wind_filtered() {
        let source = PollutionSource {
            name: "Plant".to_string(),
            lat: 56.0,
            lon: 37.0,
            source_type: "power_plant".to_string(),
            distance_km: 50.0,
        };
        // All calm winds -> no valid observations
        let pm25: Vec<f64> = vec![50.0, 60.0, 70.0];
        let wind_dirs: Vec<f64> = vec![0.0, 0.0, 0.0];
        let wind_speeds: Vec<f64> = vec![2.0, 3.0, 1.0]; // all below 5 km/h

        let results = front::calculate_cpf(55.0, 37.0, &[source], &pm25, &wind_dirs, &wind_speeds, 0.75);
        assert_eq!(results[0].cpf_score, 0.0);
        assert_eq!(results[0].hours_in_sector, 0);
    }

    #[test]
    fn test_cpf_empty_input() {
        let results = front::calculate_cpf(55.0, 37.0, &[], &[], &[], &[], 0.75);
        assert!(results.is_empty());
    }

    // --- Comfort score tests ---

    #[test]
    fn test_comfort_ideal_conditions() {
        let air = CurrentData {
            pm2_5: Some(5.0),
            pm10: Some(10.0),
            carbon_monoxide: None,
            nitrogen_dioxide: None,
            ozone: None,
            sulphur_dioxide: None,
            uv_index: Some(2.0),
            us_aqi: Some(20.0),
            european_aqi: None,
        };
        let weather = WeatherData {
            pressure_hpa: Some(1015.0),
            humidity_pct: Some(45.0),
            apparent_temp_c: Some(22.0),
            precipitation_mm: Some(0.0),
            cloud_cover_pct: Some(20.0),
        };
        let wind = WindData {
            wind_speed_10m: Some(5.0),
            wind_direction_10m: Some(180.0),
            wind_gusts_10m: Some(10.0),
        };
        let score = calculate_comfort(&air, &weather, &wind);
        assert!(score.total >= 90, "Ideal conditions should score 90+, got {}", score.total);
        assert_eq!(score.air, 100);
        assert_eq!(score.temperature, 100);
        assert_eq!(score.wind, 100);
        assert_eq!(score.uv, 100);
        assert_eq!(score.pressure, 100);
        assert_eq!(score.humidity, 100);
    }

    #[test]
    fn test_comfort_poor_conditions() {
        let air = CurrentData {
            pm2_5: Some(200.0),
            pm10: Some(300.0),
            carbon_monoxide: None,
            nitrogen_dioxide: None,
            ozone: None,
            sulphur_dioxide: None,
            uv_index: Some(12.0),
            us_aqi: Some(250.0),
            european_aqi: None,
        };
        let weather = WeatherData {
            pressure_hpa: Some(990.0),
            humidity_pct: Some(95.0),
            apparent_temp_c: Some(40.0),
            precipitation_mm: Some(10.0),
            cloud_cover_pct: Some(100.0),
        };
        let wind = WindData {
            wind_speed_10m: Some(50.0),
            wind_direction_10m: Some(0.0),
            wind_gusts_10m: Some(80.0),
        };
        let score = calculate_comfort(&air, &weather, &wind);
        assert!(score.total <= 30, "Poor conditions should score <=30, got {}", score.total);
    }

    #[test]
    fn test_comfort_unknown_data() {
        // All None values should produce a middle score (50ish)
        let air = CurrentData {
            pm2_5: None, pm10: None, carbon_monoxide: None, nitrogen_dioxide: None,
            ozone: None, sulphur_dioxide: None, uv_index: None, us_aqi: None, european_aqi: None,
        };
        let weather = WeatherData {
            pressure_hpa: None, humidity_pct: None, apparent_temp_c: None,
            precipitation_mm: None, cloud_cover_pct: None,
        };
        let wind = WindData {
            wind_speed_10m: None, wind_direction_10m: None, wind_gusts_10m: None,
        };
        let score = calculate_comfort(&air, &weather, &wind);
        // With no data, air defaults to AQI 0 (good=100), others default to 50
        assert!(score.total >= 50 && score.total <= 70, "Unknown data should score ~50-70, got {}", score.total);
    }

    #[test]
    fn test_pollen_significance() {
        let low = PollenData {
            grass_pollen: Some(5.0), birch_pollen: Some(3.0),
            alder_pollen: Some(2.0), ragweed_pollen: Some(1.0),
        };
        assert!(!low.is_significant());

        let high = PollenData {
            grass_pollen: Some(50.0), birch_pollen: Some(3.0),
            alder_pollen: Some(2.0), ragweed_pollen: Some(1.0),
        };
        assert!(high.is_significant());
    }

    #[test]
    fn test_geomagnetic_labels() {
        assert_eq!(GeomagneticData::from_kp(1.0).label, "Quiet");
        assert_eq!(GeomagneticData::from_kp(3.5).label, "Unsettled");
        assert_eq!(GeomagneticData::from_kp(6.0).label, "Storm");
    }

    #[test]
    fn test_progress_bar() {
        assert_eq!(progress_bar(100), "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}");
        assert_eq!(progress_bar(0), "\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}");
        assert_eq!(progress_bar(50).len(), progress_bar(100).len());
    }

    // --- Cross-correlation tests ---

    #[test]
    fn test_cross_correlate_identical() {
        let a: Vec<Option<f64>> = vec![Some(1.0), Some(5.0), Some(2.0), Some(8.0), Some(3.0), Some(7.0)];
        let (lag, corr) = front::cross_correlate(&a, &a, 5);
        assert_eq!(lag, 0);
        assert!(corr > 0.99, "identical series should have corr ~1.0, got {}", corr);
    }

    #[test]
    fn test_cross_correlate_shifted() {
        // b is a shifted by 2 hours
        let a: Vec<Option<f64>> = vec![Some(1.0), Some(2.0), Some(10.0), Some(3.0), Some(1.0), Some(2.0), Some(1.0), Some(1.0)];
        let b: Vec<Option<f64>> = vec![Some(1.0), Some(1.0), Some(1.0), Some(2.0), Some(10.0), Some(3.0), Some(1.0), Some(2.0)];
        let (lag, corr) = front::cross_correlate(&a, &b, 5);
        assert_eq!(lag, 2, "a leads b by 2 hours");
        assert!(corr > 0.8, "shifted series should correlate, got {}", corr);
    }

    #[test]
    fn test_cross_correlate_no_data() {
        let a: Vec<Option<f64>> = vec![None, None, None];
        let b: Vec<Option<f64>> = vec![None, None, None];
        let (lag, corr) = front::cross_correlate(&a, &b, 5);
        assert_eq!(lag, 0);
        assert!(corr.abs() < 0.01);
    }

    // --- Spike detection tests ---

    #[test]
    fn test_detect_spikes_finds_spike() {
        let times: Vec<String> = (0..10).map(|i| format!("2026-03-15T{:02}:00", i)).collect();
        // Flat then big jump at hour 5
        let values: Vec<Option<f64>> = vec![
            Some(10.0), Some(10.5), Some(10.2), Some(10.8), Some(10.3),
            Some(50.0), // spike: +39.7
            Some(48.0), Some(45.0), Some(42.0), Some(40.0),
        ];
        let spikes = front::detect_spikes(&times, &values, 2.0);
        assert!(!spikes.is_empty(), "should detect spike");
        assert!(spikes[0].value > 40.0);
        assert!(spikes[0].delta > 30.0);
    }

    #[test]
    fn test_detect_spikes_no_spikes_in_flat() {
        let times: Vec<String> = (0..10).map(|i| format!("2026-03-15T{:02}:00", i)).collect();
        let values: Vec<Option<f64>> = vec![
            Some(10.0), Some(10.1), Some(10.0), Some(10.2), Some(10.1),
            Some(10.0), Some(10.1), Some(10.2), Some(10.0), Some(10.1),
        ];
        let spikes = front::detect_spikes(&times, &values, 2.0);
        assert!(spikes.is_empty(), "flat data should have no spikes");
    }

    // --- Haversine & bearing tests ---

    #[test]
    fn test_haversine_known_distance() {
        // Moscow → Saint Petersburg ≈ 634 km
        let d = front::haversine(55.75, 37.62, 59.93, 30.32);
        assert!((d - 634.0).abs() < 20.0, "Moscow-SPb should be ~634km, got {}", d);
    }

    #[test]
    fn test_haversine_same_point() {
        assert!(front::haversine(55.0, 37.0, 55.0, 37.0) < 0.001);
    }

    #[test]
    fn test_bearing_north() {
        let b = front::bearing(55.0, 37.0, 56.0, 37.0);
        assert!((b - 0.0).abs() < 5.0 || (b - 360.0).abs() < 5.0, "due north should be ~0°, got {}", b);
    }

    #[test]
    fn test_bearing_east() {
        let b = front::bearing(55.0, 37.0, 55.0, 38.0);
        assert!((b - 90.0).abs() < 10.0, "due east should be ~90°, got {}", b);
    }

    #[test]
    fn test_bearing_label() {
        assert_eq!(front::bearing_label(0.0), "N");
        assert_eq!(front::bearing_label(90.0), "E");
        assert_eq!(front::bearing_label(180.0), "S");
        assert_eq!(front::bearing_label(270.0), "W");
        assert_eq!(front::bearing_label(45.0), "NE");
    }

    // --- Sensor clustering tests ---

    #[test]
    fn test_cluster_sensors_groups_nearby() {
        let sensors = vec![
            (1, 55.75, 37.60),
            (2, 55.751, 37.601), // ~100m from sensor 1
            (3, 55.80, 37.60),   // ~5.5km north
            (4, 56.0, 37.60),    // ~28km north
        ];
        let clusters = front::cluster_sensors(&sensors, 5.0);
        // sensor 1 and 2 should be in same cluster, 3 maybe same or separate, 4 separate
        assert!(clusters.len() >= 2, "should have at least 2 clusters, got {}", clusters.len());
        assert!(clusters.len() <= 4);
    }

    // --- Date helper tests ---

    #[test]
    fn test_epoch_days_to_date() {
        assert_eq!(epoch_days_to_date(0), "1970-01-01");
        assert_eq!(epoch_days_to_date(365), "1971-01-01");
        assert_eq!(epoch_days_to_date(730), "1972-01-01"); // leap year
        assert_eq!(epoch_days_to_date(18628), "2021-01-01");
    }

    #[test]
    fn test_date_minus_days() {
        assert_eq!(date_minus_days("2026-03-16", 1), "2026-03-15");
        assert_eq!(date_minus_days("2026-03-01", 1), "2026-02-28");
        assert_eq!(date_minus_days("2026-01-01", 1), "2025-12-31");
    }

    #[test]
    fn test_is_leap() {
        assert!(is_leap(2024));
        assert!(!is_leap(2023));
        assert!(!is_leap(2100));
        assert!(is_leap(2000));
    }

    // --- CSV parsing tests ---

    #[test]
    fn test_parse_sensor_csv() {
        let csv = "sensor_id;sensor_type;location;lat;lon;timestamp;P1;durP1;ratioP1;P2;durP2;ratioP2\n\
                   77955;SDS011;67152;36.266;32.294;2026-03-14T00:02:12;6.25;;;2.97;;\n\
                   77955;SDS011;67152;36.266;32.294;2026-03-14T00:04:38;6.55;;;3.72;;";
        let mut readings = Vec::new();
        parse_sensor_csv(csv, &mut readings);
        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].0, "2026-03-14T00:02:12");
        assert!((readings[0].1 - 2.97).abs() < 0.01); // PM2.5 = P2
        assert!((readings[1].1 - 3.72).abs() < 0.01);
    }

    #[test]
    fn test_parse_sensor_csv_filters_outliers() {
        let csv = "sensor_id;sensor_type;location;lat;lon;timestamp;P1;durP1;ratioP1;P2;durP2;ratioP2\n\
                   1;SDS011;1;0;0;2026-03-14T00:00:00;10;;;999.9;;\n\
                   1;SDS011;1;0;0;2026-03-14T00:05:00;10;;;5.0;;";
        let mut readings = Vec::new();
        parse_sensor_csv(csv, &mut readings);
        assert_eq!(readings.len(), 1, "should filter PM2.5 >= 500");
        assert!((readings[0].1 - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_aggregate_sensor_to_hourly_median() {
        let readings = vec![
            ("2026-03-14T10:01:00".to_string(), 5.0),
            ("2026-03-14T10:03:00".to_string(), 15.0),
            ("2026-03-14T10:05:00".to_string(), 10.0), // median = 10
            ("2026-03-14T11:01:00".to_string(), 20.0),
        ];
        let hourly = aggregate_sensor_to_hourly(&readings);
        assert_eq!(hourly.len(), 2);
        assert_eq!(hourly[0].0, "2026-03-14T10:00");
        assert!((hourly[0].1 - 10.0).abs() < 0.01, "median of [5,10,15] = 10");
        assert!((hourly[1].1 - 20.0).abs() < 0.01);
    }

    // --- SO2 and O3 status tests ---

    #[test]
    fn test_so2_status() {
        assert!(matches!(get_so2_status(20.0), AqiCategory::Good));
        assert!(matches!(get_so2_status(50.0), AqiCategory::Moderate));
        assert!(matches!(get_so2_status(100.0), AqiCategory::Unhealthy));
    }

    #[test]
    fn test_o3_status() {
        assert!(matches!(get_o3_status(50.0), AqiCategory::Good));
        assert!(matches!(get_o3_status(120.0), AqiCategory::Moderate));
        assert!(matches!(get_o3_status(200.0), AqiCategory::Unhealthy));
    }

    // --- Comfort edge cases ---

    #[test]
    fn test_comfort_extreme_heat() {
        let air = CurrentData {
            pm2_5: Some(5.0), pm10: Some(10.0),
            carbon_monoxide: None, nitrogen_dioxide: None, ozone: None,
            sulphur_dioxide: None, uv_index: Some(12.0),
            us_aqi: Some(20.0), european_aqi: None,
        };
        let weather = WeatherData {
            pressure_hpa: Some(1015.0), humidity_pct: Some(90.0),
            apparent_temp_c: Some(42.0), precipitation_mm: None, cloud_cover_pct: None,
        };
        let wind = WindData { wind_speed_10m: Some(2.0), wind_direction_10m: Some(180.0), wind_gusts_10m: None };
        let comfort = calculate_comfort(&air, &weather, &wind);
        assert!(comfort.temperature < 30, "42°C should score low on temp");
        assert!(comfort.uv < 20, "UV 12 should score very low");
        assert!(comfort.total < 70, "extreme heat should lower total, got {}", comfort.total);
    }
}

// ---------------------------------------------------------------------------
// Signal normalize functions (comfort sub-scores, 0-100 scale)
// Used by both CLI and WASM. Pure functions, no IO.
// ---------------------------------------------------------------------------

// AI-NOTE: signal module contains only: sigmoid/gaussian primitives + normalize_* functions + SignalComfort type.
// All weights, names, indices, matrix ops are in matrix.rs via define_signal_columns! macro.
pub mod signal {
    // -----------------------------------------------------------------------
    // Primitive curves — building blocks for all normalize functions
    // AI-NOTE: sigmoid(x, mid, k) and gaussian(x, center, σ) are the only two curves needed.
    // Every normalize_* is one line calling these primitives.
    // -----------------------------------------------------------------------

    /// Logistic sigmoid: 1 / (1 + e^(-k*(x - mid)))
    /// Maps ℝ → (0, 1). k controls steepness.
    #[inline]
    fn sigmoid(x: f64, mid: f64, k: f64) -> f64 {
        1.0 / (1.0 + (-k * (x - mid)).exp())
    }

    /// Descending sigmoid → comfort score.
    /// High x = bad → low score. `s(x) = 100 * (1 - sigmoid(x, mid, k))`
    #[inline]
    fn sigmoid_desc(x: f64, mid: f64, k: f64) -> u32 {
        (100.0 * (1.0 - sigmoid(x, mid, k))).round() as u32
    }

    /// Gaussian bell: 100 * e^(-((x - center) / σ)²)
    /// Peaks at center, symmetric decay. For "ideal range" metrics.
    #[inline]
    fn gaussian(x: f64, center: f64, sigma: f64) -> u32 {
        let z = (x - center) / sigma;
        (100.0 * (-z * z).exp()).round() as u32
    }

    /// Ascending sigmoid → comfort score.
    /// High x = good → high score. `s(x) = 100 * sigmoid(x, mid, k)`
    #[inline]
    fn sigmoid_asc(x: f64, mid: f64, k: f64) -> u32 {
        (100.0 * sigmoid(x, mid, k)).round() as u32
    }

    // -----------------------------------------------------------------------
    // Normalize functions (all → u32 0-100)
    // -----------------------------------------------------------------------

    /// Air quality: PM2.5 → AQI → sigmoid comfort.
    /// Midpoint AQI=75 (unhealthy-sensitive), k=0.04.
    /// AQI 0 ≈ 100, AQI 150 ≈ 5, AQI 300 ≈ 0.
    pub fn normalize_air(pm25: f64) -> u32 {
        let aqi = super::pm25_aqi(pm25) as f64;
        sigmoid_desc(aqi, 75.0, 0.04)
    }

    /// Temperature: Gaussian around ideal 23°C, σ=12.
    /// 23°C = 100, 10°C ≈ 38, 36°C ≈ 38, 0°C ≈ 7, -10°C ≈ 1.
    pub fn normalize_temperature(temp_c: f64) -> u32 {
        gaussian(temp_c, 23.0, 12.0)
    }

    /// UV index: descending sigmoid, midpoint 6, k=0.6.
    /// UV 0 ≈ 97, UV 3 ≈ 86, UV 8 ≈ 23, UV 11 ≈ 4.
    pub fn normalize_uv(uv: f64) -> u32 {
        sigmoid_desc(uv, 6.0, 0.6)
    }

    /// Wind speed: descending sigmoid, mid=25 km/h, k=0.12.
    /// 0 km/h ≈ 95, 10 ≈ 86, 25 = 50, 40 ≈ 14, 60 ≈ 1.
    pub fn normalize_wind(speed_kmh: f64) -> u32 {
        sigmoid_desc(speed_kmh, 25.0, 0.12)
    }

    /// Marine: wave height descending sigmoid, mid=2m, k=1.5.
    /// 0m ≈ 95, 1m ≈ 82, 2m = 50, 4m ≈ 5.
    pub fn normalize_marine(wave_height_m: f64) -> u32 {
        sigmoid_desc(wave_height_m, 2.0, 1.5)
    }

    /// Earthquake: descending sigmoid on magnitude, mid=4.5, k=1.2.
    /// No data (mag < 0) = 100. Mag 3 ≈ 86, 4.5 = 50, 6 ≈ 14.
    pub fn normalize_earthquake(magnitude: f64) -> u32 {
        if magnitude < 0.0 {
            return 100; // no data sentinel
        }
        sigmoid_desc(magnitude, 4.5, 1.2)
    }

    /// Fire proximity: ascending sigmoid on distance, mid=30km, k=0.08.
    /// 0km ≈ 8, 15km ≈ 23, 30km = 50, 60km ≈ 91, 100km ≈ 100.
    pub fn normalize_fire(distance_km: f64) -> u32 {
        sigmoid_asc(distance_km, 30.0, 0.08)
    }

    /// Pollen: descending sigmoid, mid=50 grains/m³, k=0.06.
    /// 0 ≈ 95, 20 ≈ 86, 50 = 50, 100 ≈ 5.
    pub fn normalize_pollen(max_pollen: f64) -> u32 {
        sigmoid_desc(max_pollen, 50.0, 0.06)
    }

    /// Pressure: Gaussian around 1013 hPa, σ=10.
    /// With rapid-change penalty: multiply by sigmoid_desc on |Δ|.
    /// 1013 = 100, 1003 ≈ 37, 993 ≈ 2.
    pub fn normalize_pressure(current_hpa: f64, change_3h: Option<f64>) -> u32 {
        let base = gaussian(current_hpa, 1013.0, 10.0) as f64;
        let penalty = match change_3h {
            Some(c) if c.abs() > 0.0 => {
                // Rapid change penalty: sigmoid centered at 5 hPa/3h
                1.0 - sigmoid(c.abs(), 5.0, 0.8) * 0.5
            }
            _ => 1.0,
        };
        (base * penalty).round() as u32
    }

    /// Geomagnetic: descending sigmoid on Kp, mid=4.0, k=0.8.
    /// Kp 0 ≈ 96, Kp 3 ≈ 69, Kp 5 ≈ 31, Kp 9 ≈ 2.
    pub fn normalize_geomagnetic(kp: f64) -> u32 {
        sigmoid_desc(kp, 4.0, 0.8)
    }

    /// Moon phase: cosine comfort. phase 0..1, full=0.5.
    /// cos(2π * phase) maps: new(0)=100, full(0.5)=0, quarter=50.
    /// Smooth, no piecewise.
    pub fn normalize_moon(phase: f64) -> u32 {
        let cos_val = (2.0 * std::f64::consts::PI * phase).cos();
        // cos: new=+1, full=-1. Map [-1,1] → [0,100]
        ((cos_val + 1.0) * 50.0).round() as u32
    }

    /// Daylight: ascending sigmoid on hours, mid=10h, k=0.5.
    /// 6h ≈ 12, 8h ≈ 27, 10h = 50, 12h ≈ 73, 14h ≈ 88.
    pub fn normalize_daylight(hours: f64) -> u32 {
        sigmoid_asc(hours, 10.0, 0.5)
    }

    /// Humidity: Gaussian around ideal 50%, σ=25.
    /// 50% = 100, 30% ≈ 55, 70% ≈ 55, 20% ≈ 28, 80% ≈ 28, 0% ≈ 2.
    pub fn normalize_humidity(humidity_pct: f64) -> u32 {
        gaussian(humidity_pct, 50.0, 25.0)
    }

    /// Noise: descending sigmoid on dB, mid=60dB, k=0.15.
    /// 40dB ≈ 95, 50dB ≈ 82, 60dB = 50, 75dB ≈ 10, 85dB ≈ 2.
    pub fn normalize_noise(db: f64) -> u32 {
        sigmoid_desc(db, 60.0, 0.15)
    }

    /// Moon phase calculation (Conway's algorithm). Returns 0-1.
    pub fn moon_phase(year: i32, month: u32, day: u32) -> f64 {
        let mut r = (year % 100) as f64;
        let r_mod = r as i32 % 19;
        let r_adj = if r_mod > 9 { r_mod - 19 } else { r_mod };
        r = ((r_adj * 11) % 30) as f64 + month as f64 + day as f64;
        if month < 3 {
            r += 2.0;
        }
        r -= if year < 2000 { 4.0 } else { 8.3 };
        let mut result = (r + 0.5).floor() as i32 % 30;
        if result < 0 {
            result += 30;
        }
        result as f64 / 30.0
    }

    // -------------------------------------------------------------------
    // Tests: sigmoid properties (monotonicity, bounds, smoothness)
    // -------------------------------------------------------------------

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_sigmoid_primitives() {
            // sigmoid(mid, mid, k) = 0.5 for any k
            assert!((sigmoid(5.0, 5.0, 1.0) - 0.5).abs() < 0.001);
            // sigmoid(-∞) → 0, sigmoid(+∞) → 1
            assert!(sigmoid(-100.0, 0.0, 1.0) < 0.001);
            assert!(sigmoid(100.0, 0.0, 1.0) > 0.999);
        }

        #[test]
        fn test_air_monotone_decreasing() {
            // Higher PM2.5 → lower score (monotone)
            let s0 = normalize_air(0.0);
            let s12 = normalize_air(12.0);
            let s35 = normalize_air(35.0);
            let s150 = normalize_air(150.0);
            assert!(s0 > s12, "0 > 12: {} > {}", s0, s12);
            assert!(s12 > s35, "12 > 35: {} > {}", s12, s35);
            assert!(s35 > s150, "35 > 150: {} > {}", s35, s150);
            assert!(s0 >= 90, "clean air ≥ 90, got {}", s0);
            assert!(s150 < 20, "hazardous < 20, got {}", s150);
        }

        #[test]
        fn test_temperature_bell() {
            // Peak at 23°C, symmetric decay
            let peak = normalize_temperature(23.0);
            let warm = normalize_temperature(30.0);
            let cold = normalize_temperature(10.0);
            let extreme = normalize_temperature(0.0);
            assert_eq!(peak, 100);
            assert!(warm > 25 && warm < 80, "30°C: {warm}");
            assert!(cold > 15 && cold < 60, "10°C: {cold}");
            assert!(extreme < 15, "0°C: {extreme}");
        }

        #[test]
        fn test_pressure_gaussian_with_penalty() {
            let ideal = normalize_pressure(1013.0, None);
            let off10 = normalize_pressure(1023.0, None);
            let off20 = normalize_pressure(993.0, None);
            assert_eq!(ideal, 100);
            assert!(off10 > 30, "±10 hPa: {off10}");
            assert!(off20 < 20, "±20 hPa: {off20}");
            // Rapid change penalty
            let with_change = normalize_pressure(1013.0, Some(8.0));
            assert!(with_change < ideal, "rapid change should penalize: {with_change} < {ideal}");
        }

        #[test]
        fn test_humidity_gaussian() {
            let ideal = normalize_humidity(50.0);
            let dry = normalize_humidity(20.0);
            let wet = normalize_humidity(80.0);
            assert_eq!(ideal, 100);
            assert!(dry > 10 && dry < 50, "20%: {dry}");
            assert!(wet > 10 && wet < 50, "80%: {wet}");
            // Symmetric
            assert!((normalize_humidity(40.0) as i32 - normalize_humidity(60.0) as i32).abs() <= 1);
        }

        #[test]
        fn test_moon_cosine() {
            let new_moon = normalize_moon(0.0);
            let full_moon = normalize_moon(0.5);
            let quarter = normalize_moon(0.25);
            assert_eq!(new_moon, 100);
            assert_eq!(full_moon, 0);
            assert_eq!(quarter, 50);
        }

        #[test]
        fn test_fire_ascending_sigmoid() {
            let at_0 = normalize_fire(0.0);
            let at_30 = normalize_fire(30.0);
            let at_100 = normalize_fire(100.0);
            assert!(at_0 < 15, "0km: {at_0}");
            assert!((at_30 as i32 - 50).abs() <= 5, "30km ≈ 50: {at_30}");
            assert!(at_100 > 95, "100km: {at_100}");
        }

        #[test]
        fn test_all_bounded_0_100() {
            // Every normalize function must return 0..=100
            for x in [0.0, 1.0, 5.0, 10.0, 50.0, 100.0, 200.0, 500.0] {
                assert!(normalize_air(x) <= 100);
                assert!(normalize_uv(x) <= 100);
                assert!(normalize_marine(x) <= 100);
                assert!(normalize_pollen(x) <= 100);
                assert!(normalize_noise(x) <= 100);
                assert!(normalize_geomagnetic(x.min(9.0)) <= 100);
                assert!(normalize_daylight(x.min(24.0)) <= 100);
                assert!(normalize_fire(x) <= 100);
            }
            for t in [-20.0, 0.0, 23.0, 40.0, 50.0] {
                assert!(normalize_temperature(t) <= 100);
            }
            for h in [0.0, 20.0, 50.0, 80.0, 100.0] {
                assert!(normalize_humidity(h) <= 100);
            }
        }
    }

    // AI-NOTE: SignalComfort is a named-field JSON view of matrix::SignalRow.
    // Uses HashMap for forward-compat — adding signals doesn't require struct changes.

    /// Comfort snapshot. Named fields for JSON, backed by matrix::SignalRow.
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct SignalComfort {
        pub total: u32,
        #[serde(flatten)]
        pub scores: std::collections::HashMap<String, u32>,
    }

    impl SignalComfort {
        /// Build from a matrix row. Auto-maps all columns from macro.
        pub fn from_row(row: &super::matrix::SignalRow) -> Self {
            let mut scores = std::collections::HashMap::new();
            for (i, &name) in super::matrix::SIGNAL_NAMES.iter().enumerate() {
                scores.insert(name.to_string(), row.scores[i] as u32);
            }
            Self {
                total: row.weighted_score().round() as u32,
                scores,
            }
        }

        /// Build from JSON. Reconstructs total via matrix algebra.
        pub fn from_json_scores(json: &str) -> Result<Self, serde_json::Error> {
            let raw: Self = serde_json::from_str(json)?;
            let pairs: Vec<(&str, f64)> = raw.scores.iter()
                .filter_map(|(k, &v)| {
                    super::matrix::SIGNAL_NAMES.iter()
                        .find(|&&n| n == k.as_str())
                        .map(|&n| (n, v as f64))
                })
                .collect();
            let row = super::matrix::SignalRow::from_pairs(&pairs);
            Ok(Self::from_row(&row))
        }

        /// Get a specific signal score.
        pub fn get(&self, name: &str) -> Option<u32> {
            self.scores.get(name).copied()
        }
    }
}

// ---------------------------------------------------------------------------
// WASM bindings
// ---------------------------------------------------------------------------

#[cfg(feature = "wasm")]
pub mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    // -- AQI --

    #[wasm_bindgen]
    pub fn wasm_pm25_aqi(value: f64) -> u32 {
        pm25_aqi(value)
    }

    #[wasm_bindgen]
    pub fn wasm_pm10_aqi(value: f64) -> u32 {
        pm10_aqi(value)
    }

    #[wasm_bindgen]
    pub fn wasm_overall_aqi(json: &str) -> u32 {
        let data: CurrentData = match serde_json::from_str(json) {
            Ok(d) => d,
            Err(_) => return 0,
        };
        overall_aqi(&data).unwrap_or(0)
    }

    #[wasm_bindgen]
    pub fn wasm_aqi_category(aqi: u32) -> String {
        let cat = AqiCategory::from_aqi(aqi);
        serde_json::json!({
            "label": cat.label(),
            "emoji": cat.emoji(),
            "color": cat.color_hex(),
        })
        .to_string()
    }

    #[wasm_bindgen]
    pub fn wasm_pollutant_status(pollutant: &str, value: f64) -> String {
        let cat = match pollutant {
            "pm25" => get_pm25_status(value),
            "pm10" => get_pm10_status(value),
            "co" => get_co_status(value),
            "no2" => get_no2_status(value),
            "so2" => get_so2_status(value),
            "o3" => get_o3_status(value),
            _ => return serde_json::json!({"error": "unknown pollutant"}).to_string(),
        };
        serde_json::json!({
            "label": cat.label(),
            "emoji": cat.emoji(),
            "color": cat.color_hex(),
        })
        .to_string()
    }

    // -- Signal normalize (all 11 modules) --

    #[wasm_bindgen]
    pub fn wasm_normalize_air(pm25: f64) -> u32 {
        signal::normalize_air(pm25)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_temperature(temp_c: f64) -> u32 {
        signal::normalize_temperature(temp_c)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_uv(uv: f64) -> u32 {
        signal::normalize_uv(uv)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_marine(wave_height_m: f64) -> u32 {
        signal::normalize_marine(wave_height_m)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_earthquake(magnitude: f64) -> u32 {
        signal::normalize_earthquake(magnitude)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_fire(distance_km: f64) -> u32 {
        signal::normalize_fire(distance_km)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_pollen(max_pollen: f64) -> u32 {
        signal::normalize_pollen(max_pollen)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_pressure(current_hpa: f64, change_3h: f64) -> u32 {
        let change = if change_3h.is_nan() { None } else { Some(change_3h) };
        signal::normalize_pressure(current_hpa, change)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_geomagnetic(kp: f64) -> u32 {
        signal::normalize_geomagnetic(kp)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_moon(phase: f64) -> u32 {
        signal::normalize_moon(phase)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_daylight(hours: f64) -> u32 {
        signal::normalize_daylight(hours)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_humidity(humidity_pct: f64) -> u32 {
        signal::normalize_humidity(humidity_pct)
    }

    #[wasm_bindgen]
    pub fn wasm_normalize_noise(db: f64) -> u32 {
        signal::normalize_noise(db)
    }

    // AI-NOTE: when adding a new signal, add wasm_normalize_* here too
    #[wasm_bindgen]
    pub fn wasm_normalize_wind(speed_kmh: f64) -> u32 {
        signal::normalize_wind(speed_kmh)
    }

    #[wasm_bindgen]
    pub fn wasm_moon_phase(year: i32, month: u32, day: u32) -> f64 {
        signal::moon_phase(year, month, day)
    }

    // -- Full Signal comfort index --

    /// Input JSON: `{"air":22,"temperature":85,"uv":70,"sea":90,...}`
    /// Returns JSON: `{"total":75,"air":22,...}`
    #[wasm_bindgen]
    pub fn wasm_signal_comfort(json: &str) -> String {
        match signal::SignalComfort::from_json_scores(json) {
            Ok(c) => serde_json::to_string(&c).unwrap_or_default(),
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    // -- Feature vector for ML (delegates to matrix) --

    /// 35-dim ML vector from SignalComfort JSON.
    #[wasm_bindgen]
    pub fn wasm_signal_vector(json: &str) -> String {
        let comfort = match signal::SignalComfort::from_json_scores(json) {
            Ok(c) => c,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        // Build row from HashMap scores
        let pairs: Vec<(&str, f64)> = comfort.scores.iter()
            .filter_map(|(k, &v)| {
                matrix::SIGNAL_NAMES.iter()
                    .find(|&&n| n == k.as_str())
                    .map(|&n| (n, v as f64))
            })
            .collect();
        let row = matrix::SignalRow::from_pairs(&pairs);
        let mut m = matrix::SignalMatrix::new();
        m.push(0.0, row);
        match m.to_ml_vector() {
            Some(v) => serde_json::to_string(&v).unwrap_or_default(),
            None => serde_json::json!({"error": "empty"}).to_string(),
        }
    }

    /// Feature names from matrix macro (single source of truth).
    #[wasm_bindgen]
    pub fn wasm_feature_names() -> String {
        serde_json::to_string(&matrix::SIGNAL_NAMES).unwrap_or_default()
    }

    // -- Comfort (original 6-component for CLI) --

    #[wasm_bindgen]
    pub fn wasm_comfort_score(json: &str) -> String {
        #[derive(Deserialize)]
        struct Input {
            air: CurrentData,
            weather: WeatherData,
            wind: WindData,
        }
        let input: Input = match serde_json::from_str(json) {
            Ok(i) => i,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        let score = calculate_comfort(&input.air, &input.weather, &input.wind);
        serde_json::to_string(&score).unwrap_or_default()
    }

    // -- Geo utilities --

    #[wasm_bindgen]
    pub fn wasm_geomagnetic(kp: f64) -> String {
        let data = GeomagneticData::from_kp(kp);
        serde_json::to_string(&data).unwrap_or_default()
    }

    #[wasm_bindgen]
    pub fn wasm_pollen_status(json: &str) -> String {
        let data: PollenData = match serde_json::from_str(json) {
            Ok(d) => d,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        serde_json::json!({
            "significant": data.is_significant(),
            "grass": data.grass_pollen.map(PollenData::pollen_label),
            "birch": data.birch_pollen.map(PollenData::pollen_label),
            "alder": data.alder_pollen.map(PollenData::pollen_label),
            "ragweed": data.ragweed_pollen.map(PollenData::pollen_label),
        })
        .to_string()
    }

    #[wasm_bindgen]
    pub fn wasm_haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        front::haversine(lat1, lon1, lat2, lon2)
    }

    #[wasm_bindgen]
    pub fn wasm_wind_direction(degrees: f64) -> String {
        let wind = WindData {
            wind_speed_10m: None,
            wind_direction_10m: Some(degrees),
            wind_gusts_10m: None,
        };
        serde_json::json!({
            "label": wind.direction_label(),
            "arrow": wind.direction_arrow(),
        })
        .to_string()
    }

    #[wasm_bindgen]
    pub fn wasm_progress_bar(score: u32) -> String {
        progress_bar(score)
    }

    // -- Matrix operations --

    /// Push a row into matrix JSON, return updated matrix JSON.
    /// row_json: `[80, 70, 90, ...]` (11 scores)
    #[wasm_bindgen]
    pub fn wasm_matrix_push(matrix_json: &str, ts: f64, row_json: &str) -> String {
        let mut m: matrix::SignalMatrix = match serde_json::from_str(matrix_json) {
            Ok(m) => m,
            Err(_) => matrix::SignalMatrix::new(),
        };
        let scores: [f64; matrix::N_SIGNALS] = match serde_json::from_str(row_json) {
            Ok(s) => s,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        m.push(ts, matrix::SignalRow { scores });
        serde_json::to_string(&m).unwrap_or_default()
    }

    /// Latest row as SignalComfort JSON.
    #[wasm_bindgen]
    pub fn wasm_matrix_latest(json: &str) -> String {
        let m: matrix::SignalMatrix = match serde_json::from_str(json) {
            Ok(m) => m,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        match m.to_comfort() {
            Some(c) => serde_json::to_string(&c).unwrap_or_default(),
            None => serde_json::json!({"error": "empty matrix"}).to_string(),
        }
    }

    /// Sub-matrix for last N hours.
    #[wasm_bindgen]
    pub fn wasm_matrix_slice(json: &str, hours: u32) -> String {
        let m: matrix::SignalMatrix = match serde_json::from_str(json) {
            Ok(m) => m,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        serde_json::to_string(&m.last_hours(hours)).unwrap_or_default()
    }

    /// ML feature vector (35 dimensions).
    #[wasm_bindgen]
    pub fn wasm_matrix_ml_vector(json: &str) -> String {
        let m: matrix::SignalMatrix = match serde_json::from_str(json) {
            Ok(m) => m,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        match m.to_ml_vector() {
            Some(v) => serde_json::to_string(&v).unwrap_or_default(),
            None => serde_json::json!({"error": "empty matrix"}).to_string(),
        }
    }

    /// Per-column summary statistics.
    #[wasm_bindgen]
    pub fn wasm_matrix_summary(json: &str) -> String {
        let m: matrix::SignalMatrix = match serde_json::from_str(json) {
            Ok(m) => m,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        serde_json::to_string(&m.summary()).unwrap_or_default()
    }

    /// Signal column names from macro.
    #[wasm_bindgen]
    pub fn wasm_signal_names() -> String {
        serde_json::to_string(&matrix::SIGNAL_NAMES).unwrap_or_default()
    }

    /// Signal weights from macro.
    #[wasm_bindgen]
    pub fn wasm_signal_weights() -> String {
        serde_json::to_string(&matrix::SIGNAL_WEIGHTS).unwrap_or_default()
    }

    // -- Cities database --

    /// Search cities by name prefix. Returns JSON array of {name, country, lat, lon}.
    /// Max 10 results.
    #[wasm_bindgen]
    pub fn wasm_search_cities(query: &str) -> String {
        let q = query.to_lowercase();
        let results: Vec<serde_json::Value> = cities::all()
            .iter()
            .filter(|c| c.city.to_lowercase().starts_with(&q))
            .take(10)
            .map(|c| serde_json::json!({
                "name": c.city,
                "country": c.country,
                "lat": c.latitude,
                "lon": c.longitude,
            }))
            .collect();
        serde_json::to_string(&results).unwrap_or_default()
    }

    /// Get major cities for a country. Returns JSON array.
    #[wasm_bindgen]
    pub fn wasm_major_cities(country: &str, limit: u32) -> String {
        let cities = get_major_cities(country, limit as usize);
        serde_json::to_string(&cities).unwrap_or_default()
    }

    /// List all countries. Returns JSON array of strings.
    #[wasm_bindgen]
    pub fn wasm_list_countries() -> String {
        serde_json::to_string(&list_countries()).unwrap_or_default()
    }
}
