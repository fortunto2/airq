use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Latitude of the location
    #[arg(long)]
    lat: f64,

    /// Longitude of the location
    #[arg(long)]
    lon: f64,
}

#[derive(Debug, Deserialize)]
struct AirQualityResponse {
    latitude: f64,
    longitude: f64,
    current: CurrentData,
    current_units: CurrentUnits,
}

#[derive(Debug, Deserialize)]
struct CurrentData {
    pm2_5: Option<f64>,
    pm10: Option<f64>,
    carbon_monoxide: Option<f64>,
    nitrogen_dioxide: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CurrentUnits {
    pm2_5: String,
    pm10: String,
    carbon_monoxide: String,
    nitrogen_dioxide: String,
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

async fn fetch_air_quality(lat: f64, lon: f64) -> Result<AirQualityResponse> {
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let data = fetch_air_quality(args.lat, args.lon).await?;

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
}
