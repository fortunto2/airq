//! Source merging: combine model (Open-Meteo) + sensors (Sensor.Community).
//!
//! Rule: sensors are ground truth. Model is fallback.
//! When both available: dynamic weight based on divergence.

use serde::Serialize;

/// Merged AQ reading from multiple sources.
#[derive(Debug, Clone, Serialize)]
pub struct MergedReading {
    /// Final PM2.5 after merge.
    pub pm25: f64,
    /// Final PM10 after merge.
    pub pm10: f64,
    /// Forecast value (Open-Meteo CAMS model, not real measurement).
    pub model_pm25: Option<f64>,
    pub model_pm10: Option<f64>,
    /// Sensor median (Sensor.Community, real measurements).
    pub sensor_pm25: Option<f64>,
    pub sensor_pm10: Option<f64>,
    /// Number of sensors used.
    pub sensor_count: u32,
    /// Model weight used (0.0-1.0). Lower = less trusted.
    pub model_weight: f64,
    /// Divergence ratio: model/sensor. >2 = model unreliable.
    pub divergence: f64,
    /// Source description.
    pub source: &'static str,
}

/// Merge model + sensor data with dynamic weighting.
///
/// Logic:
/// - Sensors only → use sensors (weight 1.0)
/// - Model only → use model (weight 0.5, lower confidence)
/// - Both → weighted average, weight depends on divergence
///
/// Divergence = |model - sensor| / sensor.
/// Low divergence (<0.5) → model gets 0.3 weight (they agree, sensors still primary)
/// High divergence (>2.0) → model gets 0.0 (model is wrong, ignore)
pub fn merge(
    model_pm25: Option<f64>,
    model_pm10: Option<f64>,
    sensor_pm25: Option<f64>,
    sensor_pm10: Option<f64>,
    sensor_count: u32,
) -> MergedReading {
    // Both channels
    let (pm25, pm10, model_weight, divergence, source) = match (sensor_pm25, model_pm25) {
        // Sensors available → primary source
        (Some(sp25), Some(mp25)) => {
            let sp10 = sensor_pm10.unwrap_or(sp25 * 1.5);
            let mp10 = model_pm10.unwrap_or(mp25 * 1.5);

            // Divergence: how much model differs from sensors
            let div = if sp25 > 1.0 { (mp25 / sp25).max(sp25 / mp25) } else { 1.0 };

            // Dynamic model weight: high divergence → low weight
            let mw = model_weight_from_divergence(div, sensor_count);

            let pm25 = sp25 * (1.0 - mw) + mp25 * mw;
            let pm10 = sp10 * (1.0 - mw) + mp10 * mw;

            (pm25, pm10, mw, div, "sensors+model")
        }
        // Sensors only
        (Some(sp25), None) => {
            let sp10 = sensor_pm10.unwrap_or(sp25 * 1.5);
            (sp25, sp10, 0.0, 0.0, "sensors")
        }
        // Model only (no sensors)
        (None, Some(mp25)) => {
            let mp10 = model_pm10.unwrap_or(mp25 * 1.5);
            // Model alone gets 0.5 weight (less confident)
            (mp25, mp10, 1.0, 0.0, "model-only")
        }
        // No data
        (None, None) => {
            (0.0, 0.0, 0.0, 0.0, "no-data")
        }
    };

    MergedReading {
        pm25,
        pm10,
        model_pm25,
        model_pm10,
        sensor_pm25,
        sensor_pm10,
        sensor_count,
        model_weight,
        divergence,
        source,
    }
}

/// Model weight based on divergence ratio and sensor count.
///
/// More sensors + low divergence → model gets some weight (smoothing).
/// Fewer sensors + high divergence → model gets zero (unreliable).
fn model_weight_from_divergence(divergence: f64, sensor_count: u32) -> f64 {
    // Base weight from divergence (sigmoid decay)
    // div=1.0 (agree) → 0.3, div=2.0 → 0.1, div=5.0 → ~0
    let div_weight = 0.3 / (1.0 + ((divergence - 1.0) * 2.0).exp().max(0.0));

    // Sensor count bonus: more sensors → trust sensors more → less model weight
    let sensor_discount = match sensor_count {
        0 => 1.0,      // no sensors, full model weight
        1..=2 => 0.8,  // few sensors, keep some model
        3..=5 => 0.5,  // moderate, halve model
        6..=15 => 0.3, // good coverage
        _ => 0.1,      // dense network, almost ignore model
    };

    (div_weight * sensor_discount).min(0.3)
}

// ---------------------------------------------------------------------------
// Convenience: single-source (partial merge)
// ---------------------------------------------------------------------------

/// Sensor-only reading (no model).
pub fn from_sensors(pm25: f64, pm10: f64, sensor_count: u32) -> MergedReading {
    merge(None, None, Some(pm25), Some(pm10), sensor_count)
}

/// Model-only reading (no sensors).
pub fn from_model(pm25: f64, pm10: f64) -> MergedReading {
    merge(Some(pm25), Some(pm10), None, None, 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensors_only() {
        let r = merge(None, None, Some(10.0), Some(15.0), 5);
        assert_eq!(r.source, "sensors");
        assert!((r.pm25 - 10.0).abs() < 0.01);
        assert!((r.pm10 - 15.0).abs() < 0.01);
        assert_eq!(r.model_weight, 0.0);
    }

    #[test]
    fn test_model_only() {
        let r = merge(Some(50.0), Some(70.0), None, None, 0);
        assert_eq!(r.source, "model-only");
        assert!((r.pm25 - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_both_agree() {
        // Model=12, Sensor=10 → divergence ~1.2, sensors dominate
        let r = merge(Some(12.0), Some(18.0), Some(10.0), Some(15.0), 10);
        assert_eq!(r.source, "sensors+model");
        assert!(r.model_weight < 0.15, "mw: {}", r.model_weight);
        // Result should be close to sensor values
        assert!((r.pm25 - 10.0).abs() < 1.0, "pm25: {}", r.pm25);
    }

    #[test]
    fn test_moscow_divergence() {
        // Moscow case: model=130, sensors=6.7 → divergence ~19x
        let r = merge(Some(130.0), Some(160.0), Some(6.7), Some(10.0), 10);
        assert!(r.divergence > 10.0, "div: {}", r.divergence);
        assert!(r.model_weight < 0.05, "mw: {}", r.model_weight);
        // Result should be very close to sensor values
        assert!(r.pm25 < 15.0, "pm25: {} (should be ~7, not 130)", r.pm25);
    }

    #[test]
    fn test_few_sensors_model_gets_more() {
        // Only 1 sensor, model agrees roughly
        let r1 = merge(Some(12.0), None, Some(10.0), None, 1);
        // 10 sensors, same data
        let r10 = merge(Some(12.0), None, Some(10.0), None, 10);
        // With 1 sensor, model should get more weight
        assert!(r1.model_weight > r10.model_weight);
    }

    #[test]
    fn test_no_data() {
        let r = merge(None, None, None, None, 0);
        assert_eq!(r.source, "no-data");
        assert_eq!(r.pm25, 0.0);
    }
}
