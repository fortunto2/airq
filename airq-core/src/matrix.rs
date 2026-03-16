//! SignalMatrix — unified time-series + single-point data structure.
//!
//! Macro-driven: `define_signal_columns!` is the single source of truth.
//! Adding a column = one line in the macro + one normalize function.
//!
//! Inspired by video-analyzer's `define_scoring_matrix!` pattern.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Macro: single source of truth for signal columns
// ---------------------------------------------------------------------------

/// Declare signal columns with weights. Generates:
/// - `N_SIGNALS: usize` — compile-time column count
/// - `SIGNAL_NAMES: [&str; N]` — ordered names
/// - `SIGNAL_WEIGHTS: [f64; N]` — comfort index weights
/// - `SignalRow` — single measurement (`scores: [f64; N]`)
/// - `SignalMatrix` — time-series storage
/// Helper: generate index constants for each column.
macro_rules! signal_idx {
    ( $i:expr, ) => {};
    ( $i:expr, $name:ident $(, $rest:ident)* ) => {
        #[allow(non_upper_case_globals)]
        pub const $name: usize = $i;
        signal_idx!($i + 1usize, $($rest),*);
    };
}

macro_rules! define_signal_columns {
    ( $( $name:ident $weight:expr ),* $(,)? ) => {
        /// Number of signal columns (compile-time constant).
        pub const N_SIGNALS: usize = {
            let mut n = 0usize;
            $( let _ = stringify!($name); n += 1; )*
            n
        };

        /// Column names in declaration order.
        pub const SIGNAL_NAMES: [&str; N_SIGNALS] = [
            $( stringify!($name), )*
        ];

        /// Comfort index weights (sum ≈ 1.0 for weighted columns).
        pub const SIGNAL_WEIGHTS: [f64; N_SIGNALS] = [
            $( $weight, )*
        ];

        /// Named column indices.
        pub mod idx {
            #![allow(non_upper_case_globals)]
            signal_idx!(0usize, $( $name ),*);
        }
    };
}

// ---------------------------------------------------------------------------
// Column definitions — THE source of truth
// ---------------------------------------------------------------------------

define_signal_columns! {
    air         0.22,
    temperature 0.18,
    sea         0.12,
    uv          0.08,
    earthquake  0.08,
    fire        0.05,
    pollen      0.05,
    pressure    0.05,
    geomagnetic 0.03,
    daylight    0.02,
    moon        0.00,
}

// ---------------------------------------------------------------------------
// SignalRow — single measurement
// ---------------------------------------------------------------------------

/// One row: scores for all signal columns. Values are normalized 0-100.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SignalRow {
    pub scores: [f64; N_SIGNALS],
}

impl SignalRow {
    /// Create a row with all zeros.
    pub fn zero() -> Self {
        Self { scores: [0.0; N_SIGNALS] }
    }

    /// Create a row from named values. Missing columns default to NaN.
    pub fn from_pairs(pairs: &[(&str, f64)]) -> Self {
        let mut scores = [f64::NAN; N_SIGNALS];
        for (name, value) in pairs {
            if let Some(i) = SIGNAL_NAMES.iter().position(|n| *n == *name) {
                scores[i] = *value;
            }
        }
        Self { scores }
    }

    /// Get value by column name.
    pub fn get(&self, name: &str) -> Option<f64> {
        SIGNAL_NAMES.iter().position(|n| *n == name).map(|i| self.scores[i])
    }

    /// Weighted comfort score (dot product with SIGNAL_WEIGHTS).
    pub fn weighted_score(&self) -> f64 {
        let mut sum = 0.0;
        let mut total_w = 0.0;
        for i in 0..N_SIGNALS {
            let w = SIGNAL_WEIGHTS[i];
            if w > 0.0 && !self.scores[i].is_nan() {
                sum += self.scores[i] * w;
                total_w += w;
            }
        }
        if total_w > 0.0 { sum / total_w } else { 0.0 }
    }
}

// ---------------------------------------------------------------------------
// SignalMatrix — time-series storage (AoS for row-append, column access via iter)
// ---------------------------------------------------------------------------

/// Time-series matrix: N rows × 11 signal columns + metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMatrix {
    /// Epoch seconds per row (monotonically increasing).
    pub timestamps: Vec<f64>,
    /// Signal scores per row.
    pub data: Vec<[f64; N_SIGNALS]>,
    /// Sensor count per row (from AreaAverage). 0 if unknown.
    pub sensor_count: Vec<u32>,
    /// Pollution front detected at this timestamp.
    pub front_detected: Vec<bool>,
}

impl SignalMatrix {
    /// Empty matrix.
    pub fn new() -> Self {
        Self {
            timestamps: Vec::new(),
            data: Vec::new(),
            sensor_count: Vec::new(),
            front_detected: Vec::new(),
        }
    }

    /// Pre-allocate for expected row count.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            timestamps: Vec::with_capacity(cap),
            data: Vec::with_capacity(cap),
            sensor_count: Vec::with_capacity(cap),
            front_detected: Vec::with_capacity(cap),
        }
    }

    pub fn len(&self) -> usize {
        self.timestamps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.timestamps.is_empty()
    }

    /// Append a row.
    pub fn push(&mut self, ts: f64, row: SignalRow) {
        self.timestamps.push(ts);
        self.data.push(row.scores);
        self.sensor_count.push(0);
        self.front_detected.push(false);
    }

    /// Append a row with metadata.
    pub fn push_with_meta(
        &mut self,
        ts: f64,
        row: SignalRow,
        sensors: u32,
        front: bool,
    ) {
        self.timestamps.push(ts);
        self.data.push(row.scores);
        self.sensor_count.push(sensors);
        self.front_detected.push(front);
    }

    /// Last row (most recent measurement).
    pub fn latest(&self) -> Option<(f64, SignalRow)> {
        let n = self.len();
        if n == 0 {
            return None;
        }
        Some((self.timestamps[n - 1], SignalRow { scores: self.data[n - 1] }))
    }

    /// Convert latest row to `super::signal::SignalComfort` (backward compat).
    pub fn to_comfort(&self) -> Option<super::signal::SignalComfort> {
        let (_, row) = self.latest()?;
        Some(super::signal::SignalComfort {
            total: row.weighted_score().round() as u32,
            air: row.scores[idx::air] as u32,
            temperature: row.scores[idx::temperature] as u32,
            uv: row.scores[idx::uv] as u32,
            sea: row.scores[idx::sea] as u32,
            earthquake: row.scores[idx::earthquake] as u32,
            fire: row.scores[idx::fire] as u32,
            pollen: row.scores[idx::pollen] as u32,
            pressure: row.scores[idx::pressure] as u32,
            geomagnetic: row.scores[idx::geomagnetic] as u32,
            moon: row.scores[idx::moon] as u32,
            daylight: row.scores[idx::daylight] as u32,
        })
    }

    /// Extract a single column by name as Vec<f64>.
    pub fn column(&self, name: &str) -> Option<Vec<f64>> {
        let i = SIGNAL_NAMES.iter().position(|n| *n == name)?;
        Some(self.column_idx(i))
    }

    /// Extract a single column by index.
    pub fn column_idx(&self, idx: usize) -> Vec<f64> {
        self.data.iter().map(|row| row[idx]).collect()
    }

    /// Slice by timestamp range [start, end). Returns new matrix.
    pub fn slice(&self, start_ts: f64, end_ts: f64) -> Self {
        let start = self.timestamps.partition_point(|&t| t < start_ts);
        let end = self.timestamps.partition_point(|&t| t < end_ts);
        Self {
            timestamps: self.timestamps[start..end].to_vec(),
            data: self.data[start..end].to_vec(),
            sensor_count: self.sensor_count[start..end].to_vec(),
            front_detected: self.front_detected[start..end].to_vec(),
        }
    }

    /// Last N hours of data.
    pub fn last_hours(&self, h: u32) -> Self {
        if self.is_empty() {
            return Self::new();
        }
        let end = self.timestamps[self.len() - 1];
        let start = end - (h as f64 * 3600.0);
        self.slice(start, end + 1.0) // +1 to include end
    }

    /// Last N days of data.
    pub fn last_days(&self, d: u32) -> Self {
        self.last_hours(d * 24)
    }

    /// Trim to keep only the last `max_rows` rows.
    pub fn compact(&mut self, max_rows: usize) {
        if self.len() <= max_rows {
            return;
        }
        let drop = self.len() - max_rows;
        self.timestamps.drain(..drop);
        self.data.drain(..drop);
        self.sensor_count.drain(..drop);
        self.front_detected.drain(..drop);
    }
}

// ---------------------------------------------------------------------------
// Math operations: deltas, trends, summary, ML vector
// ---------------------------------------------------------------------------

/// OLS (ordinary least squares) slope for evenly-spaced values.
/// x_i = 0, 1, 2, ..., n-1; y_i = values[i].
/// Returns slope β. NaN-safe: skips NaN values.
pub fn ols_slope(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut count = 0.0;
    for (i, &v) in values.iter().enumerate() {
        if !v.is_nan() {
            sum_x += i as f64;
            sum_y += v;
            count += 1.0;
        }
    }
    if count < 2.0 {
        return 0.0;
    }
    let mean_x = sum_x / count;
    let mean_y = sum_y / count;
    let mut num = 0.0;
    let mut den = 0.0;
    for (i, &v) in values.iter().enumerate() {
        if !v.is_nan() {
            let dx = i as f64 - mean_x;
            num += dx * (v - mean_y);
            den += dx * dx;
        }
    }
    if den.abs() < f64::EPSILON { 0.0 } else { num / den }
}

/// Per-column statistics.
#[derive(Debug, Clone, Serialize)]
pub struct ColumnStats {
    pub name: &'static str,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub std_dev: f64,
    pub count: usize,
}

/// Summary for the entire matrix.
#[derive(Debug, Clone, Serialize)]
pub struct MatrixSummary {
    pub rows: usize,
    pub columns: Vec<ColumnStats>,
}

/// ML-ready feature vector (35 dimensions).
pub const N_ML_FEATURES: usize = N_SIGNALS * 3 + 2; // 11 current + 11 delta + 11 trend + sensor + front

#[derive(Debug, Clone)]
pub struct MlVector {
    /// 35 features: [11 current] [11 delta_24h] [11 trend_7d] [sensor_count] [front]
    pub features: [f64; N_ML_FEATURES],
    /// Feature names.
    pub names: [&'static str; N_ML_FEATURES],
    /// Weighted comfort 0.0-1.0.
    pub comfort: f64,
    /// Classification label.
    pub label: &'static str,
}

impl Serialize for MlVector {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("MlVector", 4)?;
        s.serialize_field("features", &self.features.as_slice())?;
        let names_vec: Vec<&str> = self.names.iter().copied().collect();
        s.serialize_field("names", &names_vec)?;
        s.serialize_field("comfort", &self.comfort)?;
        s.serialize_field("label", &self.label)?;
        s.end()
    }
}

impl SignalMatrix {
    /// Delta: difference between last row and row `window` steps back.
    /// Returns None if matrix has fewer than `window + 1` rows.
    pub fn deltas(&self, window: usize) -> Option<[f64; N_SIGNALS]> {
        let n = self.len();
        if n == 0 || window >= n {
            return None;
        }
        let current = &self.data[n - 1];
        let past = &self.data[n - 1 - window];
        let mut result = [0.0; N_SIGNALS];
        for i in 0..N_SIGNALS {
            result[i] = current[i] - past[i];
        }
        Some(result)
    }

    /// Trend: OLS slope per column over last `window` rows. Clamped to [-1, 1].
    /// Positive = improving, negative = degrading.
    pub fn trends(&self, window: usize) -> [f64; N_SIGNALS] {
        let mut result = [0.0; N_SIGNALS];
        let n = self.len();
        if n < 2 {
            return result;
        }
        let start = if n > window { n - window } else { 0 };
        for col in 0..N_SIGNALS {
            let values: Vec<f64> = self.data[start..n].iter().map(|r| r[col]).collect();
            let slope = ols_slope(&values);
            // Normalize: slope is per-hour change in score (0-100).
            // Clamp to [-1, 1] for ML input.
            result[col] = slope.clamp(-1.0, 1.0);
        }
        result
    }

    /// Per-column min/max/mean/std_dev statistics.
    pub fn summary(&self) -> MatrixSummary {
        let mut columns = Vec::with_capacity(N_SIGNALS);
        for col in 0..N_SIGNALS {
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            let mut sum = 0.0;
            let mut count = 0usize;
            for row in &self.data {
                let v = row[col];
                if !v.is_nan() {
                    if v < min { min = v; }
                    if v > max { max = v; }
                    sum += v;
                    count += 1;
                }
            }
            let mean = if count > 0 { sum / count as f64 } else { 0.0 };
            let std_dev = if count > 1 {
                let var: f64 = self.data.iter()
                    .map(|r| r[col])
                    .filter(|v| !v.is_nan())
                    .map(|v| (v - mean).powi(2))
                    .sum::<f64>() / (count - 1) as f64;
                var.sqrt()
            } else {
                0.0
            };
            columns.push(ColumnStats {
                name: SIGNAL_NAMES[col],
                min: if count > 0 { min } else { 0.0 },
                max: if count > 0 { max } else { 0.0 },
                mean,
                std_dev,
                count,
            });
        }
        MatrixSummary { rows: self.len(), columns }
    }

    /// Build ML-ready feature vector (35 dimensions).
    pub fn to_ml_vector(&self) -> Option<MlVector> {
        let (_, row) = self.latest()?;
        let mut features = [0.0; N_ML_FEATURES];
        let mut names = [""; N_ML_FEATURES];

        // [0..11] current values normalized to 0.0-1.0
        for i in 0..N_SIGNALS {
            features[i] = row.scores[i] / 100.0;
            names[i] = SIGNAL_NAMES[i];
        }

        // [11..22] delta 24h (or available window)
        let delta_window = self.len().min(24).saturating_sub(1);
        let deltas = self.deltas(delta_window).unwrap_or([0.0; N_SIGNALS]);
        for i in 0..N_SIGNALS {
            features[N_SIGNALS + i] = deltas[i] / 100.0;
            // Names: static strings from macro + suffix
            names[N_SIGNALS + i] = SIGNAL_NAMES[i]; // caller appends "_delta"
        }

        // [22..33] trend 7d (or available window)
        let trend_window = self.len().min(168);
        let trends = self.trends(trend_window);
        for i in 0..N_SIGNALS {
            features[2 * N_SIGNALS + i] = trends[i];
            names[2 * N_SIGNALS + i] = SIGNAL_NAMES[i]; // caller appends "_trend"
        }

        // [33] sensor_count normalized
        let n = self.len();
        features[3 * N_SIGNALS] = self.sensor_count[n - 1] as f64 / 50.0;
        names[3 * N_SIGNALS] = "sensor_count";

        // [34] front_detected
        features[3 * N_SIGNALS + 1] = if self.front_detected[n - 1] { 1.0 } else { 0.0 };
        names[3 * N_SIGNALS + 1] = "front_detected";

        let comfort = row.weighted_score() / 100.0;
        let label = match (comfort * 100.0).round() as u32 {
            80..=100 => "excellent",
            60..=79 => "good",
            40..=59 => "fair",
            20..=39 => "poor",
            _ => "bad",
        };

        Some(MlVector { features, names, comfort, label })
    }
}

impl Default for SignalMatrix {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row(base: f64) -> SignalRow {
        let mut scores = [0.0; N_SIGNALS];
        for (i, s) in scores.iter_mut().enumerate() {
            *s = base + i as f64;
        }
        SignalRow { scores }
    }

    #[test]
    fn test_n_signals() {
        assert_eq!(N_SIGNALS, 11);
    }

    #[test]
    fn test_signal_names() {
        assert_eq!(SIGNAL_NAMES[0], "air");
        assert_eq!(SIGNAL_NAMES[1], "temperature");
        assert_eq!(SIGNAL_NAMES[10], "moon");
        assert_eq!(SIGNAL_NAMES.len(), N_SIGNALS);
    }

    #[test]
    fn test_signal_weights_sum() {
        let sum: f64 = SIGNAL_WEIGHTS.iter().sum();
        // moon=0.0, so sum is 0.88 not 1.0. But weighted columns should sum correctly.
        assert!((sum - 0.88).abs() < 0.001, "weights sum: {sum}");
    }

    #[test]
    fn test_idx_constants() {
        assert_eq!(idx::air, 0);
        assert_eq!(idx::temperature, 1);
        assert_eq!(idx::moon, 10);
    }

    #[test]
    fn test_signal_row_from_pairs() {
        let row = SignalRow::from_pairs(&[("air", 80.0), ("moon", 50.0)]);
        assert_eq!(row.scores[idx::air], 80.0);
        assert_eq!(row.scores[idx::moon], 50.0);
        assert!(row.scores[idx::temperature].is_nan());
    }

    #[test]
    fn test_signal_row_get() {
        let row = sample_row(50.0);
        assert_eq!(row.get("air"), Some(50.0));
        assert_eq!(row.get("temperature"), Some(51.0));
        assert_eq!(row.get("nonexistent"), None);
    }

    #[test]
    fn test_signal_row_weighted_score() {
        // All 80 → weighted score = 80 (weights cancel out)
        let row = SignalRow { scores: [80.0; N_SIGNALS] };
        let score = row.weighted_score();
        assert!((score - 80.0).abs() < 0.01, "score: {score}");
    }

    #[test]
    fn test_matrix_empty() {
        let m = SignalMatrix::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert!(m.latest().is_none());
        assert!(m.to_comfort().is_none());
    }

    #[test]
    fn test_matrix_push_latest() {
        let mut m = SignalMatrix::new();
        let row = sample_row(70.0);
        m.push(1000.0, row);
        assert_eq!(m.len(), 1);
        let (ts, latest) = m.latest().unwrap();
        assert_eq!(ts, 1000.0);
        assert_eq!(latest, row);
    }

    #[test]
    fn test_matrix_push_with_meta() {
        let mut m = SignalMatrix::new();
        m.push_with_meta(1000.0, sample_row(50.0), 5, true);
        assert_eq!(m.sensor_count[0], 5);
        assert!(m.front_detected[0]);
    }

    #[test]
    fn test_matrix_column() {
        let mut m = SignalMatrix::new();
        m.push(1000.0, sample_row(10.0));
        m.push(2000.0, sample_row(20.0));
        let col = m.column("air").unwrap();
        assert_eq!(col, vec![10.0, 20.0]);
        assert!(m.column("nonexistent").is_none());
    }

    #[test]
    fn test_matrix_slice() {
        let mut m = SignalMatrix::new();
        for i in 0..10 {
            m.push(i as f64 * 3600.0, sample_row(i as f64 * 10.0));
        }
        // Slice hours 3-6 (timestamps 10800..21600)
        let s = m.slice(10800.0, 21600.0);
        assert_eq!(s.len(), 3); // hours 3, 4, 5
        assert_eq!(s.timestamps[0], 10800.0);
    }

    #[test]
    fn test_matrix_last_hours() {
        let mut m = SignalMatrix::new();
        for i in 0..48 {
            m.push(i as f64 * 3600.0, sample_row(i as f64));
        }
        let last24 = m.last_hours(24);
        assert_eq!(last24.len(), 25); // hours 23..47 inclusive
    }

    #[test]
    fn test_matrix_compact() {
        let mut m = SignalMatrix::new();
        for i in 0..100 {
            m.push(i as f64, sample_row(i as f64));
        }
        m.compact(10);
        assert_eq!(m.len(), 10);
        assert_eq!(m.timestamps[0], 90.0);
    }

    #[test]
    fn test_matrix_to_comfort() {
        let mut m = SignalMatrix::new();
        m.push(1000.0, SignalRow { scores: [80.0; N_SIGNALS] });
        let comfort = m.to_comfort().unwrap();
        assert_eq!(comfort.air, 80);
        assert_eq!(comfort.total, 80);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut m = SignalMatrix::new();
        m.push(1000.0, sample_row(50.0));
        m.push(2000.0, sample_row(60.0));
        let json = serde_json::to_string(&m).unwrap();
        let m2: SignalMatrix = serde_json::from_str(&json).unwrap();
        assert_eq!(m2.len(), 2);
        assert_eq!(m2.data[0], m.data[0]);
    }

    // -- Phase 2: Math ops --

    #[test]
    fn test_ols_slope_linear() {
        // y = 2x → slope = 2
        let vals = vec![0.0, 2.0, 4.0, 6.0, 8.0];
        let slope = ols_slope(&vals);
        assert!((slope - 2.0).abs() < 0.001, "slope: {slope}");
    }

    #[test]
    fn test_ols_slope_flat() {
        let vals = vec![50.0, 50.0, 50.0, 50.0];
        assert_eq!(ols_slope(&vals), 0.0);
    }

    #[test]
    fn test_ols_slope_single() {
        assert_eq!(ols_slope(&[42.0]), 0.0);
    }

    #[test]
    fn test_deltas() {
        let mut m = SignalMatrix::new();
        m.push(0.0, SignalRow { scores: [50.0; N_SIGNALS] });
        m.push(3600.0, SignalRow { scores: [70.0; N_SIGNALS] });
        m.push(7200.0, SignalRow { scores: [80.0; N_SIGNALS] });

        // Delta 1 step back: 80 - 70 = 10
        let d = m.deltas(1).unwrap();
        assert!((d[0] - 10.0).abs() < 0.001);

        // Delta 2 steps back: 80 - 50 = 30
        let d = m.deltas(2).unwrap();
        assert!((d[0] - 30.0).abs() < 0.001);

        // Window too large
        assert!(m.deltas(3).is_none());
    }

    #[test]
    fn test_trends_increasing() {
        let mut m = SignalMatrix::new();
        for i in 0..10 {
            // All columns increase by 5 per step
            let scores = [i as f64 * 5.0; N_SIGNALS];
            m.push(i as f64 * 3600.0, SignalRow { scores });
        }
        let t = m.trends(10);
        // Slope = 5 per step, clamped to 1.0
        assert!((t[0] - 1.0).abs() < 0.001, "trend[0]: {}", t[0]);
    }

    #[test]
    fn test_trends_decreasing() {
        let mut m = SignalMatrix::new();
        for i in 0..10 {
            let scores = [90.0 - i as f64 * 5.0; N_SIGNALS];
            m.push(i as f64 * 3600.0, SignalRow { scores });
        }
        let t = m.trends(10);
        assert!((t[0] - (-1.0)).abs() < 0.001, "trend[0]: {}", t[0]);
    }

    #[test]
    fn test_trends_stable() {
        let mut m = SignalMatrix::new();
        for i in 0..10 {
            m.push(i as f64 * 3600.0, SignalRow { scores: [50.0; N_SIGNALS] });
        }
        let t = m.trends(10);
        assert!((t[0]).abs() < 0.001);
    }

    #[test]
    fn test_summary() {
        let mut m = SignalMatrix::new();
        m.push(0.0, SignalRow { scores: [10.0; N_SIGNALS] });
        m.push(1.0, SignalRow { scores: [20.0; N_SIGNALS] });
        m.push(2.0, SignalRow { scores: [30.0; N_SIGNALS] });
        let s = m.summary();
        assert_eq!(s.rows, 3);
        assert_eq!(s.columns.len(), N_SIGNALS);
        let air = &s.columns[0];
        assert_eq!(air.name, "air");
        assert!((air.min - 10.0).abs() < 0.001);
        assert!((air.max - 30.0).abs() < 0.001);
        assert!((air.mean - 20.0).abs() < 0.001);
        assert!(air.std_dev > 0.0);
    }

    #[test]
    fn test_ml_vector_dimensions() {
        let mut m = SignalMatrix::new();
        m.push_with_meta(0.0, SignalRow { scores: [80.0; N_SIGNALS] }, 5, true);
        let v = m.to_ml_vector().unwrap();
        assert_eq!(v.features.len(), N_ML_FEATURES);
        assert_eq!(v.features.len(), 35);
        assert_eq!(v.names.len(), 35);
        // Current: 80/100 = 0.8
        assert!((v.features[0] - 0.8).abs() < 0.001);
        // Sensor count: 5/50 = 0.1
        assert!((v.features[33] - 0.1).abs() < 0.001);
        // Front: 1.0
        assert!((v.features[34] - 1.0).abs() < 0.001);
        assert_eq!(v.label, "excellent");
    }

    #[test]
    fn test_ml_vector_empty() {
        let m = SignalMatrix::new();
        assert!(m.to_ml_vector().is_none());
    }

    #[test]
    fn test_weighted_score_matches_comfort() {
        // Ensure backward compat: weighted_score ≈ SignalComfort.total
        let row = SignalRow { scores: [75.0; N_SIGNALS] };
        let score = row.weighted_score();
        assert!((score - 75.0).abs() < 0.01, "score: {score}");

        // Different values
        let mut scores = [0.0; N_SIGNALS];
        scores[idx::air] = 100.0;       // w=0.22
        scores[idx::temperature] = 50.0; // w=0.18
        scores[idx::sea] = 80.0;         // w=0.12
        // rest are 0 with non-zero weights → pull average down
        let row = SignalRow { scores };
        let ws = row.weighted_score();
        // Manual: (100*0.22 + 50*0.18 + 80*0.12) / (0.22+0.18+0.12+0.08+0.08+0.05+0.05+0.05+0.03+0.02)
        // = (22 + 9 + 9.6) / 0.88 = 40.6 / 0.88 ≈ 46.136
        assert!((ws - 46.136).abs() < 0.1, "weighted_score: {ws}");
    }
}
