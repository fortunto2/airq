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

/// Parsed sensor reading: (sensor_id, pm25, pm10, temp, humidity, pressure).
pub type ParsedReading = (i64, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>);

/// Parse ESP8266 push payload into a Reading.
pub fn parse_push(payload: &PushPayload) -> ParsedReading {
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

    // -- Parse tests --

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
        assert!((press.unwrap() - 1013.25).abs() < 0.1); // Pa → hPa
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

    // -- BashAir fixture tests (from bashair/back/tests/api/data/) --

    /// Full measurement: SDS011 + BME280 + metadata (measurement.json from BashAir)
    #[test]
    fn test_bashair_full_measurement() {
        let json = r#"{
            "esp8266id": "11545355",
            "software_version": "NRZ-2020-133",
            "test": "1",
            "sensordatavalues": [
                {"value_type": "SDS_P1", "value": "6.35"},
                {"value_type": "SDS_P2", "value": "3.83"},
                {"value_type": "BME280_temperature", "value": "26.43"},
                {"value_type": "BME280_pressure", "value": "99505.19"},
                {"value_type": "BME280_humidity", "value": "23.77"},
                {"value_type": "samples", "value": "1039137"},
                {"value_type": "min_micro", "value": "27"},
                {"value_type": "max_micro", "value": "20370"},
                {"value_type": "interval", "value": "30000"},
                {"value_type": "signal", "value": "-50"}
            ]
        }"#;

        let payload: PushPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.esp8266id.as_deref(), Some("11545355"));
        assert_eq!(payload.software_version.as_deref(), Some("NRZ-2020-133"));
        assert_eq!(payload.sensordatavalues.len(), 10);

        let (sid, pm25, pm10, temp, hum, press) = parse_push(&payload);
        assert_eq!(sid, 11545355);
        assert!((pm25.unwrap() - 3.83).abs() < 0.01); // SDS_P2 = PM2.5
        assert!((pm10.unwrap() - 6.35).abs() < 0.01); // SDS_P1 = PM10
        assert!((temp.unwrap() - 26.43).abs() < 0.01);
        assert!((hum.unwrap() - 23.77).abs() < 0.01);
        assert!((press.unwrap() - 995.05).abs() < 0.1); // 99505 Pa → 995.05 hPa
    }

    /// Bad sensor: SDS011 only, no BME280 (measurement_bad.json from BashAir)
    #[test]
    fn test_bashair_bad_sensor_no_bme280() {
        let json = r#"{
            "esp8266id": "11545355",
            "software_version": "NRZ-2020-133",
            "test": "1",
            "sensordatavalues": [
                {"value_type": "SDS_P1", "value": "6.35"},
                {"value_type": "SDS_P2", "value": "3.83"},
                {"value_type": "samples", "value": "1039137"},
                {"value_type": "min_micro", "value": "27"},
                {"value_type": "max_micro", "value": "20370"},
                {"value_type": "interval", "value": "30000"},
                {"value_type": "signal", "value": "-50"}
            ]
        }"#;

        let payload: PushPayload = serde_json::from_str(json).unwrap();
        let (sid, pm25, pm10, temp, hum, press) = parse_push(&payload);
        assert_eq!(sid, 11545355);
        assert!((pm25.unwrap() - 3.83).abs() < 0.01);
        assert!((pm10.unwrap() - 6.35).abs() < 0.01);
        assert!(temp.is_none());  // no BME280
        assert!(hum.is_none());
        assert!(press.is_none());
    }

    /// Unknown value types are silently ignored
    #[test]
    fn test_unknown_value_types_ignored() {
        let json = r#"{
            "esp8266id": "99999",
            "sensordatavalues": [
                {"value_type": "SDS_P2", "value": "5.0"},
                {"value_type": "unknown_sensor", "value": "123"},
                {"value_type": "GPS_lat", "value": "36.27"},
                {"value_type": "GPS_lon", "value": "32.30"}
            ]
        }"#;

        let payload: PushPayload = serde_json::from_str(json).unwrap();
        let (_, pm25, pm10, temp, _, _) = parse_push(&payload);
        assert!((pm25.unwrap() - 5.0).abs() < 0.01);
        assert!(pm10.is_none());
        assert!(temp.is_none());
    }

    /// Invalid numeric values don't crash
    #[test]
    fn test_invalid_values_handled() {
        let json = r#"{
            "esp8266id": "99999",
            "sensordatavalues": [
                {"value_type": "SDS_P1", "value": "not_a_number"},
                {"value_type": "SDS_P2", "value": ""},
                {"value_type": "BME280_temperature", "value": "NaN"}
            ]
        }"#;

        let payload: PushPayload = serde_json::from_str(json).unwrap();
        let (_, pm25, pm10, temp, _, _) = parse_push(&payload);
        assert!(pm25.is_none()); // "" can't parse
        assert!(pm10.is_none()); // "not_a_number" can't parse
        // NaN parses as f64 but is still a valid float
    }

    /// Pressure: BME280 reports Pa (>10000), should convert to hPa
    #[test]
    fn test_pressure_pa_to_hpa() {
        let json = r#"{
            "esp8266id": "99999",
            "sensordatavalues": [
                {"value_type": "BME280_pressure", "value": "101325"}
            ]
        }"#;
        let payload: PushPayload = serde_json::from_str(json).unwrap();
        let (_, _, _, _, _, press) = parse_push(&payload);
        assert!((press.unwrap() - 1013.25).abs() < 0.01);
    }

    /// Pressure: if already in hPa (<10000), don't convert
    #[test]
    fn test_pressure_hpa_no_convert() {
        let json = r#"{
            "esp8266id": "99999",
            "sensordatavalues": [
                {"value_type": "BME280_pressure", "value": "1013.25"}
            ]
        }"#;
        let payload: PushPayload = serde_json::from_str(json).unwrap();
        let (_, _, _, _, _, press) = parse_push(&payload);
        assert!((press.unwrap() - 1013.25).abs() < 0.01);
    }

    // -- HTTP handler integration tests --

    #[tokio::test]
    async fn test_push_handler_ok() {
        let db = Db::open_memory().unwrap();
        let db = Arc::new(db);

        let payload = PushPayload {
            sensordatavalues: vec![
                SensorDataValue { value_type: "SDS_P1".into(), value: "6.35".into() },
                SensorDataValue { value_type: "SDS_P2".into(), value: "3.83".into() },
            ],
            esp8266id: Some("11545355".into()),
            software_version: Some("NRZ-2020-133".into()),
        };

        let result = push_handler(
            axum::extract::State(db.clone()),
            Json(payload),
        ).await;

        assert!(result.is_ok());
        let resp = result.unwrap().0;
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.sensor_id, 11545355);

        // Verify data in DB
        assert_eq!(db.sensor_count().unwrap(), 1);
        let readings = db.query_readings(11545355, 0, i64::MAX).unwrap();
        assert_eq!(readings.len(), 1);
        assert!((readings[0].pm25.unwrap() - 3.83).abs() < 0.01);
        assert!((readings[0].pm10.unwrap() - 6.35).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_push_handler_no_id_rejected() {
        let db = Db::open_memory().unwrap();
        let db = Arc::new(db);

        let payload = PushPayload {
            sensordatavalues: vec![
                SensorDataValue { value_type: "SDS_P1".into(), value: "6.35".into() },
            ],
            esp8266id: None, // no ID
            software_version: None,
        };

        let result = push_handler(
            axum::extract::State(db.clone()),
            Json(payload),
        ).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_push_handler_multiple_pushes() {
        let db = Db::open_memory().unwrap();
        let db = Arc::new(db);

        // Push twice from same sensor
        for pm in &["3.83", "5.50"] {
            let payload = PushPayload {
                sensordatavalues: vec![
                    SensorDataValue { value_type: "SDS_P2".into(), value: pm.to_string() },
                ],
                esp8266id: Some("11545355".into()),
                software_version: None,
            };
            push_handler(axum::extract::State(db.clone()), Json(payload)).await.unwrap();
        }

        // Same sensor, but readings at different timestamps
        assert_eq!(db.sensor_count().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_push_handler_sensor_marked_local() {
        let db = Db::open_memory().unwrap();
        let db = Arc::new(db);

        let payload = PushPayload {
            sensordatavalues: vec![
                SensorDataValue { value_type: "SDS_P2".into(), value: "5.0".into() },
            ],
            esp8266id: Some("99999".into()),
            software_version: None,
        };
        push_handler(axum::extract::State(db.clone()), Json(payload)).await.unwrap();

        let sensors = db.all_sensors().unwrap();
        assert_eq!(sensors.len(), 1);
        assert_eq!(sensors[0].source.as_deref(), Some("local")); // marked as local push
    }
}
