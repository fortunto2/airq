//! Root Dioxus component for Air Signal dashboard.

use crate::state::{self, MonitorSnapshot, SensorWithReading};
use airq::db::Db;
use airq::AppConfig;
use dioxus::prelude::*;
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL_SECS: u64 = 300;
const REFRESH_INTERVAL_MS: u64 = 10_000;

/// Root component.
#[component]
pub fn App() -> Element {
    // Load config from ~/.config/airq/config.toml (shared with CLI)
    let config = AppConfig::load().unwrap_or_default();
    let default_city = config.default_city.unwrap_or_else(|| "gazipasa".to_string());
    let default_radius = config.radius.unwrap_or(15.0);

    // Initialize DB and collector on first render
    let mut db: Signal<Option<Arc<Db>>> = use_signal(|| None);
    let mut snapshot: Signal<MonitorSnapshot> = use_signal(MonitorSnapshot::default);
    let mut collector_running: Signal<bool> = use_signal(|| false);
    let mut city_input: Signal<String> = use_signal(move || default_city.clone());
    let mut radius_input: Signal<f64> = use_signal(move || default_radius);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    // Start monitoring
    let start_monitoring = move |_| {
        let city_name = (city_input)().clone();
        let radius = (radius_input)();

        spawn(async move {
            // Geocode the city
            match airq::geocode(&city_name).await {
                Ok((lat, lon, resolved)) => {
                    tracing::info!("Resolved: {} ({:.2}, {:.2})", resolved, lat, lon);

                    match state::open_db() {
                        Ok(db_handle) => {
                            // Register city
                            let _ = db_handle.upsert_city(&city_name, lat, lon, radius);

                            db.set(Some(db_handle.clone()));
                            collector_running.set(true);
                            error_msg.set(None);

                            // Spawn collector in background
                            let collector_db = db_handle.clone();
                            let cities = vec![(city_name.clone(), lat, lon, radius)];
                            let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
                            let _ = shutdown_tx; // keep alive

                            tokio::spawn(airq::collector::run_collector(
                                collector_db,
                                cities,
                                Duration::from_secs(POLL_INTERVAL_SECS),
                                shutdown_rx,
                            ));

                            // Initial snapshot
                            let snap = state::build_snapshot(&db_handle);
                            snapshot.set(snap);
                        }
                        Err(e) => {
                            error_msg.set(Some(format!("DB error: {e}")));
                        }
                    }
                }
                Err(e) => {
                    error_msg.set(Some(format!("Geocode failed: {e}")));
                }
            }
        });
    };

    // Periodic refresh
    use_effect(move || {
        let is_running = (collector_running)();
        if !is_running {
            return;
        }
        spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(REFRESH_INTERVAL_MS)).await;
                if let Some(ref db_handle) = (db)() {
                    let snap = state::build_snapshot(db_handle);
                    snapshot.set(snap);
                }
            }
        });
    });

    let snap = (snapshot)();
    let is_running = (collector_running)();

    rsx! {
        style { {CSS} }

        div { class: "container",
            // Header
            header { class: "header",
                h1 { "Air Signal" }
                if is_running {
                    span { class: "status connected", "● Connected" }
                } else {
                    span { class: "status", "○ Stopped" }
                }
            }

            // Setup card (when not running)
            if !is_running {
                div { class: "card setup-card",
                    h2 { "Start Monitoring" }
                    div { class: "form-row",
                        label { "City" }
                        input {
                            r#type: "text",
                            value: "{city_input}",
                            oninput: move |e| city_input.set(e.value()),
                            placeholder: "e.g. gazipasha",
                        }
                    }
                    div { class: "form-row",
                        label { "Radius (km)" }
                        input {
                            r#type: "number",
                            value: "{radius_input}",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<f64>() {
                                    radius_input.set(v);
                                }
                            },
                        }
                    }
                    button {
                        class: "btn-start",
                        onclick: start_monitoring,
                        "Start"
                    }
                    if let Some(ref err) = (error_msg)() {
                        div { class: "error", "{err}" }
                    }
                }
            }

            // Stats row
            if is_running {
                div { class: "stats-row",
                    StatCard {
                        label: "PM2.5",
                        value: snap.avg_pm25.map(|v| format!("{v:.1}")).unwrap_or("—".into()),
                        color: pm25_color_class(snap.avg_pm25),
                    }
                    StatCard {
                        label: "PM10",
                        value: snap.avg_pm10.map(|v| format!("{v:.1}")).unwrap_or("—".into()),
                        color: pm25_color_class(snap.avg_pm10),
                    }
                    StatCard {
                        label: "Sensors",
                        value: format!("{}", snap.sensor_count),
                        color: "normal",
                    }
                }
            }

            // Sensor list
            if is_running && !snap.sensors.is_empty() {
                div { class: "card",
                    h2 { "Sensors" }
                    table { class: "sensor-table",
                        thead {
                            tr {
                                th { "ID" }
                                th { "PM2.5" }
                                th { "PM10" }
                                th { "Temp" }
                                th { "Source" }
                            }
                        }
                        tbody {
                            for sr in snap.sensors.iter() {
                                SensorRow { sr: sr.clone() }
                            }
                        }
                    }
                }
            }

            // Events
            if is_running && !snap.events.is_empty() {
                div { class: "card",
                    h2 { "Events (24h)" }
                    div { class: "event-list",
                        for event in snap.events.iter().take(20) {
                            div { class: "event-item",
                                span {
                                    class: if event.event_type == "Widespread" { "badge badge-widespread" } else { "badge badge-event" },
                                    "{event.event_type}"
                                }
                                span { class: "event-time",
                                    {format_ts(event.ts)}
                                }
                                if let Some(ref summary) = event.summary {
                                    span { class: "event-summary", " — {summary}" }
                                }
                            }
                        }
                    }
                }
            }

            // Readings count
            if is_running {
                div { class: "card stats-footer",
                    span { "Total readings: {snap.reading_count}" }
                    if let Some(ts) = snap.last_poll {
                        span { " · Last poll: {format_ts(ts)}" }
                    }
                }
            }
        }
    }
}

#[component]
fn StatCard(label: String, value: String, color: String) -> Element {
    rsx! {
        div { class: "stat-card",
            div { class: "stat-value {color}", "{value}" }
            div { class: "stat-label", "{label}" }
        }
    }
}

#[component]
fn SensorRow(sr: SensorWithReading) -> Element {
    let pm25 = sr.latest.as_ref().and_then(|r| r.pm25);
    let pm10 = sr.latest.as_ref().and_then(|r| r.pm10);
    let temp = sr.latest.as_ref().and_then(|r| r.temp);
    let source = sr.sensor.source.as_deref().unwrap_or("—");

    rsx! {
        tr {
            td { "{sr.sensor.id}" }
            td { class: "{pm25_color_class(pm25)}",
                {pm25.map(|v| format!("{v:.1}")).unwrap_or("—".into())}
            }
            td {
                {pm10.map(|v| format!("{v:.1}")).unwrap_or("—".into())}
            }
            td {
                {temp.map(|v| format!("{v:.0}°")).unwrap_or("—".into())}
            }
            td { "{source}" }
        }
    }
}

fn pm25_color_class(val: Option<f64>) -> &'static str {
    match val {
        Some(v) if v <= 12.0 => "good",
        Some(v) if v <= 35.0 => "moderate",
        Some(v) if v <= 55.0 => "unhealthy-sg",
        Some(_) => "unhealthy",
        None => "normal",
    }
}

fn format_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "—".to_string())
}

const CSS: &str = r#"
:root {
    --bg: #0a0a0a; --card: #161616; --border: #252525;
    --text: #e0e0e0; --muted: #777; --green: #4ade80;
    --yellow: #facc15; --orange: #fb923c; --red: #f87171; --blue: #60a5fa;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif; background: var(--bg); color: var(--text); -webkit-font-smoothing: antialiased; }
.container { max-width: 480px; margin: 0 auto; padding: 16px; }
.header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
h1 { font-size: 1.4rem; font-weight: 700; }
h2 { font-size: 0.8rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 10px; }
.status { font-size: 0.8rem; color: var(--muted); }
.status.connected { color: var(--green); }
.card { background: var(--card); border: 1px solid var(--border); border-radius: 14px; padding: 16px; margin-bottom: 12px; }
.setup-card { text-align: center; }
.form-row { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; }
.form-row label { font-size: 0.85rem; color: var(--muted); min-width: 80px; text-align: right; }
.form-row input { flex: 1; background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 8px 12px; color: var(--text); font-size: 0.9rem; }
.form-row input:focus { outline: none; border-color: var(--blue); }
.btn-start { background: var(--blue); color: #000; border: none; border-radius: 10px; padding: 10px 32px; font-size: 0.9rem; font-weight: 600; cursor: pointer; margin-top: 8px; }
.btn-start:hover { opacity: 0.9; }
.error { color: var(--red); font-size: 0.8rem; margin-top: 8px; }
.stats-row { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 10px; margin-bottom: 12px; }
.stat-card { background: var(--card); border: 1px solid var(--border); border-radius: 14px; padding: 14px; text-align: center; }
.stat-value { font-size: 1.8rem; font-weight: 700; }
.stat-label { font-size: 0.7rem; color: var(--muted); margin-top: 2px; }
.good { color: var(--green); }
.moderate { color: var(--yellow); }
.unhealthy-sg { color: var(--orange); }
.unhealthy { color: var(--red); }
.normal { color: var(--text); }
.sensor-table { width: 100%; font-size: 0.8rem; border-collapse: collapse; }
.sensor-table th { text-align: left; color: var(--muted); font-weight: 500; padding: 6px 8px; border-bottom: 1px solid var(--border); }
.sensor-table td { padding: 6px 8px; border-bottom: 1px solid var(--border); }
.sensor-table tr:last-child td { border-bottom: none; }
.event-list { max-height: 240px; overflow-y: auto; }
.event-item { padding: 8px 0; border-bottom: 1px solid var(--border); font-size: 0.8rem; }
.event-item:last-child { border-bottom: none; }
.badge { display: inline-block; padding: 2px 8px; border-radius: 8px; font-size: 0.7rem; font-weight: 600; margin-right: 6px; }
.badge-event { background: var(--yellow); color: #000; }
.badge-widespread { background: var(--red); color: #fff; }
.event-time { color: var(--muted); }
.event-summary { color: var(--text); }
.stats-footer { font-size: 0.75rem; color: var(--muted); text-align: center; }
"#;
