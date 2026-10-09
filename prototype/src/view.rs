//! Map camera in normalized Web Mercator ([0,1]², y down).

use std::f64::consts::PI;

pub const EARTH_KM: f64 = 6371.0;

pub fn to_merc(lat: f64, lon: f64) -> [f64; 2] {
    let x = (lon + 180.0) / 360.0;
    let s = lat.to_radians().sin();
    let y = 0.5 - ((1.0 + s) / (1.0 - s)).ln() / (4.0 * PI);
    [x, y]
}

pub fn from_merc(m: [f64; 2]) -> (f64, f64) {
    let lon = m[0] * 360.0 - 180.0;
    let lat = (PI * (1.0 - 2.0 * m[1])).sinh().atan().to_degrees();
    (lat, lon)
}

/// Great-circle distance (km) and initial bearing (deg) from a to b.
pub fn range_bearing(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (la1, lo1, la2, lo2) = (
        a.0.to_radians(),
        a.1.to_radians(),
        b.0.to_radians(),
        b.1.to_radians(),
    );
    let dlo = lo2 - lo1;
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
    let d = 2.0 * EARTH_KM * h.sqrt().asin();
    let y = dlo.sin() * la2.cos();
    let x = la1.cos() * la2.sin() - la1.sin() * la2.cos() * dlo.cos();
    (d, y.atan2(x).to_degrees().rem_euclid(360.0))
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub center: [f64; 2],
    /// Logical points per mercator unit.
    pub scale: f64,
}

impl Camera {
    /// Center on (lat, lon) showing `km_across` kilometers over `width_pts`.
    pub fn looking_at(lat: f64, lon: f64, km_across: f64, width_pts: f64) -> Camera {
        let km_per_merc = 2.0 * PI * EARTH_KM * lat.to_radians().cos();
        Camera {
            center: to_merc(lat, lon),
            scale: width_pts * km_per_merc / km_across,
        }
    }

    pub fn to_screen(&self, m: [f64; 2], screen_center: [f64; 2]) -> [f64; 2] {
        [
            (m[0] - self.center[0]) * self.scale + screen_center[0],
            (m[1] - self.center[1]) * self.scale + screen_center[1],
        ]
    }

    pub fn to_merc(&self, p: [f64; 2], screen_center: [f64; 2]) -> [f64; 2] {
        [
            (p[0] - screen_center[0]) / self.scale + self.center[0],
            (p[1] - screen_center[1]) / self.scale + self.center[1],
        ]
    }

    /// Zoom by `factor` keeping the point under `anchor` fixed.
    pub fn zoom_at(&mut self, factor: f64, anchor: [f64; 2], screen_center: [f64; 2]) {
        let before = self.to_merc(anchor, screen_center);
        self.scale = (self.scale * factor).clamp(800.0, 4.0e7);
        let after = self.to_merc(anchor, screen_center);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }
}
