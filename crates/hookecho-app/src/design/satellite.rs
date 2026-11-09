//! Satellite color tables (data colors, the same in both themes), built as
//! the renderer's 256-entry LUT over hookecho_core::goes image values
//! (1 = darkest / warmest, 255 = brightest / coldest; 0 is no data).
//!
//! - Visible: gray, as the satellite sees it.
//! - Infrared: gray for warm surfaces and low cloud, then the usual
//!   "enhanced IR" colors for cold cloud tops from about -30 °C (cyan,
//!   blue, green, yellow, red, then white for the coldest overshooting
//!   tops), as NWS and NESDIS IR loops show them.
//! - Water vapor: dry air brown, moist air white and the coldest, deepest
//!   moisture green to blue, after the common NWS water vapor table.
//!
//! Stops are in °C for the infrared bands.

use hookecho_core::goes::{Band, IR_COLD_K, IR_WARM_K};

type Stop = (f32, [u8; 3]);

const IR: [Stop; 9] = [
    (57.0, [8, 8, 8]),
    (-30.0, [200, 200, 200]),
    (-31.0, [0, 220, 240]),
    (-42.0, [0, 70, 220]),
    (-52.0, [0, 200, 60]),
    (-62.0, [250, 240, 0]),
    (-72.0, [230, 30, 20]),
    (-82.0, [40, 0, 40]),
    (-110.0, [255, 255, 255]),
];

const WV: [Stop; 6] = [
    (-5.0, [70, 40, 10]),
    (-20.0, [170, 120, 60]),
    (-32.0, [235, 235, 235]),
    (-45.0, [110, 190, 255]),
    (-60.0, [30, 160, 70]),
    (-80.0, [240, 250, 255]),
];

/// The image value for a brightness temperature, °C.
fn index_of(c: f32) -> f32 {
    let k = c + 273.15;
    1.0 + ((IR_WARM_K - k) / (IR_WARM_K - IR_COLD_K)).clamp(0.0, 1.0) * 254.0
}

fn ramp(stops: &[Stop], i: f32) -> [u8; 3] {
    let at: Vec<(f32, [u8; 3])> = stops.iter().map(|&(c, rgb)| (index_of(c), rgb)).collect();
    if i <= at[0].0 {
        return at[0].1;
    }
    for w in at.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if i <= b {
            let f = if b > a { (i - a) / (b - a) } else { 1.0 };
            let mix = |k: usize| (ca[k] as f32 + (cb[k] as f32 - ca[k] as f32) * f).round() as u8;
            return [mix(0), mix(1), mix(2)];
        }
    }
    at[at.len() - 1].1
}

/// The LUT for `band`: 256 sRGB RGBA entries.
pub fn lut(band: Band) -> [u8; 1024] {
    let mut out = [0u8; 1024];
    for i in 1..256usize {
        let rgb = match band {
            Band::Visible => {
                let v = ((i - 1) as f32 / 254.0 * 255.0).round() as u8;
                [v, v, v]
            }
            Band::Infrared => ramp(&IR, i as f32),
            Band::WaterVapor => ramp(&WV, i as f32),
        };
        out[i * 4..i * 4 + 3].copy_from_slice(&rgb);
        out[i * 4 + 3] = 255;
    }
    out
}

/// Satellite opacity under the radar.
pub const ALPHA: f32 = 0.9;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables() {
        for b in Band::ALL {
            let l = lut(b);
            assert_eq!(&l[0..4], &[0, 0, 0, 0], "no data is clear");
            assert!(l[4..].chunks(4).all(|c| c[3] == 255));
        }
        let ir = lut(Band::Infrared);
        // Warm ground is near black; -40 °C cloud tops are blue.
        let px = |c: f32| {
            let i = index_of(c).round() as usize * 4;
            [ir[i], ir[i + 1], ir[i + 2]]
        };
        assert!(px(30.0).iter().all(|&v| v < 90), "dark gray");
        let blue = px(-40.0);
        assert!(blue[2] > 200 && blue[0] < 40, "{blue:?}");
    }
}
