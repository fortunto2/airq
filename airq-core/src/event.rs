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

/// A single sensor observation (PM2.5 + PM10).
#[derive(Debug, Clone)]
pub struct SensorReading {
    pub sensor_id: u64,
    pub lat: f64,
    pub lon: f64,
    pub pm25: f64,
    pub pm10: f64,
}

/// Per-sensor dual-channel baseline (PM2.5 + PM10).
#[derive(Debug, Clone)]
pub struct DualBaseline {
    pub pm25: EwmaBaseline,
    pub pm10: EwmaBaseline,
}

impl DualBaseline {
    pub fn new() -> Self {
        Self { pm25: EwmaBaseline::default_hourly(), pm10: EwmaBaseline::default_hourly() }
    }

    pub fn with_baselines(pm25_baseline: f64, pm25_var: f64, pm10_baseline: f64, pm10_var: f64) -> Self {
        Self {
            pm25: EwmaBaseline::with_baseline(pm25_baseline, pm25_var),
            pm10: EwmaBaseline::with_baseline(pm10_baseline, pm10_var),
        }
    }

    /// True if EITHER channel is anomalous.
    pub fn is_anomaly(&self, reading: &SensorReading, k: f64) -> bool {
        self.pm25.is_anomaly(reading.pm25, k) || self.pm10.is_anomaly(reading.pm10, k)
    }

    /// Max z-score across both channels.
    pub fn max_z(&self, reading: &SensorReading) -> f64 {
        self.pm25.z_score(reading.pm25).max(self.pm10.z_score(reading.pm10))
    }

    /// Which channel triggered ("pm25", "pm10", "both", or "none").
    pub fn trigger_channel(&self, reading: &SensorReading, k: f64) -> &'static str {
        let p25 = self.pm25.is_anomaly(reading.pm25, k);
        let p10 = self.pm10.is_anomaly(reading.pm10, k);
        match (p25, p10) {
            (true, true) => "both",
            (true, false) => "pm25",
            (false, true) => "pm10",
            (false, false) => "none",
        }
    }
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

/// Compute concordance from sensor readings + dual-channel baselines.
///
/// Anomaly = EITHER PM2.5 or PM10 exceeds baseline + kσ.
/// `baselines` maps sensor_id → DualBaseline.
pub fn concordance(
    readings: &[SensorReading],
    baselines: &std::collections::HashMap<u64, DualBaseline>,
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
            if bl.is_anomaly(r, k) {
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

// ---------------------------------------------------------------------------
// Source classification: PM10/PM2.5 ratio + absolute levels + patterns
// ---------------------------------------------------------------------------

/// Pollution source classification.
#[derive(Debug, Clone, Serialize)]
pub struct SourceClassification {
    /// Primary category.
    pub category: SourceCategory,
    /// Human-readable label.
    pub label: &'static str,
    /// Confidence in classification (0.0-1.0).
    pub confidence: f64,
    /// Explanation.
    pub reason: String,
    /// Typical sources for this category.
    pub typical_sources: &'static [&'static str],
    /// Health advice.
    pub advice: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum SourceCategory {
    /// PM10/PM2.5 > 4: Saharan dust, sand storm, volcanic ash
    DustStorm,
    /// PM10/PM2.5 2.5-4: construction sites, unpaved roads, demolition
    ConstructionDust,
    /// PM10/PM2.5 1.5-2.5: multiple sources, urban mix
    MixedUrban,
    /// PM10/PM2.5 0.8-1.5, PM2.5 > 35: vehicle exhaust, heating, industrial combustion
    Combustion,
    /// PM10/PM2.5 < 0.8 or PM2.5 > 55 with low PM10 ratio: wildfire smoke, industrial fumes
    Smoke,
    /// PM2.5 < 12: background/clean air
    Clean,
    /// PM2.5 12-35: slightly elevated, typical urban background
    UrbanBackground,
}

/// Classify pollution source from PM2.5 + PM10 levels.
///
/// Based on EPA, WHO, and academic research on PM ratio fingerprints:
/// - Chow et al. (1996): PM ratios by source type
/// - Querol et al. (2004): Saharan dust episodes PM10/PM2.5 > 4
/// - Putaud et al. (2010): European urban PM composition
pub fn classify_source(ratio: f64, pm25: f64, pm10: f64) -> SourceClassification {
    // Clean air — no classification needed
    if pm25 < 12.0 && pm10 < 25.0 {
        return SourceClassification {
            category: SourceCategory::Clean,
            label: "clean air",
            confidence: 0.9,
            reason: format!("PM2.5={:.1} PM10={:.1} — within WHO guidelines", pm25, pm10),
            typical_sources: &["natural background", "ocean air", "forest"],
            advice: "Air quality is good. Enjoy outdoor activities.",
        };
    }

    // Urban background — slightly elevated but no clear source
    if pm25 < 35.0 && ratio > 0.8 && ratio < 2.5 {
        return SourceClassification {
            category: SourceCategory::UrbanBackground,
            label: "urban background",
            confidence: 0.6,
            reason: format!("PM2.5={:.1}, ratio={:.1} — typical urban levels", pm25, ratio),
            typical_sources: &["traffic", "heating", "cooking", "general urban activity"],
            advice: "Normal urban air. Sensitive groups may want to limit prolonged outdoor exertion.",
        };
    }

    // Dust storm / Saharan dust
    if ratio > 4.0 {
        let conf = (ratio / 6.0).min(1.0); // higher ratio = more confident
        return SourceClassification {
            category: SourceCategory::DustStorm,
            label: "dust/sand storm",
            confidence: conf,
            reason: format!("PM10/PM2.5={:.1} — coarse particles dominate (PM10={:.0})", ratio, pm10),
            typical_sources: &["desert dust (Saharan/Taklamakan)", "sand storm", "volcanic ash", "dry lake bed"],
            advice: "Wear N95 mask outdoors. Close windows. Rinse eyes if irritated.",
        };
    }

    // Construction / road dust
    if ratio > 2.5 {
        return SourceClassification {
            category: SourceCategory::ConstructionDust,
            label: "construction/road dust",
            confidence: 0.6,
            reason: format!("PM10/PM2.5={:.1} — elevated coarse fraction", ratio),
            typical_sources: &["construction site", "unpaved road", "demolition", "quarry", "agricultural tilling"],
            advice: "Coarse dust — standard mask helps. Keep windows closed nearby.",
        };
    }

    // Smoke / fine particles (wildfire, industrial)
    if ratio < 0.9 || (pm25 > 55.0 && ratio < 1.5) {
        let conf = if pm25 > 100.0 { 0.9 } else if pm25 > 55.0 { 0.7 } else { 0.5 };
        return SourceClassification {
            category: SourceCategory::Smoke,
            label: "smoke/fine particles",
            confidence: conf,
            reason: format!("PM2.5={:.1} high, ratio={:.1} — fine particles dominate", pm25, ratio),
            typical_sources: &["wildfire smoke", "agricultural burning", "industrial fumes", "incinerator"],
            advice: "Fine particles penetrate deep into lungs. Use N95/FFP2 mask. Run air purifier indoors.",
        };
    }

    // Combustion (traffic, heating)
    if ratio >= 0.9 && ratio <= 1.8 && pm25 >= 35.0 {
        return SourceClassification {
            category: SourceCategory::Combustion,
            label: "combustion (traffic/heating)",
            confidence: 0.65,
            reason: format!("PM2.5={:.1}, ratio={:.1} — combustion signature", pm25, ratio),
            typical_sources: &["diesel exhaust", "gasoline vehicles", "coal/gas heating", "power plant"],
            advice: "Avoid busy roads. Use air purifier. Limit outdoor exercise during rush hours.",
        };
    }

    // Mixed urban — fallback
    SourceClassification {
        category: SourceCategory::MixedUrban,
        label: "mixed sources",
        confidence: 0.4,
        reason: format!("PM2.5={:.1}, ratio={:.1} — no clear single source", pm25, ratio),
        typical_sources: &["urban mix", "traffic + heating + industry", "resuspended road dust"],
        advice: "Moderate air quality. Sensitive groups should reduce prolonged outdoor exertion.",
    }
}

/// Complete event detection result.
#[derive(Debug, Clone, Serialize)]
pub struct EventAnalysis {
    pub concordance: ConcordanceResult,
    pub directional: Option<DirectionalResult>,
    /// Median PM2.5 (all sensors).
    pub median_pm25: f64,
    /// Median PM10 (all sensors).
    pub median_pm10: f64,
    /// Median PM2.5 (anomaly sensors only).
    pub anomaly_median_pm25: Option<f64>,
    /// Median PM10 (anomaly sensors only).
    pub anomaly_median_pm10: Option<f64>,
    /// PM10/PM2.5 ratio — high (>3) suggests dust, low (~1) suggests combustion.
    pub pm10_pm25_ratio: f64,
    /// Source classification based on PM ratio + absolute levels.
    pub source_hint: SourceClassification,
    /// Confidence: 0.0-1.0.
    pub confidence: f64,
    /// Human-readable summary.
    pub summary: String,
}

/// Run full event detection (dual-channel: PM2.5 + PM10).
///
/// `center_lat/lon` — city center.
/// `readings` — all current sensor readings.
/// `baselines` — per-sensor dual baselines.
/// `k` — anomaly threshold in σ (default 3.0).
pub fn detect_event(
    center_lat: f64,
    center_lon: f64,
    readings: &[SensorReading],
    baselines: &std::collections::HashMap<u64, DualBaseline>,
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

    // Median PM10 (all sensors)
    let mut all_pm10: Vec<f64> = readings.iter().map(|r| r.pm10).collect();
    all_pm10.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_pm10 = if all_pm10.is_empty() { 0.0 } else { all_pm10[all_pm10.len() / 2] };

    // Anomaly medians
    let (anomaly_median_pm25, anomaly_median_pm10) = if anomaly_readings.is_empty() {
        (None, None)
    } else {
        let mut apm25: Vec<f64> = anomaly_readings.iter().map(|r| r.pm25).collect();
        let mut apm10: Vec<f64> = anomaly_readings.iter().map(|r| r.pm10).collect();
        apm25.sort_by(|a, b| a.partial_cmp(b).unwrap());
        apm10.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (Some(apm25[apm25.len() / 2]), Some(apm10[apm10.len() / 2]))
    };

    // PM10/PM2.5 ratio → source classification
    let pm10_pm25_ratio = if median_pm25 > 1.0 { median_pm10 / median_pm25 } else { 1.0 };
    let source_hint = classify_source(pm10_pm25_ratio, median_pm25, median_pm10);

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
        median_pm10,
        anomaly_median_pm25,
        anomaly_median_pm10,
        pm10_pm25_ratio,
        source_hint,
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
        SensorReading { sensor_id: id, lat, lon, pm25, pm10: pm25 * 1.5 }
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
        let baselines: HashMap<u64, DualBaseline> = vec![
            (1, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (2, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (3, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
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
        let baselines: HashMap<u64, DualBaseline> = vec![
            (1, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (2, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (3, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (4, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
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
        let baselines: HashMap<u64, DualBaseline> = vec![
            (1, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (2, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (3, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (4, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
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
        let baselines: HashMap<u64, DualBaseline> = vec![
            (1, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (2, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (3, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
            (4, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)),
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
        let baselines: HashMap<u64, DualBaseline> = readings.iter()
            .map(|r| (r.sensor_id, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)))
            .collect();

        let result = detect_event(center.0, center.1, &readings, &baselines, 3.0);
        assert_eq!(result.concordance.event_type, EventType::Event);
        assert!(result.directional.as_ref().unwrap().is_directional);
        assert!(result.confidence > 0.7, "confidence: {}", result.confidence);
        assert!(result.summary.contains("confirm from"));
        assert!(result.pm10_pm25_ratio > 0.0);
        assert_eq!(result.source_hint.category, SourceCategory::Combustion);
    }

    #[test]
    fn test_full_event_normal() {
        let readings = vec![
            sensor(1, 55.75, 37.60, 10.0),
            sensor(2, 55.76, 37.61, 11.0),
        ];
        let baselines: HashMap<u64, DualBaseline> = readings.iter()
            .map(|r| (r.sensor_id, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)))
            .collect();

        let result = detect_event(55.75, 37.60, &readings, &baselines, 3.0);
        assert_eq!(result.concordance.event_type, EventType::Normal);
        assert!(result.confidence < 0.1);
    }

    #[test]
    fn test_dust_storm_pm10_trigger() {
        // Dust storm: PM10 spikes but PM2.5 stays moderate
        let readings = vec![
            SensorReading { sensor_id: 1, lat: 37.1, lon: 79.9, pm25: 30.0, pm10: 150.0 },
            SensorReading { sensor_id: 2, lat: 37.15, lon: 79.95, pm25: 28.0, pm10: 140.0 },
            SensorReading { sensor_id: 3, lat: 37.05, lon: 79.85, pm25: 12.0, pm10: 50.0 },
            SensorReading { sensor_id: 4, lat: 37.0, lon: 79.8, pm25: 10.0, pm10: 40.0 },
        ];
        let baselines: HashMap<u64, DualBaseline> = readings.iter()
            .map(|r| (r.sensor_id, DualBaseline::with_baselines(15.0, 9.0, 60.0, 100.0)))
            .collect();

        let result = detect_event(37.1, 79.9, &readings, &baselines, 2.0);
        // PM10 channel should trigger on sensors 1,2
        assert!(result.pm10_pm25_ratio > 2.0, "ratio: {}", result.pm10_pm25_ratio);
        // ratio ~4.5 → dust storm (coarse particles dominate)
        assert_eq!(result.source_hint.category, SourceCategory::DustStorm);
    }

    #[test]
    fn test_source_hint_combustion() {
        // Combustion: PM2.5 ≈ PM10 (ratio ~1.0-1.5)
        let readings = vec![
            SensorReading { sensor_id: 1, lat: 55.75, lon: 37.60, pm25: 50.0, pm10: 60.0 },
            SensorReading { sensor_id: 2, lat: 55.76, lon: 37.61, pm25: 45.0, pm10: 55.0 },
        ];
        let baselines: HashMap<u64, DualBaseline> = readings.iter()
            .map(|r| (r.sensor_id, DualBaseline::with_baselines(10.0, 4.0, 15.0, 6.0)))
            .collect();

        let result = detect_event(55.75, 37.60, &readings, &baselines, 3.0);
        assert!(result.pm10_pm25_ratio < 2.0, "ratio: {}", result.pm10_pm25_ratio);
        assert!(
            result.source_hint.category == SourceCategory::Combustion
            || result.source_hint.category == SourceCategory::Smoke,
            "expected combustion/smoke, got: {:?}", result.source_hint.category
        );
    }

    // -- classify_source unit tests --

    #[test]
    fn test_classify_clean() {
        let c = classify_source(1.5, 5.0, 7.5);
        assert_eq!(c.category, SourceCategory::Clean);
        assert!(c.confidence > 0.8);
    }

    #[test]
    fn test_classify_dust_storm() {
        // Hotan: PM2.5=34, PM10=148 → ratio 4.3
        let c = classify_source(4.3, 34.0, 148.0);
        assert_eq!(c.category, SourceCategory::DustStorm);
        assert!(c.label.contains("dust"));
        assert!(c.typical_sources.iter().any(|s| s.contains("desert")));
    }

    #[test]
    fn test_classify_construction() {
        let c = classify_source(3.0, 40.0, 120.0);
        assert_eq!(c.category, SourceCategory::ConstructionDust);
    }

    #[test]
    fn test_classify_smoke() {
        // Wildfire: PM2.5=80, PM10=90 → ratio 1.1, high PM2.5
        let c = classify_source(1.1, 80.0, 90.0);
        assert_eq!(c.category, SourceCategory::Smoke);
        assert!(c.typical_sources.iter().any(|s| s.contains("wildfire")));
    }

    #[test]
    fn test_classify_combustion() {
        // Traffic: PM2.5=40, PM10=50 → ratio 1.25
        let c = classify_source(1.25, 40.0, 50.0);
        assert_eq!(c.category, SourceCategory::Combustion);
        assert!(c.typical_sources.iter().any(|s| s.contains("diesel")));
    }

    #[test]
    fn test_classify_urban_background() {
        // Moderate: PM2.5=20, PM10=30 → ratio 1.5
        let c = classify_source(1.5, 20.0, 30.0);
        assert_eq!(c.category, SourceCategory::UrbanBackground);
    }

    #[test]
    fn test_classify_has_advice() {
        // Every category should have non-empty advice
        let cases = vec![
            classify_source(1.5, 5.0, 7.5),    // clean
            classify_source(5.0, 50.0, 250.0),  // dust
            classify_source(3.0, 40.0, 120.0),  // construction
            classify_source(1.1, 80.0, 90.0),   // smoke
            classify_source(1.25, 40.0, 50.0),  // combustion
            classify_source(1.5, 20.0, 30.0),   // urban
        ];
        for c in cases {
            assert!(!c.advice.is_empty(), "{:?} has no advice", c.category);
            assert!(!c.typical_sources.is_empty(), "{:?} has no sources", c.category);
            assert!(c.confidence > 0.0 && c.confidence <= 1.0);
        }
    }
}
