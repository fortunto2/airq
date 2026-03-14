use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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

    let response = reqwest::get(&url)
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

pub fn get_major_cities(country: &str) -> Option<&'static [&'static str]> {
    match country.to_lowercase().as_str() {
        "turkey" => Some(&[
            "Istanbul",
            "Ankara",
            "Izmir",
            "Bursa",
            "Antalya",
            "Adana",
            "Gaziantep",
            "Konya",
            "Diyarbakir",
            "Gazipasa",
        ]),
        "russia" => Some(&[
            "Moscow",
            "Saint Petersburg",
            "Novosibirsk",
            "Yekaterinburg",
            "Kazan",
            "Nizhny Novgorod",
            "Chelyabinsk",
            "Krasnoyarsk",
            "Samara",
            "Ufa",
        ]),
        "usa" => Some(&[
            "New York",
            "Los Angeles",
            "Chicago",
            "Houston",
            "Phoenix",
            "Philadelphia",
            "San Antonio",
            "San Diego",
            "Dallas",
            "San Jose",
        ]),
        "germany" => Some(&[
            "Berlin",
            "Hamburg",
            "Munich",
            "Cologne",
            "Frankfurt",
            "Stuttgart",
            "Düsseldorf",
            "Leipzig",
            "Dortmund",
            "Essen",
        ]),
        "japan" => Some(&[
            "Tokyo", "Yokohama", "Osaka", "Nagoya", "Sapporo", "Fukuoka", "Kawasaki", "Kobe",
            "Kyoto", "Saitama",
        ]),
        _ => None,
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
        assert!(get_major_cities("turkey").is_some());
        assert!(get_major_cities("Turkey").is_some());
        assert!(get_major_cities("TURKEY").is_some());
        assert!(get_major_cities("unknown").is_none());

        let cities = get_major_cities("turkey").unwrap();
        assert!(cities.contains(&"Istanbul"));
    }
}
