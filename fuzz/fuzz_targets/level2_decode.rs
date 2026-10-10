//! Arbitrary bytes into the Level II path (nexrad-data/nexrad-decode and our
//! adapter): must return Ok or Err, never panic, hang or overflow.
#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(v) = hookecho_core::level2::decode_volume(data) {
        for s in &v.sweeps {
            for (_, f) in &s.moments {
                // Touch every gate through the public accessors.
                for r in 0..f.radials() {
                    for g in 0..f.gates {
                        let _ = f.value(f.raw_at(r, g));
                    }
                }
            }
        }
    }
});
