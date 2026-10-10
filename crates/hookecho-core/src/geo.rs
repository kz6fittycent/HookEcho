//! Geometry: Web Mercator, great-circle range/bearing, radar beam geometry.

use std::f64::consts::PI;

pub const EARTH_RADIUS_KM: f64 = 6371.0;
/// Effective earth radius factor for standard refraction (Doviak & Zrnić, §2.2).
pub const KE: f64 = 4.0 / 3.0;

/// (lat, lon) degrees → normalized Web Mercator ([0,1]², y down).
pub fn to_mercator(lat: f64, lon: f64) -> [f64; 2] {
    let s = lat.clamp(-85.05, 85.05).to_radians().sin();
    [
        (lon + 180.0) / 360.0,
        0.5 - ((1.0 + s) / (1.0 - s)).ln() / (4.0 * PI),
    ]
}

pub fn from_mercator(m: [f64; 2]) -> (f64, f64) {
    (
        (PI * (1.0 - 2.0 * m[1])).sinh().atan().to_degrees(),
        m[0] * 360.0 - 180.0,
    )
}

/// Kilometers per normalized mercator unit at latitude `lat`.
pub fn km_per_mercator_unit(lat: f64) -> f64 {
    2.0 * PI * EARTH_RADIUS_KM * lat.to_radians().cos()
}

/// Great-circle distance (km) and initial bearing (degrees) from `a` to `b`.
pub fn range_bearing(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (la1, lo1, la2, lo2) = (
        a.0.to_radians(),
        a.1.to_radians(),
        b.0.to_radians(),
        b.1.to_radians(),
    );
    let dlo = lo2 - lo1;
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
    let d = 2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin();
    let y = dlo.sin() * la2.cos();
    let x = la1.cos() * la2.sin() - la1.sin() * la2.cos() * dlo.cos();
    (d, y.atan2(x).to_degrees().rem_euclid(360.0))
}

/// Point at `range_km` along `bearing_deg` from `origin`.
pub fn destination(origin: (f64, f64), range_km: f64, bearing_deg: f64) -> (f64, f64) {
    let (la1, lo1) = (origin.0.to_radians(), origin.1.to_radians());
    let d = range_km / EARTH_RADIUS_KM;
    let b = bearing_deg.to_radians();
    let la2 = (la1.sin() * d.cos() + la1.cos() * d.sin() * b.cos()).asin();
    let lo2 = lo1 + (b.sin() * d.sin() * la1.cos()).atan2(d.cos() - la1.sin() * la2.sin());
    (
        la2.to_degrees(),
        (lo2.to_degrees() + 540.0).rem_euclid(360.0) - 180.0,
    )
}

/// Beam center height above radar level (km) at slant range `r_km` and
/// elevation `elev_deg`. Doviak & Zrnić (1993) eq. 2.28b.
pub fn beam_height_km(r_km: f64, elev_deg: f64) -> f64 {
    let re = KE * EARTH_RADIUS_KM;
    let e = elev_deg.to_radians();
    (r_km * r_km + re * re + 2.0 * r_km * re * e.sin()).sqrt() - re
}

/// Ground (great-circle) distance (km) under the beam at slant range `r_km`.
/// Doviak & Zrnić (1993) eq. 2.28c.
pub fn ground_range_km(r_km: f64, elev_deg: f64) -> f64 {
    let re = KE * EARTH_RADIUS_KM;
    let h = beam_height_km(r_km, elev_deg);
    re * (r_km * elev_deg.to_radians().cos() / (re + h)).asin()
}

/// Inverse of [`ground_range_km`]: slant range (km) whose beam is above
/// ground distance `s_km`. With φ = s / (kₑa) the central angle, the
/// triangle (earth center, radar, target) gives r = kₑa·sin φ / cos(θ + φ).
/// The renderer's shader uses the same closed form.
pub fn slant_range_km(s_km: f64, elev_deg: f64) -> f64 {
    let re = KE * EARTH_RADIUS_KM;
    let phi = s_km / re;
    re * phi.sin() / (elev_deg.to_radians() + phi).cos()
}

/// Map view: a center in normalized mercator and a zoom expressed as logical
/// pixels per mercator unit. Screen coordinates are logical px with the
/// origin at the top-left of the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub center: [f64; 2],
    pub scale: f64,
}

impl Camera {
    pub const MIN_SCALE: f64 = 600.0; // whole CONUS on a laptop screen
    pub const MAX_SCALE: f64 = 6.0e7; // ~1 m per logical px

    /// Centered on (lat, lon), showing `km_across` kilometers over `width` px.
    pub fn looking_at(lat: f64, lon: f64, km_across: f64, width: f64) -> Camera {
        Camera {
            center: to_mercator(lat, lon),
            scale: width * km_per_mercator_unit(lat) / km_across,
        }
    }

    pub fn to_screen(&self, m: [f64; 2], size: [f64; 2]) -> [f64; 2] {
        [
            (m[0] - self.center[0]) * self.scale + size[0] / 2.0,
            (m[1] - self.center[1]) * self.scale + size[1] / 2.0,
        ]
    }

    pub fn to_mercator(&self, p: [f64; 2], size: [f64; 2]) -> [f64; 2] {
        [
            (p[0] - size[0] / 2.0) / self.scale + self.center[0],
            (p[1] - size[1] / 2.0) / self.scale + self.center[1],
        ]
    }

    pub fn latlon_to_screen(&self, lat: f64, lon: f64, size: [f64; 2]) -> [f64; 2] {
        self.to_screen(to_mercator(lat, lon), size)
    }

    pub fn screen_to_latlon(&self, p: [f64; 2], size: [f64; 2]) -> (f64, f64) {
        from_mercator(self.to_mercator(p, size))
    }

    /// Ground kilometers per logical px at the view center.
    pub fn km_per_px(&self) -> f64 {
        km_per_mercator_unit(from_mercator(self.center).0) / self.scale
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.center[0] -= dx / self.scale;
        self.center[1] = (self.center[1] - dy / self.scale).clamp(0.0, 1.0);
    }

    /// Zoom by `factor`, keeping the point under `anchor` fixed.
    pub fn zoom_at(&mut self, factor: f64, anchor: [f64; 2], size: [f64; 2]) {
        let before = self.to_mercator(anchor, size);
        self.scale = (self.scale * factor).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        let after = self.to_mercator(anchor, size);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }
}

/// Number of bins in [`azimuth_index`] (0.1° each).
pub const AZIMUTH_BINS: usize = 3600;

/// Maps 0.1° azimuth bins to radial indices so a renderer never needs sorted
/// radials. Each radial covers [center − spacing/2, center + spacing/2); holes
/// left by jittered centers are filled from the nearest radial within 0.5°.
/// `u16::MAX` marks bins with no radial.
pub fn azimuth_index(centers_deg: &[f32], spacing_deg: &[f32]) -> Vec<u16> {
    let n = AZIMUTH_BINS as i32;
    let mut idx = vec![u16::MAX; AZIMUTH_BINS];
    for (row, (&c, &w)) in centers_deg.iter().zip(spacing_deg).enumerate() {
        let half = if w > 0.0 { w / 2.0 } else { 0.25 };
        let start = ((c - half) * 10.0).round() as i32;
        let end = ((c + half) * 10.0).round() as i32;
        for b in start..end {
            idx[b.rem_euclid(n) as usize] = row as u16;
        }
    }
    let base = idx.clone();
    for b in 0..n {
        if base[b as usize] != u16::MAX {
            continue;
        }
        for d in 1..=5 {
            let l = base[(b - d).rem_euclid(n) as usize];
            let r = base[(b + d).rem_euclid(n) as usize];
            if l != u16::MAX || r != u16::MAX {
                idx[b as usize] = if l != u16::MAX { l } else { r };
                break;
            }
        }
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_zoom_keeps_anchor_fixed() {
        let size = [1280.0, 800.0];
        let mut c = Camera::looking_at(35.33, -97.28, 100.0, size[0]);
        let anchor = [300.0, 200.0];
        let before = c.screen_to_latlon(anchor, size);
        c.zoom_at(2.5, anchor, size);
        let after = c.screen_to_latlon(anchor, size);
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        // The center latitude moves a little (zoom is around an off-center
        // anchor), so compare the scale change, not the exact value.
        let expected = 100.0 / 1280.0 / 2.5;
        assert!(
            (c.km_per_px() / expected - 1.0).abs() < 0.01,
            "{} vs {expected}",
            c.km_per_px()
        );
    }

    #[test]
    fn mercator_round_trip() {
        for (lat, lon) in [(35.333, -97.278), (-33.9, 151.2), (64.1, -21.9)] {
            let (a, b) = from_mercator(to_mercator(lat, lon));
            assert!((a - lat).abs() < 1e-9 && (b - lon).abs() < 1e-9);
        }
    }

    #[test]
    fn range_bearing_destination_round_trip() {
        let o = (35.3333, -97.2778);
        for (r, b) in [(10.0, 45.0), (230.0, 270.0), (460.0, 181.0)] {
            let p = destination(o, r, b);
            let (r2, b2) = range_bearing(o, p);
            assert!((r2 - r).abs() < 1e-6, "{r2} vs {r}");
            assert!((b2 - b).abs() < 1e-6, "{b2} vs {b}");
        }
    }

    #[test]
    fn slant_and_ground_are_inverse() {
        for elev in [0.5, 1.5, 6.4, 19.5] {
            for r in [2.0, 50.0, 230.0, 460.0] {
                let s = ground_range_km(r, elev);
                assert!(
                    (slant_range_km(s, elev) - r).abs() < 1e-6,
                    "elev {elev} r {r}"
                );
            }
        }
    }

    /// Hand-computed from eq. 2.28b with kₑa = 8494.7 km (small-angle
    /// expansion h ≈ (r² + 2r·kₑa·sinθ) / 2kₑa): 0.5° at 100 km ≈ 1.461 km,
    /// at 230 km ≈ 5.121 km ARL.
    #[test]
    fn beam_height_matches_reference() {
        assert!((beam_height_km(100.0, 0.5) - 1.461).abs() < 0.01);
        assert!((beam_height_km(230.0, 0.5) - 5.121).abs() < 0.02);
    }

    #[test]
    fn azimuth_index_has_no_holes_for_real_spacing() {
        // 720 radials at 0.5° with ±0.02° jitter, as in super-res sweeps.
        let centers: Vec<f32> = (0..720)
            .map(|i| i as f32 * 0.5 + 0.25 + if i % 3 == 0 { 0.02 } else { -0.02 })
            .collect();
        let idx = azimuth_index(&centers, &vec![0.5; 720]);
        assert!(idx.iter().all(|&v| v != u16::MAX));
        assert_eq!(idx[(10.3 * 10.0) as usize], 20); // 10.3° falls in radial 20 (10.0–10.5°)
    }
}
