//! Arbitrary bytes into the Level III decoder (hookecho-core::level3): must
//! return Ok or Err, never panic, hang or overflow.
#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(p) = hookecho_core::level3::decode(data) {
        let f = &p.field;
        // Touch every gate through the public accessors.
        for r in 0..f.radials() {
            for g in 0..f.gates {
                let _ = f.value(f.raw_at(r, g));
            }
        }
        let _ = hookecho_core::level3::sweep_volume(&[p], "TEST");
    }
});
