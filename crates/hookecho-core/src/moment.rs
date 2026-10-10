//! Radar moments and their raw gate storage.

/// Level II Message 31 moments (ICD 2620002). KDP is not among them; it
/// comes from Level III product 163 (docs/DECODER_PLAN.md).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Moment {
    Reflectivity,
    Velocity,
    SpectrumWidth,
    DifferentialReflectivity,
    DifferentialPhase,
    CorrelationCoefficient,
    ClutterFilterPower,
}

impl Moment {
    pub const ALL: [Moment; 7] = [
        Moment::Reflectivity,
        Moment::Velocity,
        Moment::SpectrumWidth,
        Moment::DifferentialReflectivity,
        Moment::DifferentialPhase,
        Moment::CorrelationCoefficient,
        Moment::ClutterFilterPower,
    ];

    /// The data block name in Message 31 ("REF", "VEL", ...).
    pub fn code(self) -> &'static str {
        match self {
            Moment::Reflectivity => "REF",
            Moment::Velocity => "VEL",
            Moment::SpectrumWidth => "SW",
            Moment::DifferentialReflectivity => "ZDR",
            Moment::DifferentialPhase => "PHI",
            Moment::CorrelationCoefficient => "RHO",
            Moment::ClutterFilterPower => "CFP",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Moment::Reflectivity => "Reflectivity",
            Moment::Velocity => "Velocity",
            Moment::SpectrumWidth => "Spectrum width",
            Moment::DifferentialReflectivity => "Differential reflectivity",
            Moment::DifferentialPhase => "Differential phase",
            Moment::CorrelationCoefficient => "Correlation coefficient",
            Moment::ClutterFilterPower => "Clutter filter power",
        }
    }

    /// Short name for chips and pickers.
    pub fn short_name(self) -> &'static str {
        match self {
            Moment::Reflectivity => "Reflectivity",
            Moment::Velocity => "Velocity",
            Moment::SpectrumWidth => "Spectrum width",
            Moment::DifferentialReflectivity => "ZDR",
            Moment::DifferentialPhase => "PHI",
            Moment::CorrelationCoefficient => "CC",
            Moment::ClutterFilterPower => "CFP",
        }
    }

    /// Native units after scale/offset decoding.
    pub fn unit(self) -> &'static str {
        match self {
            Moment::Reflectivity => "dBZ",
            Moment::Velocity | Moment::SpectrumWidth => "m/s",
            Moment::DifferentialReflectivity => "dB",
            Moment::DifferentialPhase => "°",
            Moment::CorrelationCoefficient => "",
            Moment::ClutterFilterPower => "dB",
        }
    }

    /// Products a user can pick (CFP is a diagnostic, not shown by default).
    pub fn user_facing(self) -> bool {
        self != Moment::ClutterFilterPower
    }
}

/// Raw gate codes, 8- or 16-bit as transmitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawGates {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl RawGates {
    pub fn len(&self) -> usize {
        match self {
            RawGates::U8(v) => v.len(),
            RawGates::U16(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, i: usize) -> u16 {
        match self {
            RawGates::U8(v) => v[i] as u16,
            RawGates::U16(v) => v[i],
        }
    }

    /// How many gates hold each raw value (index = raw value).
    pub fn histogram(&self) -> Vec<u32> {
        let mut h = vec![0u32; if self.word_size() == 8 { 256 } else { 65536 }];
        match self {
            RawGates::U8(v) => v.iter().for_each(|&r| h[r as usize] += 1),
            RawGates::U16(v) => v.iter().for_each(|&r| h[r as usize] += 1),
        }
        h
    }

    pub fn word_size(&self) -> u8 {
        match self {
            RawGates::U8(_) => 8,
            RawGates::U16(_) => 16,
        }
    }

    /// Codes widened to u16 (for uniform GPU upload).
    pub fn to_u16(&self) -> Vec<u16> {
        match self {
            RawGates::U8(v) => v.iter().map(|&b| b as u16).collect(),
            RawGates::U16(v) => v.clone(),
        }
    }
}
