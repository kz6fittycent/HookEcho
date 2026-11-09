//! Allocator tuning.
//!
//! glibc raises its mmap threshold each time a large mmapped block is freed
//! (up to 32 MiB on 64-bit), so after the first few Level II decodes the
//! decoder's big transient buffers come from the heap instead, and freed
//! heap memory mostly stays in the process. With a 20-frame loop that is
//! about 68 MB of resident memory holding nothing (bench, nexrad 1.0.0,
//! 2026-11-09). Setting `M_MMAP_THRESHOLD` explicitly turns the dynamic
//! adjustment off, so blocks of 128 KiB and up are always mmapped and go
//! back to the OS when freed.
//!
//! Source: glibc manual, "Malloc Tunable Parameters", and mallopt(3).

/// glibc's documented default for `M_MMAP_THRESHOLD`.
const MMAP_THRESHOLD_BYTES: libc::c_int = 128 * 1024;

/// Pins the mmap threshold. Call once, first thing in `main`.
pub fn pin_mmap_threshold() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: mallopt only sets an allocator parameter. It is called before
    // any other thread exists, and a failure (return 0) leaves glibc's
    // default behaviour in place, which is harmless.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, MMAP_THRESHOLD_BYTES);
    }
}
