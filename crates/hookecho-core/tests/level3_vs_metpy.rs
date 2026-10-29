//! Level III (TDWR) decoding checked against MetPy on real products from
//! Dallas Love Field (TDAL). References come from
//! tools/verify/dump_metpy_level3.py (committed JSON).

use hookecho_core::Moment;
use hookecho_core::level3::{Error, Product, decode, sweep_volume};
use serde_json::Value;
use sha2::{Digest, Sha256};

const SCAN: &str = "2026_10_28_21_08_23";

fn data(name: &str) -> Vec<u8> {
    let p = format!(
        "{}/../../tests/data/level3/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn product(id: &str) -> (Product, Value) {
    let name = format!("DAL_{id}_{SCAN}");
    let p = decode(&data(&name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let r = serde_json::from_slice(&data(&format!("{name}.metpy.json"))).unwrap();
    (p, r)
}

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b}");
}

fn check(id: &str, moment: Moment, gate_km: f32) {
    let (p, r) = product(id);
    assert_eq!(p.code as i64, r["code"].as_i64().unwrap(), "{id} code");
    assert_eq!(p.moment, moment, "{id} moment");
    assert_eq!(p.awips_id.as_deref(), Some(&*format!("{id}DAL")));
    close(p.lat, r["lat"].as_f64().unwrap(), 1e-6, "lat");
    close(p.lon, r["lon"].as_f64().unwrap(), 1e-6, "lon");
    assert_eq!(p.height_ft as i64, r["height_ft"].as_i64().unwrap());
    close(
        p.elevation_deg as f64,
        r["elevation_deg"].as_f64().unwrap(),
        1e-6,
        "elevation",
    );
    // 2026-10-28 21:08:23 UTC.
    assert_eq!(p.volume_time_ms, 1_793_221_703_000, "{id} volume time");
    assert_eq!(r["volume_time"], "2026-10-28T21:08:23Z");

    let f = &p.field;
    assert_eq!(f.gates as u64, r["gates"].as_u64().unwrap(), "{id} gates");
    assert_eq!(f.gate_km, gate_km);
    assert_eq!(p.radials.len() as u64, r["radials"].as_u64().unwrap());
    close(
        f.value(2).unwrap() as f64,
        r["value_at_2"].as_f64().unwrap(),
        1e-4,
        "value at code 2",
    );
    close(
        f.value(255).unwrap() as f64,
        r["value_at_255"].as_f64().unwrap(),
        1e-4,
        "value at code 255",
    );
    assert_eq!(f.value(0), None, "code 0 is below threshold");
    assert_eq!(f.value(1), None, "code 1 is range folded");

    for (k, (rad, (start, delta))) in p
        .radials
        .iter()
        .zip(
            r["start_az"]
                .as_array()
                .unwrap()
                .iter()
                .zip(r["delta_az"].as_array().unwrap()),
        )
        .enumerate()
    {
        let (start, delta) = (start.as_f64().unwrap(), delta.as_f64().unwrap());
        close(rad.azimuth_spacing_deg as f64, delta, 0.011, "delta");
        close(
            rad.azimuth_deg as f64,
            (start + delta / 2.0).rem_euclid(360.0),
            0.011,
            &format!("{id} radial {k} azimuth"),
        );
    }
    let codes: Vec<u8> = f.raw.to_u16().iter().map(|&c| c as u8).collect();
    let hash: String = Sha256::digest(&codes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hash, r["raw_sha256"].as_str().unwrap(), "{id} raw codes");
}

#[test]
fn short_range_reflectivity() {
    check("TZ0", Moment::Reflectivity, 0.15);
}

#[test]
fn long_range_reflectivity() {
    check("TZL", Moment::Reflectivity, 0.3);
}

#[test]
fn velocity() {
    check("TV0", Moment::Velocity, 0.15);
}

/// TZ0 and TV0 share a radial layout, so they combine into one sweep the
/// renderer can draw like a Level II tilt.
#[test]
fn scan_as_sweep() {
    let (z, _) = product("TZ0");
    let (v, _) = product("TV0");
    let vol = sweep_volume(&[z, v], "TDAL").unwrap();
    assert_eq!(vol.site.as_deref(), Some("TDAL"));
    let s = &vol.sweeps[0];
    assert_eq!(s.radials.len(), 360);
    assert!(s.field(Moment::Reflectivity).is_some() && s.field(Moment::Velocity).is_some());
    assert!(s.complete);
}

#[test]
fn rejects_what_it_cannot_read() {
    let tz0 = data(&format!("DAL_TZ0_{SCAN}"));
    assert!(matches!(decode(&tz0[..200]), Err(Error::Malformed(_))));
    assert!(matches!(decode(&tz0[..40]), Err(Error::Truncated(_))));
    assert_eq!(decode(&[]).unwrap_err(), Error::Truncated(0));
    let mut other = tz0.clone();
    other[30..32].copy_from_slice(&94i16.to_be_bytes()); // a WSR-88D product
    assert_eq!(decode(&other).unwrap_err(), Error::Unsupported(94));
}
