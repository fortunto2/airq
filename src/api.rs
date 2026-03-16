//! REST API handlers for airq serve.

use crate::db::Db;
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct ReadingsQuery {
    pub sensor: Option<i64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CityQuery {
    pub city: Option<i64>,
    pub from: Option<i64>,
}

#[derive(Debug, Serialize)]
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

pub async fn readings_handler(
    State(db): State<Arc<Db>>,
    Query(q): Query<ReadingsQuery>,
) -> Json<Vec<crate::db::Reading>> {
    let sensor = q.sensor.unwrap_or(0);
    let from = q.from.unwrap_or(0);
    let to = q.to.unwrap_or(i64::MAX);
    let readings = db.query_readings(sensor, from, to).unwrap_or_default();
    Json(readings)
}

pub async fn sensors_handler(
    State(db): State<Arc<Db>>,
    Query(q): Query<CityQuery>,
) -> Json<Vec<crate::db::Sensor>> {
    if let Some(city_id) = q.city {
        Json(db.sensors_for_city(city_id).unwrap_or_default())
    } else {
        Json(db.all_sensors().unwrap_or_default())
    }
}

pub async fn events_handler(
    State(db): State<Arc<Db>>,
    Query(q): Query<CityQuery>,
) -> Json<Vec<crate::db::Event>> {
    let city_id = q.city.unwrap_or(0);
    let from = q.from.unwrap_or(0);
    Json(db.query_events(city_id, from).unwrap_or_default())
}

pub async fn cities_handler(
    State(db): State<Arc<Db>>,
) -> Json<Vec<crate::db::City>> {
    Json(db.all_cities().unwrap_or_default())
}
