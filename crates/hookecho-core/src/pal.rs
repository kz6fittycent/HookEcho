//! GRLevelX color tables (`.pal`), after Gibson Ridge's public color table
//! file specification (docs/PROVENANCE.md). A file is `Key: value` lines;
//! `;` starts a comment. Supported:
//!
//! - `Product:` the moment (BR, BV, SW, ZDR, CC, PHI and common aliases)
//! - `Units:` label; for speeds (KTS, MPH, KPH) it also implies `Scale:`
//! - `Scale:` / `Offset:` file values are `data * scale + offset`
//! - `RF:` range-folded color
//! - `Color:` / `Color4:` value, color, and optionally a second color: a
//!   gradient to the next entry (to the second color, if given)
//! - `SolidColor:` / `SolidColor4:` value and a flat color up to the next entry
//!
//! Unknown keys (`Step:`, masks, ...) are ignored so newer files still load.

use crate::Moment;

/// Most entries a table may have; real tables have well under 100.
pub const MAX_ENTRIES: usize = 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// In file units.
    pub value: f32,
    pub color: [u8; 4],
    /// Second color: the gradient ends here instead of at the next entry's color.
    pub end: Option<[u8; 4]>,
    pub solid: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColorTable {
    pub product: Option<Moment>,
    pub units: Option<String>,
    pub scale: f32,
    pub offset: f32,
    pub range_folded: Option<[u8; 4]>,
    /// Ascending by value.
    pub entries: Vec<Entry>,
}

#[derive(Debug, PartialEq)]
pub enum PalError {
    /// 1-based line number and what was wrong.
    Line(usize, String),
    NoColors,
    TooManyEntries,
}

impl std::fmt::Display for PalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PalError::Line(n, why) => write!(f, "line {n}: {why}"),
            PalError::NoColors => write!(f, "no Color lines"),
            PalError::TooManyEntries => write!(f, "more than {MAX_ENTRIES} colors"),
        }
    }
}

impl std::error::Error for PalError {}

/// The moment a `Product:` name refers to.
pub fn product(name: &str) -> Option<Moment> {
    Some(match name.trim().to_ascii_uppercase().as_str() {
        "BR" | "R" | "REF" | "Z" | "DBZ" | "BREF" => Moment::Reflectivity,
        "BV" | "V" | "VEL" | "SRV" | "BVEL" => Moment::Velocity,
        "SW" | "SPW" => Moment::SpectrumWidth,
        "ZDR" | "DR" => Moment::DifferentialReflectivity,
        "CC" | "RHO" | "RHOHV" => Moment::CorrelationCoefficient,
        "PHI" | "PH" | "PHIDP" => Moment::DifferentialPhase,
        _ => return None,
    })
}

/// The `Product:` name written for a moment (None for CFP).
pub fn product_code(m: Moment) -> Option<&'static str> {
    Some(match m {
        Moment::Reflectivity => "BR",
        Moment::Velocity => "BV",
        Moment::SpectrumWidth => "SW",
        Moment::DifferentialReflectivity => "ZDR",
        Moment::CorrelationCoefficient => "CC",
        Moment::DifferentialPhase => "PHI",
        Moment::ClutterFilterPower => return None,
    })
}

/// Speed units relative to m/s (the Level II velocity unit).
fn speed_scale(units: &str) -> Option<f32> {
    Some(match units.trim().to_ascii_uppercase().as_str() {
        "KTS" | "KT" | "KNOTS" => 1.943_844_5,
        "MPH" => 2.236_936_3,
        "KPH" | "KMH" | "KM/H" => 3.6,
        "M/S" | "MPS" => 1.0,
        _ => return None,
    })
}

fn numbers(s: &str, line: usize) -> Result<Vec<f32>, PalError> {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| PalError::Line(line, format!("not a number: {t}")))
        })
        .collect()
}

fn color(n: &[f32], alpha: bool) -> [u8; 4] {
    let b = |v: f32| v.clamp(0.0, 255.0).round() as u8;
    [b(n[0]), b(n[1]), b(n[2]), if alpha { b(n[3]) } else { 255 }]
}

pub fn parse(text: &str) -> Result<ColorTable, PalError> {
    let mut t = ColorTable {
        product: None,
        units: None,
        scale: 1.0,
        offset: 0.0,
        range_folded: None,
        entries: Vec::new(),
    };
    let mut scale = None;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split(';').next().unwrap_or("").trim();
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let rest = rest.trim();
        match key.as_str() {
            // A later unknown name doesn't clear a known one.
            "product" => t.product = product(rest).or(t.product),
            "units" => t.units = Some(rest.to_string()),
            "scale" => {
                let v = numbers(rest, n)?;
                match v.first() {
                    Some(&s) if s > 0.0 => scale = Some(s),
                    _ => return Err(PalError::Line(n, "scale must be positive".into())),
                }
            }
            "offset" => {
                t.offset = *numbers(rest, n)?
                    .first()
                    .ok_or_else(|| PalError::Line(n, "missing offset".into()))?
            }
            "rf" => {
                let v = numbers(rest, n)?;
                if v.len() < 3 {
                    return Err(PalError::Line(n, "RF needs r g b".into()));
                }
                t.range_folded = Some(color(&v, v.len() >= 4));
            }
            "color" | "color4" | "solidcolor" | "solidcolor4" => {
                let alpha = key.ends_with('4');
                let per = if alpha { 4 } else { 3 };
                let v = numbers(rest, n)?;
                if v.len() != 1 + per && v.len() != 1 + 2 * per {
                    return Err(PalError::Line(
                        n,
                        format!("{key} needs a value and {per} color numbers"),
                    ));
                }
                if t.entries.len() == MAX_ENTRIES {
                    return Err(PalError::TooManyEntries);
                }
                t.entries.push(Entry {
                    value: v[0],
                    color: color(&v[1..], alpha),
                    end: (v.len() == 1 + 2 * per).then(|| color(&v[1 + per..], alpha)),
                    solid: key.starts_with("solid"),
                });
            }
            _ => {}
        }
    }
    if t.entries.is_empty() {
        return Err(PalError::NoColors);
    }
    t.scale = scale
        .or_else(|| {
            matches!(
                t.product,
                Some(Moment::Velocity | Moment::SpectrumWidth) | None
            )
            .then(|| t.units.as_deref().and_then(speed_scale))
            .flatten()
        })
        .unwrap_or(1.0);
    t.entries.sort_by(|a, b| a.value.total_cmp(&b.value));
    Ok(t)
}

impl ColorTable {
    /// File value to data units.
    pub fn to_data(&self, v: f32) -> f32 {
        (v - self.offset) / self.scale
    }

    /// Piecewise-linear stops in data units, strictly ascending. A flat or
    /// two-color segment ends just below the next entry, so the jump there
    /// is sharp. Below the first stop is transparent; above the last, the
    /// last color holds.
    pub fn stops(&self) -> Vec<(f32, [u8; 4])> {
        let mut out: Vec<(f32, [u8; 4])> = Vec::new();
        let mut push = |v: f32, c: [u8; 4]| {
            if out.last().is_none_or(|l| v > l.0) {
                out.push((v, c));
            } else if let Some(l) = out.last_mut()
                && v == l.0
            {
                l.1 = c;
            }
        };
        for (i, e) in self.entries.iter().enumerate() {
            let v = self.to_data(e.value);
            push(v, e.color);
            let Some(next) = self.entries.get(i + 1) else {
                continue;
            };
            let nv = self.to_data(next.value);
            let end = if e.solid { Some(e.color) } else { e.end };
            if let Some(end) = end
                && nv > v
            {
                push(nv - (nv - v) * 1e-4, end);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; A test table
Product: BV
Units: KTS
Step: 10
RF: 119 0 125
Color: -64 0 255 255
Color: -10 0 64 0  20 20 20
SolidColor: 0 128 128 128
Color4: 10 255 0 0 128
Color: 64 255 255 0
SomethingNew: 1 2 3
";

    #[test]
    fn parses_header_and_entries() {
        let t = parse(SAMPLE).unwrap();
        assert_eq!(t.product, Some(Moment::Velocity));
        assert_eq!(t.units.as_deref(), Some("KTS"));
        assert!((t.scale - 1.943_844_5).abs() < 1e-6, "KTS implies scale");
        assert_eq!(t.range_folded, Some([119, 0, 125, 255]));
        assert_eq!(t.entries.len(), 5);
        assert_eq!(t.entries[1].end, Some([20, 20, 20, 255]));
        assert!(t.entries[2].solid);
        assert_eq!(t.entries[3].color, [255, 0, 0, 128]);
    }

    #[test]
    fn stops_are_ascending_with_sharp_edges() {
        let t = parse(SAMPLE).unwrap();
        let s = t.stops();
        assert!(s.windows(2).all(|w| w[0].0 < w[1].0), "{s:?}");
        // -64 kts is -32.9 m/s.
        assert!((s[0].0 + 32.92).abs() < 0.01);
        // The two-color segment ends in its second color just below 0.
        let i = s.iter().position(|p| p.1 == [20, 20, 20, 255]).unwrap();
        assert!(s[i].0 < 0.0 && s[i + 1].0 == 0.0);
        // The solid segment holds gray up to just below 10 kts.
        assert_eq!(s[i + 1].1, [128, 128, 128, 255]);
        assert_eq!(s[i + 2].1, [128, 128, 128, 255]);
        assert!(s[i + 2].0 < t.to_data(10.0));
    }

    #[test]
    fn explicit_scale_wins_and_order_is_fixed() {
        let t = parse("Units: MPH\nScale: 2\nColor: 20 1 2 3\nColor: 10 4 5 6\n").unwrap();
        assert_eq!(t.scale, 2.0);
        assert_eq!(t.entries[0].value, 10.0);
        let r = parse("Product: BR\nUnits: dBZ\nColor: 5 1 2 3\n").unwrap();
        assert_eq!(r.scale, 1.0);
    }

    #[test]
    fn product_codes_round_trip() {
        for m in Moment::ALL {
            if let Some(c) = product_code(m) {
                assert_eq!(product(c), Some(m));
            }
        }
        let t = parse("Product: BV\nProduct: XYZ\nColor: 1 2 3 4\n").unwrap();
        assert_eq!(t.product, Some(Moment::Velocity));
    }

    #[test]
    fn errors() {
        assert_eq!(parse("Product: BR\n"), Err(PalError::NoColors));
        assert!(matches!(
            parse("Color: 10 1 2\n"),
            Err(PalError::Line(1, _))
        ));
        assert!(matches!(
            parse("Scale: -1\nColor: 1 2 3 4\n"),
            Err(PalError::Line(1, _))
        ));
        assert!(matches!(
            parse("Color: NaN 1 2 3\n"),
            Err(PalError::Line(1, _))
        ));
        let many: String = (0..=MAX_ENTRIES)
            .map(|i| format!("Color: {i} 1 2 3\n"))
            .collect();
        assert_eq!(parse(&many), Err(PalError::TooManyEntries));
    }
}
