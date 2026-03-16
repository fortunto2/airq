//! Event detection: distinguish real pollution events from sensor noise.
//!
//! Key insight: statistical outlier on 1 sensor = noise.
//! Outlier on 2+ sensors in same wind sector = event (fire, emission).
//!
//! Three layers:
//! 1. EWMA baseline per sensor — adaptive threshold
//! 2. Concordance — how many sensors confirm the anomaly
//! 3. Directional clustering — are anomaly sensors in one wind sector

use serde::Serialize;

// ---------------------------------------------------------------------------
// EWMA (Exponential Weighted Moving Average) baseline
// ---------------------------------------------------------------------------

/// Per-sensor adaptive baseline. Tracks "normal" level.
#[derive(Debug, Clone)]
pub struct EwmaBaseline {
    /// Smoothing factor (0.0-1.0). Lower = smoother. Default 0.1.
    alpha: f64,
    /// Current baseline estimate.
    pub baseline: f64,
    /// Rolling variance (for dynamic threshold).
    variance: f64,
    /// Number of observations.
    count: u64,
}

impl EwmaBaseline {
    pub fn new(alpha: f64) -> Self {
        Self { alpha, baseline: 0.0, variance: 0.0, count: 0 }
    }

    /// Default α=0.1 (slow adaptation, good for hourly data).
    pub fn default_hourly() -> Self {
        Self::new(0.1)
    }

    /// Create with pre-set baseline and variance (for bootstrapping from known median).
    pub fn with_baseline(baseline: f64, variance: f64) -> Self {
        Self { alpha: 0.1, baseline, variance, count: 100 }
    }

    /// Feed a new observation. Returns true if anomaly (> baseline + k*σ).
    pub fn update(&mut self, value: f64) -> bool {
        if self.count == 0 {
            self.baseline = value;
            self.variance = 0.0;
            self.count = 1;
            return false;
        }

        let diff = value - self.baseline;
        self.baseline += self.alpha * diff;
        // Welford-style running variance with EWMA
        self.variance = (1.0 - self.alpha) * (self.variance + self.alpha * diff * diff);
        self.count += 1;
        false // anomaly check is separate — use is_anomaly()
    }

    /// Current standard deviation. Minimum 1.0 to prevent zero-variance traps.
    pub fn std_dev(&self) -> f64 {
        self.variance.sqrt().max(1.0)
    }

    /// Check if value is anomalous (above baseline + k * σ).
    /// k=3.0 is standard. k=2.0 is more sensitive.
    pub fn is_anomaly(&self, value: f64, k: f64) -> bool {
        if self.count < 5 {
            return false; // not enough data
        }
        let threshold = self.baseline + k * self.std_dev();
        value > threshold
    }

    /// How many σ above baseline. Negative = below baseline.
    pub fn z_score(&self, value: f64) -> f64 {
        let sd = self.std_dev();
        if sd < f64::EPSILON { 0.0 } else { (value - self.baseline) / sd }
    }
}

// ---------------------------------------------------------------------------
// Sensor reading with location
// ---------------------------------------------------------------------------

/// A single sensor observation.
#[derive(Debug, Clone)]
pub struct SensorReading {
    pub sensor_id: u64,
    pub lat: f64,
    pub lon: f64,
    pub pm25: f64,
}

// ---------------------------------------------------------------------------
// Concordance: how many sensors confirm an anomaly
// ---------------------------------------------------------------------------

/// Result of concordance analysis.
#[derive(Debug, Clone, Serialize)]
pub struct ConcordanceResult {
    /// Fraction of sensors showing anomaly (0.0-1.0).
    pub concordance: f64,
    /// Total sensors analyzed.
    pub total_sensors: usize,
    /// Number of anomalous sensors.
    pub anomaly_count: usize,
    /// Classification.
    pub event_type: EventType,
    /// Anomalous sensor IDs.
    pub anomaly_sensor_ids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum EventType {
    /// No significant anomaly.
    Normal,
    /// Single sensor spike — likely noise.
    Noise,
    /// 2+ sensors confirm — real event.
    Event,
    /// >50% sensors affected — widespread pollution.
    Widespread,
}

/// Compute concordance from a set of sensor readings + their EWMA baselines.
///
/// `baselines` maps sensor_id → EwmaBaseline.
/// `k` is the anomaly threshold in standard deviations (default 3.0).
pub fn concordance(
    readings: &[SensorReading],
    baselines: &std::collections::HashMap<u64, EwmaBaseline>,
    k: f64,
) -> ConcordanceResult {
    if readings.is_empty() {
        return ConcordanceResult {
            concordance: 0.0,
            total_sensors: 0,
            anomaly_count: 0,
            event_type: EventType::Normal,
            anomaly_sensor_ids: vec![],
        };
    }

    let mut anomaly_ids = Vec::new();

    for r in readings {
        if let Some(bl) = baselines.get(&r.sensor_id) {
            if bl.is_anomaly(r.pm25, k) {
                anomaly_ids.push(r.sensor_id);
            }
        }
    }

    let total = readings.len();
    let anomaly_count = anomaly_ids.len();
    let concordance = anomaly_count as f64 / total as f64;

    let event_type = match anomaly_count {
        0 => EventType::Normal,
        1 => EventType::Noise,
        n if (n as f64 / total as f64) > 0.5 => EventType::Widespread,
        _ => EventType::Event,
    };

    ConcordanceResult {
        concordance,
        total_sensors: total,
        anomaly_count,
        event_type,
        anomaly_sensor_ids: anomaly_ids,
    }
}

// ---------------------------------------------------------------------------
// Directional clustering: are anomaly sensors in one wind sector?
// ---------------------------------------------------------------------------

/// Directional analysis of anomaly sensors.
#[derive(Debug, Clone, Serialize)]
pub struct DirectionalResult {
    /// Dominant bearing from city center to anomaly cluster (degrees, 0=N).
    pub bearing_deg: f64,
    /// Compass label ("NE", "SW", etc.).
    pub bearing_label: String,
    /// Angular spread of anomaly sensors (degrees). <90° = tight cluster.
    pub spread_deg: f64,
    /// True if anomaly sensors cluster in one 90° sector.
    pub is_directional: bool,
    /// Number of anomaly sensors in the dominant sector.
    pub sensors_in_sector: usize,
}

/// Analyze whether anomaly sensors cluster directionally relative to a center point.
///
/// `center_lat/lon` — city center or target point.
/// `anomaly_readings` — only the anomalous sensors.
pub fn directional_cluster(
    center_lat: f64,
    center_lon: f64,
    anomaly_readings: &[SensorReading],
) -> Option<DirectionalResult> {
    if anomaly_readings.len() < 2 {
        return None;
    }

    // Compute bearing from center to each anomaly sensor
    let bearings: Vec<f64> = anomaly_readings
        .iter()
        .map(|r| super::front::bearing(center_lat, center_lon, r.lat, r.lon))
        .collect();

    // Circular mean bearing
    let (sin_sum, cos_sum): (f64, f64) = bearings
        .iter()
        .map(|b| (b.to_radians().sin(), b.to_radians().cos()))
        .fold((0.0, 0.0), |(s, c), (si, ci)| (s + si, c + ci));
    let mean_bearing = sin_sum.atan2(cos_sum).to_degrees().rem_euclid(360.0);

    // Angular spread: max angular distance between any two bearings
    let mut max_spread = 0.0_f64;
    for i in 0..bearings.len() {
        for j in (i + 1)..bearings.len() {
            let diff = (bearings[i] - bearings[j]).abs();
            let angular = diff.min(360.0 - diff);
            if angular > max_spread {
                max_spread = angular;
            }
        }
    }

    // Count sensors within ±45° of mean bearing (90° sector)
    let sensors_in_sector = bearings
        .iter()
        .filter(|&&b| {
            let diff = (b - mean_bearing).abs();
            let angular = diff.min(360.0 - diff);
            angular <= 45.0
        })
        .count();

    let is_directional = max_spread < 90.0 && anomaly_readings.len() >= 2;

    Some(DirectionalResult {
        bearing_deg: mean_bearing,
        bearing_label: super::front::bearing_label(mean_bearing).to_string(),
        spread_deg: max_spread,
        is_directional,
        sensors_in_sector,
    })
}

// ---------------------------------------------------------------------------
// Full event analysis: EWMA + concordance + directional
// ---------------------------------------------------------------------------

/// Complete event detection result.
#[derive(Debug, Clone, Serialize)]
pub struct EventAnalysis {
    pub concordance: ConcordanceResult,
    pub directional: Option<DirectionalResult>,
    /// Median PM2.5 (all sensors).
    pub median_pm25: f64,
    /// Median PM2.5 (anomaly sensors only).
    pub anomaly_median_pm25: Option<f64>,
    /// Confidence: 0.0-1.0. Higher when concordance + directional agree.
    pub confidence: f64,
    /// Human-readable summary.
    pub summary: String,
}

/// Run full event detection.
///
/// `center_lat/lon` — city center.
/// `readings` — all current sensor readings.
/// `baselines` — per-sensor EWMA baselines (updated externally).
/// `k` — anomaly threshold in σ (default 3.0).
pub fn detect_event(
    center_lat: f64,
    center_lon: f64,
    readings: &[SensorReading],
    baselines: &std::collections::HashMap<u64, EwmaBaseline>,
    k: f64,
) -> EventAnalysis {
    let conc = concordance(readings, baselines, k);

    // Extract anomaly readings
    let anomaly_readings: Vec<SensorReading> = readings
        .iter()
        .filter(|r| conc.anomaly_sensor_ids.contains(&r.sensor_id))
        .cloned()
        .collect();

    let dir = directional_cluster(center_lat, center_lon, &anomaly_readings);

    // Median PM2.5 (all sensors)
    let mut all_pm25: Vec<f64> = readings.iter().map(|r| r.pm25).collect();
    all_pm25.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_pm25 = if all_pm25.is_empty() {
        0.0
    } else {
        all_pm25[all_pm25.len() / 2]
    };

    // Median PM2.5 (anomaly sensors)
    let anomaly_median = if anomaly_readings.is_empty() {
        None
    } else {
        let mut apm: Vec<f64> = anomaly_readings.iter().map(|r| r.pm25).collect();
        apm.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(apm[apm.len() / 2])
    };

    // Confidence scoring
    let conc_score: f64 = match conc.event_type {
        EventType::Normal => 0.0,
        EventType::Noise => 0.1,
        EventType::Event => 0.6,
        EventType::Widespread => 0.8,
    };
    let dir_bonus: f64 = match &dir {
        Some(d) if d.is_directional => 0.3,
        Some(_) => 0.1,
        None => 0.0,
    };
    let confidence = (conc_score + dir_bonus).min(1.0);

    // Summary
    let summary = match (&conc.event_type, &dir) {
        (EventType::Normal, _) => "Normal conditions".to_string(),
        (EventType::Noise, _) => format!(
            "Single sensor spike (sensor #{}), likely noise",
            conc.anomaly_sensor_ids.first().unwrap_or(&0)
        ),
        (EventType::Event, Some(d)) if d.is_directional => format!(
            "Event: {} sensors confirm from {} (spread {}°, confidence {:.0}%)",
            conc.anomaly_count, d.bearing_label, d.spread_deg.round(), confidence * 100.0
        ),
        (EventType::Event, _) => format!(
            "Event: {} of {} sensors anomalous (scattered, confidence {:.0}%)",
            conc.anomaly_count, conc.total_sensors, confidence * 100.0
        ),
        (EventType::Widespread, _) => format!(
            "Widespread: {}/{} sensors above threshold (confidence {:.0}%)",
            conc.anomaly_count, conc.total_sensors, confidence * 100.0
        ),
    };

    EventAnalysis {
        concordance: conc,
        directional: dir,
        median_pm25,
        anomaly_median_pm25: anomaly_median,
        confidence,
        summary,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_baseline(baseline: f64, variance: f64) -> EwmaBaseline {
        EwmaBaseline {
            alpha: 0.1,
            baseline,
            variance,
            count: 100, // pretend we have enough data
        }
    }

    fn sensor(id: u64, lat: f64, lon: f64, pm25: f64) -> SensorReading {
        SensorReading { sensor_id: id, lat, lon, pm25 }
    }

    #[test]
    fn test_ewma_basic() {
        let mut bl = EwmaBaseline::default_hourly();
        // Feed 20 values around 10
        for _ in 0..20 {
            bl.update(10.0);
        }
        assert!((bl.baseline - 10.0).abs() < 0.5);
        assert!(!bl.is_anomaly(12.0, 3.0));
        // Spike
        assert!(!bl.is_anomaly(10.0, 3.0)); // normal
    }

    #[test]
    fn test_ewma_detects_spike() {
        let mut bl = EwmaBaseline::new(0.1);
        // Build stable baseline around 10 with some variance
        for i in 0..50 {
            bl.update(10.0 + (i % 3) as f64); // 10, 11, 12, 10, 11, 12...
        }
        // Big spike should be anomaly
        assert!(bl.is_anomaly(50.0, 3.0), "50 should be anomaly with baseline ~11");
        // Small variation should not
        assert!(!bl.is_anomaly(13.0, 3.0), "13 should not be anomaly");
    }

    #[test]
    fn test_ewma_z_score() {
        let bl = make_baseline(10.0, 4.0); // σ = 2
        assert!((bl.z_score(10.0)).abs() < 0.01); // at baseline
        assert!((bl.z_score(14.0) - 2.0).abs() < 0.01); // 2σ above
        assert!((bl.z_score(6.0) - (-2.0)).abs() < 0.01); // 2σ below
    }

    #[test]
    fn test_concordance_normal() {
        let readings = vec![
            sensor(1, 55.75, 37.60, 12.0),
            sensor(2, 55.76, 37.61, 11.0),
            sensor(3, 55.77, 37.62, 13.0),
        ];
        let baselines: HashMap<u64, EwmaBaseline> = vec![
            (1, make_baseline(10.0, 4.0)),
            (2, make_baseline(10.0, 4.0)),
            (3, make_baseline(10.0, 4.0)),
        ].into_iter().collect();

        let result = concordance(&readings, &baselines, 3.0);
        assert_eq!(result.event_type, EventType::Normal);
        assert_eq!(result.anomaly_count, 0);
    }

    #[test]
    fn test_concordance_single_noise() {
        let readings = vec![
            sensor(1, 55.75, 37.60, 50.0), // spike!
            sensor(2, 55.76, 37.61, 11.0),
            sensor(3, 55.77, 37.62, 10.0),
            sensor(4, 55.78, 37.63, 12.0),
        ];
        let baselines: HashMap<u64, EwmaBaseline> = vec![
            (1, make_baseline(10.0, 4.0)),
            (2, make_baseline(10.0, 4.0)),
            (3, make_baseline(10.0, 4.0)),
            (4, make_baseline(10.0, 4.0)),
        ].into_iter().collect();

        let result = concordance(&readings, &baselines, 3.0);
        assert_eq!(result.event_type, EventType::Noise);
        assert_eq!(result.anomaly_count, 1);
    }

    #[test]
    fn test_concordance_event() {
        let readings = vec![
            sensor(1, 55.75, 37.60, 50.0),
            sensor(2, 55.76, 37.61, 45.0), // 2 sensors spiked
            sensor(3, 55.77, 37.62, 10.0),
            sensor(4, 55.78, 37.63, 12.0),
        ];
        let baselines: HashMap<u64, EwmaBaseline> = vec![
            (1, make_baseline(10.0, 4.0)),
            (2, make_baseline(10.0, 4.0)),
            (3, make_baseline(10.0, 4.0)),
            (4, make_baseline(10.0, 4.0)),
        ].into_iter().collect();

        let result = concordance(&readings, &baselines, 3.0);
        assert_eq!(result.event_type, EventType::Event);
        assert_eq!(result.anomaly_count, 2);
        assert!(result.concordance > 0.0);
    }

    #[test]
    fn test_concordance_widespread() {
        let readings = vec![
            sensor(1, 55.75, 37.60, 50.0),
            sensor(2, 55.76, 37.61, 45.0),
            sensor(3, 55.77, 37.62, 48.0), // 3 of 4 = 75%
            sensor(4, 55.78, 37.63, 12.0),
        ];
        let baselines: HashMap<u64, EwmaBaseline> = vec![
            (1, make_baseline(10.0, 4.0)),
            (2, make_baseline(10.0, 4.0)),
            (3, make_baseline(10.0, 4.0)),
            (4, make_baseline(10.0, 4.0)),
        ].into_iter().collect();

        let result = concordance(&readings, &baselines, 3.0);
        assert_eq!(result.event_type, EventType::Widespread);
    }

    #[test]
    fn test_directional_cluster_tight() {
        // Two sensors both NE of center
        let readings = vec![
            sensor(1, 55.80, 37.65, 50.0), // NE
            sensor(2, 55.82, 37.68, 45.0), // NE
        ];
        let result = directional_cluster(55.75, 37.60, &readings).unwrap();
        assert!(result.is_directional, "sensors in same sector should be directional");
        assert!(result.spread_deg < 90.0, "spread: {}", result.spread_deg);
    }

    #[test]
    fn test_directional_cluster_scattered() {
        // Sensors in opposite directions
        let readings = vec![
            sensor(1, 55.85, 37.60, 50.0), // N
            sensor(2, 55.65, 37.60, 45.0), // S
        ];
        let result = directional_cluster(55.75, 37.60, &readings).unwrap();
        assert!(!result.is_directional, "opposite sensors should NOT be directional");
        assert!(result.spread_deg > 90.0);
    }

    #[test]
    fn test_directional_needs_2_sensors() {
        let readings = vec![sensor(1, 55.80, 37.65, 50.0)];
        assert!(directional_cluster(55.75, 37.60, &readings).is_none());
    }

    #[test]
    fn test_full_event_detection() {
        let center = (55.75, 37.60); // Moscow
        let readings = vec![
            sensor(1, 55.80, 37.65, 50.0), // NE, spike
            sensor(2, 55.82, 37.68, 45.0), // NE, spike
            sensor(3, 55.70, 37.55, 10.0), // SW, normal
            sensor(4, 55.72, 37.58, 11.0), // SW, normal
        ];
        let baselines: HashMap<u64, EwmaBaseline> = readings.iter()
            .map(|r| (r.sensor_id, make_baseline(10.0, 4.0)))
            .collect();

        let result = detect_event(center.0, center.1, &readings, &baselines, 3.0);
        assert_eq!(result.concordance.event_type, EventType::Event);
        assert!(result.directional.as_ref().unwrap().is_directional);
        assert!(result.confidence > 0.7, "confidence: {}", result.confidence);
        assert!(result.summary.contains("confirm from"));
    }

    #[test]
    fn test_full_event_normal() {
        let readings = vec![
            sensor(1, 55.75, 37.60, 10.0),
            sensor(2, 55.76, 37.61, 11.0),
        ];
        let baselines: HashMap<u64, EwmaBaseline> = readings.iter()
            .map(|r| (r.sensor_id, make_baseline(10.0, 4.0)))
            .collect();

        let result = detect_event(55.75, 37.60, &readings, &baselines, 3.0);
        assert_eq!(result.concordance.event_type, EventType::Normal);
        assert!(result.confidence < 0.1);
    }
}
