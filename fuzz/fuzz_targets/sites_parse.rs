//! Site lists also arrive via `radar-data` releases; parsing must never panic.
#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = hookecho_core::sites::parse(s);
    }
});
