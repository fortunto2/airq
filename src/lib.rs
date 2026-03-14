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

pub enum Status {
    Good,
    Moderate,
    Poor,
}

impl Status {
    pub fn colorize(&self, text: &str) -> colored::ColoredString {
        use colored::Colorize;
        match self {
            Status::Good => text.green(),
            Status::Moderate => text.yellow(),
            Status::Poor => text.red(),
        }
    }
}

pub fn get_pm25_status(value: f64) -> Status {
    if value <= 15.0 {
        Status::Good
    } else if value <= 35.0 {
        Status::Moderate
    } else {
        Status::Poor
    }
}

pub fn get_pm10_status(value: f64) -> Status {
    if value <= 45.0 {
        Status::Good
    } else if value <= 100.0 {
        Status::Moderate
    } else {
        Status::Poor
    }
}

pub fn get_co_status(value: f64) -> Status {
    if value <= 4000.0 {
        Status::Good
    } else if value <= 10000.0 {
        Status::Moderate
    } else {
        Status::Poor
    }
}

pub fn get_no2_status(value: f64) -> Status {
    if value <= 25.0 {
        Status::Good
    } else if value <= 50.0 {
        Status::Moderate
    } else {
        Status::Poor
    }
}

pub async fn fetch_open_meteo(lat: f64, lon: f64) -> Result<AirQualityResponse> {
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}&current=pm2_5,pm10,carbon_monoxide,nitrogen_dioxide&timezone=auto",
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
    let url = format!("https://data.sensor.community/airrohr/v1/sensor/{}/", sensor_id);
    
    let response = reqwest::get(&url)
        .await
        .context("Failed to send request to Sensor.Community API")?
        .json::<Vec<SensorCommunityResponse>>()
        .await
        .context("Failed to parse JSON response")?;

    let latest = response.into_iter().next().context("No data found for sensor")?;

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
        current: CurrentData { pm2_5, pm10, carbon_monoxide: None, nitrogen_dioxide: None },
        current_units: CurrentUnits {
            pm2_5: "µg/m³".to_string(),
            pm10: "µg/m³".to_string(),
            carbon_monoxide: "".to_string(),
            nitrogen_dioxide: "".to_string(),
        },
    })
}

pub async fn fetch_sensor_community_nearby(lat: f64, lon: f64, radius: f64) -> Result<Vec<SensorInfo>> {
    let url = format!("https://data.sensor.community/airrohr/v1/filter/area={},{},{}", lat, lon, radius);
    
    let response = reqwest::get(&url)
        .await
        .context("Failed to send request to Sensor.Community API")?
        .json::<Vec<serde_json::Value>>()
        .await
        .context("Failed to parse JSON response")?;

    let mut sensors = std::collections::HashSet::new();
    for item in response {
        if let Some(sensor) = item.get("sensor")
            && let Some(id) = sensor.get("id").and_then(|id| id.as_u64()) {
            sensors.insert(id);
        }
    }

    Ok(sensors.into_iter().map(|id| SensorInfo { id }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pm25_status() {
        assert!(matches!(get_pm25_status(10.0), Status::Good));
        assert!(matches!(get_pm25_status(15.0), Status::Good));
        assert!(matches!(get_pm25_status(20.0), Status::Moderate));
        assert!(matches!(get_pm25_status(35.0), Status::Moderate));
        assert!(matches!(get_pm25_status(40.0), Status::Poor));
    }

    #[test]
    fn test_pm10_status() {
        assert!(matches!(get_pm10_status(30.0), Status::Good));
        assert!(matches!(get_pm10_status(45.0), Status::Good));
        assert!(matches!(get_pm10_status(60.0), Status::Moderate));
        assert!(matches!(get_pm10_status(100.0), Status::Moderate));
        assert!(matches!(get_pm10_status(120.0), Status::Poor));
    }

    #[test]
    fn test_co_status() {
        assert!(matches!(get_co_status(2000.0), Status::Good));
        assert!(matches!(get_co_status(4000.0), Status::Good));
        assert!(matches!(get_co_status(6000.0), Status::Moderate));
        assert!(matches!(get_co_status(10000.0), Status::Moderate));
        assert!(matches!(get_co_status(12000.0), Status::Poor));
    }

    #[test]
    fn test_no2_status() {
        assert!(matches!(get_no2_status(15.0), Status::Good));
        assert!(matches!(get_no2_status(25.0), Status::Good));
        assert!(matches!(get_no2_status(35.0), Status::Moderate));
        assert!(matches!(get_no2_status(50.0), Status::Moderate));
        assert!(matches!(get_no2_status(60.0), Status::Poor));
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
            },
            current_units: CurrentUnits {
                pm2_5: "ug/m3".to_string(),
                pm10: "ug/m3".to_string(),
                carbon_monoxide: "ug/m3".to_string(),
                nitrogen_dioxide: "ug/m3".to_string(),
            },
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"latitude\":52.52"));
        assert!(json.contains("\"pm2_5\":10.0"));
    }
}
