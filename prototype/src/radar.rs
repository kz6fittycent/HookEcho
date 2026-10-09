//! Level II sweep extraction for the prototype, via the `nexrad` crates.

use nexrad_model::data::{DataMoment, MomentData, Radial, Scan};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Moment {
    Reflectivity,
    Velocity,
}

/// Number of azimuth bins in the lookup table (0.1° each).
pub const AZ_BINS: usize = 3600;

/// One sweep of one moment, ready for upload: raw gate bytes per radial plus
/// an azimuth→row lookup so the shader never needs sorted radials.
pub struct SweepData {
    pub moment: Moment,
    pub elevation_deg: f32,
    pub radials: usize,
    pub gates: usize,
    pub first_gate_km: f32,
    pub gate_km: f32,
    pub scale: f32,
    pub offset: f32,
    /// `radials * gates` raw 8-bit values (0 = below threshold, 1 = range folded).
    pub raw: Vec<u8>,
    /// AZ_BINS entries; u16::MAX = no radial.
    pub az_lookup: Vec<u16>,
}

impl SweepData {
    /// Raw value at ground range `ground_km` and azimuth `az_deg`.
    pub fn raw_at(&self, ground_km: f32, az_deg: f32) -> Option<u8> {
        let slant = ground_km / self.elevation_deg.to_radians().cos();
        let gate = ((slant - self.first_gate_km) / self.gate_km + 0.5).floor();
        if gate < 0.0 || gate as usize >= self.gates {
            return None;
        }
        let bin = ((az_deg.rem_euclid(360.0) / 0.1) as usize).min(AZ_BINS - 1);
        let row = self.az_lookup[bin];
        if row == u16::MAX {
            return None;
        }
        Some(self.raw[row as usize * self.gates + gate as usize])
    }

    pub fn value(&self, raw: u8) -> Option<f32> {
        (raw > 1).then(|| (raw as f32 - self.offset) / self.scale)
    }
}

pub struct Volume {
    pub scan: Scan,
}

impl Volume {
    pub fn load(path: &std::path::Path) -> Result<Volume, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = nexrad_data::volume::File::new(bytes)
            .decompress()
            .map_err(|e| e.to_string())?;
        let scan = file.scan().map_err(|e| e.to_string())?;
        Ok(Volume { scan })
    }

    /// The lowest sweep that carries `moment` (the split-cut Doppler sweep for velocity).
    pub fn lowest(&self, moment: Moment) -> Option<SweepData> {
        let pick = |r: &Radial| -> Option<MomentData> {
            match moment {
                Moment::Reflectivity => r.reflectivity().cloned(),
                Moment::Velocity => r.velocity().cloned(),
            }
        };
        let sweep = self
            .scan
            .sweeps()
            .iter()
            .find(|s| s.radials().first().and_then(&pick).is_some())?;
        let radials = sweep.radials();
        let first = pick(&radials[0])?;
        let gates = radials
            .iter()
            .filter_map(|r| pick(r).map(|m| m.gate_count() as usize))
            .max()?;
        let mut raw = vec![0u8; radials.len() * gates];
        let mut az_lookup = vec![u16::MAX; AZ_BINS];
        for (row, r) in radials.iter().enumerate() {
            let Some(m) = pick(r) else { continue };
            if m.data_word_size() != 8 {
                continue; // 16-bit moments (e.g. PHI) are out of prototype scope.
            }
            let vals = m.raw_values();
            raw[row * gates..row * gates + vals.len()].copy_from_slice(vals);
            // Prototype assumption: the Message 31 azimuth is the radial *center*.
            // Phase 1 must confirm this against ICD 2620002 and MetPy before relying on it.
            let half = r.azimuth_spacing_degrees() / 2.0;
            let start = ((r.azimuth_angle_degrees() - half) / 0.1).round() as i32;
            let end = ((r.azimuth_angle_degrees() + half) / 0.1).round() as i32;
            for b in start..end {
                az_lookup[b.rem_euclid(AZ_BINS as i32) as usize] = row as u16;
            }
        }
        // Radial centers jitter, so rounded spans can leave 0.1° holes between
        // neighbors. Fill each hole from the nearest radial within half a degree.
        let filled = az_lookup.clone();
        for b in 0..AZ_BINS {
            if filled[b] != u16::MAX {
                continue;
            }
            for d in 1..=5i32 {
                let l = filled[(b as i32 - d).rem_euclid(AZ_BINS as i32) as usize];
                let r = filled[(b as i32 + d).rem_euclid(AZ_BINS as i32) as usize];
                if l != u16::MAX || r != u16::MAX {
                    az_lookup[b] = if l != u16::MAX { l } else { r };
                    break;
                }
            }
        }
        Some(SweepData {
            moment,
            elevation_deg: sweep.elevation_angle_degrees().unwrap_or(0.5),
            radials: radials.len(),
            gates,
            first_gate_km: first.first_gate_range_km() as f32,
            gate_km: first.gate_interval_km() as f32,
            scale: first.scale(),
            offset: first.offset(),
            raw,
            az_lookup,
        })
    }
}

/// Beam height above radar level, 4/3 effective earth radius
/// (Doviak & Zrnić 1993, eq. 2.28b).
pub fn beam_height_km(slant_km: f32, elev_deg: f32) -> f32 {
    let ke_a = 4.0 / 3.0 * 6371.0_f32;
    let e = elev_deg.to_radians();
    (slant_km * slant_km + ke_a * ke_a + 2.0 * slant_km * ke_a * e.sin()).sqrt() - ke_a
}
