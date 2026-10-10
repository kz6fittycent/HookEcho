//! Level II adapter (docs/DECODER_PLAN.md): the only module that touches the
//! `nexrad-*` crates. Everything else in Hook Echo sees these types.

use crate::moment::{Moment, RawGates};
use nexrad_model::data::{CFPMomentData, DataMoment, MomentData, Radial, Scan};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Level II decode: {0}")]
    Decode(String),
}

/// One decoded volume (possibly partial: real-time volumes grow sweep by sweep).
#[derive(Debug, Clone)]
pub struct Volume {
    /// ICAO identifier from the radial data, e.g. "KTLX", when present.
    pub site: Option<String>,
    pub vcp: u16,
    pub sweeps: Vec<Sweep>,
}

#[derive(Debug, Clone)]
pub struct Sweep {
    /// 1-based elevation number within the VCP.
    pub elevation_number: u8,
    /// Mean measured elevation of the radials, degrees.
    pub elevation_deg: f32,
    /// The VCP's commanded angle for this cut (what users call the tilt,
    /// e.g. 0.5°), when the volume carries its VCP (Message 5/7).
    pub nominal_elevation_deg: Option<f32>,
    pub radials: Vec<RadialInfo>,
    pub moments: Vec<(Moment, Field)>,
    /// Whether the last radial carried end-of-elevation (or end-of-volume)
    /// status, i.e. the sweep is complete.
    pub complete: bool,
}

impl Sweep {
    pub fn field(&self, m: Moment) -> Option<&Field> {
        self.moments.iter().find(|(k, _)| *k == m).map(|(_, f)| f)
    }

    /// Start and end collection times, ms since the Unix epoch.
    pub fn time_range_ms(&self) -> Option<(i64, i64)> {
        Some((self.radials.first()?.time_ms, self.radials.last()?.time_ms))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RadialInfo {
    /// Azimuth of the radial center, degrees clockwise from north.
    pub azimuth_deg: f32,
    pub azimuth_spacing_deg: f32,
    pub elevation_deg: f32,
    /// Collection time, ms since the Unix epoch.
    pub time_ms: i64,
}

/// All gates of one moment over one sweep: `radials × gates`, radial-major in
/// file order. Radials without this moment are all zeros (below threshold).
#[derive(Debug, Clone)]
pub struct Field {
    pub gates: usize,
    pub first_gate_km: f32,
    pub gate_km: f32,
    /// Decoded value = (raw - offset) / scale; raw 0 = below threshold,
    /// raw 1 = range folded (ICD 2620002, Message 31 data moment block).
    pub scale: f32,
    pub offset: f32,
    pub raw: RawGates,
}

impl Field {
    pub fn radials(&self) -> usize {
        self.raw.len().checked_div(self.gates).unwrap_or(0)
    }

    pub fn raw_at(&self, radial: usize, gate: usize) -> u16 {
        self.raw.get(radial * self.gates + gate)
    }

    pub fn value(&self, raw: u16) -> Option<f32> {
        (raw > 1).then(|| (raw as f32 - self.offset) / self.scale)
    }
}

/// Operating mode implied by a VCP number. Clear-air VCPs scan slowly for
/// sensitivity (NWS: VCPs 31, 32 and 35); all others are precipitation mode.
pub fn vcp_is_clear_air(vcp: u16) -> bool {
    matches!(vcp, 31 | 32 | 35)
}

/// Archive II volume header length (ICD 2620010): "AR2V00xx." + volume
/// number + date + time + ICAO.
const VOLUME_HEADER_LEN: usize = 24;

/// Decodes an Archive II volume file (gzip-wrapped or not).
///
/// Never panics: input shorter than a volume header is rejected up front
/// (nexrad-data 1.0.0-rc.7 slices past the header unchecked; found by
/// fuzzing), and any other panic inside the decoder crates is converted to
/// an error so a corrupt download can't take the app down.
pub fn decode_volume(bytes: &[u8]) -> Result<Volume, Error> {
    let gzip = bytes.starts_with(&[0x1f, 0x8b]);
    if !gzip && bytes.len() < VOLUME_HEADER_LEN {
        return Err(Error::Decode(format!(
            "{} bytes is shorter than an Archive II header",
            bytes.len()
        )));
    }
    std::panic::catch_unwind(|| {
        let file = nexrad_data::volume::File::new(bytes.to_vec())
            .decompress()
            .map_err(|e| Error::Decode(e.to_string()))?;
        if file.data().len() < VOLUME_HEADER_LEN {
            return Err(Error::Decode(
                "decompressed volume shorter than an Archive II header".into(),
            ));
        }
        let scan = file.scan().map_err(|e| Error::Decode(e.to_string()))?;
        Ok(convert(&scan))
    })
    .unwrap_or_else(|_| {
        Err(Error::Decode(
            "malformed Level II data (decoder panicked)".into(),
        ))
    })
}

fn convert(scan: &Scan) -> Volume {
    let cuts: Vec<f32> = scan
        .coverage_pattern()
        .elevation_cuts()
        .iter()
        .map(|c| c.elevation_angle_degrees() as f32)
        .collect();
    let sweeps = scan
        .sweeps()
        .iter()
        .map(|s| {
            let radials = s.radials();
            let infos: Vec<RadialInfo> = radials
                .iter()
                .map(|r| RadialInfo {
                    azimuth_deg: r.azimuth_angle_degrees(),
                    azimuth_spacing_deg: r.azimuth_spacing_degrees(),
                    elevation_deg: r.elevation_angle_degrees(),
                    time_ms: r.collection_timestamp(),
                })
                .collect();
            let elevation_deg = if infos.is_empty() {
                0.0
            } else {
                infos.iter().map(|r| r.elevation_deg).sum::<f32>() / infos.len() as f32
            };
            let complete = radials.last().is_some_and(|r| {
                use nexrad_model::data::RadialStatus::*;
                matches!(r.radial_status(), ElevationEnd | ScanEnd)
            });
            let moments = Moment::ALL
                .iter()
                .filter_map(|&m| field(radials, m).map(|f| (m, f)))
                .collect();
            Sweep {
                elevation_number: s.elevation_number(),
                elevation_deg,
                nominal_elevation_deg: cuts
                    .get((s.elevation_number() as usize).wrapping_sub(1))
                    .copied(),
                radials: infos,
                moments,
                complete,
            }
        })
        .collect();
    Volume {
        site: scan.site().map(|s| s.identifier_string()),
        vcp: scan.coverage_pattern_number().number(),
        sweeps,
    }
}

/// Geometry and raw bytes common to `MomentData` and `CFPMomentData`.
struct Block<'a> {
    gates: usize,
    first_gate_km: f32,
    gate_km: f32,
    word_size: u8,
    scale: f32,
    offset: f32,
    bytes: &'a [u8],
}

fn block_of<T: DataMoment>(m: &T) -> Block<'_> {
    Block {
        gates: m.gate_count() as usize,
        first_gate_km: m.first_gate_range_km() as f32,
        gate_km: m.gate_interval_km() as f32,
        word_size: m.data_word_size(),
        scale: m.scale(),
        offset: m.offset(),
        bytes: m.raw_values(),
    }
}

fn moment_block(r: &Radial, m: Moment) -> Option<Block<'_>> {
    let data: Option<&MomentData> = match m {
        Moment::Reflectivity => r.reflectivity(),
        Moment::Velocity => r.velocity(),
        Moment::SpectrumWidth => r.spectrum_width(),
        Moment::DifferentialReflectivity => r.differential_reflectivity(),
        Moment::DifferentialPhase => r.differential_phase(),
        Moment::CorrelationCoefficient => r.correlation_coefficient(),
        Moment::ClutterFilterPower => {
            let cfp: Option<&CFPMomentData> = r.clutter_filter_power();
            return cfp.map(block_of);
        }
    };
    data.map(block_of)
}

fn field(radials: &[Radial], m: Moment) -> Option<Field> {
    let first = radials.iter().find_map(|r| moment_block(r, m))?;
    let gates = radials
        .iter()
        .filter_map(|r| moment_block(r, m))
        .map(|b| b.gates)
        .max()?;
    let sixteen = first.word_size == 16;
    let mut raw = if sixteen {
        RawGates::U16(vec![0; radials.len() * gates])
    } else {
        RawGates::U8(vec![0; radials.len() * gates])
    };
    for (row, r) in radials.iter().enumerate() {
        let Some(b) = moment_block(r, m) else {
            continue;
        };
        let base = row * gates;
        match &mut raw {
            RawGates::U8(v) => v[base..base + b.bytes.len().min(gates)]
                .copy_from_slice(&b.bytes[..b.bytes.len().min(gates)]),
            RawGates::U16(v) => {
                // 16-bit moments are big-endian on the wire (ICD 2620002, Message 31).
                for (k, pair) in b.bytes.as_chunks::<2>().0.iter().take(gates).enumerate() {
                    v[base + k] = u16::from_be_bytes([pair[0], pair[1]]);
                }
            }
        }
    }
    Some(Field {
        gates,
        first_gate_km: first.first_gate_km,
        gate_km: first.gate_km,
        scale: first.scale,
        offset: first.offset,
        raw,
    })
}
