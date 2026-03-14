use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Latitude of the location
    #[arg(long)]
    lat: f64,

    /// Longitude of the location
    #[arg(long)]
    lon: f64,

    /// Output raw JSON
    #[arg(long)]
    json: bool,

    /// Data provider
    #[arg(long, value_enum, default_value_t = Provider::OpenMeteo)]
    provider: Provider,

    /// Sensor ID for Sensor.Community provider
    #[arg(long)]
    sensor_id: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AirQualityResponse {
    latitude: f64,
    longitude: f64,
    current: CurrentData,
    current_units: CurrentUnits,
}

#[derive(Debug, Deserialize, Serialize)]
struct CurrentData {
    pm2_5: Option<f64>,
    pm10: Option<f64>,
    carbon_monoxide: Option<f64>,
    nitrogen_dioxide: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CurrentUnits {
    pm2_5: String,
    pm10: String,
    carbon_monoxide: String,
    nitrogen_dioxide: String,
}

#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq)]
enum Provider {
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

enum Status {
    Good,
    Moderate,
    Poor,
}

impl Status {
    fn colorize(&self, text: &str) -> colored::ColoredString {
        match self {
            Status::Good => text.green(),
            Status::Moderate => text.yellow(),
            Status::Poor => text.red(),
        }
    }
}

fn get_pm25_status(value: f64) -> Status {
    if value <= 15.0 {
        Status::Good
    } else if value <= 35.0 {
        Status::Moderate
    } else {
        Status::Poor
    }
}

fn get_pm10_status(value: f64) -> Status {
    if value <= 45.0 {
        Status::Good
    } else if value <= 100.0 {
        Status::Moderate
    } else {
        Status::Poor
    }
}

fn get_co_status(value: f64) -> Status {
    if value <= 4000.0 {
        Status::Good
    } else if value <= 10000.0 {
        Status::Moderate
    } else {
        Status::Poor
    }
}

fn get_no2_status(value: f64) -> Status {
    if value <= 25.0 {
        Status::Good
    } else if value <= 50.0 {
        Status::Moderate
    } else {
        Status::Poor
    }
}

async fn fetch_open_meteo(lat: f64, lon: f64) -> Result<AirQualityResponse> {
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

async fn fetch_sensor_community(sensor_id: u64) -> Result<AirQualityResponse> {
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let data = match args.provider {
        Provider::OpenMeteo => fetch_open_meteo(args.lat, args.lon).await?,
        Provider::SensorCommunity => {
            let sensor_id = args.sensor_id.context("sensor-id is required for sensor-community provider")?;
            fetch_sensor_community(sensor_id).await?
        }
    };

    if args.json {
        let json_output = serde_json::to_string_pretty(&data)?;
        println!("{}", json_output);
        return Ok(());
    }

    println!("Air Quality for Coordinates: {}, {}", data.latitude, data.longitude);
    println!("--------------------------------------------------");

    if let Some(pm25) = data.current.pm2_5 {
        let status = get_pm25_status(pm25);
        let text = format!("PM2.5: {} {}", pm25, data.current_units.pm2_5);
        println!("{}", status.colorize(&text));
    } else {
        println!("PM2.5: N/A");
    }

    if let Some(pm10) = data.current.pm10 {
        let status = get_pm10_status(pm10);
        let text = format!("PM10: {} {}", pm10, data.current_units.pm10);
        println!("{}", status.colorize(&text));
    } else {
        println!("PM10: N/A");
    }

    if let Some(co) = data.current.carbon_monoxide {
        let status = get_co_status(co);
        let text = format!("CO: {} {}", co, data.current_units.carbon_monoxide);
        println!("{}", status.colorize(&text));
    } else {
        println!("CO: N/A");
    }

    if let Some(no2) = data.current.nitrogen_dioxide {
        let status = get_no2_status(no2);
        let text = format!("NO2: {} {}", no2, data.current_units.nitrogen_dioxide);
        println!("{}", status.colorize(&text));
    } else {
        println!("NO2: N/A");
    }

    Ok(())
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
