//! Embedded HTML dashboard for airq serve.
//!
//! Single-page: Leaflet map + PM2.5 chart + event list + sensor table.
//! Phone-first, dark theme, minimal JS, no build step.

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

const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1">
<title>Air Signal</title>
<link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css"/>
<style>
:root { --bg: #0a0a0a; --card: #1a1a1a; --border: #2a2a2a; --text: #e0e0e0; --muted: #888; --green: #4ade80; --yellow: #facc15; --red: #f87171; --blue: #60a5fa; }
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, system-ui, sans-serif; background: var(--bg); color: var(--text); }
.container { max-width: 600px; margin: 0 auto; padding: 12px; }
header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
h1 { font-size: 1.3rem; }
#status { color: var(--green); font-size: 0.8rem; }
.card { background: var(--card); border-radius: 12px; padding: 14px; margin-bottom: 10px; border: 1px solid var(--border); }
.card h2 { font-size: 0.85rem; color: var(--muted); margin-bottom: 8px; text-transform: uppercase; letter-spacing: 0.5px; }
.stats { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 10px; }
.stat-val { font-size: 1.8rem; font-weight: 700; }
.stat-label { font-size: 0.75rem; color: var(--muted); }
#map { height: 250px; border-radius: 10px; }
#chart { width: 100%; height: 120px; }
.event-list { max-height: 200px; overflow-y: auto; }
.event-item { padding: 8px 0; border-bottom: 1px solid var(--border); font-size: 0.85rem; }
.event-item:last-child { border-bottom: none; }
.badge { display: inline-block; padding: 2px 8px; border-radius: 8px; font-size: 0.75rem; font-weight: 600; margin-right: 6px; }
.badge-event { background: var(--yellow); color: #000; }
.badge-widespread { background: var(--red); color: #fff; }
.sensor-table { width: 100%; font-size: 0.8rem; }
.sensor-table th { text-align: left; color: var(--muted); font-weight: 500; padding: 4px 6px; }
.sensor-table td { padding: 4px 6px; }
.aqi-good { color: var(--green); }
.aqi-moderate { color: var(--yellow); }
.aqi-bad { color: var(--red); }
</style>
</head>
<body>
<div class="container">
  <header>
    <h1>Air Signal</h1>
    <span id="status">Connecting...</span>
  </header>

  <div class="stats">
    <div class="card"><div class="stat-val" id="pm25">—</div><div class="stat-label">PM2.5</div></div>
    <div class="card"><div class="stat-val" id="pm10">—</div><div class="stat-label">PM10</div></div>
    <div class="card"><div class="stat-val" id="sensors">—</div><div class="stat-label">Sensors</div></div>
  </div>

  <div class="card"><h2>Map</h2><div id="map"></div></div>

  <div class="card">
    <h2>PM2.5 — Last 24h</h2>
    <canvas id="chart"></canvas>
  </div>

  <div class="card">
    <h2>Events</h2>
    <div class="event-list" id="events"><span style="color:var(--muted)">No events yet</span></div>
  </div>

  <div class="card">
    <h2>Sensors</h2>
    <table class="sensor-table">
      <thead><tr><th>ID</th><th>PM2.5</th><th>PM10</th><th>Source</th></tr></thead>
      <tbody id="sensor-rows"></tbody>
    </table>
  </div>
</div>

<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
<script>
let map, markers = [], chartCtx, chartData = [];

// Init map
function initMap() {
  map = L.map('map', { zoomControl: false }).setView([36.27, 32.30], 10);
  L.tileLayer('https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png', {
    attribution: '&copy; OSM &amp; CARTO', maxZoom: 18
  }).addTo(map);
}

// Color by PM2.5 level
function pm25Color(v) {
  if (v == null) return '#666';
  if (v <= 12) return '#4ade80';
  if (v <= 35) return '#facc15';
  if (v <= 55) return '#fb923c';
  return '#f87171';
}

function pm25Class(v) {
  if (v == null) return '';
  if (v <= 12) return 'aqi-good';
  if (v <= 35) return 'aqi-moderate';
  return 'aqi-bad';
}

// Tiny canvas chart (no library)
function drawChart(canvas, data) {
  const ctx = canvas.getContext('2d');
  const w = canvas.width = canvas.offsetWidth * 2;
  const h = canvas.height = canvas.offsetHeight * 2;
  ctx.scale(2, 2);
  const cw = w/2, ch = h/2;
  ctx.clearRect(0, 0, cw, ch);
  if (data.length < 2) { ctx.fillStyle='#666'; ctx.fillText('Waiting for data...', 10, ch/2); return; }

  const vals = data.map(d => d.v);
  const max = Math.max(...vals, 50);
  const xStep = cw / (data.length - 1);

  // WHO guideline line at 15
  const guideY = ch - (15 / max) * (ch - 20) - 10;
  ctx.strokeStyle = '#333'; ctx.lineWidth = 1; ctx.setLineDash([4,4]);
  ctx.beginPath(); ctx.moveTo(0, guideY); ctx.lineTo(cw, guideY); ctx.stroke();
  ctx.setLineDash([]);
  ctx.fillStyle = '#555'; ctx.font = '10px sans-serif';
  ctx.fillText('WHO 15', 4, guideY - 3);

  // Data line
  ctx.beginPath(); ctx.strokeStyle = '#60a5fa'; ctx.lineWidth = 1.5;
  for (let i = 0; i < data.length; i++) {
    const x = i * xStep;
    const y = ch - (data[i].v / max) * (ch - 20) - 10;
    if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
  }
  ctx.stroke();

  // Fill
  ctx.lineTo((data.length-1)*xStep, ch); ctx.lineTo(0, ch); ctx.closePath();
  ctx.fillStyle = 'rgba(96,165,250,0.1)'; ctx.fill();
}

async function refresh() {
  try {
    const [statusR, sensorsR, eventsR, citiesR] = await Promise.all([
      fetch('/api/status'), fetch('/api/sensors'), fetch('/api/events?from=0'), fetch('/api/cities')
    ]);
    const status = await statusR.json();
    const sensors = await sensorsR.json();
    const events = await eventsR.json();
    const cities = await citiesR.json();

    // Status
    document.getElementById('status').textContent = 'Connected — ' + status.readings + ' readings';
    document.getElementById('sensors').textContent = status.sensors;

    // Clear markers
    markers.forEach(m => map.removeLayer(m));
    markers = [];

    // Fetch latest readings for each sensor and show on map
    let totalPm25 = 0, totalPm10 = 0, pmCount = 0;
    const sensorRows = [];

    for (const s of sensors) {
      if (s.lat && s.lon) {
        // Get latest reading
        const rr = await fetch(`/api/readings?sensor=${s.id}&from=${Math.floor(Date.now()/1000)-600}&to=${Math.floor(Date.now()/1000)+60}`);
        const readings = await rr.json();
        const latest = readings[readings.length - 1];
        const pm25 = latest ? latest.pm25 : null;
        const pm10 = latest ? latest.pm10 : null;

        if (pm25 != null) { totalPm25 += pm25; pmCount++; }
        if (pm10 != null) totalPm10 += pm10;

        const circle = L.circleMarker([s.lat, s.lon], {
          radius: 8, fillColor: pm25Color(pm25), fillOpacity: 0.8,
          color: '#333', weight: 1
        }).addTo(map);
        circle.bindPopup(`<b>Sensor #${s.id}</b><br>PM2.5: ${pm25 != null ? pm25.toFixed(1) : '—'}<br>PM10: ${pm10 != null ? pm10.toFixed(1) : '—'}`);
        markers.push(circle);

        sensorRows.push({id: s.id, pm25, pm10, source: s.source || '—'});
      }
    }

    // Average PM
    if (pmCount > 0) {
      const avgPm25 = totalPm25 / pmCount;
      const avgPm10 = totalPm10 / pmCount;
      const pm25El = document.getElementById('pm25');
      const pm10El = document.getElementById('pm10');
      pm25El.textContent = avgPm25.toFixed(1);
      pm10El.textContent = avgPm10.toFixed(1);
      pm25El.className = 'stat-val ' + pm25Class(avgPm25);
      pm10El.className = 'stat-val ' + pm25Class(avgPm10);
    }

    // Fit map to sensors
    if (cities.length > 0) {
      map.setView([cities[0].lat, cities[0].lon], 11);
      // Draw city radius
      for (const c of cities) {
        const circle = L.circle([c.lat, c.lon], {
          radius: c.radius * 1000, color: '#60a5fa', fillOpacity: 0.05, weight: 1, dashArray: '4'
        }).addTo(map);
        markers.push(circle);
      }
    }

    // Sensor table
    const tbody = document.getElementById('sensor-rows');
    tbody.innerHTML = sensorRows.map(s =>
      `<tr><td>${s.id}</td><td class="${pm25Class(s.pm25)}">${s.pm25 != null ? s.pm25.toFixed(1) : '—'}</td><td>${s.pm10 != null ? s.pm10.toFixed(1) : '—'}</td><td>${s.source}</td></tr>`
    ).join('');

    // Events
    const eventsEl = document.getElementById('events');
    if (events.length === 0) {
      eventsEl.innerHTML = '<span style="color:var(--muted)">No events detected</span>';
    } else {
      eventsEl.innerHTML = events.slice(0, 20).map(e => {
        const badge = e.event_type === 'Widespread' ? 'badge-widespread' : 'badge-event';
        const time = new Date(e.ts * 1000).toLocaleTimeString();
        return `<div class="event-item"><span class="badge ${badge}">${e.event_type}</span>${time} — ${e.summary || ''}</div>`;
      }).join('');
    }

    // Chart data (accumulate PM2.5 averages over time)
    if (pmCount > 0) {
      chartData.push({ t: Date.now(), v: totalPm25 / pmCount });
      if (chartData.length > 144) chartData.shift(); // keep ~24h at 10min intervals
      drawChart(document.getElementById('chart'), chartData);
    }
  } catch(e) {
    document.getElementById('status').textContent = 'Error: ' + e.message;
    document.getElementById('status').style.color = 'var(--red)';
  }
}

initMap();
refresh();
setInterval(refresh, 30000);
</script>
</body>
</html>"##;
