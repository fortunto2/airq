//! Entry point for `airq serve` — wires collector, push, API, and web dashboard.

use crate::db::Db;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// Configuration for the serve command.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub cities: Vec<CityConfig>,
    pub port: u16,
    pub db_path: PathBuf,
    pub interval_secs: u64,
}

#[derive(Debug, Clone)]
pub struct CityConfig {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub radius_km: f64,
}

/// Run the serve daemon: collector + push + API + web.
pub async fn run_serve(config: ServeConfig) -> Result<()> {
    let db = Db::open(&config.db_path)
        .context("open database")?;
    let db = Arc::new(db);

    // Register cities
    for city in &config.cities {
        db.upsert_city(&city.name, city.lat, city.lon, city.radius_km)
            .context("register city")?;
    }

    // Shutdown signal
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Build city list for collector
    let city_list: Vec<(String, f64, f64, f64)> = config
        .cities
        .iter()
        .map(|c| (c.name.clone(), c.lat, c.lon, c.radius_km))
        .collect();

    // Spawn collector
    let collector_db = db.clone();
    let collector_shutdown = shutdown_rx.clone();
    let collector_handle = tokio::spawn(crate::collector::run_collector(
        collector_db,
        city_list,
        Duration::from_secs(config.interval_secs),
        collector_shutdown,
    ));

    // Build Axum router
    let app = build_router(db.clone());

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context(format!("bind to {}", addr))?;

    eprintln!("🌍 airq serve running on http://localhost:{}", config.port);
    eprintln!("   Cities: {}", config.cities.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
    eprintln!("   DB: {}", config.db_path.display());
    eprintln!("   Poll interval: {}s", config.interval_secs);

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await
        .context("run axum server")?;

    // Wait for collector to finish
    let _ = collector_handle.await;

    eprintln!("airq serve stopped.");
    Ok(())
}

fn build_router(db: Arc<Db>) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/api/push", post(crate::push::push_handler))
        .route("/api/status", get(crate::api::status_handler))
        .route("/api/readings", get(crate::api::readings_handler))
        .route("/api/sensors", get(crate::api::sensors_handler))
        .route("/api/events", get(crate::api::events_handler))
        .route("/api/cities", get(crate::api::cities_handler))
        .route("/", get(crate::web::dashboard_handler))
        .with_state(db)
}

async fn shutdown_signal(tx: watch::Sender<bool>) {
    tokio::signal::ctrl_c()
        .await
        .expect("install Ctrl+C handler");
    eprintln!("\nShutting down...");
    let _ = tx.send(true);
}
