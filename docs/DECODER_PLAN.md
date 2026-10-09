# Decoder plan (Phase 0 proposal)

Status: **proposed, awaiting approval**.

## What I evaluated

The `nexrad` family of crates by Daniel Way: <https://github.com/danielway/nexrad>.

| Crate | Version checked | Purpose | Verdict |
|---|---|---|---|
| `nexrad-decode` | 1.0.0-rc.3 (2026-02-28) | Archive II / Message 31 binary decoding per ICD 2620002 | **Use** |
| `nexrad-model` | 1.0.0-rc.2 | Scan / Sweep / Radial / MomentData model and site registry | **Use** |
| `nexrad-data` | 1.0.0-rc.7 (2026-04-03) | Volume files, LDM record bzip2 decompression, AWS archive and **real-time chunk** access | **Use**, but behind our own fetch layer |
| `nexrad-process` | 1.0.0-rc.1 | Filters, composite, SRV, cell detection | Read only; we implement and cite our own algorithms |
| `nexrad-render` | 1.0.0-rc.4 | CPU PNG rendering | **Don't use**; our renderer runs on the GPU |
| `nexrad` (facade) | 1.0.0-rc.4 | Convenience re-exports | Don't use; depend on the sub-crates directly |

Findings:

- **License:** MIT, which is compatible with GPL-3.0-or-later.
- **Maintenance:** the last commits were on 2026-07-21 (sign-bit fix for negative tilts, live-decode resilience). There are 4 open issues and the repo was pushed again on 2026-10-09. The project has one main maintainer, which is a bus-factor risk.
- **Safety:** every crate has `#![forbid(unsafe_code)]`, and there are `cargo-fuzz` targets for message decoding, record splitting and decompression.
- **Coverage:** all Message 31 moments are present: REF, VEL, SW, ZDR, PHI, RHO (CC) and CFP. The legacy Message 1 format, VCP (Message 5/7) and RDA status messages are also decoded.
- **GPU friendliness:** `MomentData` exposes `raw_values()` (raw u8/u16 gates) plus `scale()` and `offset()`. We can upload raw bytes as an `R8Unorm`/`R16Uint` texture and decode in the shader, with no float conversion on the CPU.
- **Real-time:** `nexrad-data::aws::realtime` understands the `unidata-nexrad-level2-chunks` layout (volume index, chunk types S/I/E, elevation-to-chunk mapping, next-chunk timing estimates). That is exactly what progressive sweeps need.
- **Gaps:**
  - **No Level III at all.** ICD 2620001 products (super-res L3, dual-pol L3, HCA, QPE, VIL, EET, storm tracks, hail, meso, TVS) must be our own decoder.
  - **KDP is not a Level II moment.** Message 31 carries PHI (ΦDP) but not KDP. The kickoff lists KDP under Phase 1 "Level II moments". I propose getting KDP from the Level III dual-pol product (`N0K`/`DKD` family, product 163) in Phase 3. A Level II KDP derived from ΦDP (range-derivative with filtering, cited from the literature) can go in Phase 5 if wanted.
  - Pre-1.0 versions (release candidates), so the API can still move.

## Proposal

1. **Level II:** depend on `nexrad-decode`, `nexrad-model` and `nexrad-data` with **exact version pins** (`=1.0.0-rc.x`) and `default-features = false` where possible.
   - Wrap them in a thin adapter module, `hookecho-core::level2`, that converts to our own `Sweep`/`Moment` types (raw bytes, scale, offset, gate geometry, sorted azimuth table).
   - Nothing outside that module touches `nexrad-*` types, so swapping or forking the decoder later is a one-module change.
2. **Fetching:** use our own `reqwest` + `tokio` client for S3 listing and chunk download, with conditional requests, backoff, disk cache and rate limits. We use `nexrad-data` only for the **pure** parts: record splitting, bzip2 decompression, chunk naming and the timing model. Their AWS client stays off, so all network policy lives in one place we control (User-Agent, caching, budget).
3. **Level III:** write our own `hookecho-core::level3` decoder from ICD 2620001.
   - Covers the Product Description Block, symbology blocks, radial (packet 16, AF1F), raster, and the generic-product (packet 28) path for the dual-pol and super-res families, plus the graphic packets for storm tracks, hail, meso and TVS.
   - Comments cite ICD sections for anything non-obvious.
   - `#![forbid(unsafe_code)]`, fuzzed with `cargo-fuzz` before Phase 2.
4. **Verification** (Phase 1 and Phase 3): a `tools/verify/` Python script (MetPy and Py-ART in a venv, not shipped) dumps gate values for the same files to `.npz`.
   - A Rust test compares them gate-by-gate with tolerance ≤ ½ quantization step.
   - The reference dumps are committed, so CI doesn't need Python.
5. **Upstream:** report any bugs found upstream rather than patching silently. If upstream stalls, fork into `crates/vendor/` under MIT with attribution.

## License note needing your decision

All bzip2 decoders, including the pure-Rust `libbz2-rs-sys` that `nexrad-data` uses through `bzip2` 0.6, carry the **`bzip2-1.0.6`** license. It's a permissive BSD-style license that is compatible with GPL-3.0, but it's not on the allowed list in KICKOFF section 1. Level II chunks are bzip2-compressed, so some bzip2 implementation is unavoidable. **Proposal:** add `bzip2-1.0.6` to the `cargo-deny` allowlist as a single, documented exception.

The wgpu and egui dependency tree will likely also need `Unicode-3.0` (already allowed as "Unicode"), `BSL-1.0` and possibly `CC0-1.0`. I'll list the exact set from a real `cargo deny` run in Phase 1 and ask before adding anything beyond `bzip2-1.0.6`.
