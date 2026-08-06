//! Spherical geometry. No dependencies, on purpose.
//!
//! These four functions used to live inside `front`, which opens with
//! `use petgraph::graph::...`. That made a WASM build that only wanted to measure the distance
//! between two coordinates link a graph library — 300 KB in a bundle whose whole argument is that
//! the reading is already on screen. They are pure arithmetic and belong in a module that costs
//! nothing to depend on; `front` re-exports them, so every existing caller keeps working.

/// Haversine distance in km between two points.
pub fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    r * 2.0 * a.sqrt().asin()
}

/// Bearing in degrees (clockwise from N) from point A to point B.
pub fn bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Compass label from bearing degrees.
pub fn bearing_label(deg: f64) -> &'static str {
    let dirs = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let idx = ((deg + 22.5) % 360.0 / 45.0) as usize;
    dirs[idx.min(7)]
}

/// Arrow from bearing degrees (direction pollution is moving TO).
pub fn bearing_arrow(deg: f64) -> &'static str {
    let arrows = [
        "\u{2191}", "\u{2197}", "\u{2192}", "\u{2198}", "\u{2193}", "\u{2199}", "\u{2190}",
        "\u{2196}",
    ];
    let idx = ((deg + 22.5) % 360.0 / 45.0) as usize;
    arrows[idx.min(7)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_known_distance() {
        // Gazipaşa → Alanya, ~30 km along the coast.
        let d = haversine(36.266, 32.294, 36.544, 31.999);
        assert!((d - 40.0).abs() < 5.0, "expected ~40 km, got {d}");
    }

    #[test]
    fn haversine_is_symmetric_and_zero_at_a_point() {
        assert_eq!(haversine(50.0, 8.0, 50.0, 8.0), 0.0);
        let a = haversine(48.5, 9.2, 52.5, 13.4);
        let b = haversine(52.5, 13.4, 48.5, 9.2);
        assert!((a - b).abs() < 1e-9);
    }

    #[test]
    fn bearing_cardinals() {
        assert_eq!(bearing_label(bearing(0.0, 0.0, 1.0, 0.0)), "N");
        assert_eq!(bearing_label(bearing(0.0, 0.0, 0.0, 1.0)), "E");
        assert_eq!(bearing_label(bearing(1.0, 0.0, 0.0, 0.0)), "S");
        assert_eq!(bearing_label(bearing(0.0, 1.0, 0.0, 0.0)), "W");
    }
}
