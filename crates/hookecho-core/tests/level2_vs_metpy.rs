//! Level II decoding checked gate-by-gate against MetPy on real samples.
//! References come from tools/verify/dump_metpy.py (committed JSON).

use hookecho_core::Moment;
use hookecho_core::level2::{Volume, decode_volume};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn data(name: &str) -> Vec<u8> {
    let p = format!("{}/../../tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn reference(name: &str) -> Value {
    serde_json::from_slice(&data(name)).unwrap()
}

fn moment(code: &str) -> Moment {
    *Moment::ALL
        .iter()
        .find(|m| m.code() == code)
        .unwrap_or_else(|| panic!("unknown moment {code}"))
}

fn check(volume: &Volume, reference: &Value) {
    let sweeps = reference["sweeps"].as_array().unwrap();
    assert_eq!(volume.sweeps.len(), sweeps.len(), "sweep count");
    for (i, (ours, theirs)) in volume.sweeps.iter().zip(sweeps).enumerate() {
        assert_eq!(
            ours.radials.len() as u64,
            theirs["radials"].as_u64().unwrap(),
            "sweep {i} radials"
        );
        for (k, (r, (az, el))) in ours
            .radials
            .iter()
            .zip(
                theirs["azimuth"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(theirs["elevation"].as_array().unwrap()),
            )
            .enumerate()
        {
            assert!(
                (r.azimuth_deg as f64 - az.as_f64().unwrap()).abs() < 1e-3,
                "sweep {i} radial {k} azimuth"
            );
            assert!(
                (r.elevation_deg as f64 - el.as_f64().unwrap()).abs() < 1e-3,
                "sweep {i} radial {k} elevation"
            );
        }
        let theirs_m = theirs["moments"].as_object().unwrap();
        assert_eq!(ours.moments.len(), theirs_m.len(), "sweep {i} moment set");
        for (code, m) in theirs_m {
            let f = ours
                .field(moment(code))
                .unwrap_or_else(|| panic!("sweep {i} missing {code}"));
            let ctx = format!("sweep {i} {code}");
            assert_eq!(f.gates as u64, m["gates"].as_u64().unwrap(), "{ctx} gates");
            assert_eq!(
                f.raw.word_size() as u64,
                m["word_size"].as_u64().unwrap(),
                "{ctx} word size"
            );
            assert!(
                (f.first_gate_km as f64 - m["first_gate_km"].as_f64().unwrap()).abs() < 1e-6,
                "{ctx} first gate"
            );
            assert!(
                (f.gate_km as f64 - m["gate_km"].as_f64().unwrap()).abs() < 1e-6,
                "{ctx} gate width"
            );
            assert!(
                (f.scale as f64 - m["scale"].as_f64().unwrap()).abs() < 1e-4,
                "{ctx} scale"
            );
            assert!(
                (f.offset as f64 - m["offset"].as_f64().unwrap()).abs() < 1e-4,
                "{ctx} offset"
            );
            for spot in m["spot"].as_array().unwrap() {
                let s: Vec<u64> = spot
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect();
                assert_eq!(
                    f.raw_at(s[0] as usize, s[1] as usize) as u64,
                    s[2],
                    "{ctx} raw at radial {} gate {}",
                    s[0],
                    s[1]
                );
            }
            let bytes: Vec<u8> = f
                .raw
                .to_u16()
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(
                digest,
                m["sha256"].as_str().unwrap(),
                "{ctx}: raw gate codes differ from MetPy"
            );
        }
    }
}

#[test]
fn ktlx_2013_uncompressed_gzip_matches_metpy() {
    let v = decode_volume(&data("KTLX20130520_201643_3sweeps.ar2.gz")).unwrap();
    assert_eq!(v.vcp, 12);
    check(&v, &reference("KTLX20130520_201643_3sweeps.metpy.json"));
}

#[test]
fn ktlx_2024_ldm_bzip2_matches_metpy() {
    let v = decode_volume(&data("KTLX20240428_031312_3sweeps.ar2")).unwrap();
    check(&v, &reference("KTLX20240428_031312_3sweeps.metpy.json"));
    assert!(
        v.sweeps.iter().all(|s| s.complete),
        "all three sweeps end with end-of-elevation"
    );
}

/// Physical sanity on decoded values: the Moore debris ball (2013-05-20
/// 20:16 UTC, ~22 km west of KTLX) should exceed 65 dBZ at 0.5°.
#[test]
fn moore_debris_ball_is_present() {
    let v = decode_volume(&data("KTLX20130520_201643_3sweeps.ar2.gz")).unwrap();
    let s = &v.sweeps[0];
    let f = s.field(Moment::Reflectivity).unwrap();
    let max = (0..s.radials.len())
        .filter(|&r| (255.0..280.0).contains(&s.radials[r].azimuth_deg))
        .flat_map(|r| (0..((30.0 - f.first_gate_km) / f.gate_km) as usize).map(move |g| (r, g)))
        .filter_map(|(r, g)| f.value(f.raw_at(r, g)))
        .fold(f32::MIN, f32::max);
    assert!(max > 65.0, "max reflectivity near Moore was {max}");
}
