//! REST API handlers for airq serve.

use crate::db::{City, Db, Event, Reading, Sensor};
use crate::push::{PushPayload, PushResponse};
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, OpenApi, ToSchema};

// ---------------------------------------------------------------------------
// OpenAPI spec
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Air Signal API",
        version = "1.0.0",
        description = "Air quality monitoring — readings, sensors, events, cities. Push endpoint for ESP8266/ESP32 sensors."
    ),
    paths(status_handler, readings_handler, sensors_handler, events_handler, cities_handler, crate::push::push_handler),
    components(schemas(StatusResponse, Reading, Sensor, City, Event, PushPayload, PushResponse, crate::push::SensorDataValue)),
    tags(
        (name = "status", description = "Server status"),
        (name = "data", description = "Sensor readings and events"),
        (name = "push", description = "ESP8266/ESP32 data ingestion"),
    )
)]
pub struct ApiDoc;

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, IntoParams)]
pub struct ReadingsQuery {
    /// Sensor ID
    pub sensor: Option<i64>,
    /// Unix timestamp (start)
    pub from: Option<i64>,
    /// Unix timestamp (end)
    pub to: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct CityQuery {
    /// City ID
    pub city: Option<i64>,
    /// Unix timestamp (start)
    pub from: Option<i64>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusResponse {
    pub uptime_secs: u64,
    pub cities: usize,
    pub sensors: i64,
    pub readings: i64,
    pub last_poll: Option<i64>,
}

static START_TIME: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub fn init_start_time() {
    START_TIME.get_or_init(std::time::Instant::now);
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Server status: uptime, counts, last poll timestamp
#[utoipa::path(
    get, path = "/api/status",
    tag = "status",
    responses((status = 200, description = "Server status", body = StatusResponse))
)]
pub async fn status_handler(
    State(db): State<Arc<Db>>,
) -> Json<StatusResponse> {
    let uptime = START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);
    let cities = db.all_cities().unwrap_or_default().len();
    let sensors = db.sensor_count().unwrap_or(0);
    let readings = db.reading_count().unwrap_or(0);
    let last_poll = db.last_reading_ts().unwrap_or(None);

    Json(StatusResponse {
        uptime_secs: uptime,
        cities,
        sensors,
        readings,
        last_poll,
    })
}

/// Query sensor readings by sensor ID and time range
#[utoipa::path(
    get, path = "/api/readings",
    tag = "data",
    params(ReadingsQuery),
    responses((status = 200, description = "List of readings", body = Vec<Reading>))
)]
pub async fn readings_handler(
    State(db): State<Arc<Db>>,
    Query(q): Query<ReadingsQuery>,
) -> Json<Vec<Reading>> {
    let sensor = q.sensor.unwrap_or(0);
    let from = q.from.unwrap_or(0);
    let to = q.to.unwrap_or(i64::MAX);
    let readings = db.query_readings(sensor, from, to).unwrap_or_default();
    Json(readings)
}

/// List sensors, optionally filtered by city
#[utoipa::path(
    get, path = "/api/sensors",
    tag = "data",
    params(CityQuery),
    responses((status = 200, description = "List of sensors", body = Vec<Sensor>))
)]
pub async fn sensors_handler(
    State(db): State<Arc<Db>>,
    Query(q): Query<CityQuery>,
) -> Json<Vec<Sensor>> {
    if let Some(city_id) = q.city {
        Json(db.sensors_for_city(city_id).unwrap_or_default())
    } else {
        Json(db.all_sensors().unwrap_or_default())
    }
}

/// Query detected pollution events by city and time
#[utoipa::path(
    get, path = "/api/events",
    tag = "data",
    params(CityQuery),
    responses((status = 200, description = "List of events", body = Vec<Event>))
)]
pub async fn events_handler(
    State(db): State<Arc<Db>>,
    Query(q): Query<CityQuery>,
) -> Json<Vec<Event>> {
    let city_id = q.city.unwrap_or(0);
    let from = q.from.unwrap_or(0);
    Json(db.query_events(city_id, from).unwrap_or_default())
}

/// List all configured cities
#[utoipa::path(
    get, path = "/api/cities",
    tag = "status",
    responses((status = 200, description = "List of cities", body = Vec<City>))
)]
pub async fn cities_handler(
    State(db): State<Arc<Db>>,
) -> Json<Vec<City>> {
    Json(db.all_cities().unwrap_or_default())
}
