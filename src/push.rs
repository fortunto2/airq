//! ESP8266 push receiver: POST /api/push
//!
//! Accepts Sensor.Community JSON format from ESP8266 "Send to own API" config.

use crate::db::{Db, Reading};
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;

/// ESP8266/ESP32 push payload (Sensor.Community format)
#[derive(Debug, Deserialize, ToSchema)]
pub struct PushPayload {
    pub sensordatavalues: Vec<SensorDataValue>,
    #[serde(default)]
    pub esp8266id: Option<String>,
    #[serde(default)]
    pub software_version: Option<String>,
}

/// Single sensor data value
#[derive(Debug, Deserialize, ToSchema)]
pub struct SensorDataValue {
    /// Value type: SDS_P1 (PM10), SDS_P2 (PM2.5), BME280_temperature, etc.
    pub value_type: String,
    /// String-encoded numeric value
    pub value: String,
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct PushResponse {
    pub status: &'static str,
    pub sensor_id: i64,
}

/// Parse ESP8266 push payload into a Reading.
pub fn parse_push(payload: &PushPayload) -> (i64, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    let sensor_id = payload
        .esp8266id
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let mut pm25 = None;
    let mut pm10 = None;
    let mut temp = None;
    let mut humidity = None;
    let mut pressure = None;

    for v in &payload.sensordatavalues {
        let val = v.value.parse::<f64>().ok();
        match v.value_type.as_str() {
            "SDS_P1" | "P1" => pm10 = val,
            "SDS_P2" | "P2" => pm25 = val,
            "BME280_temperature" | "temperature" => temp = val,
            "BME280_humidity" | "humidity" => humidity = val,
            "BME280_pressure" | "pressure" => {
                // BME280 reports Pa, convert to hPa if > 10000
                pressure = val.map(|v| if v > 10000.0 { v / 100.0 } else { v });
            }
            _ => {}
        }
    }

    (sensor_id, pm25, pm10, temp, humidity, pressure)
}

/// Push sensor data from ESP8266/ESP32 (Sensor.Community JSON format)
#[utoipa::path(
    post, path = "/api/push",
    tag = "push",
    request_body = PushPayload,
    responses(
        (status = 200, description = "Data accepted", body = PushResponse),
        (status = 400, description = "Missing esp8266id"),
    )
)]
pub async fn push_handler(
    State(db): State<Arc<Db>>,
    Json(payload): Json<PushPayload>,
) -> Result<Json<PushResponse>, StatusCode> {
    let (sensor_id, pm25, pm10, temp, humidity, pressure) = parse_push(&payload);

    if sensor_id == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Upsert sensor as "local"
    let _ = db.upsert_sensor(sensor_id, None, None, None, Some("local"));

    let reading = Reading {
        ts: now,
        sensor: sensor_id,
        lat: None,
        lon: None,
        pm25,
        pm10,
        temp,
        humidity,
        pressure,
    };

    db.insert_reading(&reading).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PushResponse {
        status: "ok",
        sensor_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_push_sds011() {
        let payload = PushPayload {
            sensordatavalues: vec![
                SensorDataValue { value_type: "SDS_P1".into(), value: "12.5".into() },
                SensorDataValue { value_type: "SDS_P2".into(), value: "8.3".into() },
                SensorDataValue { value_type: "BME280_temperature".into(), value: "22.1".into() },
                SensorDataValue { value_type: "BME280_humidity".into(), value: "45.0".into() },
                SensorDataValue { value_type: "BME280_pressure".into(), value: "101325".into() },
            ],
            esp8266id: Some("15072310".into()),
            software_version: Some("NRZ-2020-133".into()),
        };

        let (sid, pm25, pm10, temp, hum, press) = parse_push(&payload);
        assert_eq!(sid, 15072310);
        assert!((pm25.unwrap() - 8.3).abs() < 0.01);
        assert!((pm10.unwrap() - 12.5).abs() < 0.01);
        assert!((temp.unwrap() - 22.1).abs() < 0.01);
        assert!((hum.unwrap() - 45.0).abs() < 0.01);
        assert!((press.unwrap() - 1013.25).abs() < 0.1); // Pa → hPa conversion
    }

    #[test]
    fn test_parse_push_no_id() {
        let payload = PushPayload {
            sensordatavalues: vec![],
            esp8266id: None,
            software_version: None,
        };
        let (sid, _, _, _, _, _) = parse_push(&payload);
        assert_eq!(sid, 0);
    }

    #[test]
    fn test_parse_push_p1_p2_aliases() {
        let payload = PushPayload {
            sensordatavalues: vec![
                SensorDataValue { value_type: "P1".into(), value: "20.0".into() },
                SensorDataValue { value_type: "P2".into(), value: "10.0".into() },
            ],
            esp8266id: Some("12345".into()),
            software_version: None,
        };
        let (_, pm25, pm10, _, _, _) = parse_push(&payload);
        assert!((pm25.unwrap() - 10.0).abs() < 0.01);
        assert!((pm10.unwrap() - 20.0).abs() < 0.01);
    }
}
