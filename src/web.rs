//! Embedded HTML dashboard for airq serve.

use crate::db::Db;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use std::sync::Arc;

/// Serve the dashboard HTML page.
pub async fn dashboard_handler(
    State(_db): State<Arc<Db>>,
) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        DASHBOARD_HTML,
    )
}

const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Air Signal</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, system-ui, sans-serif; background: #0a0a0a; color: #e0e0e0; }
.container { max-width: 600px; margin: 0 auto; padding: 16px; }
h1 { font-size: 1.4rem; margin-bottom: 12px; }
.card { background: #1a1a1a; border-radius: 12px; padding: 16px; margin-bottom: 12px; }
.card h2 { font-size: 1rem; color: #888; margin-bottom: 8px; }
.stat { font-size: 2rem; font-weight: 700; }
.loading { color: #666; }
#status { color: #4ade80; font-size: 0.85rem; }
</style>
</head>
<body>
<div class="container">
  <h1>Air Signal</h1>
  <div id="status" class="loading">Loading...</div>
  <div class="card"><h2>Sensors</h2><div class="stat" id="sensor-count">—</div></div>
  <div class="card"><h2>Readings</h2><div class="stat" id="reading-count">—</div></div>
  <div class="card"><h2>Cities</h2><div class="stat" id="city-count">—</div></div>
</div>
<script>
async function refresh() {
  try {
    const r = await fetch('/api/status');
    const d = await r.json();
    document.getElementById('sensor-count').textContent = d.sensors;
    document.getElementById('reading-count').textContent = d.readings;
    document.getElementById('city-count').textContent = d.cities;
    document.getElementById('status').textContent = 'Connected — uptime ' + Math.floor(d.uptime_secs/60) + 'm';
  } catch(e) {
    document.getElementById('status').textContent = 'Disconnected';
  }
}
refresh();
setInterval(refresh, 10000);
</script>
</body>
</html>"#;
