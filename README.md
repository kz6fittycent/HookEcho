# Hook Echo

A fast, clean weather radar viewer for storm chasers and weather enthusiasts, written in Rust. Linux first (Snap Store), with Windows and macOS builds.

- **Full-resolution NEXRAD Level II**, every moment and tilt, drawn on the GPU with wgpu.
- **Map-first and calm:** floating glass panels over edge-to-edge radar, in dark and light themes.
- **No account, no setup, no telemetry.** Radar comes from NOAA's free public feeds.

Status: early development (Phase 1 of `docs/KICKOFF.md`). See `CLAUDE.md` for the current state and `docs/FEATURE_PARITY.md` for the feature plan.

## Building

```sh
cargo build --release -p hookecho-app
./target/release/hookecho                    # the app
./target/release/hookecho --smoke-test       # offscreen self-check (CI)
./target/release/hookecho --screenshots DIR  # numbered PNGs from bundled data
./target/release/hookecho --bench            # performance budgets (KICKOFF §4)
```

Requires Rust 1.99 or newer and a GPU driver with Vulkan, Metal, DX12 or OpenGL. With no GPU, Mesa's lavapipe (`mesa-vulkan-drivers`) works for the offscreen modes.

| Check | Command |
|---|---|
| Tests | `cargo test --workspace` |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` |
| Licenses and advisories | `cargo deny check` |
| No literal UI colors, sizes or fonts | `scripts/check-design-tokens.sh` |
| Regenerate THIRD_PARTY_LICENSES.md | `scripts/gen-licenses.sh` |
| Fuzz a parser (nightly) | `cd fuzz && cargo +nightly fuzz run level2_decode` |
| Regenerate bundled data | `cargo run -p hookecho-data -- sites --out crates/hookecho-core/data/sites.tsv` |

## Releases

The pipeline matches Desktop Scriptures' (see `docs/CI_NOTES.md`):

- `snap-builds.yml`: every push and PR builds and reviews the snap, then runs Rust CI (fmt, clippy, tests, cargo-deny, the design-token check, the smoke test, screenshots and the bench against `bench/baseline.json`).
- `call-for-testing.yml`: after a green build on `main`, uploads to `candidate` and opens a call-for-testing issue with VM screenshots.
- `promote-to-stable.yml`: commenting `/promote <revision> stable done` on that issue releases to stable, after your approval.
- `security-rebuild.yml`: daily, rebuilds the stable release when Ubuntu publishes fixes for packages bundled in the snap.
- `desktop-builds.yml`: Windows installer and macOS `.dmg`; test pre-releases on `main`, and a GitHub Release on `v*` tags (the tag must match the `Cargo.toml` version).
- `refresh-radar-data.yml`: weekly, regenerates the site list and basemap and publishes them to the `radar-data` release.

### One-time setup (owner)

These can't be automated and are deliberately left for you:

1. **Register the snap name:** `snapcraft register hookecho`.
2. **Store credentials secret:** `snapcraft export-login --snaps hookecho --channels stable,candidate --acls package_access,package_push,package_update,package_release creds.txt`, then add the file's contents as the repository secret `SNAPCRAFT_STORE_CREDENTIALS`.
3. **Release approval:** create the GitHub environment `promote-stable` with yourself as a required reviewer. `promote-to-stable.yml` waits there before releasing.
4. **Bench baselines for CI:** after the first green `snap-builds.yml` run, add the `bench-result.json` from its `rust-ci-results` artifact to `bench/baseline.json` under its adapter name.

## License

GPL-3.0-or-later. Third-party licenses are in `THIRD_PARTY_LICENSES.md`, and the sources of every data format and algorithm are in `docs/PROVENANCE.md`.
