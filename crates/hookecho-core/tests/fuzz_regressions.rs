//! Inputs that crashed a fuzz target once (fuzz/). Each must now be a clean
//! error, never a panic.

fn input(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../tests/data/fuzz-regressions/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn level2_empty_and_short_inputs_are_errors() {
    assert!(hookecho_core::level2::decode_volume(&input("level2-empty.bin")).is_err());
    assert!(hookecho_core::level2::decode_volume(&[]).is_err());
    assert!(hookecho_core::level2::decode_volume(b"AR2V0006.").is_err());
    assert!(
        hookecho_core::level2::decode_volume(&[0x1f, 0x8b]).is_err(),
        "truncated gzip"
    );
}

#[test]
fn basemap_delta_overflow_is_an_error() {
    assert!(hookecho_core::basemap::decode(&input("basemap-overflow.bin")).is_err());
    let _ = hookecho_core::basemap::decode(&input("basemap-neg-overflow.bin"));
}

#[test]
fn basemap_that_decodes_round_trips() {
    // Once decoded fine but re-encoded to something undecodable (huge coords).
    if let Ok(map) = hookecho_core::basemap::decode(&input("basemap-roundtrip.bin")) {
        let again = hookecho_core::basemap::encode(&map);
        assert_eq!(hookecho_core::basemap::decode(&again).unwrap(), map);
    }
}
