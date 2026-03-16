//! Root Dioxus component for Air Signal dashboard.
//!
//! Layout: left sidebar (nav) + main content area.
//! Views: Dashboard, Map, Comfort, Events, History, Sources, Settings.

use crate::state::{self, CityData, MonitorSnapshot, SensorWithReading};
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
    Network,
    Settings,
}

impl View {
    fn icon(&self) -> &'static str {
        match self {
            Self::Dashboard => "\u{25c9}",  // ◉
            Self::Map       => "\u{25cb}",  // ○
            Self::Comfort   => "\u{2606}",  // ☆
            Self::Events    => "\u{26a0}",  // ⚠
            Self::History   => "\u{2630}",  // ☰
            Self::Sources   => "\u{2316}",  // ⌖
            Self::Network   => "\u{2301}",  // ⌁
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
            Self::Network   => "Network",
            Self::Settings  => "Settings",
        }
    }

    fn all_top() -> &'static [View] {
        &[Self::Dashboard, Self::Map, Self::Comfort, Self::Events, Self::History, Self::Sources]
    }

    fn all_bottom() -> &'static [View] {
        &[Self::Network, Self::Settings]
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
    let mut city_data: Signal<CityData> = use_signal(CityData::default);
    // Network state
    let local_ip = state::get_local_ip().unwrap_or_else(|| "unknown".to_string());
    let all_ips = state::get_all_local_ips();
    let mut lan_sensors: Signal<Vec<state::LanSensor>> = use_signal(Vec::new);
    let mut scanning: Signal<bool> = use_signal(|| false);
    let mut server_running: Signal<bool> = use_signal(|| false);
    let mut server_shutdown: Signal<Option<tokio::sync::watch::Sender<bool>>> = use_signal(|| None);

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
                Ok((db_handle, snap, stx, lat, lon)) => {
                    db.set(Some(db_handle));
                    snapshot.set(snap);
                    collector_running.set(true);
                    error_msg.set(None);
                    shutdown_handle.set(Some(stx));
                    // Fetch live API data for comfort matrix
                    let cd = state::fetch_city_data(lat, lon).await;
                    city_data.set(cd);
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
                    for view in View::all_top() {
                        button {
                            class: if current_view == *view { "nav-item active" } else { "nav-item" },
                            onclick: move |_| active_view.set(*view),
                            title: "{view.label()}",
                            span { class: "nav-icon", {view.icon()} }
                            span { class: "nav-label", {view.label()} }
                        }
                    }
                }

                // Bottom: Network + Settings
                div { class: "sidebar-bottom",
                    for view in View::all_bottom() {
                        button {
                            class: if current_view == *view { "nav-item active" } else { "nav-item" },
                            onclick: move |_| active_view.set(*view),
                            title: "{view.label()}",
                            span { class: "nav-icon", {view.icon()} }
                            span { class: "nav-label", {view.label()} }
                        }
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
                                                                Ok((db_handle, snap, stx, lat, lon)) => {
                                                                    db.set(Some(db_handle));
                                                                    snapshot.set(snap);
                                                                    collector_running.set(true);
                                                                    error_msg.set(None);
                                                                    shutdown_handle.set(Some(stx));
                                                                    let cd = state::fetch_city_data(lat, lon).await;
                                                                    city_data.set(cd);
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
                                                Ok((db_handle, snap, stx, lat, lon)) => {
                                                    db.set(Some(db_handle));
                                                    snapshot.set(snap);
                                                    collector_running.set(true);
                                                    error_msg.set(None);
                                                    shutdown_handle.set(Some(stx));
                                                    let cd = state::fetch_city_data(lat, lon).await;
                                                    city_data.set(cd);
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
                        DashboardView { snap: snap.clone(), is_running: is_running, city_data: (city_data)() }
                    },
                    View::Map => rsx! {
                        MapView { snap: snap.clone() }
                    },
                    View::Comfort => rsx! {
                        ComfortView { snap: snap.clone(), city_data: (city_data)() }
                    },
                    View::Events => rsx! {
                        EventsView { snap: snap.clone() }
                    },
                    View::History => rsx! {
                        HistoryView { snap: snap.clone() }
                    },
                    View::Sources => rsx! {
                        SourcesView { snap: snap.clone(), city_data: (city_data)() }
                    },
                    View::Network => rsx! {
                        NetworkView {
                            local_ip: local_ip.clone(),
                            all_ips: all_ips.clone(),
                            lan_sensors: lan_sensors,
                            scanning: scanning,
                            is_running: is_running,
                            server_running: server_running,
                            server_shutdown: server_shutdown,
                            db: db,
                            port: 8080,
                        }
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
/// Return type includes lat/lon for city data fetching.
async fn start_collector(
    city: &str,
    radius: f64,
    interval: u64,
) -> Result<(Arc<Db>, MonitorSnapshot, tokio::sync::watch::Sender<bool>, f64, f64), String> {
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
    Ok((db_handle, snap, shutdown_tx, lat, lon))
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

/// Dashboard: stats + sensor table + recent events
#[component]
fn DashboardView(snap: MonitorSnapshot, is_running: bool, city_data: CityData) -> Element {
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
                StatCard { label: "PM2.5", value: fmt_pm(snap.avg_pm25), unit: "\u{00b5}g/m\u{00b3}", color: pm25_color(snap.avg_pm25) }
                StatCard { label: "PM10", value: fmt_pm(snap.avg_pm10), unit: "\u{00b5}g/m\u{00b3}", color: pm25_color(snap.avg_pm10) }
                StatCard { label: "Sensors", value: format!("{}", snap.sensor_count), unit: "active", color: "normal" }
                StatCard { label: "Readings", value: format!("{}", snap.reading_count), unit: "total", color: "normal" }
            }
        }

        // Compact comfort score widget
        if city_data.loaded {
            div { class: "card comfort-compact",
                div { class: "comfort-compact-row",
                    div { class: "comfort-score-compact {score_color(city_data.comfort_total)}",
                        "{city_data.comfort_total}"
                    }
                    div { class: "comfort-compact-info",
                        div { class: "comfort-compact-label", "Comfort: {city_data.comfort_label}" }
                        div { class: "comfort-compact-details",
                            "Air {city_data.air_score} · Temp {city_data.temperature_score} · Wind {city_data.wind_score} · UV {city_data.uv_score} · Press {city_data.pressure_score} · Hum {city_data.humidity_score}"
                        }
                    }
                }
            }
        }

        // Extended pollutant summary row
        if city_data.loaded && (city_data.co.is_some() || city_data.no2.is_some() || city_data.o3.is_some()) {
            {
                let co_label = city_data.co.map(|v| airq_core::get_co_status(v).label()).unwrap_or("--");
                let no2_label = city_data.no2.map(|v| airq_core::get_no2_status(v).label()).unwrap_or("--");
                let o3_label = city_data.o3.map(|v| airq_core::get_o3_status(v).label()).unwrap_or("--");
                let uv_display = fmt_opt(city_data.uv_index, 1);
                let co_color = city_data.co.map(|v| pollutant_color(airq_core::get_co_status(v))).unwrap_or("normal");
                let no2_color = city_data.no2.map(|v| pollutant_color(airq_core::get_no2_status(v))).unwrap_or("normal");
                let o3_color = city_data.o3.map(|v| pollutant_color(airq_core::get_o3_status(v))).unwrap_or("normal");
                let co_val = fmt_opt(city_data.co, 0);
                let no2_val = fmt_opt(city_data.no2, 1);
                let o3_val = fmt_opt(city_data.o3, 1);
                rsx! {
                    div { class: "stats-grid",
                        StatCard { label: "CO", value: co_val, unit: co_label, color: co_color.to_string() }
                        StatCard { label: "NO\u{2082}", value: no2_val, unit: no2_label, color: no2_color.to_string() }
                        StatCard { label: "O\u{2083}", value: o3_val, unit: o3_label, color: o3_color.to_string() }
                        StatCard { label: "UV Index", value: uv_display, unit: "", color: "normal".to_string() }
                    }
                }
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
        let source = sr.sensor.source.as_deref().unwrap_or("unknown");
        Some(format!("{{lat:{lat},lon:{lon},pm25:{pm25:.1},pm10:{pm10:.1},id:{},source:'{source}'}}", sr.sensor.id))
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
    let city_name = snap.active_city.as_ref().map(|c| c.name.as_str()).unwrap_or("Air Signal");

    // Run map JS on every render (props change = re-render = map updates)
    let js = format!(r#"
        (async function() {{
            // --- Load Leaflet CSS ---
            if (!document.getElementById('leaflet-css')) {{
                var link = document.createElement('link');
                link.id = 'leaflet-css';
                link.rel = 'stylesheet';
                link.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
                document.head.appendChild(link);
            }}
            // Pulsing animation removed — was too aggressive on map
            // --- Load Leaflet JS ---
            if (!window.L) {{
                await new Promise((resolve, reject) => {{
                    var s = document.createElement('script');
                    s.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
                    s.onload = resolve;
                    s.onerror = reject;
                    document.head.appendChild(s);
                }});
            }}
            // --- Load Leaflet.heat plugin ---
            if (!window.L.heatLayer) {{
                await new Promise((resolve, reject) => {{
                    var s = document.createElement('script');
                    s.src = 'https://unpkg.com/leaflet.heat@0.2.0/dist/leaflet-heat.js';
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

            // --- Base layers ---
            var darkLayer = L.tileLayer('https://{{s}}.basemaps.cartocdn.com/dark_all/{{z}}/{{x}}/{{y}}{{r}}.png', {{maxZoom: 18, attribution: '&copy; CartoDB'}});
            var osmLayer = L.tileLayer('https://{{s}}.tile.openstreetmap.org/{{z}}/{{x}}/{{y}}.png', {{maxZoom: 19, attribution: '&copy; OpenStreetMap'}});
            var voyagerLayer = L.tileLayer('https://{{s}}.basemaps.cartocdn.com/rastertiles/voyager/{{z}}/{{x}}/{{y}}{{r}}.png', {{maxZoom: 18, attribution: '&copy; CartoDB'}});
            darkLayer.addTo(map);
            var baseMaps = {{'CartoDB Dark': darkLayer, 'OpenStreetMap': osmLayer, 'CartoDB Voyager': voyagerLayer}};

            var sensors = {sensors_data};
            var cities = {cities_data};

            // --- PM2.5 heatmap overlay ---
            var heatPoints = [];
            sensors.forEach(function(s) {{
                if (s.pm25 > 0) {{
                    heatPoints.push([s.lat, s.lon, s.pm25 / 50.0]);
                }}
            }});
            var heatLayer = L.heatLayer(heatPoints, {{radius: 25, blur: 15, maxZoom: 17, gradient: {{0.2:'#4ade80', 0.5:'#facc15', 0.7:'#fb923c', 1.0:'#f87171'}}}});

            var overlayMaps = {{'PM2.5 Heatmap': heatLayer}};
            L.control.layers(baseMaps, overlayMaps, {{position: 'topright'}}).addTo(map);

            // --- Sensor markers with proportional size and pulsing ---
            sensors.forEach(function(s) {{
                var color = s.pm25<=12?'#4ade80':s.pm25<=35?'#facc15':s.pm25<=55?'#fb923c':'#f87171';
                var radius = Math.max(6, Math.min(14, 6 + (s.pm25 / 55.0) * 8));
                var marker = L.circleMarker([s.lat, s.lon], {{
                    radius: radius,
                    fillColor: color,
                    fillOpacity: 0.85,
                    color: '#333',
                    weight: 1,
                    className: ''
                }});
                var popupHtml = '<div style="font-family:system-ui;font-size:13px;line-height:1.6">'
                    + '<b style="font-size:14px">Sensor #' + s.id + '</b><br>'
                    + '<span style="color:' + color + ';font-weight:700">PM2.5: ' + s.pm25 + '</span> \u00b5g/m\u00b3<br>'
                    + 'PM10: ' + s.pm10 + ' \u00b5g/m\u00b3<br>'
                    + '<span style="color:#888;font-size:11px">' + s.source + '</span>'
                    + '</div>';
                marker.bindPopup(popupHtml);
                marker.addTo(map);
            }});

            // --- City circles ---
            cities.forEach(function(c) {{
                L.circle([c.lat, c.lon], {{radius:c.radius*1000, color:'#60a5fa', fillOpacity:0.05, weight:1, dashArray:'4'}}).addTo(map);
            }});

            // --- City label ---
            var cityLabel = L.divIcon({{
                className: 'airq-city-label',
                html: '<div style="color:#60a5fa;font-size:16px;font-weight:700;text-shadow:0 1px 4px rgba(0,0,0,0.8);white-space:nowrap">{city_name}</div>',
                iconSize: [0, 0],
                iconAnchor: [-10, 30]
            }});
            L.marker([{center_lat}, {center_lon}], {{icon: cityLabel, interactive: false}}).addTo(map);

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

/// Color class for a comfort score (0-100).
fn score_color(score: u32) -> &'static str {
    match score {
        80..=100 => "green",
        50..=79 => "yellow",
        30..=49 => "orange",
        _ => "red",
    }
}

/// Format an optional f64 with given precision, or "--" if None.
fn fmt_opt(val: Option<f64>, precision: usize) -> String {
    match val {
        Some(v) => format!("{v:.prec$}", prec = precision),
        None => "--".to_string(),
    }
}

/// Comfort: full 6-signal matrix + source + WHO
#[component]
fn ComfortView(snap: MonitorSnapshot, city_data: CityData) -> Element {
    let pm25 = snap.avg_pm25.unwrap_or(0.0);
    let pm10 = snap.avg_pm10.unwrap_or(0.0);

    // Signal matrix rows: (label, raw_value_str, unit, score, weight_pct)
    let signals: Vec<(&str, String, &str, u32, u32)> = if city_data.loaded {
        vec![
            ("Air (AQI)", format!("{}", city_data.aqi), "", city_data.air_score, 30),
            ("Temperature", fmt_opt(city_data.temperature_c, 1), "\u{00b0}C", city_data.temperature_score, 25),
            ("Wind", fmt_opt(city_data.wind_kmh, 1), "km/h", city_data.wind_score, 10),
            ("UV", fmt_opt(city_data.uv_index, 1), "", city_data.uv_score, 10),
            ("Pressure", fmt_opt(city_data.pressure_hpa, 0), "hPa", city_data.pressure_score, 15),
            ("Humidity", fmt_opt(city_data.humidity_pct, 0), "%", city_data.humidity_score, 10),
        ]
    } else {
        vec![]
    };

    rsx! {
        div { class: "view-header",
            h1 { "Air Comfort" }
        }

        // Total comfort score (hero)
        if city_data.loaded {
            div { class: "card comfort-hero",
                div { class: "comfort-score-big {score_color(city_data.comfort_total)}",
                    "{city_data.comfort_total}"
                }
                div { class: "comfort-label", "{city_data.comfort_label}" }
            }

            // Signal matrix table
            div { class: "card",
                h2 { "Signal Matrix" }
                table { class: "data-table",
                    thead {
                        tr {
                            th { "Signal" }
                            th { class: "num", "Raw Value" }
                            th { class: "num", "Score" }
                            th { class: "num", "Weight" }
                            th { class: "num", "Weighted" }
                            th { style: "width:120px", "Bar" }
                        }
                    }
                    tbody {
                        for (label, raw, unit, score, weight) in signals.iter() {
                            {
                                let weighted = (*score as f64 * *weight as f64 / 100.0).round() as u32;
                                let bar_width = format!("{}%", score);
                                let bar_color = score_color(*score);
                                let raw_display = if unit.is_empty() {
                                    raw.clone()
                                } else {
                                    format!("{raw} {unit}")
                                };
                                rsx! {
                                    tr {
                                        td { "{label}" }
                                        td { class: "num", "{raw_display}" }
                                        td { class: "num {bar_color}", "{score}/100" }
                                        td { class: "num", "{weight}%" }
                                        td { class: "num", "{weighted}" }
                                        td {
                                            div { class: "progress-bar",
                                                div {
                                                    class: "progress-fill {bar_color}",
                                                    style: "width:{bar_width}",
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            div { class: "card empty-state",
                p { "Loading comfort data from APIs..." }
            }
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
                        td { class: "num", "15 \u{00b5}g/m\u{00b3}" }
                        td { class: "num {pm25_color(Some(pm25))}", "{pm25:.1}" }
                        td { if pm25 <= 15.0 { "OK" } else { "Exceeded" } }
                    }
                    tr {
                        td { "PM10" }
                        td { class: "num", "45 \u{00b5}g/m\u{00b3}" }
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
    let city_name = snap.active_city.as_ref().map(|c| c.name.as_str()).unwrap_or("All cities");
    let sensor_count = snap.sensor_count;

    rsx! {
        div { class: "view-header",
            h1 { "Events \u{2014} {city_name}" }
            span { class: "view-subtitle", "{snap.events.len()} in last 24h \u{00b7} {sensor_count} sensors analyzed" }
        }

        if snap.events.is_empty() {
            div { class: "card empty-state",
                p { "No pollution events detected in the last 24 hours." }
                p { class: "muted", "Events are detected when multiple sensors show anomalous readings simultaneously." }
                p { class: "muted", "Monitoring {sensor_count} sensors in {city_name}." }
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
                        let details = parts.join(" \u{00b7} ");
                        rsx! { span { "{details}" } }
                    }
                }
                // PM ratio interpretation
                if let Some(ratio) = event.ratio {
                    {
                        let ev_pm25 = event.pm25.unwrap_or(0.0);
                        let ev_pm10 = event.pm10.unwrap_or(0.0);
                        let source = airq_core::event::classify_source(ratio, ev_pm25, ev_pm10);
                        let interp = format!("Source: {} ({})", source.label, source.reason);
                        rsx! {
                            div { class: "event-source-interp muted", "{interp}" }
                        }
                    }
                }
                if let Some(ref summary) = event.summary {
                    div { class: "event-summary-full", "{summary}" }
                }
            }
        }
    }
}

/// History: readings over time — sensor table with latest data
#[component]
fn HistoryView(snap: MonitorSnapshot) -> Element {
    let total_readings = snap.total_reading_count;
    let active_count = snap.sensors.iter().filter(|sr| sr.latest.is_some()).count();

    rsx! {
        div { class: "view-header",
            h1 { "History" }
            span { class: "view-subtitle", "{total_readings} total readings in DB" }
        }

        // Stats row
        div { class: "stats-grid",
            StatCard { label: "Total Readings", value: format!("{total_readings}"), unit: "in database", color: "normal".to_string() }
            StatCard { label: "Active Sensors", value: format!("{active_count}"), unit: "with data", color: "normal".to_string() }
            StatCard { label: "Total Sensors", value: format!("{}", snap.sensor_count), unit: "registered", color: "normal".to_string() }
        }

        div { class: "card",
            h2 { "Latest Readings Per Sensor" }
            if snap.sensors.is_empty() {
                p { class: "muted", "No sensor data yet. Start monitoring to collect readings." }
            } else {
                table { class: "data-table",
                    thead {
                        tr {
                            th { "Sensor" }
                            th { class: "num", "PM2.5" }
                            th { class: "num", "PM10" }
                            th { "Source" }
                            th { "Last Reading" }
                            th { "Status" }
                        }
                    }
                    tbody {
                        for sr in snap.sensors.iter() {
                            {
                                let pm25 = sr.latest.as_ref().and_then(|r| r.pm25);
                                let pm10 = sr.latest.as_ref().and_then(|r| r.pm10);
                                let source = sr.sensor.source.as_deref().unwrap_or("\u{2014}");
                                let time_str = sr.latest.as_ref().map(|r| format_ts(r.ts)).unwrap_or_else(|| "\u{2014}".to_string());
                                let has_data = sr.latest.is_some();
                                let status_class = if has_data { "good" } else { "muted" };
                                let status_text = if has_data { "Active" } else { "No data" };
                                let pm25_display = fmt_opt(pm25, 1);
                                let pm10_display = fmt_opt(pm10, 1);
                                let pm25_cls = pm25_color(pm25);
                                rsx! {
                                    tr {
                                        td { "#{sr.sensor.id}" }
                                        td { class: "num {pm25_cls}", "{pm25_display}" }
                                        td { class: "num", "{pm10_display}" }
                                        td { "{source}" }
                                        td { class: "muted", "{time_str}" }
                                        td { span { class: "{status_class}", "{status_text}" } }
                                    }
                                }
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
fn SourcesView(snap: MonitorSnapshot, city_data: CityData) -> Element {
    let pm25 = snap.avg_pm25.unwrap_or(0.0);
    let pm10 = snap.avg_pm10.unwrap_or(0.0);
    let ratio = if pm25 > 1.0 { pm10 / pm25 } else { 1.0 };
    let source = airq_core::event::classify_source(ratio, pm25, pm10);
    let ratio_display = format!("{ratio:.2}");

    rsx! {
        div { class: "view-header",
            h1 { "Source Attribution" }
        }

        div { class: "card",
            h2 { "Current Classification" }
            div { class: "source-hero",
                div { class: "source-hero-label", "{source.label}" }
                div { class: "source-hero-ratio", "PM10/PM2.5 ratio: {ratio_display}" }
            }
            div { class: "source-info",
                div { class: "source-reason", "{source.reason}" }
                div { class: "source-advice", "{source.advice}" }
            }
        }

        // Extended Pollutants card
        if city_data.loaded {
            div { class: "card",
                h2 { "Extended Pollutants" }
                div { class: "pollutant-grid",
                    {
                        // CO
                        let co_val = fmt_opt(city_data.co, 0);
                        let co_unit = "\u{00b5}g/m\u{00b3}".to_string();
                        let co_status = city_data.co.map(|v| airq_core::get_co_status(v));
                        let co_label = co_status.as_ref().map(|s| s.label()).unwrap_or("--");
                        let co_color = co_status.map(pollutant_color).unwrap_or("normal");
                        // NO2
                        let no2_val = fmt_opt(city_data.no2, 1);
                        let no2_unit = "\u{00b5}g/m\u{00b3}".to_string();
                        let no2_status = city_data.no2.map(|v| airq_core::get_no2_status(v));
                        let no2_label = no2_status.as_ref().map(|s| s.label()).unwrap_or("--");
                        let no2_color = no2_status.map(pollutant_color).unwrap_or("normal");
                        // SO2
                        let so2_val = fmt_opt(city_data.so2, 1);
                        let so2_unit = "\u{00b5}g/m\u{00b3}".to_string();
                        let so2_status = city_data.so2.map(|v| airq_core::get_so2_status(v));
                        let so2_label = so2_status.as_ref().map(|s| s.label()).unwrap_or("--");
                        let so2_color = so2_status.map(pollutant_color).unwrap_or("normal");
                        // O3
                        let o3_val = fmt_opt(city_data.o3, 1);
                        let o3_unit = "\u{00b5}g/m\u{00b3}".to_string();
                        let o3_status = city_data.o3.map(|v| airq_core::get_o3_status(v));
                        let o3_label = o3_status.as_ref().map(|s| s.label()).unwrap_or("--");
                        let o3_color = o3_status.map(pollutant_color).unwrap_or("normal");

                        rsx! {
                            div { class: "pollutant-item",
                                div { class: "pollutant-name", "CO" }
                                div { class: "pollutant-value {co_color}", "{co_val}" }
                                div { class: "pollutant-unit", "{co_unit}" }
                                div { class: "pollutant-status {co_color}", "{co_label}" }
                            }
                            div { class: "pollutant-item",
                                div { class: "pollutant-name", "NO\u{2082}" }
                                div { class: "pollutant-value {no2_color}", "{no2_val}" }
                                div { class: "pollutant-unit", "{no2_unit}" }
                                div { class: "pollutant-status {no2_color}", "{no2_label}" }
                            }
                            div { class: "pollutant-item",
                                div { class: "pollutant-name", "SO\u{2082}" }
                                div { class: "pollutant-value {so2_color}", "{so2_val}" }
                                div { class: "pollutant-unit", "{so2_unit}" }
                                div { class: "pollutant-status {so2_color}", "{so2_label}" }
                            }
                            div { class: "pollutant-item",
                                div { class: "pollutant-name", "O\u{2083}" }
                                div { class: "pollutant-value {o3_color}", "{o3_val}" }
                                div { class: "pollutant-unit", "{o3_unit}" }
                                div { class: "pollutant-status {o3_color}", "{o3_label}" }
                            }
                        }
                    }
                }
            }
        }

        div { class: "card",
            h2 { "PM Ratio Guide" }
            table { class: "data-table",
                thead { tr { th { "Ratio" } th { "Source Type" } th { "Examples" } } }
                tbody {
                    tr { td { "> 4.0" } td { "Dust/Sand Storm" } td { class: "muted", "Saharan dust, volcanic ash" } }
                    tr { td { "2.5\u{2013}4.0" } td { "Construction Dust" } td { class: "muted", "Building sites, unpaved roads" } }
                    tr { td { "1.5\u{2013}2.5" } td { "Mixed Urban" } td { class: "muted", "Traffic + heating + industry" } }
                    tr { td { "0.9\u{2013}1.5" } td { "Combustion" } td { class: "muted", "Diesel, coal heating, power plants" } }
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

/// Network: local server info + LAN sensor discovery
#[component]
fn NetworkView(
    local_ip: String,
    all_ips: Vec<(String, &'static str)>,
    lan_sensors: Signal<Vec<state::LanSensor>>,
    scanning: Signal<bool>,
    is_running: bool,
    server_running: Signal<bool>,
    server_shutdown: Signal<Option<tokio::sync::watch::Sender<bool>>>,
    db: Signal<Option<Arc<Db>>>,
    port: u16,
) -> Element {
    let mut lan_sensors = lan_sensors;
    let mut scanning = scanning;
    let mut server_running = server_running;
    let mut server_shutdown = server_shutdown;

    let scan_network = {
        let ip = local_ip.clone();
        move |_| {
            let ip = ip.clone();
            scanning.set(true);
            spawn(async move {
                let found = state::scan_lan_sensors(&ip).await;
                lan_sensors.set(found);
                scanning.set(false);
            });
        }
    };

    let toggle_server = move |_| {
        if (server_running)() {
            // Stop
            if let Some(tx) = (server_shutdown)() {
                let _ = tx.send(true);
            }
            server_shutdown.set(None);
            server_running.set(false);
        } else {
            // Start
            if let Some(ref db_handle) = (db)() {
                let db_handle = db_handle.clone();
                spawn(async move {
                    match state::start_http_server(db_handle, port).await {
                        Ok(tx) => {
                            server_shutdown.set(Some(tx));
                            server_running.set(true);
                        }
                        Err(e) => {
                            tracing::error!("Failed to start server: {e}");
                        }
                    }
                });
            }
        }
    };

    let sensors = (lan_sensors)();
    let is_scanning = (scanning)();
    let srv_running = (server_running)();
    let server_url = format!("http://{local_ip}:{port}");
    let push_url = format!("http://{local_ip}:{port}/api/push");

    rsx! {
        div { class: "view-header",
            h1 { "Network" }
            span { class: "view-subtitle", "{local_ip}" }
        }

        // Server card
        div { class: "card",
            h2 { "Local Server" }
            div { class: "net-info",
                // All interfaces
                for (ip, label) in all_ips.iter() {
                    div { class: "net-row",
                        span { class: "net-label", "{label}" }
                        span { class: "net-value", "{ip}" }
                    }
                }
                div { class: "net-row",
                    span { class: "net-label", "Port" }
                    span { class: "net-value", "{port}" }
                }
                div { class: "net-row",
                    span { class: "net-label", "Dashboard" }
                    span { class: "net-value net-url", "{server_url}" }
                }
                div { class: "net-row",
                    span { class: "net-label", "Push API" }
                    span { class: "net-value net-url", "{push_url}" }
                }
                div { class: "net-row",
                    span { class: "net-label", "Collector" }
                    if is_running {
                        span { class: "net-value good", "Running" }
                    } else {
                        span { class: "net-value muted", "Stopped" }
                    }
                }
                div { class: "net-row",
                    span { class: "net-label", "HTTP Server" }
                    if srv_running {
                        span { class: "net-value good", "Running on :{port}" }
                    } else {
                        span { class: "net-value muted", "Stopped" }
                    }
                }
            }
            div { class: "net-actions",
                button {
                    class: if srv_running { "btn-server btn-stop" } else { "btn-server btn-start" },
                    onclick: toggle_server,
                    disabled: !is_running,
                    if srv_running { "Stop Server" } else { "Start Server" }
                }
                if !is_running {
                    p { class: "muted small", "Start monitoring first (click a city above)" }
                }
            }
        }

        // ESP8266 config hint
        div { class: "card",
            h2 { "ESP8266 Setup" }
            p { class: "muted", "Configure your sensor to send data here:" }
            div { class: "settings-cmd",
                span { class: "muted", "Send to own API: " }
                code { "{push_url}" }
            }
            p { class: "muted small", "In sensor config: Server = {local_ip}, Path = /api/push, Port = {port}" }
        }

        // Daemon install instructions
        div { class: "card",
            h2 { "Install as Daemon" }
            p { class: "muted small", "Run headless server on boot (without dashboard)" }

            div { class: "daemon-section",
                div { class: "daemon-os", "macOS (launchd)" }
                div { class: "settings-cmd",
                    code { "airq serve --city gazipasa --radius 15 --port 8080" }
                }
                div { class: "settings-cmd",
                    code { "# Create ~/Library/LaunchAgents/com.airsignal.serve.plist" }
                }
                div { class: "settings-cmd",
                    code { "launchctl load ~/Library/LaunchAgents/com.airsignal.serve.plist" }
                }
            }

            div { class: "daemon-section",
                div { class: "daemon-os", "Linux (systemd)" }
                div { class: "settings-cmd",
                    code { "# Create ~/.config/systemd/user/air-signal.service" }
                }
                div { class: "settings-cmd",
                    code { "systemctl --user enable --now air-signal" }
                }
            }

            div { class: "daemon-section",
                div { class: "daemon-os", "Windows" }
                div { class: "settings-cmd",
                    code { "nssm install AirSignal airq.exe serve --city gazipasa" }
                }
                div { class: "settings-cmd",
                    code { "nssm start AirSignal" }
                }
            }
        }

        // LAN Sensors
        div { class: "card",
            div { class: "net-header-row",
                h2 { "Sensors in Network" }
                button {
                    class: "btn-scan",
                    onclick: scan_network,
                    disabled: is_scanning,
                    if is_scanning { "Scanning..." } else { "Scan LAN" }
                }
            }

            if is_scanning {
                div { class: "scan-progress", "Scanning 254 addresses..." }
            }

            if sensors.is_empty() && !is_scanning {
                p { class: "muted", "No sensors found. Click Scan LAN to search." }
            }

            if !sensors.is_empty() {
                table { class: "data-table",
                    thead {
                        tr {
                            th { "IP" }
                            th { "ID" }
                            th { class: "num", "PM2.5" }
                            th { class: "num", "PM10" }
                            th { class: "num", "Temp" }
                            th { "Firmware" }
                        }
                    }
                    tbody {
                        for sensor in sensors.iter() {
                            {
                                let ip = sensor.ip.clone();
                                let config_url = format!("http://{ip}");
                                if let Some(ref d) = sensor.data {
                                    let id = d.esp_id.clone();
                                    let pm25 = fmt_opt(d.pm25, 1);
                                    let pm10 = fmt_opt(d.pm10, 1);
                                    let temp = d.temp.map(|v| format!("{v:.0}°")).unwrap_or("—".into());
                                    let fw = d.software_version.clone();
                                    let pm25_cls = pm25_color(d.pm25);
                                    rsx! {
                                        tr {
                                            td { class: "net-url", "{config_url}" }
                                            td { "{id}" }
                                            td { class: "num {pm25_cls}", "{pm25}" }
                                            td { class: "num", "{pm10}" }
                                            td { class: "num", "{temp}" }
                                            td { class: "muted", "{fw}" }
                                        }
                                    }
                                } else {
                                    rsx! {
                                        tr {
                                            td { class: "net-url", "{config_url}" }
                                            td { colspan: "5", class: "muted", "Reachable (no sensor data)" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Settings: monitoring, server, cities, DB info — with save to config.toml
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
    let mut port_input: Signal<u16> = use_signal(|| 8080);
    let mut cities_text: Signal<String> = use_signal(move || config_cities.join(", "));
    let mut save_status: Signal<Option<String>> = use_signal(|| None);

    let save_config = move |_| {
        let city = (city_input)();
        let radius = (radius_input)();
        let cities_str = (cities_text)();
        let cities: Vec<String> = cities_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let config = AppConfig {
            default_city: Some(city),
            cities: Some(cities),
            sensor_id: None,
            radius: Some(radius),
            sources: None,
        };

        match config.save() {
            Ok(()) => {
                let path = format!("{}", AppConfig::path().display());
                save_status.set(Some(format!("Saved to {path}")));
            }
            Err(e) => {
                save_status.set(Some(format!("Error: {e}")));
            }
        }
    };

    let config_path = format!("{}", AppConfig::path().display());

    rsx! {
        div { class: "view-header",
            h1 { "Settings" }
        }

        // Monitoring
        div { class: "card",
            h2 { "Monitoring" }
            div { class: "settings-form",
                div { class: "form-row",
                    label { "Default City" }
                    input {
                        r#type: "text",
                        value: "{city_input}",
                        oninput: move |e| city_input.set(e.value()),
                    }
                }
                div { class: "form-row",
                    label { "Radius (km)" }
                    input {
                        r#type: "number",
                        value: "{radius_input}",
                        oninput: move |e| { if let Ok(v) = e.value().parse::<f64>() { radius_input.set(v); } },
                    }
                }
                div { class: "form-row",
                    label { "Poll Interval" }
                    input {
                        r#type: "number",
                        value: "{interval_input}",
                        oninput: move |e| { if let Ok(v) = e.value().parse::<u64>() { interval_input.set(v); } },
                    }
                    span { class: "form-hint", "sec" }
                }
            }
            if is_running {
                div { class: "settings-status good", "Collector running" }
            }
            if let Some(ref err) = error_msg {
                div { class: "error", "{err}" }
            }
        }

        // Server (headless mode)
        div { class: "card",
            h2 { "Server (headless)" }
            p { class: "muted small", "Settings for `airq serve` daemon mode (without dashboard)" }
            div { class: "settings-form",
                div { class: "form-row",
                    label { "Port" }
                    input {
                        r#type: "number",
                        value: "{port_input}",
                        oninput: move |e| { if let Ok(v) = e.value().parse::<u16>() { port_input.set(v); } },
                    }
                }
                div { class: "form-row",
                    label { "Bind Address" }
                    input { r#type: "text", value: "0.0.0.0", disabled: true }
                }
                div { class: "form-row",
                    label { "API Endpoints" }
                    div { class: "settings-endpoints",
                        code { "/api/status" }
                        code { "/api/readings" }
                        code { "/api/sensors" }
                        code { "/api/events" }
                        code { "/api/cities" }
                        code { "/api/push" }
                    }
                }
            }
            {
                let port = (port_input)();
                let cmd = format!("airq serve --city gazipasa --radius 15 --port {port}");
                rsx! {
                    div { class: "settings-cmd",
                        span { class: "muted", "Run: " }
                        code { "{cmd}" }
                    }
                }
            }
        }

        // Cities (editable)
        div { class: "card",
            h2 { "Cities" }
            div { class: "settings-form",
                div { class: "form-row",
                    label { "City List" }
                    input {
                        r#type: "text",
                        value: "{cities_text}",
                        oninput: move |e| cities_text.set(e.value()),
                        placeholder: "gazipasa, istanbul, moscow",
                    }
                }
            }
            p { class: "muted small", "Comma-separated. Used for top bar switcher." }
        }

        // Save button
        div { class: "card",
            button {
                class: "btn-save",
                onclick: save_config,
                "Save to config.toml"
            }
            if let Some(ref status) = (save_status)() {
                {
                    let is_err = status.starts_with("Error");
                    let cls = if is_err { "error" } else { "save-ok" };
                    rsx! { div { class: "{cls}", "{status}" } }
                }
            }
            p { class: "muted small", "Config: {config_path}" }
        }

        // Database
        div { class: "card",
            h2 { "Database" }
            {
                let db_path = format!("{}", state::default_db_path().display());
                let total = snap.total_reading_count;
                let sensors = snap.sensor_count;
                let n_cities = snap.cities.len();
                rsx! {
                    div { class: "settings-info",
                        div { "Path: {db_path}" }
                        div { "Total readings: {total}" }
                        div { "Sensors: {sensors}" }
                        div { "Cities: {n_cities}" }
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
            td { class: "num {pm25_color(pm25)}", {fmt_opt(pm25, 1)} }
            td { class: "num", {fmt_opt(pm10, 1)} }
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

fn pollutant_color(cat: airq_core::AqiCategory) -> &'static str {
    match cat {
        airq_core::AqiCategory::Good => "good",
        airq_core::AqiCategory::Moderate => "moderate",
        airq_core::AqiCategory::UnhealthySensitive => "unhealthy-sg",
        _ => "unhealthy",
    }
}

fn fmt_pm(val: Option<f64>) -> String {
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
/* Network */
.net-info { font-size: 0.85rem; }
.net-row { display: flex; padding: 6px 0; border-bottom: 1px solid var(--border); }
.net-row:last-child { border-bottom: none; }
.net-label { color: var(--muted); min-width: 120px; flex-shrink: 0; }
.net-value { color: var(--text); }
.net-url { color: var(--blue); font-family: monospace; font-size: 0.82rem; }
.net-header-row { display: flex; justify-content: space-between; align-items: center; }
.btn-scan { padding: 5px 14px; border-radius: 8px; border: 1px solid var(--blue); background: none; color: var(--blue); font-size: 0.8rem; cursor: pointer; }
.btn-scan:hover { background: rgba(96,165,250,0.1); }
.btn-scan:disabled { opacity: 0.5; cursor: default; }
.scan-progress { color: var(--yellow); font-size: 0.8rem; padding: 8px 0; animation: pulse 1.5s infinite; }
.net-actions { margin-top: 12px; display: flex; align-items: center; gap: 12px; }
.btn-server { padding: 8px 20px; border-radius: 8px; font-size: 0.85rem; font-weight: 600; cursor: pointer; border: none; }
.btn-start { background: var(--green); color: #000; }
.btn-stop { background: var(--red); color: #fff; }
.btn-server:disabled { opacity: 0.4; cursor: default; }
.btn-server:hover:not(:disabled) { opacity: 0.9; }

.daemon-section { margin-bottom: 12px; }
.daemon-os { font-size: 0.85rem; font-weight: 600; margin-bottom: 4px; color: var(--text); }

.form-hint { font-size: 0.75rem; color: var(--muted); flex-shrink: 0; }
.btn-save { background: var(--blue); color: #000; border: none; border-radius: 10px; padding: 10px 28px; font-size: 0.9rem; font-weight: 600; cursor: pointer; }
.btn-save:hover { opacity: 0.9; }
.save-ok { color: var(--green); font-size: 0.8rem; margin-top: 8px; }
.settings-endpoints { display: flex; flex-wrap: wrap; gap: 6px; }
.settings-endpoints code { background: var(--bg); padding: 3px 8px; border-radius: 6px; font-size: 0.75rem; color: var(--blue); }
.settings-cmd { margin-top: 10px; padding: 8px 12px; background: var(--bg); border-radius: 8px; font-size: 0.8rem; }
.settings-cmd code { color: var(--green); }
.muted { color: var(--muted); }
.small { font-size: 0.75rem; }

/* Comfort hero */
.comfort-hero { text-align: center; padding: 24px; }
.comfort-score-big { font-size: 4rem; font-weight: 800; line-height: 1; }
.comfort-score-big.green { color: var(--green); }
.comfort-score-big.yellow { color: var(--yellow); }
.comfort-score-big.orange { color: var(--orange); }
.comfort-score-big.red { color: var(--red); }
.comfort-label { font-size: 1.2rem; color: var(--muted); margin-top: 4px; }

/* Progress bar in matrix table */
.progress-bar { width: 100%; height: 8px; background: rgba(255,255,255,0.08); border-radius: 4px; overflow: hidden; }
.progress-fill { height: 100%; border-radius: 4px; transition: width 0.3s ease; }
.progress-fill.green { background: var(--green); }
.progress-fill.yellow { background: var(--yellow); }
.progress-fill.orange { background: var(--orange); }
.progress-fill.red { background: var(--red); }

/* Score colors in table cells */
td.green, .num.green { color: var(--green); }
td.yellow, .num.yellow { color: var(--yellow); }
td.orange, .num.orange { color: var(--orange); }
td.red, .num.red { color: var(--red); }

/* Compact comfort widget on dashboard */
.comfort-compact { padding: 16px; }
.comfort-compact-row { display: flex; align-items: center; gap: 16px; }
.comfort-score-compact { font-size: 2.4rem; font-weight: 800; min-width: 64px; text-align: center; }
.comfort-score-compact.green { color: var(--green); }
.comfort-score-compact.yellow { color: var(--yellow); }
.comfort-score-compact.orange { color: var(--orange); }
.comfort-score-compact.red { color: var(--red); }
.comfort-compact-label { font-size: 1rem; font-weight: 600; }
.comfort-compact-details { font-size: 0.82rem; color: var(--muted); margin-top: 2px; }

/* Pollutant grid (Sources view) */
.pollutant-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 12px; }
.pollutant-item { background: var(--bg); border: 1px solid var(--border); border-radius: 10px; padding: 14px; text-align: center; }
.pollutant-name { font-size: 0.75rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.3px; margin-bottom: 4px; }
.pollutant-value { font-size: 1.6rem; font-weight: 700; line-height: 1.1; }
.pollutant-unit { font-size: 0.65rem; color: var(--muted); margin-top: 2px; }
.pollutant-status { font-size: 0.72rem; font-weight: 600; margin-top: 4px; }

/* Event source interpretation */
.event-source-interp { font-size: 0.78rem; margin-top: 4px; font-style: italic; }
"#;
