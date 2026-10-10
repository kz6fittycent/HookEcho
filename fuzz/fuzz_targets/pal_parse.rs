//! Color tables are user files; parsing and building stops must never panic.
#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data)
        && let Ok(t) = hookecho_core::pal::parse(s)
    {
        let stops = t.stops();
        assert!(stops.windows(2).all(|w| w[0].0 < w[1].0));
    }
});
