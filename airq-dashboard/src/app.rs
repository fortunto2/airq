//! Root Dioxus component for Air Signal dashboard.
//!
//! Layout: left sidebar (nav) + main content area.
//! Views: Dashboard, Map, Comfort, Events, History, Sources, Settings.

use crate::state::{self, MonitorSnapshot, SensorWithReading};
use airq::db::Db;
use airq::AppConfig;
use dioxus::prelude::*;
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL_SECS: u64 = 300;
const REFRESH_INTERVAL_MS: u64 = 10_000;

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum View {
    Dashboard,
    Map,
    Comfort,
    Events,
    History,
    Sources,
    Settings,
}

impl View {
    fn icon(&self) -> &'static str {
        match self {
            Self::Dashboard => "\u{25c9}",  // ◉
            Self::Map       => "\u{25cb}",  // ○ (map pin)
            Self::Comfort   => "\u{2606}",  // ☆
            Self::Events    => "\u{26a0}",  // ⚠
            Self::History   => "\u{2630}",  // ☰
            Self::Sources   => "\u{2316}",  // ⌖
            Self::Settings  => "\u{2699}",  // ⚙
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Map       => "Map",
            Self::Comfort   => "Comfort",
            Self::Events    => "Events",
            Self::History   => "History",
            Self::Sources   => "Sources",
            Self::Settings  => "Settings",
        }
    }

    fn all() -> &'static [View] {
        &[Self::Dashboard, Self::Map, Self::Comfort, Self::Events, Self::History, Self::Sources]
    }
}

// ---------------------------------------------------------------------------
// Root App
// ---------------------------------------------------------------------------

#[component]
pub fn App() -> Element {
    let config = AppConfig::load().unwrap_or_default();
    let default_city = config.default_city.clone().unwrap_or_else(|| "gazipasa".to_string());
    let default_radius = config.radius.unwrap_or(15.0);
    let config_cities = config.cities.clone().unwrap_or_default();

    let mut db: Signal<Option<Arc<Db>>> = use_signal(|| None);
    let mut snapshot: Signal<MonitorSnapshot> = use_signal(MonitorSnapshot::default);
    let mut collector_running: Signal<bool> = use_signal(|| false);
    let mut active_view: Signal<View> = use_signal(|| View::Dashboard);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let default_city2 = default_city.clone();
    let mut active_city: Signal<String> = use_signal(move || default_city.clone());
    let mut search_input: Signal<String> = use_signal(String::new);
    let mut loading_city: Signal<Option<String>> = use_signal(|| None);
    let mut added_cities: Signal<Vec<String>> = use_signal(Vec::new);
    let mut suggestions: Signal<Vec<String>> = use_signal(Vec::new);
    // Keep shutdown_tx alive — dropping it kills the collector
    let mut shutdown_handle: Signal<Option<tokio::sync::watch::Sender<bool>>> = use_signal(|| None);

    // Settings state
    let mut city_input: Signal<String> = use_signal(move || default_city2.clone());
    let mut radius_input: Signal<f64> = use_signal(move || default_radius);
    let mut interval_input: Signal<u64> = use_signal(|| POLL_INTERVAL_SECS);

    // Auto-start on launch if config has a city
    use_effect(move || {
        let is_running = (collector_running)();
        if is_running {
            return;
        }
        let city_name = (city_input)().clone();
        let radius = (radius_input)();
        let interval = (interval_input)();

        if city_name.is_empty() {
            return;
        }

        spawn(async move {
            match start_collector(&city_name, radius, interval).await {
                Ok((db_handle, snap, stx)) => {
                    db.set(Some(db_handle));
                    snapshot.set(snap);
                    collector_running.set(true);
                    error_msg.set(None);
                    shutdown_handle.set(Some(stx));
                }
                Err(e) => {
                    error_msg.set(Some(e));
                }
            }
        });
    });

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
                    let city = (active_city)();
                    let snap = state::build_snapshot(db_handle, Some(&city));
                    snapshot.set(snap);
                }
            }
        });
    });

    let snap = (snapshot)();
    let is_running = (collector_running)();
    let current_view = (active_view)();

    rsx! {
        style { {CSS} }

        div { class: "app-layout",
            // Left sidebar
            nav { class: "sidebar",
                div { class: "sidebar-logo", "AS" }

                div { class: "sidebar-nav",
                    for view in View::all() {
                        button {
                            class: if current_view == *view { "nav-item active" } else { "nav-item" },
                            onclick: move |_| active_view.set(*view),
                            title: "{view.label()}",
                            span { class: "nav-icon", {view.icon()} }
                            span { class: "nav-label", {view.label()} }
                        }
                    }
                }

                // Settings at bottom
                div { class: "sidebar-bottom",
                    button {
                        class: if current_view == View::Settings { "nav-item active" } else { "nav-item" },
                        onclick: move |_| active_view.set(View::Settings),
                        title: "Settings",
                        span { class: "nav-icon", {View::Settings.icon()} }
                        span { class: "nav-label", {View::Settings.label()} }
                    }

                    // Connection status
                    div { class: "sidebar-status",
                        if is_running {
                            span { class: "dot dot-green" }
                            span { class: "status-text", "Live" }
                        } else {
                            span { class: "dot dot-gray" }
                            span { class: "status-text", "Off" }
                        }
                    }
                }
            }

            // Main area: top bar + content
            div { class: "main-area",
                // Top bar: city switcher
                div { class: "topbar",
                    // All cities: config + dynamically added
                    div { class: "city-switcher",
                        {
                            let all_cities: Vec<String> = config_cities.iter()
                                .chain((added_cities)().iter())
                                .cloned()
                                .collect();
                            rsx! {
                                for city in all_cities.iter() {
                                    {
                                        let c = city.clone();
                                        let c2 = city.clone();
                                        let c3 = city.clone();
                                        let is_active = (active_city)() == *city;
                                        let is_config = config_cities.contains(city);
                                        rsx! {
                                            div { class: if is_active { "city-chip city-chip-active" } else { "city-chip" },
                                                button {
                                                    class: "city-chip-btn",
                                                    onclick: move |_| {
                                                        let city_name = c.clone();
                                                        active_city.set(city_name.clone());
                                                        loading_city.set(Some(city_name.clone()));
                                                        let radius = (radius_input)();
                                                        let interval = (interval_input)();
                                                        spawn(async move {
                                                            match start_collector(&city_name, radius, interval).await {
                                                                Ok((db_handle, snap, stx)) => {
                                                                    db.set(Some(db_handle));
                                                                    snapshot.set(snap);
                                                                    collector_running.set(true);
                                                                    error_msg.set(None);
                                                                    shutdown_handle.set(Some(stx));
                                                                }
                                                                Err(e) => error_msg.set(Some(e)),
                                                            }
                                                            loading_city.set(None);
                                                        });
                                                    },
                                                    "{c2}"
                                                }
                                                // Remove button (only for dynamically added)
                                                if !is_config {
                                                    button {
                                                        class: "city-chip-x",
                                                        onclick: move |_| {
                                                            let mut cities = (added_cities)();
                                                            cities.retain(|x| x != &c3);
                                                            added_cities.set(cities);
                                                        },
                                                        "×"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Search with autocomplete
                    div { class: "city-search-wrap",
                        input {
                            r#type: "text",
                            placeholder: "Add city...",
                            value: "{search_input}",
                            oninput: move |e| {
                                let val = e.value();
                                search_input.set(val.clone());
                                // Autocomplete from built-in cities DB
                                if val.len() >= 2 {
                                    let query = val.to_lowercase();
                                    let matches: Vec<String> = cities::all()
                                        .iter()
                                        .filter(|c| c.city.to_lowercase().starts_with(&query))
                                        .take(6)
                                        .map(|c| c.city.to_string())
                                        .collect();
                                    suggestions.set(matches);
                                } else {
                                    suggestions.set(Vec::new());
                                }
                            },
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    let city_name = (search_input)().trim().to_string();
                                    if !city_name.is_empty() {
                                        search_input.set(String::new());
                                        suggestions.set(Vec::new());
                                        // Add to dynamic cities
                                        let mut cities = (added_cities)();
                                        if !cities.contains(&city_name) && !config_cities.contains(&city_name) {
                                            cities.push(city_name.clone());
                                            added_cities.set(cities);
                                        }
                                        // Switch to it
                                        active_city.set(city_name.clone());
                                        loading_city.set(Some(city_name.clone()));
                                        let radius = (radius_input)();
                                        let interval = (interval_input)();
                                        spawn(async move {
                                            match start_collector(&city_name, radius, interval).await {
                                                Ok((db_handle, snap, stx)) => {
                                                    db.set(Some(db_handle));
                                                    snapshot.set(snap);
                                                    collector_running.set(true);
                                                    error_msg.set(None);
                                                    shutdown_handle.set(Some(stx));
                                                }
                                                Err(e) => error_msg.set(Some(e)),
                                            }
                                            loading_city.set(None);
                                        });
                                    }
                                }
                            },
                        }
                        // Suggestions dropdown
                        if !(suggestions)().is_empty() {
                            div { class: "suggestions",
                                for s in (suggestions)().iter() {
                                    {
                                        let name = s.clone();
                                        let name2 = s.clone();
                                        rsx! {
                                            button {
                                                class: "suggestion-item",
                                                onclick: move |_| {
                                                    search_input.set(name.clone());
                                                    suggestions.set(Vec::new());
                                                },
                                                "{name2}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Loading indicator
                    if let Some(ref city) = (loading_city)() {
                        span { class: "topbar-loading", "Loading {city}..." }
                    }
                    if let Some(ref err) = (error_msg)() {
                        span { class: "topbar-error", "{err}" }
                    }
                }

                // Content
                main { class: "content",
                match current_view {
                    View::Dashboard => rsx! {
                        DashboardView { snap: snap.clone(), is_running: is_running }
                    },
                    View::Map => rsx! {
                        MapView { snap: snap.clone() }
                    },
                    View::Comfort => rsx! {
                        ComfortView { snap: snap.clone() }
                    },
                    View::Events => rsx! {
                        EventsView { snap: snap.clone() }
                    },
                    View::History => rsx! {
                        HistoryView { snap: snap.clone() }
                    },
                    View::Sources => rsx! {
                        SourcesView { snap: snap.clone() }
                    },
                    View::Settings => rsx! {
                        SettingsView {
                            city_input: city_input,
                            radius_input: radius_input,
                            interval_input: interval_input,
                            is_running: is_running,
                            error_msg: (error_msg)().clone(),
                            config_cities: config_cities.clone(),
                            snap: snap.clone(),
                        }
                    },
                }
            }
            } // main-area
        }
    }
}

/// Return type includes shutdown_tx — MUST be kept alive or collector stops.
async fn start_collector(
    city: &str,
    radius: f64,
    interval: u64,
) -> Result<(Arc<Db>, MonitorSnapshot, tokio::sync::watch::Sender<bool>), String> {
    let (lat, lon, resolved) = airq::geocode(city).await
        .map_err(|e| format!("Geocode failed: {e}"))?;
    tracing::info!("Resolved: {} ({:.2}, {:.2})", resolved, lat, lon);

    let db_handle = state::open_db()
        .map_err(|e| format!("DB error: {e}"))?;
    let _ = db_handle.upsert_city(city, lat, lon, radius);

    // Do an immediate fetch before starting the loop
    if let Err(e) = airq::collector::collect_once(&db_handle, city, lat, lon, radius).await {
        tracing::warn!("Initial collect failed: {e}");
    }

    let collector_db = db_handle.clone();
    let cities = vec![(city.to_string(), lat, lon, radius)];
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(airq::collector::run_collector(
        collector_db,
        cities,
        Duration::from_secs(interval),
        shutdown_rx,
    ));

    let snap = state::build_snapshot(&db_handle, Some(city));
    Ok((db_handle, snap, shutdown_tx))
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

/// Dashboard: stats + sensor table + recent events
#[component]
fn DashboardView(snap: MonitorSnapshot, is_running: bool) -> Element {
    rsx! {
        div { class: "view-header",
            h1 { "Dashboard" }
            if let Some(ts) = snap.last_poll {
                span { class: "view-subtitle", "Last poll: {format_ts(ts)}" }
            }
        }

        if !is_running {
            div { class: "card empty-state",
                p { "Not connected. Open Settings to configure and start monitoring." }
            }
        }

        // Stats
        if is_running {
            div { class: "stats-grid",
                StatCard { label: "PM2.5", value: fmt_pm(snap.avg_pm25), unit: "μg/m³", color: pm25_color(snap.avg_pm25) }
                StatCard { label: "PM10", value: fmt_pm(snap.avg_pm10), unit: "μg/m³", color: pm25_color(snap.avg_pm10) }
                StatCard { label: "Sensors", value: format!("{}", snap.sensor_count), unit: "active", color: "normal" }
                StatCard { label: "Readings", value: format!("{}", snap.reading_count), unit: "total", color: "normal" }
            }
        }

        // Sensor table
        if is_running && !snap.sensors.is_empty() {
            div { class: "card",
                h2 { "Active Sensors" }
                table { class: "data-table",
                    thead {
                        tr {
                            th { "Sensor" }
                            th { class: "num", "PM2.5" }
                            th { class: "num", "PM10" }
                            th { class: "num", "Temp" }
                            th { class: "num", "Humidity" }
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

        // Recent events (compact)
        if is_running && !snap.events.is_empty() {
            div { class: "card",
                h2 { "Recent Events" }
                for event in snap.events.iter().take(5) {
                    EventRow { event: event.clone() }
                }
            }
        }
    }
}

/// Map: Leaflet via document::eval() (scripts don't run via innerHTML)
#[component]
fn MapView(snap: MonitorSnapshot) -> Element {
    let sensors_json: Vec<String> = snap.sensors.iter().filter_map(|sr| {
        let lat = sr.sensor.lat?;
        let lon = sr.sensor.lon?;
        let pm25 = sr.latest.as_ref().and_then(|r| r.pm25).unwrap_or(0.0);
        let pm10 = sr.latest.as_ref().and_then(|r| r.pm10).unwrap_or(0.0);
        Some(format!("{{lat:{lat},lon:{lon},pm25:{pm25:.1},pm10:{pm10:.1},id:{}}}", sr.sensor.id))
    }).collect();
    let sensors_data = format!("[{}]", sensors_json.join(","));

    let cities_json: Vec<String> = snap.cities.iter().map(|c| {
        format!("{{lat:{},lon:{},radius:{},name:'{}'}}", c.lat, c.lon, c.radius, c.name)
    }).collect();
    let cities_data = format!("[{}]", cities_json.join(","));

    let (center_lat, center_lon) = if let Some(ref c) = snap.active_city {
        (c.lat, c.lon)
    } else if let Some(c) = snap.cities.first() {
        (c.lat, c.lon)
    } else {
        (36.27, 32.30)
    };

    // Run map JS on every render (props change = re-render = map updates)
    let js = format!(r#"
        (async function() {{
            if (!document.getElementById('leaflet-css')) {{
                var link = document.createElement('link');
                link.id = 'leaflet-css';
                link.rel = 'stylesheet';
                link.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
                document.head.appendChild(link);
            }}
            if (!window.L) {{
                await new Promise((resolve, reject) => {{
                    var s = document.createElement('script');
                    s.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
                    s.onload = resolve;
                    s.onerror = reject;
                    document.head.appendChild(s);
                }});
            }}
            await new Promise(r => setTimeout(r, 150));
            var el = document.getElementById('airq-map');
            if (!el) return;
            if (window._airqMap) {{ window._airqMap.remove(); window._airqMap = null; }}
            var map = L.map('airq-map', {{zoomControl: false}}).setView([{center_lat}, {center_lon}], 11);
            window._airqMap = map;
            L.tileLayer('https://{{s}}.basemaps.cartocdn.com/dark_all/{{z}}/{{x}}/{{y}}{{r}}.png', {{maxZoom: 18}}).addTo(map);
            var sensors = {sensors_data};
            var cities = {cities_data};
            sensors.forEach(function(s) {{
                var color = s.pm25<=12?'#4ade80':s.pm25<=35?'#facc15':s.pm25<=55?'#fb923c':'#f87171';
                L.circleMarker([s.lat, s.lon], {{radius:8, fillColor:color, fillOpacity:0.85, color:'#333', weight:1}})
                 .bindPopup('<b>#'+s.id+'</b><br>PM2.5: '+s.pm25+'<br>PM10: '+s.pm10)
                 .addTo(map);
            }});
            cities.forEach(function(c) {{
                L.circle([c.lat, c.lon], {{radius:c.radius*1000, color:'#60a5fa', fillOpacity:0.05, weight:1, dashArray:'4'}}).addTo(map);
            }});
            setTimeout(function() {{ map.invalidateSize(); }}, 200);
        }})();
    "#);
    // Spawn eval so it runs after DOM is ready
    spawn(async move {
        document::eval(&js);
    });

    rsx! {
        div { class: "view-header",
            h1 { "Sensor Map" }
            span { class: "view-subtitle", "{snap.sensor_count} sensors" }
        }
        div { class: "map-container",
            div { id: "airq-map", style: "width:100%;height:100%;background:#0a0a0a;border-radius:12px;" }
        }
    }
}

/// Comfort: 14-signal breakdown
#[component]
fn ComfortView(snap: MonitorSnapshot) -> Element {
    let pm25 = snap.avg_pm25.unwrap_or(0.0);
    let pm10 = snap.avg_pm10.unwrap_or(0.0);

    // Calculate AQI from PM2.5
    let aqi = airq::pm25_aqi(pm25);
    let category = airq::AqiCategory::from_aqi(aqi);
    let cat_label = category.label().to_string();

    rsx! {
        div { class: "view-header",
            h1 { "Air Comfort" }
        }

        div { class: "stats-grid",
            StatCard { label: "AQI", value: format!("{aqi}"), unit: cat_label, color: aqi_color(aqi) }
            StatCard { label: "PM2.5", value: format!("{pm25:.1}"), unit: "μg/m³".to_string(), color: pm25_color(Some(pm25)) }
            StatCard { label: "PM10", value: format!("{pm10:.1}"), unit: "μg/m³".to_string(), color: pm25_color(Some(pm10)) }
        }

        // Source classification
        if pm25 > 0.0 || pm10 > 0.0 {
            div { class: "card",
                h2 { "Source Classification" }
                {
                    let ratio = if pm25 > 1.0 { pm10 / pm25 } else { 1.0 };
                    let source = airq_core::event::classify_source(ratio, pm25, pm10);
                    let conf_pct = format!("{:.0}%", source.confidence * 100.0);
                    let label = source.label.to_string();
                    let reason = source.reason.clone();
                    let advice = source.advice.to_string();
                    let typical = source.typical_sources.join(", ");
                    rsx! {
                        div { class: "source-info",
                            div { class: "source-label", "{label}" }
                            div { class: "source-confidence", "Confidence: {conf_pct}" }
                            div { class: "source-reason", "{reason}" }
                            div { class: "source-advice", "{advice}" }
                            div { class: "source-typical",
                                strong { "Typical sources: " }
                                "{typical}"
                            }
                        }
                    }
                }
            }
        }

        // WHO guidelines reference
        div { class: "card",
            h2 { "WHO Guidelines" }
            table { class: "data-table",
                thead { tr { th { "Pollutant" } th { class: "num", "24h limit" } th { class: "num", "Current" } th { "Status" } } }
                tbody {
                    tr {
                        td { "PM2.5" }
                        td { class: "num", "15 μg/m³" }
                        td { class: "num {pm25_color(Some(pm25))}", "{pm25:.1}" }
                        td { if pm25 <= 15.0 { "OK" } else { "Exceeded" } }
                    }
                    tr {
                        td { "PM10" }
                        td { class: "num", "45 μg/m³" }
                        td { class: "num {pm25_color(Some(pm10))}", "{pm10:.1}" }
                        td { if pm10 <= 45.0 { "OK" } else { "Exceeded" } }
                    }
                }
            }
        }
    }
}

/// Events: full event log with details
#[component]
fn EventsView(snap: MonitorSnapshot) -> Element {
    rsx! {
        div { class: "view-header",
            h1 { "Events" }
            span { class: "view-subtitle", "{snap.events.len()} in last 24h" }
        }

        if snap.events.is_empty() {
            div { class: "card empty-state",
                p { "No pollution events detected in the last 24 hours." }
                p { class: "muted", "Events are detected when multiple sensors show anomalous readings simultaneously." }
            }
        }

        for event in snap.events.iter() {
            div { class: "card event-card",
                div { class: "event-header",
                    span {
                        class: if event.event_type == "Widespread" { "badge badge-widespread" } else { "badge badge-event" },
                        "{event.event_type}"
                    }
                    span { class: "event-time", {format_ts(event.ts)} }
                    if let Some(dir) = &event.direction {
                        span { class: "badge badge-dir", "{dir}" }
                    }
                }
                div { class: "event-details",
                    {
                        let mut parts = Vec::new();
                        if let Some(pm25) = event.pm25 { parts.push(format!("PM2.5: {pm25:.1}")); }
                        if let Some(pm10) = event.pm10 { parts.push(format!("PM10: {pm10:.1}")); }
                        if let Some(ratio) = event.ratio { parts.push(format!("Ratio: {ratio:.1}")); }
                        parts.push(format!("Confidence: {:.0}%", event.confidence * 100.0));
                        let details = parts.join(" · ");
                        rsx! { span { "{details}" } }
                    }
                }
                if let Some(ref summary) = event.summary {
                    div { class: "event-summary-full", "{summary}" }
                }
            }
        }
    }
}

/// History: readings over time (placeholder — will use chart)
#[component]
fn HistoryView(snap: MonitorSnapshot) -> Element {
    rsx! {
        div { class: "view-header",
            h1 { "History" }
            span { class: "view-subtitle", "{snap.reading_count} readings stored" }
        }

        div { class: "card",
            h2 { "Sensor Readings" }
            if snap.sensors.is_empty() {
                p { class: "muted", "No sensor data yet. Start monitoring to collect readings." }
            }
            for sr in snap.sensors.iter() {
                if sr.latest.is_some() {
                    div { class: "history-sensor",
                        strong { "Sensor #{sr.sensor.id}" }
                        if let Some(ref r) = sr.latest {
                            {
                                let detail = format!(" — PM2.5: {:.1}, PM10: {:.1} at {}",
                                    r.pm25.unwrap_or(0.0), r.pm10.unwrap_or(0.0), format_ts(r.ts));
                                rsx! { span { class: "muted", "{detail}" } }
                            }
                        }
                    }
                }
            }
        }

        div { class: "card empty-state",
            p { class: "muted", "Charts coming soon. Historical data is stored in SQLite and accessible via the REST API." }
        }
    }
}

/// Sources: pollution source attribution
#[component]
fn SourcesView(snap: MonitorSnapshot) -> Element {
    let pm25 = snap.avg_pm25.unwrap_or(0.0);
    let pm10 = snap.avg_pm10.unwrap_or(0.0);
    let ratio = if pm25 > 1.0 { pm10 / pm25 } else { 1.0 };
    let source = airq_core::event::classify_source(ratio, pm25, pm10);

    rsx! {
        div { class: "view-header",
            h1 { "Source Attribution" }
        }

        div { class: "card",
            h2 { "Current Classification" }
            div { class: "source-hero",
                div { class: "source-hero-label", "{source.label}" }
                div { class: "source-hero-ratio", "PM10/PM2.5 ratio: {ratio:.2}" }
            }
            div { class: "source-info",
                div { class: "source-reason", "{source.reason}" }
                div { class: "source-advice", "{source.advice}" }
            }
        }

        div { class: "card",
            h2 { "PM Ratio Guide" }
            table { class: "data-table",
                thead { tr { th { "Ratio" } th { "Source Type" } th { "Examples" } } }
                tbody {
                    tr { td { "> 4.0" } td { "Dust/Sand Storm" } td { class: "muted", "Saharan dust, volcanic ash" } }
                    tr { td { "2.5–4.0" } td { "Construction Dust" } td { class: "muted", "Building sites, unpaved roads" } }
                    tr { td { "1.5–2.5" } td { "Mixed Urban" } td { class: "muted", "Traffic + heating + industry" } }
                    tr { td { "0.9–1.5" } td { "Combustion" } td { class: "muted", "Diesel, coal heating, power plants" } }
                    tr { td { "< 0.9" } td { "Smoke" } td { class: "muted", "Wildfire, agricultural burning" } }
                }
            }
        }

        div { class: "card",
            h2 { "Nearby Pollution Sources" }
            p { class: "muted", "Run `airq blame --city <name>` for detailed CPF wind-direction analysis with OSM source mapping." }
        }
    }
}

/// Settings: city, radius, interval, DB info
#[component]
fn SettingsView(
    city_input: Signal<String>,
    radius_input: Signal<f64>,
    interval_input: Signal<u64>,
    is_running: bool,
    error_msg: Option<String>,
    config_cities: Vec<String>,
    snap: MonitorSnapshot,
) -> Element {
    let mut city_input = city_input;
    let mut radius_input = radius_input;
    let mut interval_input = interval_input;

    rsx! {
        div { class: "view-header",
            h1 { "Settings" }
        }

        div { class: "card",
            h2 { "Monitoring" }
            div { class: "settings-form",
                div { class: "form-row",
                    label { "City" }
                    input {
                        r#type: "text",
                        value: "{city_input}",
                        oninput: move |e| city_input.set(e.value()),
                        disabled: is_running,
                    }
                }
                div { class: "form-row",
                    label { "Radius (km)" }
                    input {
                        r#type: "number",
                        value: "{radius_input}",
                        oninput: move |e| { if let Ok(v) = e.value().parse::<f64>() { radius_input.set(v); } },
                        disabled: is_running,
                    }
                }
                div { class: "form-row",
                    label { "Interval (s)" }
                    input {
                        r#type: "number",
                        value: "{interval_input}",
                        oninput: move |e| { if let Ok(v) = e.value().parse::<u64>() { interval_input.set(v); } },
                        disabled: is_running,
                    }
                }
            }
            if let Some(ref err) = error_msg {
                div { class: "error", "{err}" }
            }
            if is_running {
                div { class: "settings-status good", "Collector running" }
            }
        }

        // Config cities from airq config.toml
        if !config_cities.is_empty() {
            div { class: "card",
                h2 { "Configured Cities" }
                div { class: "city-chips",
                    for city in config_cities.iter() {
                        span { class: "chip", "{city}" }
                    }
                }
                p { class: "muted small", "From ~/.config/airq/config.toml (shared with CLI)" }
            }
        }

        // DB info
        div { class: "card",
            h2 { "Database" }
            {
                let db_path = format!("{}", state::default_db_path().display());
                let readings = snap.reading_count;
                let sensors = snap.sensor_count;
                let cities = snap.cities.len();
                rsx! {
                    div { class: "settings-info",
                        div { "Path: {db_path}" }
                        div { "Readings: {readings}" }
                        div { "Sensors: {sensors}" }
                        div { "Cities: {cities}" }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared components
// ---------------------------------------------------------------------------

#[component]
fn StatCard(label: String, value: String, unit: String, color: String) -> Element {
    rsx! {
        div { class: "stat-card",
            div { class: "stat-value {color}", "{value}" }
            div { class: "stat-unit", "{unit}" }
            div { class: "stat-label", "{label}" }
        }
    }
}

#[component]
fn SensorRow(sr: SensorWithReading) -> Element {
    let pm25 = sr.latest.as_ref().and_then(|r| r.pm25);
    let pm10 = sr.latest.as_ref().and_then(|r| r.pm10);
    let temp = sr.latest.as_ref().and_then(|r| r.temp);
    let humidity = sr.latest.as_ref().and_then(|r| r.humidity);
    let source = sr.sensor.source.as_deref().unwrap_or("—");

    rsx! {
        tr {
            td { "#{sr.sensor.id}" }
            td { class: "num {pm25_color(pm25)}", {fmt_opt(pm25)} }
            td { class: "num", {fmt_opt(pm10)} }
            td { class: "num", {temp.map(|v| format!("{v:.0}°")).unwrap_or("—".into())} }
            td { class: "num", {humidity.map(|v| format!("{v:.0}%")).unwrap_or("—".into())} }
            td { "{source}" }
        }
    }
}

#[component]
fn EventRow(event: airq::db::Event) -> Element {
    rsx! {
        div { class: "event-row",
            span {
                class: if event.event_type == "Widespread" { "badge badge-widespread" } else { "badge badge-event" },
                "{event.event_type}"
            }
            span { class: "event-time", {format_ts(event.ts)} }
            if let Some(ref summary) = event.summary {
                span { class: "muted", " — {summary}" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pm25_color(val: Option<f64>) -> &'static str {
    match val {
        Some(v) if v <= 12.0 => "good",
        Some(v) if v <= 35.0 => "moderate",
        Some(v) if v <= 55.0 => "unhealthy-sg",
        Some(_) => "unhealthy",
        None => "normal",
    }
}

fn aqi_color(aqi: u32) -> &'static str {
    match aqi {
        0..=50 => "good",
        51..=100 => "moderate",
        101..=150 => "unhealthy-sg",
        _ => "unhealthy",
    }
}

fn fmt_pm(val: Option<f64>) -> String {
    val.map(|v| format!("{v:.1}")).unwrap_or("—".into())
}

fn fmt_opt(val: Option<f64>) -> String {
    val.map(|v| format!("{v:.1}")).unwrap_or("—".into())
}

fn format_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "—".to_string())
}

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

const CSS: &str = r#"
:root {
    --bg: #0a0a0a; --sidebar: #111; --card: #161616; --border: #222;
    --text: #e0e0e0; --muted: #666; --green: #4ade80;
    --yellow: #facc15; --orange: #fb923c; --red: #f87171; --blue: #60a5fa;
    --sidebar-w: 180px;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif; background: var(--bg); color: var(--text); -webkit-font-smoothing: antialiased; overflow: hidden; height: 100vh; }

/* Layout */
.app-layout { display: flex; height: 100vh; }
.sidebar { width: var(--sidebar-w); background: var(--sidebar); border-right: 1px solid var(--border); display: flex; flex-direction: column; flex-shrink: 0; }
.main-area { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
.topbar { display: flex; align-items: center; gap: 10px; padding: 10px 24px; border-bottom: 1px solid var(--border); background: var(--sidebar); flex-shrink: 0; }
.city-switcher { display: flex; gap: 4px; flex-wrap: wrap; }
.city-chip { display: flex; align-items: center; border-radius: 16px; border: 1px solid var(--border); overflow: hidden; transition: all 0.15s; }
.city-chip:hover { border-color: var(--blue); }
.city-chip-active { background: rgba(96,165,250,0.15); border-color: var(--blue); }
.city-chip-btn { padding: 5px 10px; border: none; background: none; color: var(--muted); font-size: 0.8rem; cursor: pointer; text-transform: capitalize; }
.city-chip-active .city-chip-btn { color: var(--blue); font-weight: 600; }
.city-chip-x { padding: 5px 6px 5px 0; border: none; background: none; color: var(--muted); font-size: 0.9rem; cursor: pointer; line-height: 1; }
.city-chip-x:hover { color: var(--red); }
.city-search-wrap { flex-shrink: 0; position: relative; }
.city-search-wrap input { background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 5px 12px; color: var(--text); font-size: 0.8rem; width: 160px; }
.city-search-wrap input:focus { outline: none; border-color: var(--blue); }
.city-search-wrap input::placeholder { color: var(--muted); }
.suggestions { position: absolute; top: 100%; left: 0; right: 0; background: var(--card); border: 1px solid var(--border); border-radius: 8px; margin-top: 4px; z-index: 100; overflow: hidden; }
.suggestion-item { display: block; width: 100%; padding: 6px 12px; border: none; background: none; color: var(--text); font-size: 0.8rem; cursor: pointer; text-align: left; }
.suggestion-item:hover { background: rgba(96,165,250,0.1); }
.topbar-loading { color: var(--yellow); font-size: 0.75rem; animation: pulse 1.5s infinite; }
@keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.5; } }
.topbar-error { color: var(--red); font-size: 0.75rem; }
.content { flex: 1; overflow-y: auto; padding: 24px 28px; }

/* Sidebar */
.sidebar-logo { padding: 20px 16px 16px; font-size: 1.2rem; font-weight: 800; color: var(--blue); letter-spacing: 1px; }
.sidebar-nav { flex: 1; padding: 4px 8px; }
.sidebar-bottom { padding: 8px; border-top: 1px solid var(--border); }
.nav-item { display: flex; align-items: center; gap: 10px; width: 100%; padding: 9px 12px; border: none; background: none; color: var(--muted); font-size: 0.85rem; border-radius: 8px; cursor: pointer; text-align: left; transition: all 0.15s; }
.nav-item:hover { background: rgba(255,255,255,0.05); color: var(--text); }
.nav-item.active { background: rgba(96,165,250,0.12); color: var(--blue); }
.nav-icon { font-size: 1.1rem; width: 20px; text-align: center; }
.nav-label { font-weight: 500; }
.sidebar-status { display: flex; align-items: center; gap: 6px; padding: 10px 12px; font-size: 0.75rem; color: var(--muted); }
.dot { width: 7px; height: 7px; border-radius: 50%; display: inline-block; }
.dot-green { background: var(--green); box-shadow: 0 0 6px var(--green); }
.dot-gray { background: #555; }
.status-text { font-weight: 500; }

/* View header */
.view-header { display: flex; align-items: baseline; gap: 12px; margin-bottom: 20px; }
.view-header h1 { font-size: 1.5rem; font-weight: 700; }
.view-subtitle { font-size: 0.8rem; color: var(--muted); }

/* Cards */
.card { background: var(--card); border: 1px solid var(--border); border-radius: 12px; padding: 16px; margin-bottom: 14px; }
h2 { font-size: 0.75rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.6px; margin-bottom: 10px; }
.empty-state { text-align: center; padding: 32px 16px; }
.empty-state p { color: var(--muted); font-size: 0.9rem; margin-bottom: 8px; }

/* Stats grid */
.stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 12px; margin-bottom: 16px; }
.stat-card { background: var(--card); border: 1px solid var(--border); border-radius: 12px; padding: 16px; text-align: center; }
.stat-value { font-size: 2rem; font-weight: 700; line-height: 1.1; }
.stat-unit { font-size: 0.7rem; color: var(--muted); margin-top: 2px; }
.stat-label { font-size: 0.7rem; color: var(--muted); margin-top: 4px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.3px; }
.good { color: var(--green); }
.moderate { color: var(--yellow); }
.unhealthy-sg { color: var(--orange); }
.unhealthy { color: var(--red); }
.normal { color: var(--text); }

/* Data tables */
.data-table { width: 100%; font-size: 0.82rem; border-collapse: collapse; }
.data-table th { text-align: left; color: var(--muted); font-weight: 500; padding: 6px 10px; border-bottom: 1px solid var(--border); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.3px; }
.data-table td { padding: 8px 10px; border-bottom: 1px solid var(--border); }
.data-table tr:last-child td { border-bottom: none; }
.data-table .num { text-align: right; font-variant-numeric: tabular-nums; }

/* Events */
.event-row { padding: 8px 0; border-bottom: 1px solid var(--border); font-size: 0.82rem; }
.event-row:last-child { border-bottom: none; }
.event-card { }
.event-header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
.event-details { font-size: 0.8rem; color: var(--muted); margin-bottom: 4px; }
.event-summary-full { font-size: 0.85rem; color: var(--text); }
.event-time { color: var(--muted); font-size: 0.8rem; }
.badge { display: inline-block; padding: 2px 8px; border-radius: 8px; font-size: 0.7rem; font-weight: 600; }
.badge-event { background: var(--yellow); color: #000; }
.badge-widespread { background: var(--red); color: #fff; }
.badge-dir { background: var(--blue); color: #000; }

/* Map */
.map-container { height: calc(100vh - 120px); border-radius: 12px; overflow: hidden; }

/* Comfort / Sources */
.source-info { font-size: 0.85rem; line-height: 1.6; }
.source-label { font-size: 1.1rem; font-weight: 600; margin-bottom: 4px; }
.source-confidence { color: var(--muted); font-size: 0.8rem; }
.source-reason { margin: 8px 0; }
.source-advice { color: var(--blue); margin: 8px 0; padding: 10px; background: rgba(96,165,250,0.08); border-radius: 8px; }
.source-typical { color: var(--muted); font-size: 0.82rem; }
.source-hero { text-align: center; padding: 20px; }
.source-hero-label { font-size: 1.4rem; font-weight: 700; text-transform: capitalize; }
.source-hero-ratio { color: var(--muted); font-size: 0.85rem; margin-top: 4px; }

/* History */
.history-sensor { padding: 6px 0; border-bottom: 1px solid var(--border); font-size: 0.85rem; }

/* Settings */
.settings-form { max-width: 400px; }
.form-row { display: flex; align-items: center; gap: 12px; margin-bottom: 10px; }
.form-row label { font-size: 0.85rem; color: var(--muted); min-width: 90px; text-align: right; }
.form-row input { flex: 1; background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 8px 12px; color: var(--text); font-size: 0.9rem; }
.form-row input:focus { outline: none; border-color: var(--blue); }
.form-row input:disabled { opacity: 0.5; }
.error { color: var(--red); font-size: 0.8rem; margin-top: 8px; }
.settings-status { font-size: 0.85rem; margin-top: 8px; padding: 8px 12px; border-radius: 8px; background: rgba(74,222,128,0.08); }
.settings-info { font-size: 0.82rem; color: var(--muted); line-height: 1.8; }
.settings-info div { font-variant-numeric: tabular-nums; }
.city-chips { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
.chip { display: inline-block; padding: 4px 12px; background: rgba(96,165,250,0.1); border: 1px solid rgba(96,165,250,0.2); border-radius: 16px; font-size: 0.8rem; color: var(--blue); }
.muted { color: var(--muted); }
.small { font-size: 0.75rem; }
"#;
