//! Conditional Probability Function — which neighbour is making the air bad.
//!
//! For each nearby source it asks a question a resident actually has: *of all the hours the wind
//! blew from that direction, what fraction were in the worst quarter of readings?* A cement works
//! that only matters when the wind is northerly scores high; a motorway that is always there
//! scores flat. It also reports the average when the wind is from the source against the average
//! from everywhere else, which is the whole finding in one comparison —
//! "from the port: 31 µg/m³, otherwise 12".
//!
//! Lives outside `front` because that module opens with `use petgraph`, and this needs nothing but
//! arithmetic. Keeping it here is what lets it reach a browser at all.

/// CPF result for a single pollution source.
#[derive(Debug, serde::Serialize)]
pub struct CpfResult {
    pub source: crate::PollutionSource,
    /// CPF score 0.0-1.0
    pub cpf_score: f64,
    /// Bearing from sensor to source (degrees)
    pub bearing_deg: f64,
    /// Total hours where wind came from source sector (and speed > threshold)
    pub hours_in_sector: usize,
    /// Hours in sector where PM2.5 exceeded threshold
    pub high_hours_in_sector: usize,
    /// Average PM2.5 when wind is from source direction
    pub avg_pm25_in_sector: f64,
    /// Average PM2.5 from other directions (background)
    pub avg_pm25_other: f64,
}

/// Calculate CPF for each source given hourly observations.
///
/// `wind_dirs` and `wind_speeds` must be same length as `pm25_values`.
/// `percentile` -- fraction (0.75 or 0.90) to compute the high-pollution threshold.
pub fn calculate_cpf(
    sensor_lat: f64,
    sensor_lon: f64,
    sources: &[crate::PollutionSource],
    pm25_values: &[f64],
    wind_dirs: &[f64],
    wind_speeds: &[f64],
    percentile: f64,
) -> Vec<CpfResult> {
    if pm25_values.is_empty() {
        return Vec::new();
    }

    // 1. Calculate threshold = percentile of pm25_values
    let mut sorted_pm = pm25_values.to_vec();
    sorted_pm.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((sorted_pm.len() as f64 * percentile) as usize).min(sorted_pm.len() - 1);
    let threshold = sorted_pm[idx];

    let half_sector = 15.0_f64; // +/-15 degree sector width
    let min_wind_speed = 5.0; // km/h -- filter calm winds

    let mut results: Vec<CpfResult> = Vec::new();

    for source in sources {
        let brng = crate::geo::bearing(sensor_lat, sensor_lon, source.lat, source.lon);

        let mut sector_count = 0usize;
        let mut high_count = 0usize;
        let mut sector_pm_sum = 0.0;
        let mut other_pm_sum = 0.0;
        let mut other_count = 0usize;

        for i in 0..pm25_values.len() {
            if wind_speeds[i] < min_wind_speed {
                continue; // calm wind -- unreliable direction
            }

            // Angular difference (wind comes FROM wind_dir, source is at brng)
            let diff = ((wind_dirs[i] - brng + 540.0) % 360.0) - 180.0;
            let in_sector = diff.abs() <= half_sector;

            if in_sector {
                sector_count += 1;
                sector_pm_sum += pm25_values[i];
                if pm25_values[i] > threshold {
                    high_count += 1;
                }
            } else {
                other_pm_sum += pm25_values[i];
                other_count += 1;
            }
        }

        let cpf = if sector_count > 0 {
            high_count as f64 / sector_count as f64
        } else {
            0.0
        };

        let avg_in = if sector_count > 0 {
            sector_pm_sum / sector_count as f64
        } else {
            0.0
        };

        let avg_other = if other_count > 0 {
            other_pm_sum / other_count as f64
        } else {
            0.0
        };

        results.push(CpfResult {
            source: source.clone(),
            cpf_score: cpf,
            bearing_deg: brng,
            hours_in_sector: sector_count,
            high_hours_in_sector: high_count,
            avg_pm25_in_sector: avg_in,
            avg_pm25_other: avg_other,
        });
    }

    // Sort by CPF descending
    results.sort_by(|a, b| b.cpf_score.partial_cmp(&a.cpf_score).unwrap());
    results
}
