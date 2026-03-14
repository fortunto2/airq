use airq::{
    fetch_open_meteo, fetch_sensor_community, fetch_sensor_community_nearby, get_co_status,
    get_no2_status, get_pm10_status, get_pm25_status, Provider,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Latitude of the location
    #[arg(long, required_unless_present = "city")]
    lat: Option<f64>,

    /// Longitude of the location
    #[arg(long, required_unless_present = "city")]
    lon: Option<f64>,

    /// Preset city name (moscow, istanbul, gazipasa, berlin, tokyo)
    #[arg(long)]
    city: Option<String>,

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

#[derive(Subcommand, Debug)]
enum Commands {
    /// Find nearby sensors from sensor.community
    Nearby {
        /// Latitude of the location
        #[arg(long, required_unless_present = "city")]
        lat: Option<f64>,

        /// Longitude of the location
        #[arg(long, required_unless_present = "city")]
        lon: Option<f64>,

        /// Preset city name
        #[arg(long)]
        city: Option<String>,

        /// Search radius in km
        #[arg(long, default_value_t = 10.0)]
        radius: f64,
    },
}

fn get_city_coords(city: &str) -> Option<(f64, f64)> {
    match city.to_lowercase().as_str() {
        "moscow" => Some((55.7558, 37.6173)),
        "istanbul" => Some((41.0082, 28.9784)),
        "gazipasa" => Some((36.2694, 32.3179)),
        "berlin" => Some((52.5200, 13.4050)),
        "tokyo" => Some((35.6762, 139.6503)),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Commands::Nearby { lat, lon, city, radius }) = cli.command {
        let (lat, lon) = if let Some(city_name) = city {
            get_city_coords(&city_name).context(format!("Unknown city: {}", city_name))?
        } else {
            (lat.unwrap(), lon.unwrap())
        };

        let sensors = fetch_sensor_community_nearby(lat, lon, radius).await?;
        
        if sensors.is_empty() {
            println!("No sensors found within {}km of {}, {}", radius, lat, lon);
        } else {
            println!("Found {} sensors within {}km:", sensors.len(), radius);
            for sensor in sensors {
                println!("- Sensor ID: {}", sensor.id);
            }
        }
        
        return Ok(());
    }

    let (lat, lon) = if let Some(city_name) = cli.city {
        get_city_coords(&city_name).context(format!("Unknown city: {}", city_name))?
    } else {
        (cli.lat.unwrap(), cli.lon.unwrap())
    };

    let data = match cli.provider {
        Provider::OpenMeteo => fetch_open_meteo(lat, lon).await?,
        Provider::SensorCommunity => {
            let sensor_id = cli
                .sensor_id
                .context("sensor-id is required for sensor-community provider")?;
            fetch_sensor_community(sensor_id).await?
        }
    };

    if cli.json {
        let json_output = serde_json::to_string_pretty(&data)?;
        println!("{}", json_output);
        return Ok(());
    }

    println!(
        "Air Quality for Coordinates: {}, {}",
        data.latitude, data.longitude
    );
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
