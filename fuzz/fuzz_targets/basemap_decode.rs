//! The bundled basemap is trusted, but `radar-data` releases update it at
//! runtime (refresh-radar-data.yml), so the decoder must reject garbage safely.
#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(map) = hookecho_core::basemap::decode(data) {
        // Re-encoding a decoded map must round-trip.
        let again = hookecho_core::basemap::encode(&map);
        assert_eq!(hookecho_core::basemap::decode(&again).unwrap(), map);
    }
});
