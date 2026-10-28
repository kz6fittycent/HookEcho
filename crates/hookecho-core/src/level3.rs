//! Level III decoder (docs/DECODER_PLAN.md: our own, from NOAA ROC ICD
//! 2620001, *ICD for the RPG to Class 1 User*, Rev AD / Build 24.0). For now
//! the 8-bit radial products of the FAA's Terminal Doppler Weather Radars
//! (TDWR), which reach Level III through the Supplemental Product Generator
//! (ICD 2620063): 180 base reflectivity, 182 base velocity and 186 long-range
//! base reflectivity. Other products are reported as unsupported.
//!
//! A product file is an optional WMO/AWIPS text header, then the Message
//! Header Block (9 halfwords) and Product Description Block (51 halfwords,
//! §3.3.1, Figure 3-6), then the Product Symbology Block, bzip2-compressed
//! when PDB halfword 51 (P8) is 1. The symbology block holds one layer with
//! a Digital Radial Data Array packet (code 16, Figure 3-11c).
//!
//! Every read is bounds-checked: product files come from the network.

use crate::level2::{Field, RadialInfo, Sweep, Volume};
use crate::moment::{Moment, RawGates};
use std::io::Read;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("Level III: truncated at byte {0}")]
    Truncated(usize),
    #[error("Level III: product code {0} isn't supported")]
    Unsupported(i16),
    #[error("Level III: {0}")]
    Malformed(String),
}

/// One decoded radial product.
#[derive(Debug, Clone)]
pub struct Product {
    pub code: i16,
    /// The AWIPS identifier from the text header, e.g. "TZ0DAL", if present.
    pub awips_id: Option<String>,
    pub lat: f64,
    pub lon: f64,
    /// Height of the radar above mean sea level, feet (PDB halfword 15).
    pub height_ft: i16,
    /// Volume scan start, ms since the Unix epoch (PDB halfwords 21-23).
    pub volume_time_ms: i64,
    /// Elevation angle, degrees (PDB halfword 30, P3, in tenths).
    pub elevation_deg: f32,
    pub moment: Moment,
    pub radials: Vec<RadialInfo>,
    pub field: Field,
}

/// What a product code means: the moment it carries and its gate spacing.
/// TDWR gates are 150 m (short range, 48 nm) and 300 m (long range,
/// 225 nm): ICD 2620063; cross-checked against MetPy 1.7.1's product table
/// (90 and 416 km over 592 and 1390 bins).
fn product_kind(code: i16) -> Option<(Moment, f32)> {
    match code {
        180 => Some((Moment::Reflectivity, 0.15)),
        182 => Some((Moment::Velocity, 0.15)),
        186 => Some((Moment::Reflectivity, 0.3)),
        _ => None,
    }
}

/// Big-endian reader over a byte slice that never panics.
struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).ok_or(Error::Truncated(self.at))?;
        let s = self.b.get(self.at..end).ok_or(Error::Truncated(self.at))?;
        self.at = end;
        Ok(s)
    }
    fn i16(&mut self) -> Result<i16, Error> {
        Ok(i16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
}

/// The WMO/AWIPS text header some feeds prepend ("SDUS54 KFWD 282108\r\r\n
/// TZ0DAL\r\r\n"): its length and the AWIPS identifier. A product starts
/// with a binary message code instead, so no header means (0, None).
fn text_header(b: &[u8]) -> (usize, Option<String>) {
    // A WMO heading starts with letters; a message code (halfword 1) is a
    // small number, so its first byte is 0.
    if b.first().is_none_or(|c| !c.is_ascii_uppercase()) {
        return (0, None);
    }
    let lines: Vec<usize> = b
        .windows(3)
        .enumerate()
        .filter(|(_, w)| *w == b"\r\r\n")
        .map(|(i, _)| i)
        .take(2)
        .collect();
    match lines.as_slice() {
        [first, second] => {
            let id = std::str::from_utf8(&b[first + 3..*second])
                .ok()
                .map(|s| s.trim().to_string());
            (second + 3, id)
        }
        _ => (0, None),
    }
}

/// Decodes one Level III product file.
pub fn decode(bytes: &[u8]) -> Result<Product, Error> {
    let (skip, awips_id) = text_header(bytes);
    let msg = &bytes[skip..];
    let mut c = Cursor { b: msg, at: 0 };

    // Message Header Block, halfwords 1-9 (Figure 3-3).
    let code = c.i16()?;
    c.bytes(16)?;
    let (moment, gate_km) = product_kind(code).ok_or(Error::Unsupported(code))?;

    // Product Description Block, halfwords 10-60 (Figure 3-6).
    if c.i16()? != -1 {
        return Err(Error::Malformed("no block divider before the PDB".into()));
    }
    let lat = c.i32()? as f64 / 1000.0;
    let lon = c.i32()? as f64 / 1000.0;
    let height_ft = c.i16()?;
    c.bytes(8)?; // 16 product code, 17 mode, 18 VCP, 19 sequence number
    c.bytes(2)?; // 20 volume scan number
    let date = c.u16()? as i64; // 21: days, 1 = 1 January 1970
    let secs = c.i32()? as i64; // 22-23: seconds after midnight UTC
    c.bytes(10)?; // 24-28: generation date/time, P1, P2
    c.bytes(2)?; // 29 elevation number
    let elevation_deg = c.i16()? as f32 / 10.0; // 30: P3
    // 31-33: minimum value and increment (tenths), number of levels.
    let min = c.i16()? as f32 / 10.0;
    let step = c.i16()? as f32 / 10.0;
    c.bytes(2)?;
    if step <= 0.0 {
        return Err(Error::Malformed(format!("data level increment {step}")));
    }
    c.bytes(34)?; // 34-46: thresholds, 47-50: P4-P7
    let compressed = c.u16()? == 1; // 51: P8, 1 = bzip2
    c.bytes(4)?; // 52-53: uncompressed size
    c.bytes(14)?; // 54 version/spot blank, 55-60 block offsets

    let symbology: Vec<u8> = if compressed {
        let mut out = Vec::new();
        bzip2::read::BzDecoder::new(&msg[c.at..])
            .take(MAX_SYMBOLOGY_BYTES)
            .read_to_end(&mut out)
            .map_err(|e| Error::Malformed(format!("bzip2: {e}")))?;
        out
    } else {
        msg[c.at..].to_vec()
    };
    let (mut radials, gates, mut raw) = radial_packet(&symbology)?;
    // Radials come in collection order, starting wherever the antenna was;
    // sort them by azimuth so every product of a scan lines up.
    let mut order: Vec<usize> = (0..radials.len()).collect();
    order.sort_by(|&a, &b| radials[a].0.total_cmp(&radials[b].0));
    raw = order
        .iter()
        .flat_map(|&i| raw[i * gates..(i + 1) * gates].iter().copied())
        .collect();
    radials = order.iter().map(|&i| radials[i]).collect();

    // Codes 0 and 1 are below threshold and range folded; code 2 is `min`
    // and each code above adds `step` (ICD Figure 3-6 Note 1). In Field's
    // terms, value = (raw - offset) / scale.
    let field = Field {
        gates,
        first_gate_km: 0.0,
        gate_km,
        scale: 1.0 / step,
        offset: 2.0 - min / step,
        raw: RawGates::U8(raw),
    };
    let volume_time_ms = ((date - 1) * 86_400 + secs) * 1000;
    let radials = radials
        .into_iter()
        .map(|(start, delta)| RadialInfo {
            azimuth_deg: (start + delta / 2.0).rem_euclid(360.0),
            azimuth_spacing_deg: delta,
            elevation_deg,
            time_ms: volume_time_ms,
        })
        .collect();
    Ok(Product {
        code,
        awips_id,
        lat,
        lon,
        height_ft,
        volume_time_ms,
        elevation_deg,
        moment,
        radials,
        field,
    })
}

/// A decompressed symbology block is at most a few hundred KB (1390 bins ×
/// 360 radials for long-range reflectivity); refuse bzip2 bombs well above.
const MAX_SYMBOLOGY_BYTES: u64 = 8 << 20;

/// (start, delta) per radial in degrees, gates per radial, and the
/// radial-major 8-bit codes, from the symbology block's packet 16.
type Radials = (Vec<(f32, f32)>, usize, Vec<u8>);

fn radial_packet(block: &[u8]) -> Result<Radials, Error> {
    let mut c = Cursor { b: block, at: 0 };
    // Symbology block header (Figure 3-6 sheet 4): divider, block ID 1,
    // block length, number of layers; then the layer divider and length.
    if c.i16()? != -1 || c.i16()? != 1 {
        return Err(Error::Malformed("not a product symbology block".into()));
    }
    c.bytes(4)?;
    c.bytes(2)?;
    if c.i16()? != -1 {
        return Err(Error::Malformed("no layer divider".into()));
    }
    c.bytes(4)?;

    // Digital Radial Data Array packet (Figure 3-11c).
    let packet = c.u16()?;
    if packet != 16 {
        return Err(Error::Malformed(format!(
            "packet code {packet}, expected 16"
        )));
    }
    let first_bin = c.i16()?;
    let bins = c.i16()?;
    c.bytes(6)?; // I and J center of sweep, range scale factor
    let count = c.i16()?;
    if first_bin != 0 || !(1..=1840).contains(&bins) || !(1..=720).contains(&count) {
        return Err(Error::Malformed(format!(
            "{count} radials of {bins} bins from bin {first_bin}"
        )));
    }
    let (bins, count) = (bins as usize, count as usize);
    let mut radials = Vec::with_capacity(count);
    let mut raw = Vec::with_capacity(bins * count);
    for _ in 0..count {
        let len = c.u16()? as usize;
        let start = c.i16()? as f32 / 10.0;
        let delta = c.i16()? as f32 / 10.0;
        let data = c.bytes(len)?;
        // A radial may be padded to a halfword (ICD Figure 3-11c Note 1).
        let levels = data
            .get(..bins)
            .ok_or_else(|| Error::Malformed(format!("radial of {len} bytes, {bins} bins")))?;
        raw.extend_from_slice(levels);
        radials.push((start, delta));
    }
    Ok((radials, bins, raw))
}

/// The products of one TDWR scan as a one-sweep volume: reflectivity and
/// velocity side by side, each field at its own gate spacing. The sweep
/// takes the first product's radials; a later product joins only if its
/// radials have the same azimuths (a sweep shares one set), and replaces an
/// earlier one of the same moment.
pub fn sweep_volume(products: &[Product], site: &str) -> Option<Volume> {
    let first = products.first()?;
    let same_radials = |p: &Product| {
        p.radials.len() == first.radials.len()
            && p.radials
                .iter()
                .zip(&first.radials)
                .all(|(a, b)| (a.azimuth_deg - b.azimuth_deg).abs() < 0.05)
    };
    let mut moments: Vec<(Moment, Field)> = Vec::new();
    for p in products.iter().filter(|p| same_radials(p)) {
        moments.retain(|(m, _)| *m != p.moment);
        moments.push((p.moment, p.field.clone()));
    }
    Some(Volume {
        site: Some(site.to_string()),
        vcp: 0,
        vcp_cuts_deg: vec![first.elevation_deg],
        sweeps: vec![Sweep {
            elevation_number: 1,
            elevation_deg: first.elevation_deg,
            nominal_elevation_deg: Some(first.elevation_deg),
            radials: first.radials.clone(),
            moments,
            complete: true,
        }],
    })
}
