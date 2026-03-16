use airq::{
    AppConfig, AqiCategory, Provider, aggregate_history, calculate_comfort,
    fetch_history, fetch_open_meteo, fetch_sensor_community, fetch_sensor_community_nearby,
    geocode, get_co_status, get_major_cities, get_no2_status, get_pm10_status, get_pm25_status,
    get_so2_status, get_o3_status, overall_aqi, pm25_aqi, progress_bar,
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
  airq --city tokyo --full                Extended (pollen, quakes, Kp)
  airq --city berlin --provider open-meteo Model only
  airq comfort --city tokyo               Detailed comfort breakdown
  airq history --city istanbul --days 7    Last 7 days sparkline
  airq top --country turkey                Top cities by AQI
  airq compare --city berlin               Side-by-side providers
  airq nearby --city gazipasa              Find sensors nearby
  airq blame --city moscow --radius 30    Find pollution sources (CPF)

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

    /// Show all cities from config
    #[arg(long)]
    all: bool,

    /// Show extended data (pollen, earthquakes, geomagnetic)
    #[arg(long)]
    full: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize or update configuration
    Init {
        /// Default city to set
        #[arg(long)]
        city: Option<String>,
    },
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
    /// Detect pollution fronts moving toward your city
    Front {
        /// City name (or uses default from config)
        #[arg(long)]
        city: Option<String>,
        /// Search radius for nearby cities in km
        #[arg(long, default_value_t = 100.0)]
        radius: f64,
        /// Number of past days to analyze
        #[arg(long, default_value_t = 2)]
        days: u32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate HTML report with map and pollution analysis
    Report {
        /// City name (or uses default from config)
        #[arg(long)]
        city: Option<String>,
        /// Search radius for nearby cities in km
        #[arg(long, default_value_t = 150.0)]
        radius: f64,
        /// Number of past days to analyze
        #[arg(long, default_value_t = 3)]
        days: u32,
        /// Output file path
        #[arg(long, default_value = "airq-report.html")]
        output: String,
        /// Also export as PDF (requires Chrome or wkhtmltopdf)
        #[arg(long)]
        pdf: bool,
    },
    /// Show top cities by AQI in a country (any country supported)
    Top {
        /// Country name (e.g., france, brazil, usa, japan, india)
        #[arg(long)]
        country: String,
        /// Number of cities to show
        #[arg(long, default_value_t = 5)]
        count: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// List all available countries
        #[arg(long)]
        list: bool,
    },
    /// Show detailed comfort index breakdown
    Comfort {
        /// City name
        #[arg(long)]
        city: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Identify pollution sources using wind-direction analysis (CPF)
    Blame {
        /// City name (or uses default from config)
        #[arg(long)]
        city: Option<String>,
        /// Search radius for sources in km
        #[arg(long, default_value_t = 20.0)]
        radius: f64,
        /// Number of past days to analyze
        #[arg(long, default_value_t = 7)]
        days: u32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate shell completions
    Completions {
        /// Shell type
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = AppConfig::load().unwrap_or_default();

    if let Some(Commands::Completions { shell }) = &cli.command {
        use clap::CommandFactory;
        clap_complete::generate(
            *shell,
            &mut Cli::command(),
            "airq",
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    if let Some(Commands::Init { city }) = &cli.command {
        let mut new_config = AppConfig::load().unwrap_or_default();
        
        if let Some(c) = city {
            new_config.default_city = Some(c.clone());
            println!("Set default city to: {}", c);
        } else {
            use std::io::{self, Write};
            print!("Enter default city: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();
            if !input.is_empty() {
                new_config.default_city = Some(input.to_string());
                println!("Set default city to: {}", input);
            }
        }
        
        new_config.save()?;
        return Ok(());
    }

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

    if let Some(Commands::Front {
        city,
        radius,
        days,
        json,
    }) = &cli.command
    {
        let city_name = city
            .clone()
            .or(config.default_city.clone())
            .context("Specify --city or set default with `airq init`")?;
        let (lat, lon, resolved_name) = geocode(&city_name).await?;
        println!("Analyzing pollution fronts around {}...", resolved_name);

        // Find nearby cities from built-in database
        let nearby = airq::front::nearby_cities(lat, lon, *radius, 10);
        if nearby.is_empty() {
            println!("No nearby cities found within {}km", radius);
            return Ok(());
        }

        // Fetch Open-Meteo history + wind + nearby sensors in parallel
        let (target_history, wind, dust_sensors) = tokio::join!(
            fetch_history(lat, lon, *days),
            airq::fetch_wind(lat, lon),
            airq::fetch_nearby_dust_sensors(lat, lon, *radius)
        );
        let target_history = target_history?;
        let wind = wind.ok();
        let dust_sensors = dust_sensors.unwrap_or_default();

        // Fetch Open-Meteo history for neighbors in batches
        let mut neighbor_data = Vec::new();
        for chunk in nearby.chunks(5) {
            let futures = chunk.iter().map(|c| async move {
                let dist = airq::front::haversine(lat, lon, c.lat, c.lon);
                match fetch_history(c.lat, c.lon, *days).await {
                    Ok(h) => Some((c.name.to_string(), c.lat, c.lon, dist, h.hourly.time, h.hourly.pm2_5)),
                    Err(_) => None,
                }
            });
            let batch: Vec<_> = futures::future::join_all(futures)
                .await
                .into_iter()
                .flatten()
                .collect();
            neighbor_data.extend(batch);
        }

        // Fetch Sensor.Community archive data async for nearby sensors
        let mut sensor_data: std::collections::HashMap<String, airq::front::SensorHourlyData> =
            std::collections::HashMap::new();
        if !dust_sensors.is_empty() {
            println!("Found {} dust sensors, fetching history...", dust_sensors.len());
            let sensor_ids: Vec<u64> = dust_sensors.iter().map(|(id, _, _)| *id).take(20).collect();
            let sensor_futures = sensor_ids.iter().map(|sid| {
                let sid = *sid;
                async move {
                    airq::fetch_sensor_archive(sid, *days).await.ok().map(|data| (sid, data))
                }
            });
            let sensor_results: Vec<_> = futures::future::join_all(sensor_futures)
                .await
                .into_iter()
                .flatten()
                .collect();

            // Map sensor readings to nearest city name
            for (sid, readings) in &sensor_results {
                if readings.is_empty() { continue; }
                // Find which neighbor this sensor is closest to
                let sensor_loc = dust_sensors.iter().find(|(id, _, _)| id == sid);
                if let Some((_, slat, slon)) = sensor_loc {
                    let mut best_city = resolved_name.clone();
                    let mut best_dist = airq::front::haversine(lat, lon, *slat, *slon);
                    for (name, _, _, _, _, _) in &neighbor_data {
                        // Find neighbor coords
                        if let Some(nb) = nearby.iter().find(|n| n.name == name) {
                            let d = airq::front::haversine(nb.lat, nb.lon, *slat, *slon);
                            if d < best_dist {
                                best_dist = d;
                                best_city = name.clone();
                            }
                        }
                    }
                    let entry = sensor_data.entry(best_city).or_default();
                    for (ts, val) in readings {
                        entry.entry(ts.clone()).or_insert(*val);
                    }
                }
            }
            if !sensor_data.is_empty() {
                println!("Sensor data for {} cities", sensor_data.len());
            }
        }

        let analysis = airq::front::build_graph_dual(
            &resolved_name,
            lat, lon,
            neighbor_data,
            &target_history.hourly.time,
            &target_history.hourly.pm2_5,
            &sensor_data,
        );

        if *json {
            let json_data = serde_json::json!({
                "city": resolved_name,
                "radius_km": radius,
                "days": days,
                "nearby_count": analysis.graph.node_count() - 1,
                "fronts": analysis.fronts.iter().map(|f| serde_json::json!({
                    "from": f.from_city,
                    "to": f.to_city,
                    "speed_kmh": (f.speed_kmh * 10.0).round() / 10.0,
                    "direction": airq::front::bearing_label(f.bearing_deg),
                    "bearing": f.bearing_deg.round(),
                    "lag_hours": f.lag_hours,
                    "correlation": (f.correlation * 100.0).round() / 100.0,
                })).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&json_data)?);
            return Ok(());
        }

        // Display nearby cities
        println!("Nearby cities ({}km radius):", radius);
        for (node_idx, _) in &analysis.spikes {
            let node = &analysis.graph[*node_idx];
            if node.distance_from_target > 0.0 {
                let brng = airq::front::bearing(lat, lon, node.lat, node.lon);
                println!(
                    "  {} ({:.0}km {})",
                    node.name,
                    node.distance_from_target,
                    airq::front::bearing_label(brng)
                );
            }
        }

        // Display wind context
        if let Some(ref w) = wind {
            if let Some(speed) = w.wind_speed_10m {
                let arrow = w.direction_arrow().unwrap_or("");
                let dir = w.direction_label().unwrap_or("");
                println!("\nCurrent wind: {:.1} km/h {} {}", speed, arrow, dir);
            }
        }

        // Display spikes
        let mut has_spikes = false;
        println!("\nSpike detection (last {}h):", days * 24);
        for (node_idx, spikes) in &analysis.spikes {
            let node = &analysis.graph[*node_idx];
            if !spikes.is_empty() {
                has_spikes = true;
                for spike in spikes.iter().take(3) {
                    let time_short = spike.time.replace('T', " ");
                    let cat = AqiCategory::from_aqi(pm25_aqi(spike.value));
                    println!(
                        "  {} {}: {:.1} µg/m³ (+{:.1}) {}",
                        cat.emoji(),
                        node.name,
                        spike.value,
                        spike.delta,
                        time_short,
                    );
                }
            }
        }
        if !has_spikes {
            println!("  No significant spikes detected.");
        }

        // Display top fronts (max 5, only strong correlations)
        let strong_fronts: Vec<_> = analysis.fronts.iter()
            .filter(|f| f.correlation > 0.7 && f.speed_kmh < 100.0)
            .take(5)
            .collect();

        if !strong_fronts.is_empty() {
            println!("\nPollution fronts detected:");
            for front in &strong_fronts {
                let dir_label = airq::front::bearing_label(front.bearing_deg);
                let arrow = airq::front::bearing_arrow(front.bearing_deg);
                println!(
                    "  {} {} → {} | {:.0} km/h {} | lag {}h | corr {:.0}%",
                    arrow,
                    front.from_city,
                    front.to_city,
                    front.speed_kmh,
                    dir_label,
                    front.lag_hours,
                    front.correlation * 100.0,
                );
            }

            // ETA: only for fronts moving TOWARD target city
            let mut warned = false;
            for front in &strong_fronts {
                if front.to_city == resolved_name {
                    // Front is already arriving at target
                    println!(
                        "\n  ⚠ {} → {} front detected (lag {}h, {:.0} km/h)",
                        front.from_city, resolved_name, front.lag_hours, front.speed_kmh
                    );
                    warned = true;
                } else {
                    // Check if front passed through a neighbor and is heading our way
                    let to_node = analysis.graph.node_indices()
                        .find(|n| analysis.graph[*n].name == front.to_city);
                    if let Some(to_idx) = to_node {
                        let to = &analysis.graph[to_idx];
                        let brng_to_target = airq::front::bearing(to.lat, to.lon, lat, lon);
                        let angle_diff = ((front.bearing_deg - brng_to_target).abs() % 360.0).min(
                            360.0 - (front.bearing_deg - brng_to_target).abs() % 360.0
                        );
                        // Front is roughly heading toward target (within 60°)
                        if angle_diff < 60.0 && front.speed_kmh > 1.0 {
                            let dist = airq::front::haversine(to.lat, to.lon, lat, lon);
                            let eta = dist / front.speed_kmh;
                            if eta < 24.0 {
                                println!(
                                    "\n  ⚠ Front heading toward {} — ETA ~{:.0}h ({:.0}km, {:.0} km/h)",
                                    resolved_name, eta, dist, front.speed_kmh
                                );
                                warned = true;
                            }
                        }
                    }
                }
            }
            if !warned {
                println!("\n  No fronts heading toward {} currently.", resolved_name);
            }
        } else {
            println!("\nNo significant pollution fronts detected in the last {} days.", days);
        }

        return Ok(());
    }

    if let Some(Commands::Report {
        city,
        radius,
        days,
        output,
        pdf,
    }) = &cli.command
    {
        let city_name = city
            .clone()
            .or(config.default_city.clone())
            .context("Specify --city or set default with `airq init`")?;
        let (lat, lon, resolved_name) = geocode(&city_name).await?;
        println!("Generating report for {}...", resolved_name);

        // Find nearby cities
        let nearby = airq::front::nearby_cities(lat, lon, *radius, 10);
        if nearby.is_empty() {
            println!("No nearby cities found within {}km", radius);
            return Ok(());
        }

        // Fetch Open-Meteo + wind + sensors + pollution sources in parallel
        let (target_history, wind, wind_history, dust_sensors, pollution_sources) = tokio::join!(
            fetch_history(lat, lon, *days),
            airq::fetch_wind(lat, lon),
            airq::fetch_wind_history(lat, lon, *days),
            airq::fetch_nearby_dust_sensors(lat, lon, *radius),
            airq::fetch_pollution_sources(lat, lon, 20.0) // 20km for sources
        );
        let target_history = target_history?;
        let wind = wind.ok();
        let wind_history = wind_history.ok();
        let dust_sensors = dust_sensors.unwrap_or_default();
        let pollution_sources = pollution_sources.unwrap_or_default();

        let mut neighbor_data = Vec::new();
        for chunk in nearby.chunks(5) {
            let futures = chunk.iter().map(|c| async move {
                let dist = airq::front::haversine(lat, lon, c.lat, c.lon);
                match fetch_history(c.lat, c.lon, *days).await {
                    Ok(h) => Some((c.name.to_string(), c.lat, c.lon, dist, h.hourly.time, h.hourly.pm2_5)),
                    Err(_) => None,
                }
            });
            let batch: Vec<_> = futures::future::join_all(futures)
                .await
                .into_iter()
                .flatten()
                .collect();
            neighbor_data.extend(batch);
        }

        // Cluster sensors and fetch archive data per cluster
        let clusters = airq::front::cluster_sensors(&dust_sensors, 5.0);
        let mut cluster_data: std::collections::HashMap<String, Vec<(String, f64)>> =
            std::collections::HashMap::new();

        if !clusters.is_empty() {
            println!("Found {} sensors in {} zones, fetching history...",
                dust_sensors.len(), clusters.len());

            // Pick 1-2 representative sensors per cluster (max 30 total)
            let mut fetch_list: Vec<(String, u64)> = Vec::new();
            for cluster in &clusters {
                for sid in cluster.sensor_ids.iter().take(2) {
                    fetch_list.push((cluster.id.clone(), *sid));
                }
            }
            fetch_list.truncate(30);

            // Fetch in parallel
            let sensor_futures = fetch_list.iter().map(|(cid, sid)| {
                let cid = cid.clone();
                let sid = *sid;
                async move {
                    airq::fetch_sensor_archive(sid, *days).await.ok().map(|data| (cid, data))
                }
            });
            let sensor_results: Vec<_> = futures::future::join_all(sensor_futures)
                .await
                .into_iter()
                .flatten()
                .collect();

            for (cid, readings) in sensor_results {
                if !readings.is_empty() {
                    let entry = cluster_data.entry(cid).or_default();
                    for (ts, val) in readings {
                        // Merge: keep first value per timestamp (or could median)
                        if !entry.iter().any(|(t, _)| t == &ts) {
                            entry.push((ts, val));
                        }
                    }
                }
            }
            // Sort each cluster's data by time
            for data in cluster_data.values_mut() {
                data.sort_by(|a, b| a.0.cmp(&b.0));
            }
            println!("Sensor data for {} zones", cluster_data.len());
        }

        // Build two analyses: city-level (Open-Meteo) + sensor-level (clusters)
        let mut sensor_data_map: std::collections::HashMap<String, airq::front::SensorHourlyData> =
            std::collections::HashMap::new();
        // Map cluster data to nearest city for dual-source
        for (cid, data) in &cluster_data {
            if let Some(cluster) = clusters.iter().find(|c| &c.id == cid) {
                let mut best_city = resolved_name.clone();
                let mut best_dist = airq::front::haversine(lat, lon, cluster.lat, cluster.lon);
                for (name, _, _, _, _, _) in &neighbor_data {
                    if let Some(nb) = nearby.iter().find(|n| n.name == name) {
                        let d = airq::front::haversine(nb.lat, nb.lon, cluster.lat, cluster.lon);
                        if d < best_dist {
                            best_dist = d;
                            best_city = name.clone();
                        }
                    }
                }
                let entry = sensor_data_map.entry(best_city).or_default();
                for (ts, val) in data {
                    entry.entry(ts.clone()).or_insert(*val);
                }
            }
        }

        // City-level analysis (dual-source)
        let city_analysis = airq::front::build_graph_dual(
            &resolved_name, lat, lon,
            neighbor_data,
            &target_history.hourly.time,
            &target_history.hourly.pm2_5,
            &sensor_data_map,
        );

        // Sensor cluster analysis (if we have enough clusters with data)
        let analysis = if cluster_data.len() >= 3 {
            let sensor_analysis = airq::front::build_sensor_graph(
                &resolved_name, lat, lon,
                &clusters, &cluster_data,
            );
            // Use sensor analysis if it found more fronts, otherwise city-level
            if sensor_analysis.fronts.len() > city_analysis.fronts.len() {
                println!("Using sensor-level analysis ({} zones)", clusters.len());
                sensor_analysis
            } else {
                city_analysis
            }
        } else {
            city_analysis
        };

        // Collect latest PM2.5 per sensor from cluster data
        let mut sensor_values: Vec<(u64, f64)> = Vec::new();
        for (cid, data) in &cluster_data {
            if let Some(cluster) = clusters.iter().find(|c| &c.id == cid) {
                // Get latest PM2.5 value from this cluster
                if let Some((_, val)) = data.last() {
                    for sid in &cluster.sensor_ids {
                        sensor_values.push((*sid, *val));
                    }
                }
            }
        }

        // Calculate CPF for pollution sources (if wind history available)
        let cpf_results = if let Some(ref wh) = wind_history {
            // Align PM2.5 + wind
            let mut pm_vals = Vec::new();
            let mut w_dirs = Vec::new();
            let mut w_spds = Vec::new();
            let wind_map: std::collections::HashMap<&str, (f64, f64)> = wh.hourly.time.iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    let dir = wh.hourly.wind_direction_10m.get(i).and_then(|v| *v)?;
                    let spd = wh.hourly.wind_speed_10m.get(i).and_then(|v| *v)?;
                    Some((t.as_str(), (dir, spd)))
                })
                .collect();
            for (i, t) in target_history.hourly.time.iter().enumerate() {
                if let Some(pm) = target_history.hourly.pm2_5.get(i).and_then(|v| *v) {
                    if let Some(&(dir, spd)) = wind_map.get(t.as_str()) {
                        pm_vals.push(pm);
                        w_dirs.push(dir);
                        w_spds.push(spd);
                    }
                }
            }
            if !pm_vals.is_empty() && !pollution_sources.is_empty() {
                airq::front::calculate_cpf(lat, lon, &pollution_sources, &pm_vals, &w_dirs, &w_spds, 0.75)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let html = airq::front::generate_report_full(
            &resolved_name,
            lat, lon,
            &analysis,
            wind.as_ref(),
            *days,
            &dust_sensors,
            &sensor_values,
            &pollution_sources,
            &cpf_results,
        );

        std::fs::write(output, &html)?;
        println!("Report saved to: {}", output);

        if *pdf {
            let html_path = std::fs::canonicalize(output)?;
            let pdf_path = output.replace(".html", ".pdf");

            // Try Chrome headless first, then wkhtmltopdf
            let chrome_paths = [
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "google-chrome",
                "chromium-browser",
                "chromium",
            ];

            let mut converted = false;
            for chrome in &chrome_paths {
                let result = std::process::Command::new(chrome)
                    .args([
                        "--headless",
                        "--disable-gpu",
                        "--no-sandbox",
                        &format!("--print-to-pdf={}", pdf_path),
                        &format!("file://{}", html_path.display()),
                    ])
                    .output();
                if let Ok(out) = result {
                    if out.status.success() {
                        println!("PDF saved to: {}", pdf_path);
                        converted = true;
                        break;
                    }
                }
            }

            if !converted {
                // Fallback to wkhtmltopdf
                let result = std::process::Command::new("wkhtmltopdf")
                    .args(["--enable-local-file-access", output, &pdf_path])
                    .output();
                match result {
                    Ok(out) if out.status.success() => {
                        println!("PDF saved to: {}", pdf_path);
                    }
                    _ => {
                        println!("PDF export failed. Install Chrome or wkhtmltopdf.");
                    }
                }
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
        list,
    }) = &cli.command
    {
        if *list {
            let countries = airq::list_countries();
            println!("{} countries available:", countries.len());
            for c in &countries {
                println!("  {}", c);
            }
            return Ok(());
        }

        // Fetch more cities than requested to rank properly, but cap at 15 to avoid rate limits
        let fetch_count = (*count).max(5).min(15) * 2;
        let cities = get_major_cities(country, fetch_count);
        if cities.is_empty() {
            println!("No cities found for country: {}", country);
            println!("Use `airq top --country x --list` to see available countries.");
            return Ok(());
        }

        // Fetch in batches of 5 to avoid rate limiting
        let mut results = Vec::new();
        for chunk in cities.chunks(5) {
            let futures = chunk.iter().map(|city| async move {
                if let Ok(data) = fetch_open_meteo(city.lat, city.lon).await {
                    let pm25 = data.current.pm2_5.unwrap_or(0.0);
                    let aqi = overall_aqi(&data.current).unwrap_or(0);
                    return Some((city.name.to_string(), aqi, pm25));
                }
                None
            });
            let batch: Vec<_> = futures::future::join_all(futures)
                .await
                .into_iter()
                .flatten()
                .collect();
            results.extend(batch);
        }

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

            let padded_name = format!("{:width$}", name, width = 17);
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

    if let Some(Commands::Blame {
        city,
        radius,
        days,
        json,
    }) = &cli.command
    {
        let city_name = city
            .clone()
            .or(config.default_city.clone())
            .context("Specify --city or set default with `airq init`")?;
        let (lat, lon, resolved_name) = geocode(&city_name).await?;

        if !*json {
            println!("Searching pollution sources near {}...", resolved_name);
        }

        // Fetch PM2.5 history + wind history + pollution sources in parallel
        let (pm_history, wind_history, sources) = tokio::join!(
            fetch_history(lat, lon, *days),
            airq::fetch_wind_history(lat, lon, *days),
            airq::fetch_pollution_sources(lat, lon, *radius)
        );
        let pm_history = pm_history?;
        let wind_history = wind_history?;
        let mut sources = sources?;

        // Merge manual sources from config
        if let Some(config_sources) = &config.sources {
            for cs in config_sources {
                let ps = airq::PollutionSource::from_config(cs, lat, lon);
                if ps.distance_km <= *radius {
                    sources.push(ps);
                }
            }
        }

        if sources.is_empty() {
            println!("No pollution sources found within {}km", radius);
            return Ok(());
        }

        // Align PM2.5 and wind data by timestamp
        let pm_times = &pm_history.hourly.time;
        let wind_times = &wind_history.hourly.time;

        // Build lookup of wind data by timestamp
        let mut wind_map: std::collections::HashMap<&str, (f64, f64)> =
            std::collections::HashMap::new();
        for (i, t) in wind_times.iter().enumerate() {
            if let (Some(dir), Some(spd)) = (
                wind_history.hourly.wind_direction_10m.get(i).and_then(|v| *v),
                wind_history.hourly.wind_speed_10m.get(i).and_then(|v| *v),
            ) {
                wind_map.insert(t.as_str(), (dir, spd));
            }
        }

        // Collect aligned data
        let mut pm25_vals: Vec<f64> = Vec::new();
        let mut wind_dirs: Vec<f64> = Vec::new();
        let mut wind_speeds: Vec<f64> = Vec::new();

        for (i, t) in pm_times.iter().enumerate() {
            if let Some(pm) = pm_history.hourly.pm2_5.get(i).and_then(|v| *v)
                && let Some(&(dir, spd)) = wind_map.get(t.as_str())
            {
                pm25_vals.push(pm);
                wind_dirs.push(dir);
                wind_speeds.push(spd);
            }
        }

        if pm25_vals.is_empty() {
            println!("No aligned PM2.5 + wind data found.");
            return Ok(());
        }

        // Calculate CPF
        let results =
            airq::front::calculate_cpf(lat, lon, &sources, &pm25_vals, &wind_dirs, &wind_speeds, 0.75);

        if *json {
            println!("{}", serde_json::to_string_pretty(&results)?);
            return Ok(());
        }

        // Count source types
        let mut type_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for s in &sources {
            *type_counts.entry(s.source_type.as_str()).or_default() += 1;
        }
        let type_summary: Vec<String> = type_counts
            .iter()
            .map(|(t, c)| format!("{} {}s", c, t.replace('_', " ")))
            .collect();

        println!(
            "\nPollution source attribution — {} ({} days)",
            resolved_name, days
        );
        println!(
            "Sources found: {} ({})",
            sources.len(),
            type_summary.join(", ")
        );
        println!("Aligned hourly observations: {}", pm25_vals.len());

        // Table header
        println!();
        println!(
            "{:<3} {:<26} {:<14} {:<6} {:<6} {:>10}",
            "#", "Source", "Type", "Dist", "CPF", "Avg PM2.5"
        );
        println!("{}", "-".repeat(70));

        for (i, r) in results.iter().enumerate() {
            if r.hours_in_sector == 0 {
                continue; // skip sources with no wind from that direction
            }
            let name = if r.source.name.chars().count() > 25 {
                let truncated: String = r.source.name.chars().take(22).collect();
                format!("{}...", truncated)
            } else {
                r.source.name.clone()
            };

            let type_label = r.source.source_type.replace('_', " ");

            // Color CPF: green < 0.3, yellow < 0.6, red >= 0.6
            let cpf_str = format!("{:.2}", r.cpf_score);
            let line = format!(
                "{:<3} {:<26} {:<14} {:<6} {:<6} {:>7.1} ug/m3",
                i + 1,
                name,
                type_label,
                format!("{}km", r.source.distance_km as u32),
                cpf_str,
                r.avg_pm25_in_sector,
            );

            use colored::Colorize;
            if r.cpf_score >= 0.6 {
                println!("{}", line.red());
            } else if r.cpf_score >= 0.3 {
                println!("{}", line.yellow());
            } else {
                println!("{}", line);
            }
        }

        // Background average
        if let Some(first_with_other) = results.iter().find(|r| r.hours_in_sector > 0) {
            println!(
                "  {:<28} {:>30} {:>7.1} ug/m3",
                "Background (other dirs)", "", first_with_other.avg_pm25_other
            );
        }

        return Ok(());
    }

    if let Some(Commands::Comfort { city, json }) = &cli.command {
        let city_name = city
            .clone()
            .or(config.default_city.clone())
            .context("Specify --city or set default with `airq init`")?;
        let (lat, lon, resolved_name) = geocode(&city_name).await?;

        let (air_res, wind_res, weather_res) = tokio::join!(
            fetch_open_meteo(lat, lon),
            airq::fetch_wind(lat, lon),
            airq::fetch_weather(lat, lon)
        );
        let air = air_res?;
        let wind = wind_res?;
        let weather = weather_res?;

        let comfort = calculate_comfort(&air.current, &weather, &wind);

        if *json {
            let data = serde_json::json!({
                "city": resolved_name,
                "comfort": comfort,
            });
            println!("{}", serde_json::to_string_pretty(&data)?);
            return Ok(());
        }

        println!("{} — Comfort Index: {}/100 — {}\n", resolved_name, comfort.total, comfort.label());

        let aqi = overall_aqi(&air.current).unwrap_or(0);
        let aqi_cat = AqiCategory::from_aqi(aqi);
        let temp_str = weather.apparent_temp_c
            .map(|t| format!("{:.0}\u{00b0}C", t))
            .unwrap_or_else(|| "N/A".into());
        let wind_str = wind.wind_speed_10m
            .map(|s| {
                let dir = wind.direction_label().unwrap_or("");
                format!("{:.0} km/h {}", s, dir)
            })
            .unwrap_or_else(|| "N/A".into());
        let uv_str = air.current.uv_index
            .map(|u| {
                let label = match u {
                    v if v < 3.0 => "Low",
                    v if v < 6.0 => "Moderate",
                    v if v < 8.0 => "High",
                    v if v < 11.0 => "Very High",
                    _ => "Extreme",
                };
                format!("{:.1} ({})", u, label)
            })
            .unwrap_or_else(|| "N/A".into());
        let pressure_str = weather.pressure_hpa
            .map(|p| format!("{:.0} hPa", p))
            .unwrap_or_else(|| "N/A".into());
        let humidity_str = weather.humidity_pct
            .map(|h| format!("{:.0}%", h))
            .unwrap_or_else(|| "N/A".into());

        println!("  Air Quality  {:>3}/100  {}  AQI {} ({})", comfort.air, progress_bar(comfort.air), aqi, aqi_cat.label());
        println!("  Temperature  {:>3}/100  {}  {}", comfort.temperature, progress_bar(comfort.temperature), temp_str);
        println!("  Wind         {:>3}/100  {}  {}", comfort.wind, progress_bar(comfort.wind), wind_str);
        println!("  UV           {:>3}/100  {}  {}", comfort.uv, progress_bar(comfort.uv), uv_str);
        println!("  Pressure     {:>3}/100  {}  {}", comfort.pressure, progress_bar(comfort.pressure), pressure_str);
        println!("  Humidity     {:>3}/100  {}  {}", comfort.humidity, progress_bar(comfort.humidity), humidity_str);

        return Ok(());
    }

    if cli.all {
        let cities = config.cities.unwrap_or_default();
        if cities.is_empty() {
            println!("No cities configured. Run `airq init` or edit config.");
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

        println!("# City              AQI  PM2.5");
        for (i, (name, aqi, pm25)) in results.iter().enumerate() {
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

    let (lat, lon) = if let Some(city_name) = cli.city.or(config.default_city) {
        let (lat, lon, resolved_name) = geocode(&city_name).await?;
        println!("Resolved city: {}", resolved_name);
        (lat, lon)
    } else if let (Some(lat), Some(lon)) = (cli.lat, cli.lon) {
        (lat, lon)
    } else {
        use clap::CommandFactory;
        Cli::command().print_help()?;
        return Ok(());
    };

    // Per-source raw values for breakdown display
    struct SourceBreakdown {
        om_pm25: Option<f64>,
        om_pm10: Option<f64>,
        sc_pm25: Option<f64>,
        sc_pm10: Option<f64>,
        sensor_count: usize,
    }

    // Per-source raw values for breakdown display
    #[allow(dead_code)]
    struct ExtendedData {
        weather: Option<airq::WeatherData>,
        pollen: Option<airq::PollenData>,
        earthquakes: Option<Vec<airq::EarthquakeInfo>>,
        geomagnetic: Option<airq::GeomagneticData>,
    }

    let (data, sources_msg, breakdown, wind, extended) = match cli.provider {
        Provider::All => {
            let (om_res, sc_res, wind_res, weather_res) = tokio::join!(
                fetch_open_meteo(lat, lon),
                airq::fetch_area_average(lat, lon, 5.0),
                airq::fetch_wind(lat, lon),
                airq::fetch_weather(lat, lon)
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

            // Fetch extended data in parallel if --full
            let ext = if cli.full {
                let (pollen_res, eq_res, geo_res) = tokio::join!(
                    airq::fetch_pollen(lat, lon),
                    airq::fetch_nearby_earthquakes(lat, lon, 200.0, 7),
                    airq::fetch_geomagnetic()
                );
                ExtendedData {
                    weather: weather_res.ok(),
                    pollen: pollen_res.ok(),
                    earthquakes: eq_res.ok(),
                    geomagnetic: geo_res.ok(),
                }
            } else {
                ExtendedData {
                    weather: weather_res.ok(),
                    pollen: None,
                    earthquakes: None,
                    geomagnetic: None,
                }
            };

            (data, msg, Some(bd), wind_res.ok(), ext)
        }
        Provider::OpenMeteo => {
            let (om_res, wind_res, weather_res) = tokio::join!(
                fetch_open_meteo(lat, lon),
                airq::fetch_wind(lat, lon),
                airq::fetch_weather(lat, lon)
            );
            let ext = ExtendedData {
                weather: weather_res.ok(),
                pollen: None,
                earthquakes: None,
                geomagnetic: None,
            };
            (om_res?, "Open-Meteo".to_string(), None, wind_res.ok(), ext)
        }
        Provider::SensorCommunity => {
            let sensor_id = cli
                .sensor_id
                .context("sensor-id is required for sensor-community provider")?;
            let (sc_res, wind_res, weather_res) = tokio::join!(
                fetch_sensor_community(sensor_id),
                airq::fetch_wind(lat, lon),
                airq::fetch_weather(lat, lon)
            );
            let ext = ExtendedData {
                weather: weather_res.ok(),
                pollen: None,
                earthquakes: None,
                geomagnetic: None,
            };
            (sc_res?, format!("Sensor.Community (#{})", sensor_id), None, wind_res.ok(), ext)
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
            v if v < 3.0 => ("\u{2600}\u{fe0f}", "Low"),
            v if v < 6.0 => ("\u{1f324}\u{fe0f}", "Moderate"),
            v if v < 8.0 => ("\u{1f31e}", "High"),
            v if v < 11.0 => ("\u{1f975}", "Very High"),
            _ => ("\u{1f525}", "Extreme"),
        };
        println!("UV Index: {} {} ({})", uv, emoji, label);
    }

    // Humidity & Pressure (compact, one line)
    if let Some(ref w) = extended.weather {
        let mut parts = Vec::new();
        if let Some(h) = w.humidity_pct {
            parts.push(format!("Humidity: {:.0}%", h));
        }
        if let Some(p) = w.pressure_hpa {
            let p_label = if (1010.0..=1020.0).contains(&p) {
                "stable"
            } else if p < 1005.0 {
                "low"
            } else if p > 1025.0 {
                "high"
            } else {
                "normal"
            };
            parts.push(format!("Pressure: {:.0} hPa ({})", p, p_label));
        }
        if !parts.is_empty() {
            println!("{}", parts.join(" | "));
        }
    }

    // Wind
    if let Some(ref w) = wind {
        if let Some(speed) = w.wind_speed_10m {
            let arrow = w.direction_arrow().unwrap_or("");
            let dir = w.direction_label().unwrap_or("");
            let gusts = w.wind_gusts_10m
                .map(|g| format!(" (gusts {:.0})", g))
                .unwrap_or_default();
            println!("Wind:   {:.1} km/h {} {}{}", speed, arrow, dir, gusts);
        }
    }

    // Comfort score (always shown, one line)
    {
        let default_weather = airq::WeatherData {
            pressure_hpa: None,
            humidity_pct: None,
            apparent_temp_c: None,
            precipitation_mm: None,
            cloud_cover_pct: None,
        };
        let default_wind = airq::WindData {
            wind_speed_10m: None,
            wind_direction_10m: None,
            wind_gusts_10m: None,
        };
        let w_ref = extended.weather.as_ref().unwrap_or(&default_weather);
        let wind_ref = wind.as_ref().unwrap_or(&default_wind);
        let comfort = calculate_comfort(&data.current, w_ref, wind_ref);
        println!("Comfort: {}/100 {} {}", comfort.total, comfort.emoji(), comfort.label());
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
        text.push_str(&format!(" \u{2014} {}", cat.label()));

        println!("{} {}", cat.emoji(), cat.colorize(&text));
    } else if let Some(aqi) = overall_aqi(&data.current) {
        let cat = AqiCategory::from_aqi(aqi);
        println!("--------------------------------------------------");
        let text = format!("US AQI: {} \u{2014} {}", aqi, cat.label());
        println!("{} {}", cat.emoji(), cat.colorize(&text));
    }

    // Extended data (--full flag)
    if cli.full {
        // Pollen
        if let Some(ref pollen) = extended.pollen {
            if pollen.is_significant() {
                println!("\n\u{1f33e} Pollen levels:");
                let show_pollen = |name: &str, val: Option<f64>| {
                    if let Some(v) = val {
                        if v > 10.0 {
                            println!("  {}: {:.0} grains/m\u{00b3} ({})", name, v, airq::PollenData::pollen_label(v));
                        }
                    }
                };
                show_pollen("Grass", pollen.grass_pollen);
                show_pollen("Birch", pollen.birch_pollen);
                show_pollen("Alder", pollen.alder_pollen);
                show_pollen("Ragweed", pollen.ragweed_pollen);
            }
        }

        // Earthquakes (only M3+ within 200km in last 7 days)
        if let Some(ref quakes) = extended.earthquakes {
            let significant: Vec<_> = quakes.iter().filter(|q| q.magnitude >= 3.0).take(3).collect();
            if !significant.is_empty() {
                println!("\n\u{1f30d} Recent earthquakes (M3+, 200km, 7d):");
                for q in &significant {
                    println!("  M{:.1} \u{2014} {} ({:.0}km away, {})", q.magnitude, q.place, q.distance_km, q.time);
                }
            }
        }

        // Geomagnetic (only if Kp >= 3)
        if let Some(ref geo) = extended.geomagnetic {
            if geo.kp_index >= 3.0 {
                let emoji = if geo.kp_index >= 5.0 { "\u{26a1}" } else { "\u{1f9f2}" };
                println!("\n{} Geomagnetic: Kp {:.1} ({})", emoji, geo.kp_index, geo.label);
            }
        }
    }

    Ok(())
}
