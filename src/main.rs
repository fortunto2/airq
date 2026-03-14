use airq::{
    AqiCategory, Provider, aggregate_history, fetch_history, fetch_open_meteo,
    fetch_sensor_community, fetch_sensor_community_nearby, geocode, get_co_status,
    get_major_cities, get_no2_status, get_pm10_status, get_pm25_status, overall_aqi, pm25_aqi,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Latitude of the location
    #[arg(long)]
    lat: Option<f64>,

    /// Longitude of the location
    #[arg(long)]
    lon: Option<f64>,

    /// City name to resolve coordinates
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

        /// City name to resolve coordinates
        #[arg(long)]
        city: Option<String>,

        /// Search radius in km
        #[arg(long, default_value_t = 10.0)]
        radius: f64,
    },
    /// Show historical AQI data for a location
    History {
        /// City name to resolve coordinates
        #[arg(long)]
        city: String,
        /// Number of past days to show
        #[arg(long, default_value_t = 7)]
        days: u32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show top cities by AQI in a country
    Top {
        /// Country name (e.g., turkey, russia, usa, germany, japan)
        #[arg(long)]
        country: String,
        /// Number of cities to show
        #[arg(long, default_value_t = 5)]
        count: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Commands::Nearby {
        lat,
        lon,
        city,
        radius,
    }) = cli.command
    {
        let (lat, lon) = if let Some(city_name) = city {
            let (lat, lon, resolved_name) = geocode(&city_name).await?;
            println!("Resolved city: {}", resolved_name);
            (lat, lon)
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

    if let Some(Commands::History { city, days, json }) = &cli.command {
        let (lat, lon, resolved_name) = geocode(city).await?;
        let history = fetch_history(lat, lon, *days).await?;
        let daily_data = aggregate_history(&history.hourly);

        if *json {
            let json_data: Vec<serde_json::Value> = daily_data
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "date": d.date,
                        "pm2_5": d.pm2_5,
                        "pm10": d.pm10,
                        "aqi": d.pm2_5.map(|v| pm25_aqi(v)),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json_data)?);
            return Ok(());
        }

        println!("{} — last {} days", resolved_name, days);
        for day in daily_data {
            let pm25 = day.pm2_5.unwrap_or(0.0);
            let aqi = pm25_aqi(pm25);
            let cat = AqiCategory::from_aqi(aqi);

            // Sparkline logic (0-5 blocks based on AQI 0-150+)
            let blocks = match aqi {
                0..=25 => 1,
                26..=50 => 2,
                51..=100 => 3,
                101..=150 => 4,
                _ => 5,
            };
            let sparkline = format!("{}{}", "█".repeat(blocks), "░".repeat(5 - blocks));

            let text = format!(
                "{}: {} {:.1} µg/m³ (AQI {} {})",
                day.date,
                sparkline,
                pm25,
                aqi,
                cat.emoji()
            );
            println!("{}", cat.colorize(&text));
        }
        return Ok(());
    }

    if let Some(Commands::Top {
        country,
        count,
        json,
    }) = &cli.command
    {
        let cities = get_major_cities(country).unwrap_or(&[]);
        if cities.is_empty() {
            println!("No major cities found for country: {}", country);
            return Ok(());
        }

        let futures = cities.iter().map(|city| async move {
            if let Ok((lat, lon, resolved_name)) = geocode(city).await {
                if let Ok(data) = fetch_open_meteo(lat, lon).await {
                    let pm25 = data.current.pm2_5.unwrap_or(0.0);
                    let aqi = overall_aqi(&data.current).unwrap_or(0);
                    return Some((resolved_name, aqi, pm25));
                }
            }
            None
        });

        let mut results: Vec<_> = futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));

        if *json {
            let json_data: Vec<serde_json::Value> = results
                .iter()
                .take(*count)
                .enumerate()
                .map(|(i, (name, aqi, pm25))| {
                    serde_json::json!({
                        "rank": i + 1,
                        "city": name,
                        "aqi": aqi,
                        "pm2_5": pm25,
                        "category": AqiCategory::from_aqi(*aqi).label(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json_data)?);
            return Ok(());
        }

        println!("# City              AQI  PM2.5");
        for (i, (name, aqi, pm25)) in results.iter().take(*count).enumerate() {
            let cat = AqiCategory::from_aqi(*aqi);

            // Format city name to fixed width
            let short_name = name.split(',').next().unwrap_or(name);
            let padded_name = format!("{:width$}", short_name, width = 17);
            let padded_aqi = format!("{:<4}", aqi);

            let text = format!(
                "{} {} {} {} {:.1}",
                i + 1,
                padded_name,
                padded_aqi,
                cat.emoji(),
                pm25
            );
            println!("{}", cat.colorize(&text));
        }
        return Ok(());
    }

    let (lat, lon) = if let Some(city_name) = cli.city {
        let (lat, lon, resolved_name) = geocode(&city_name).await?;
        println!("Resolved city: {}", resolved_name);
        (lat, lon)
    } else if let (Some(lat), Some(lon)) = (cli.lat, cli.lon) {
        (lat, lon)
    } else {
        anyhow::bail!("Provide --city or --lat + --lon. Run airq --help for usage.")
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

    // Overall AQI
    if let Some(aqi) = overall_aqi(&data.current) {
        let cat = AqiCategory::from_aqi(aqi);
        println!("--------------------------------------------------");
        let text = format!("AQI: {} — {}", aqi, cat.label());
        println!("{} {}", cat.emoji(), cat.colorize(&text));
    }

    Ok(())
}
