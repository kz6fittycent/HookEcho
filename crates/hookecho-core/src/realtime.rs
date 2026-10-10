//! Real-time Level II volumes assembled from the chunks NOAA uploads to
//! `s3://unidata-nexrad-level2-chunks/` (docs/PROVENANCE.md).
//!
//! Layout (observed and documented by Unidata): `SITE/<volume>/<YYYYMMDD-HHMMSS>-<seq>-<kind>`,
//! where `volume` cycles 1..=999, `seq` starts at 1, and `kind` is `S` (start:
//! the 24-byte Archive II volume header plus the metadata record), `I`
//! (intermediate) or `E` (end). Each `I`/`E` chunk is exactly one LDM record:
//! a 4-byte big-endian length and bzip2 data.
//!
//! Each chunk is decompressed once and appended to an *uncompressed* Archive
//! II stream (the same format older archive files use), so re-decoding the
//! growing volume after every chunk is cheap: no bzip2 work is ever repeated.

use crate::level2::{self, Volume};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum ChunkKind {
    Start,
    Intermediate,
    End,
}

/// A parsed chunk object key.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct ChunkKey {
    pub site: String,
    pub volume: u16,
    /// Volume start time as in the key, `YYYYMMDD-HHMMSS` (UTC).
    pub stamp: String,
    pub seq: u16,
    pub kind: ChunkKind,
}

impl ChunkKey {
    /// Parses `KTLX/276/20261008-140202-001-S`.
    pub fn parse(key: &str) -> Option<ChunkKey> {
        let mut parts = key.split('/');
        let site = parts.next()?.to_string();
        let volume: u16 = parts.next()?.parse().ok()?;
        let name = parts.next()?;
        if parts.next().is_some() || site.len() != 4 || !(1..=999).contains(&volume) {
            return None;
        }
        // YYYYMMDD-HHMMSS-SSS-K
        let b = name.as_bytes();
        if b.len() != 21 || b[8] != b'-' || b[15] != b'-' || b[19] != b'-' {
            return None;
        }
        let stamp = &name[..15];
        if !stamp
            .bytes()
            .enumerate()
            .all(|(i, c)| i == 8 || c.is_ascii_digit())
        {
            return None;
        }
        let seq: u16 = name[16..19].parse().ok()?;
        let kind = match b[20] {
            b'S' => ChunkKind::Start,
            b'I' => ChunkKind::Intermediate,
            b'E' => ChunkKind::End,
            _ => return None,
        };
        (seq >= 1).then_some(ChunkKey {
            site,
            volume,
            stamp: stamp.to_string(),
            seq,
            kind,
        })
    }

    pub fn key(&self) -> String {
        let k = match self.kind {
            ChunkKind::Start => 'S',
            ChunkKind::Intermediate => 'I',
            ChunkKind::End => 'E',
        };
        format!(
            "{}/{}/{}-{:03}-{}",
            self.site, self.volume, self.stamp, self.seq, k
        )
    }

    /// Volume start time, seconds since the Unix epoch.
    pub fn start_time(&self) -> Option<i64> {
        stamp_to_unix(&self.stamp)
    }
}

/// `YYYYMMDD-HHMMSS` (UTC) → Unix seconds.
pub fn stamp_to_unix(s: &str) -> Option<i64> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (
        n(0, 4)?,
        n(4, 6)?,
        n(6, 8)?,
        n(9, 11)?,
        n(11, 13)?,
        n(13, 15)?,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Days from civil (Howard Hinnant, public domain).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// The next volume number in the 1..=999 ring.
pub fn next_volume(v: u16) -> u16 {
    if v >= 999 { 1 } else { v + 1 }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("chunk {0} belongs to a different volume")]
    WrongVolume(String),
    #[error("start chunk is not an Archive II volume header")]
    BadStart,
    #[error("chunk {seq}: {reason}")]
    BadChunk { seq: u16, reason: String },
    #[error(transparent)]
    Decode(#[from] level2::Error),
}

/// One real-time volume being assembled chunk by chunk.
#[derive(Debug)]
pub struct LiveVolume {
    pub site: String,
    pub volume: u16,
    pub stamp: String,
    /// Uncompressed Archive II stream: header, then decompressed records.
    stream: Vec<u8>,
    /// Next sequence number to append.
    next_seq: u16,
    /// Chunks that arrived ahead of a gap, by sequence.
    pending: BTreeMap<u16, (ChunkKind, Vec<u8>)>,
    complete: bool,
}

/// Archive II volume header length (ICD 2620010).
const HEADER: usize = 24;

impl LiveVolume {
    pub fn new(site: &str, volume: u16, stamp: &str) -> LiveVolume {
        LiveVolume {
            site: site.to_string(),
            volume,
            stamp: stamp.to_string(),
            stream: Vec::new(),
            next_seq: 1,
            pending: BTreeMap::new(),
            complete: false,
        }
    }

    pub fn for_key(k: &ChunkKey) -> LiveVolume {
        LiveVolume::new(&k.site, k.volume, &k.stamp)
    }

    /// Chunks appended so far (contiguous from 1).
    pub fn chunks(&self) -> u16 {
        self.next_seq - 1
    }

    /// The end chunk has been appended.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Sequence number this volume needs next.
    pub fn next_seq(&self) -> u16 {
        self.next_seq
    }

    /// Adds a chunk (any order). Returns true if the appended stream grew,
    /// i.e. there's something new to decode.
    pub fn push(&mut self, key: &ChunkKey, data: Vec<u8>) -> Result<bool, Error> {
        if key.site != self.site || key.volume != self.volume || key.stamp != self.stamp {
            return Err(Error::WrongVolume(key.key()));
        }
        if key.seq < self.next_seq {
            return Ok(false); // duplicate
        }
        self.pending.insert(key.seq, (key.kind, data));
        let mut grew = false;
        while let Some((kind, data)) = self.pending.remove(&self.next_seq) {
            self.append(self.next_seq, kind, &data)?;
            self.next_seq += 1;
            grew = true;
            if kind == ChunkKind::End {
                self.complete = true;
                self.pending.clear();
                break;
            }
        }
        Ok(grew)
    }

    fn append(&mut self, seq: u16, kind: ChunkKind, data: &[u8]) -> Result<(), Error> {
        let records = if kind == ChunkKind::Start || seq == 1 {
            if data.len() < HEADER || !data.starts_with(b"AR2V") {
                return Err(Error::BadStart);
            }
            self.stream.extend_from_slice(&data[..HEADER]);
            &data[HEADER..]
        } else {
            data
        };
        // One or more [len][bzip2] LDM records.
        let mut i = 0;
        while i + 4 <= records.len() {
            let len =
                i32::from_be_bytes(records[i..i + 4].try_into().unwrap()).unsigned_abs() as usize;
            let end = i + 4 + len;
            if end > records.len() {
                return Err(Error::BadChunk {
                    seq,
                    reason: format!("record of {len} bytes overruns the chunk"),
                });
            }
            let rec = nexrad_data::volume::Record::from_slice(&records[i..end]);
            let out = rec.decompress().map_err(|e| Error::BadChunk {
                seq,
                reason: e.to_string(),
            })?;
            self.stream.extend_from_slice(out.data());
            i = end;
        }
        Ok(())
    }

    /// Decodes everything received so far. Partial sweeps come back with
    /// `complete == false`.
    pub fn decode(&self) -> Result<Volume, Error> {
        Ok(level2::decode_volume(&self.stream)?)
    }

    /// Bytes held (for memory budgets).
    pub fn bytes(&self) -> usize {
        self.stream.len() + self.pending.values().map(|(_, d)| d.len()).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_reject_garbage() {
        let k = ChunkKey::parse("KTLX/276/20261008-140202-001-S").unwrap();
        assert_eq!((k.volume, k.seq, k.kind), (276, 1, ChunkKind::Start));
        assert_eq!(k.key(), "KTLX/276/20261008-140202-001-S");
        assert_eq!(k.start_time(), Some(1_791_468_122)); // 2026-10-08T14:02:02Z
        for bad in [
            "KTLX/0/20261008-140202-001-S",
            "KTLX/276/20261008-140202-001-X",
            "KTLX/276/x",
            "KTLX/276/20261008-140202-000-I",
            "KTLX/1000/20261008-140202-001-S",
        ] {
            assert!(ChunkKey::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(next_volume(999), 1);
        assert_eq!(stamp_to_unix("19700101-000000"), Some(0));
    }
}
