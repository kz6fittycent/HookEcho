//! Original Hook Echo color tables (docs/DESIGN.md §7), expressed as gradient
//! control points in product units, baked to a 256-entry LUT indexed by the
//! raw gate byte so the shader does a single texel fetch.

use crate::radar::SweepData;

/// (value, r, g, b, alpha 0..=255). Linear gradient between stops.
type Stop = (f32, u8, u8, u8, u8);

/// Reflectivity, dBZ. Cool and dim below the convective range so the
/// 40–60 dBZ structure (hooks, notches) carries the most luminance contrast.
const REFLECTIVITY: &[Stop] = &[
    (10.0, 44, 86, 122, 0),
    (15.0, 48, 118, 150, 170),
    (20.0, 44, 150, 150, 235),
    (25.0, 46, 172, 118, 255),
    (30.0, 34, 140, 66, 255),
    (34.99, 26, 108, 48, 255),
    (35.0, 238, 214, 64, 255),
    (40.0, 247, 176, 44, 255),
    (45.0, 242, 128, 34, 255),
    (50.0, 226, 62, 42, 255),
    (55.0, 176, 22, 44, 255),
    (60.0, 208, 38, 140, 255),
    (65.0, 236, 120, 226, 255),
    (70.0, 246, 206, 252, 255),
    (75.0, 255, 255, 255, 255),
];

/// Velocity, m/s. Colorblind-safe blue/teal (inbound) vs orange/coral
/// (outbound) around a near-neutral zero.
const VELOCITY: &[Stop] = &[
    (-45.0, 196, 246, 255, 255),
    (-32.0, 40, 206, 232, 255),
    (-20.0, 24, 150, 210, 255),
    (-10.0, 30, 108, 158, 255),
    (-2.5, 46, 66, 82, 255),
    (0.0, 58, 58, 62, 255),
    (2.5, 86, 60, 50, 255),
    (10.0, 168, 78, 48, 255),
    (20.0, 222, 108, 46, 255),
    (32.0, 255, 156, 64, 255),
    (45.0, 255, 232, 176, 255),
];

const RANGE_FOLDED: [u8; 4] = [104, 74, 150, 200];

pub fn stops(moment: crate::radar::Moment) -> &'static [Stop] {
    match moment {
        crate::radar::Moment::Reflectivity => REFLECTIVITY,
        crate::radar::Moment::Velocity => VELOCITY,
    }
}

pub fn sample(stops: &[Stop], v: f32) -> [u8; 4] {
    let first = stops[0];
    if v < first.0 {
        return [0, 0, 0, 0];
    }
    for w in stops.windows(2) {
        let (a, b) = (w[0], w[1]);
        if v >= a.0 && v < b.0 {
            let t = (v - a.0) / (b.0 - a.0);
            let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            return [l(a.1, b.1), l(a.2, b.2), l(a.3, b.3), l(a.4, b.4)];
        }
    }
    let last = stops[stops.len() - 1];
    [last.1, last.2, last.3, last.4]
}

/// sRGB RGBA bytes, 256 entries.
pub fn lut(sweep: &SweepData) -> Vec<u8> {
    let stops = stops(sweep.moment);
    let mut out = Vec::with_capacity(256 * 4);
    for raw in 0..=255u8 {
        let c = match raw {
            0 => [0, 0, 0, 0],
            1 if sweep.moment == crate::radar::Moment::Velocity => RANGE_FOLDED,
            1 => [0, 0, 0, 0],
            _ => sample(stops, sweep.value(raw).unwrap_or(0.0)),
        };
        out.extend_from_slice(&c);
    }
    out
}
