//! Radar color tables (docs/DESIGN.md §7). Our own originals; gradient stops
//! in product units, baked into a 1024-entry LUT over [min, max]. Phase 2 adds
//! the alternates and GRLevelX `.pal` import on top of this type.

use hookecho_core::Moment;

pub const LUT_SIZE: usize = 1024;

/// (name, stops, range-folded color).
type Definition = (&'static str, Vec<(f32, [u8; 4])>, [u8; 4]);

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub name: String,
    /// (value, [r, g, b, a]) in product units, ascending. Linear gradient.
    pub stops: Vec<(f32, [u8; 4])>,
    /// Color for range-folded gates; alpha 0 hides them.
    pub range_folded: [u8; 4],
}

const fn c(r: u8, g: u8, b: u8) -> [u8; 4] {
    [r, g, b, 255]
}

impl Palette {
    pub fn min(&self) -> f32 {
        self.stops.first().map_or(0.0, |s| s.0)
    }

    pub fn max(&self) -> f32 {
        self.stops.last().map_or(1.0, |s| s.0)
    }

    /// Color at `v` (transparent below the first stop, clamped above the last).
    pub fn sample(&self, v: f32) -> [u8; 4] {
        let Some(first) = self.stops.first() else {
            return [0; 4];
        };
        if v < first.0 {
            return [0; 4];
        }
        for w in self.stops.windows(2) {
            let ((va, a), (vb, b)) = (w[0], w[1]);
            if v >= va && v < vb {
                let t = (v - va) / (vb - va);
                return std::array::from_fn(|i| {
                    (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8
                });
            }
        }
        self.stops.last().unwrap().1
    }

    /// sRGB RGBA bytes, LUT_SIZE entries spanning [min, max].
    pub fn lut(&self) -> Vec<u8> {
        let (lo, hi) = (self.min(), self.max());
        (0..LUT_SIZE)
            .flat_map(|i| self.sample(lo + (hi - lo) * i as f32 / (LUT_SIZE - 1) as f32))
            .collect()
    }

    /// The default ("Hook Echo Standard") palette for a moment.
    pub fn default_for(m: Moment) -> Palette {
        let (name, stops, rf): Definition = match m {
            // Cool and dim below the convective range so the 40-60 dBZ
            // structure carries the most luminance contrast; < 10 dBZ hidden.
            Moment::Reflectivity => (
                "Hook Echo Standard",
                vec![
                    (10.0, [44, 86, 122, 0]),
                    (15.0, [48, 118, 150, 170]),
                    (20.0, [44, 150, 150, 235]),
                    (25.0, c(46, 172, 118)),
                    (30.0, c(34, 140, 66)),
                    (34.99, c(26, 108, 48)),
                    (35.0, c(238, 214, 64)),
                    (40.0, c(247, 176, 44)),
                    (45.0, c(242, 128, 34)),
                    (50.0, c(226, 62, 42)),
                    (55.0, c(176, 22, 44)),
                    (60.0, c(208, 38, 140)),
                    (65.0, c(236, 120, 226)),
                    (70.0, c(246, 206, 252)),
                    (75.0, c(255, 255, 255)),
                ],
                [0; 4],
            ),
            // Colorblind-safe diverging: teal/blue inbound, orange/coral outbound.
            Moment::Velocity => (
                "Hook Echo Diverging",
                vec![
                    (-45.0, c(196, 246, 255)),
                    (-32.0, c(40, 206, 232)),
                    (-20.0, c(24, 150, 210)),
                    (-10.0, c(30, 108, 158)),
                    (-2.5, c(46, 66, 82)),
                    (0.0, c(58, 58, 62)),
                    (2.5, c(86, 60, 50)),
                    (10.0, c(168, 78, 48)),
                    (20.0, c(222, 108, 46)),
                    (32.0, c(255, 156, 64)),
                    (45.0, c(255, 232, 176)),
                ],
                [104, 74, 150, 200],
            ),
            // Plasma-like ramp (after matplotlib "plasma", CC0).
            Moment::SpectrumWidth => (
                "Hook Echo Plasma",
                vec![
                    (0.0, [13, 8, 135, 0]),
                    (1.0, c(13, 8, 135)),
                    (4.0, c(126, 3, 168)),
                    (8.0, c(204, 71, 120)),
                    (12.0, c(248, 149, 64)),
                    (16.0, c(240, 249, 33)),
                ],
                [104, 74, 150, 200],
            ),
            Moment::DifferentialReflectivity => (
                "Hook Echo ZDR",
                vec![
                    (-4.0, c(84, 72, 140)),
                    (-1.0, c(122, 134, 168)),
                    (0.0, c(178, 182, 188)),
                    (1.0, c(104, 186, 196)),
                    (2.0, c(232, 218, 64)),
                    (3.0, c(246, 152, 42)),
                    (4.5, c(222, 54, 54)),
                    (6.0, c(232, 104, 204)),
                    (8.0, c(255, 255, 255)),
                ],
                [0; 4],
            ),
            // Low CC (debris, non-meteorological) in cool colors that stand
            // apart from the warm 0.95-1.0 rain range.
            Moment::CorrelationCoefficient => (
                "Hook Echo CC",
                vec![
                    (0.20, c(36, 36, 56)),
                    (0.45, c(62, 72, 160)),
                    (0.70, c(42, 150, 192)),
                    (0.85, c(110, 196, 170)),
                    (0.90, c(232, 220, 64)),
                    (0.95, c(244, 144, 42)),
                    (0.98, c(214, 44, 44)),
                    (1.00, c(240, 122, 230)),
                    (1.05, c(255, 255, 255)),
                ],
                [0; 4],
            ),
            // Inferno-like ramp (after matplotlib "inferno", CC0).
            Moment::DifferentialPhase => (
                "Hook Echo Inferno",
                vec![
                    (0.0, c(0, 0, 4)),
                    (72.0, c(87, 16, 110)),
                    (144.0, c(188, 55, 84)),
                    (216.0, c(249, 142, 9)),
                    (288.0, c(252, 255, 164)),
                    (360.0, c(255, 255, 255)),
                ],
                [0; 4],
            ),
            Moment::ClutterFilterPower => (
                "Grayscale",
                vec![(-20.0, c(30, 30, 30)), (40.0, c(235, 235, 235))],
                [0; 4],
            ),
        };
        Palette {
            name: name.into(),
            stops,
            range_folded: rf,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_moment_has_a_valid_default() {
        for m in Moment::ALL {
            let p = Palette::default_for(m);
            assert!(
                p.stops.windows(2).all(|w| w[0].0 < w[1].0),
                "{m:?} stops not ascending"
            );
            assert_eq!(p.lut().len(), LUT_SIZE * 4);
        }
    }

    #[test]
    fn sampling() {
        let p = Palette::default_for(Moment::Reflectivity);
        assert_eq!(p.sample(5.0), [0; 4], "below 10 dBZ is hidden");
        assert_eq!(p.sample(35.0), [238, 214, 64, 255]);
        assert_eq!(p.sample(90.0), [255, 255, 255, 255]);
    }
}
