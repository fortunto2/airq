use airq::{
    AqiCategory, Provider, aggregate_history, fetch_history, fetch_open_meteo,
    fetch_sensor_community, fetch_sensor_community_nearby, geocode, get_co_status,
    get_major_cities, get_no2_status, get_pm10_status, get_pm25_status, get_so2_status, get_o3_status, overall_aqi, pm25_aqi,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Air quality CLI — any city, model + real sensors merged",
    long_about = None,
    after_help = "\x1b[1mExamples:\x1b[0m
  airq --city tokyo                        Current air quality (model + sensors)
  airq --city gazipasa --json              JSON output
  airq --city berlin --provider open-meteo Model only
  airq history --city istanbul --days 7    Last 7 days sparkline
  airq top --country turkey                Top cities by AQI
  airq compare --city berlin               Side-by-side providers
  airq nearby --city gazipasa              Find sensors nearby

\x1b[1mData sources:\x1b[0m
  Open-Meteo (global model) + Sensor.Community (15K+ real sensors)
  All free, no API key needed.
"
)]
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
    #[arg(long, value_enum, default_value_t = Provider::All)]
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
    /// Compare Open-Meteo vs Sensor.Community side-by-side
    Compare {
        /// City name
        #[arg(long)]
        city: String,
        /// Sensor ID (single sensor) or omit for area average
        #[arg(long)]
        sensor_id: Option<u64>,
        /// Radius in km for area average (default 5)
        #[arg(long, default_value_t = 5.0)]
        radius: f64,
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
                        "aqi": d.us_aqi.map(|v| v.round() as u32).or_else(|| d.pm2_5.map(|v| pm25_aqi(v))),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json_data)?);
            return Ok(());
        }

        println!("{} — last {} days", resolved_name, days);
        for day in daily_data {
            let pm25 = day.pm2_5.unwrap_or(0.0);
            let aqi = day.us_aqi.map(|v| v.round() as u32).unwrap_or_else(|| pm25_aqi(pm25));
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

    if let Some(Commands::Compare {
        city,
        sensor_id,
        radius,
        json,
    }) = &cli.command
    {
        use airq::fetch_area_average;
        let (lat, lon, resolved_name) = geocode(city).await?;

        // Fetch Open-Meteo
        let om = fetch_open_meteo(lat, lon).await.ok();
        let om_pm25 = om.as_ref().and_then(|d| d.current.pm2_5);
        let om_pm10 = om.as_ref().and_then(|d| d.current.pm10);
        let om_aqi = om.as_ref().and_then(|d| d.current.us_aqi).map(|v| v.round() as u32);

        // Fetch Sensor.Community: single sensor or area average
        let (sc_pm25, sc_pm10, sc_label, sensor_info) = if let Some(sid) = sensor_id {
            let sc = fetch_sensor_community(*sid).await.ok();
            let pm25 = sc.as_ref().and_then(|d| d.current.pm2_5);
            let pm10 = sc.as_ref().and_then(|d| d.current.pm10);
            (pm25, pm10, format!("Sensor #{}", sid), String::new())
        } else {
            match fetch_area_average(lat, lon, *radius).await.ok() {
                Some(a) if a.sensor_count > 0 => {
                    let info = format!("{} sensors, {}km radius", a.sensor_count, radius);
                    (a.pm2_5_median, a.pm10_median, "Area Median".into(), info)
                }
                _ => (None, None, "No sensors".into(), String::new()),
            }
        };
        let sc_aqi = sc_pm25.map(|v| pm25_aqi(v));

        let avg_pm25 = match (om_pm25, sc_pm25) {
            (Some(a), Some(b)) => Some((a + b) / 2.0),
            (Some(a), None) | (None, Some(a)) => Some(a),
            _ => None,
        };
        let avg_pm10 = match (om_pm10, sc_pm10) {
            (Some(a), Some(b)) => Some((a + b) / 2.0),
            (Some(a), None) | (None, Some(a)) => Some(a),
            _ => None,
        };
        let avg_aqi = match (om_aqi, sc_aqi) {
            (Some(a), Some(b)) => Some((a + b) / 2),
            (Some(a), None) | (None, Some(a)) => Some(a),
            _ => None,
        };

        if *json {
            let data = serde_json::json!({
                "city": resolved_name,
                "open_meteo": { "pm2_5": om_pm25, "pm10": om_pm10, "us_aqi": om_aqi },
                "sensor_community": { "pm2_5": sc_pm25, "pm10": sc_pm10, "us_aqi": sc_aqi },
                "average": { "pm2_5": avg_pm25, "pm10": avg_pm10, "us_aqi": avg_aqi },
            });
            println!("{}", serde_json::to_string_pretty(&data)?);
            return Ok(());
        }

        let fmt = |v: Option<f64>| v.map(|x| format!("{:.1}", x)).unwrap_or_else(|| "N/A".into());
        let fmt_u = |v: Option<u32>| v.map(|x| format!("{}", x)).unwrap_or_else(|| "N/A".into());

        println!("{} — Provider Comparison", resolved_name);
        if !sensor_info.is_empty() {
            println!("Sensor.Community: {}", sensor_info);
        }
        println!("┌──────────┬───────────┬─────────────────┬─────────┐");
        println!("│ Metric   │ Open-Meteo│ {:>15} │ Average │", sc_label);
        println!("├──────────┼───────────┼─────────────────┼─────────┤");
        println!("│ PM2.5    │ {:>9} │ {:>15} │ {:>7} │", fmt(om_pm25), fmt(sc_pm25), fmt(avg_pm25));
        println!("│ PM10     │ {:>9} │ {:>15} │ {:>7} │", fmt(om_pm10), fmt(sc_pm10), fmt(avg_pm10));
        println!("│ US AQI   │ {:>9} │ {:>15} │ {:>7} │", fmt_u(om_aqi), format!("{} (calc)", fmt_u(sc_aqi)), fmt_u(avg_aqi));
        println!("└──────────┴───────────┴─────────────────┴─────────┘");

        if let Some(aqi) = avg_aqi {
            let cat = AqiCategory::from_aqi(aqi);
            println!("\n{} Average AQI: {} — {}", cat.emoji(), aqi, cat.label());
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

    // Per-source raw values for breakdown display
    struct SourceBreakdown {
        om_pm25: Option<f64>,
        om_pm10: Option<f64>,
        sc_pm25: Option<f64>,
        sc_pm10: Option<f64>,
        sensor_count: usize,
    }

    let (data, sources_msg, breakdown) = match cli.provider {
        Provider::All => {
            let (om_res, sc_res) = tokio::join!(
                fetch_open_meteo(lat, lon),
                airq::fetch_area_average(lat, lon, 5.0)
            );
            let mut data = om_res?;
            let om_pm25 = data.current.pm2_5;
            let om_pm10 = data.current.pm10;
            let mut sc_pm25 = None;
            let mut sc_pm10 = None;
            let mut sensor_count = 0;
            let mut msg = "Open-Meteo only (no nearby sensors)".to_string();

            if let Ok(sc_data) = sc_res {
                if sc_data.sensor_count > 0 {
                    sensor_count = sc_data.sensor_count;
                    sc_pm25 = sc_data.pm2_5_median;
                    sc_pm10 = sc_data.pm10_median;
                    msg = format!(
                        "Open-Meteo (model) + Sensor.Community ({} sensors, 5km median)",
                        sensor_count
                    );

                    // Merge: average if both available
                    if let (Some(om), Some(sc)) = (om_pm25, sc_pm25) {
                        data.current.pm2_5 = Some((om + sc) / 2.0);
                    }
                    if let (Some(om), Some(sc)) = (om_pm10, sc_pm10) {
                        data.current.pm10 = Some((om + sc) / 2.0);
                    }
                    data.current.us_aqi = overall_aqi(&data.current).map(|v| v as f64);
                }
            }
            let bd = SourceBreakdown { om_pm25, om_pm10, sc_pm25, sc_pm10, sensor_count };
            (data, msg, Some(bd))
        }
        Provider::OpenMeteo => (
            fetch_open_meteo(lat, lon).await?,
            "Open-Meteo".to_string(),
            None,
        ),
        Provider::SensorCommunity => {
            let sensor_id = cli
                .sensor_id
                .context("sensor-id is required for sensor-community provider")?;
            (
                fetch_sensor_community(sensor_id).await?,
                format!("Sensor.Community (#{})", sensor_id),
                None,
            )
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
    println!("Sources: {}", sources_msg);
    println!("--------------------------------------------------");

    let show = |name: &str, val: Option<f64>, unit: &str, status_fn: fn(f64) -> airq::AqiCategory| {
        if let Some(v) = val {
            let cat = status_fn(v);
            println!("{}", cat.colorize(&format!("{:<6}{:.1} {}", name, v, unit)));
        }
    };

    // PM2.5/PM10 with per-source breakdown
    if let Some(ref bd) = breakdown {
        if bd.sensor_count > 0 {
            let fmt = |v: Option<f64>| {
                v.map(|x| format!("{:.1}", x))
                    .unwrap_or_else(|| "—".into())
            };
            if let Some(avg) = data.current.pm2_5 {
                let cat = get_pm25_status(avg);
                println!(
                    "{}",
                    cat.colorize(&format!(
                        "PM2.5  {:.1} avg  ({} model, {} sensors) {}",
                        avg,
                        fmt(bd.om_pm25),
                        fmt(bd.sc_pm25),
                        &data.current_units.pm2_5
                    ))
                );
            }
            if let Some(avg) = data.current.pm10 {
                let cat = get_pm10_status(avg);
                println!(
                    "{}",
                    cat.colorize(&format!(
                        "PM10   {:.1} avg  ({} model, {} sensors) {}",
                        avg,
                        fmt(bd.om_pm10),
                        fmt(bd.sc_pm10),
                        &data.current_units.pm10
                    ))
                );
            }
        } else {
            show("PM2.5", data.current.pm2_5, &data.current_units.pm2_5, get_pm25_status);
            show("PM10", data.current.pm10, &data.current_units.pm10, get_pm10_status);
        }
    } else {
        show("PM2.5", data.current.pm2_5, &data.current_units.pm2_5, get_pm25_status);
        show("PM10", data.current.pm10, &data.current_units.pm10, get_pm10_status);
    }

    show("CO", data.current.carbon_monoxide, &data.current_units.carbon_monoxide, get_co_status);
    show("NO2", data.current.nitrogen_dioxide, &data.current_units.nitrogen_dioxide, get_no2_status);
    show("O3", data.current.ozone, &data.current_units.ozone, get_o3_status);
    show("SO2", data.current.sulphur_dioxide, &data.current_units.sulphur_dioxide, get_so2_status);

    if let Some(uv) = data.current.uv_index {
        let (emoji, label) = match uv {
            v if v < 3.0 => ("☀️", "Low"),
            v if v < 6.0 => ("🌤️", "Moderate"),
            v if v < 8.0 => ("🌞", "High"),
            v if v < 11.0 => ("🥵", "Very High"),
            _ => ("🔥", "Extreme"),
        };
        println!("UV Index: {} {} ({})", uv, emoji, label);
    }

    // Overall AQI
    if let Some(api_aqi) = data.current.us_aqi {
        let aqi = api_aqi.round() as u32;
        let cat = AqiCategory::from_aqi(aqi);
        println!("--------------------------------------------------");
        
        let mut text = format!("US AQI: {}", aqi);
        if let Some(eu_aqi) = data.current.european_aqi {
            text.push_str(&format!(" | EU AQI: {}", eu_aqi.round() as u32));
        }
        text.push_str(&format!(" — {}", cat.label()));
        
        println!("{} {}", cat.emoji(), cat.colorize(&text));
    } else if let Some(aqi) = overall_aqi(&data.current) {
        let cat = AqiCategory::from_aqi(aqi);
        println!("--------------------------------------------------");
        let text = format!("US AQI: {} — {}", aqi, cat.label());
        println!("{} {}", cat.emoji(), cat.colorize(&text));
    }

    Ok(())
}
