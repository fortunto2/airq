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
}
