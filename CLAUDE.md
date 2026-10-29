# Hook Echo: working notes for Claude

Hook Echo is a clean-room weather radar viewer in Rust that aims for 1:1 feature/workflow parity with the leading commercial viewer.

- **Platform:** Linux only, via the Snap Store. The owner dropped Windows and macOS on 2026-10-10, overriding KICKOFF §§1–2 and §8 (`desktop-builds.yml` and its packaging were removed). Don't add other platforms back unless asked.
- **License:** GPL-3.0-or-later.
- **The full brief** is `docs/KICKOFF.md`. Read it before any non-trivial change. It is the source of truth for rules, stack, budgets, design and phases.

## Non-negotiables (short form, details in KICKOFF)

- **Clean room.** No decompiling or inspecting the original's binaries, traffic or private APIs, and no leaked code. Learn features only from public material. Never use the original's name, logo or text.
- **Every data format and algorithm** gets a public source in `docs/PROVENANCE.md`.
- **Dependency licenses:** MIT / Apache-2.0 / BSD / ISC / Zlib / MPL-2.0 / OFL / Unicode, plus the approved exception `bzip2-1.0.6`, enforced by `cargo-deny`. Any other exception needs the owner's approval. Keep `THIRD_PARTY_LICENSES.md` current.
- **No telemetry, no accounts, no paid services.** Ask before adding any new outbound endpoint.
- **No `unsafe` in `hookecho-core`.** Fuzz the L2/L3 parsers before Phase 2.
- **UI:** stock egui widgets never appear. No literal colors, sizes or fonts outside `crates/hookecho-app/src/design/`. Follow `docs/DESIGN.md`.
- **Commits:** small, with tests passing. **Ask before pushing.** Update `docs/FEATURE_PARITY.md` in the same commit as any feature change.

## Layout

```
crates/hookecho-core     decoding, fetching, caching, app state, storm math (no UI, no GPU)
crates/hookecho-render   wgpu renderer: radar, basemap, overlays, blur
crates/hookecho-app      egui shell + design system (only crate that knows egui)
crates/hookecho-data     build-time generator for the bundled site list and basemap
prototype/               Phase 0 throwaway main-screen prototype (not a workspace member)
docs/                    KICKOFF, DESIGN, FEATURE_PARITY, PROVENANCE, DECODER_PLAN
assets/                  fonts, icons, sample data (large samples are fetched, not committed)
```

## Toolchain

- Rust is installed with rustup in `~/.cargo/bin`, **without** PATH changes to shell rc files. Run it as `~/.cargo/bin/cargo` or `PATH=$HOME/.cargo/bin:$PATH cargo ...`.
- Sample data: `assets/samples/fetch.sh` downloads the Level II samples from the public Unidata bucket.
- Dev machine: an NVIDIA RTX 2080 (Vulkan) plus llvmpipe. The integrated-GPU budgets in KICKOFF §4 still need a real iGPU check.

## Phases

0. Preflight and design gate: decoder evaluation, these docs, main-screen prototype, **owner approval**.
1. Core and CI skeleton: L2 decode of all moments, GPU radial render, basemap, site picker, tilts, `--smoke-test`/`--screenshots`/`--bench`, Snap and CI.
2. Usable viewer: real-time chunks and progressive sweeps, loop, inspector, palettes and `.pal` import, command palette, side sheet, themes, shortcuts.
3. Chaser features: warnings/watches/SPC/GLM, SRV, L3 storm attributes, location and distance tool, multi-pane.
4. Polish and release: dealiasing, all budgets met on an iGPU, release pipeline.
5. Close parity gaps in the order the owner approves, then "beyond parity".

## Status (keep this current)

**Phase 2 signed off by the owner (2026-10-29). Phase 3 in progress.** Public repo: https://github.com/kz6fittycent/HookEcho. Last merge to `main`: PR #27 (`b102f77`, radar data refresh).

- Phase 0 was approved: design, the `bzip2-1.0.6` exception, and KDP from Level III.
- **Linux only** (owner, 2026-10-10).
- The snap is registered and the store secret exists. Each push to `main` uploads a candidate and opens a call-for-testing issue (latest: revision 11, CFT #26, closed).
- **Endpoints approved 2026-10-29** ("all are approved"): api.weather.gov, spc.noaa.gov, aviationweather.gov, services.swpc.noaa.gov, `noaa-goes19` on AWS. All free, no accounts. If a service ever needs money or a key, the owner says to pass that on to the user; none does today.

Done before 2026-10-29 (merged): Phases 0–2; owner fixes 1–3 (icon PR #17; start near the user PR #18; TDWR PRs #21, #22, #24); NWS storm-based warnings (PR #24).

**Branch `feat/data-layers` (2026-10-29, not pushed yet: ask first).** One commit per layer, owner's order 1–5 plus location:
1. Watches (SPC outlines) finish data layer 1.
2. SPC outlooks (days 1–3), watches and MDs: `hookecho-core::spc`, `hookecho-app::spc`; Layers → Storm Prediction Center. `hookecho-core::poll` is the shared polling thread + conditional-GET client; alerts moved onto it.
3. Surface obs / command center: `hookecho-core::obs` (AWC METAR cache file + Data API history), `hookecho-app::stations` (station plots, station panel bottom left).
4. GOES-East satellite: `hookecho-core::goes` (hdf5-pure), a satellite pass in `hookecho-render` (per-pixel fixed-grid projection), `design/satellite.rs`. Bands 1 (visible, 1 km), 9, 13. Band 2 skipped: 65 MB a scan.
5. Space weather: `hookecho-core::space` (SWPC), `hookecho-app::space` (Menu → Space weather card; Layers → Aurora forecast).
6. "Use my location": option A of `docs/LOCATION.md` (XDG portal via `ashpd`); snap's `location-observe` plug dropped. Error path tested (location off on the dev machine); the success path needs the owner to try it.
New dependencies: `hdf5-pure` =0.47.0, `ashpd` =0.13.13 (both MIT/Apache, `cargo deny` clean). Screenshots 22–29 in `target/shots/p3/` (29 needs `HOOKECHO_GOES_FILE`).

Upstream: nexrad-data's short-input panic is already fixed on its `main` and in 1.0.0 (checked 2026-10-29), so no issue was filed. Upgrading from rc.7 to 1.0.0 is a candidate change (our guard can stay).

Still planned: GLM lightning (P3), local storm reports, ProbSevere, models and soundings, mosaic. Moved to Phase 3: `.pal` rain/snow/mix/topped masks and indexed units (need Level III hydrometeor classes and echo tops). Command center next steps: more stations at once, history graphs.

Where to pick up next session:
- Ask the owner before pushing `feat/data-layers` and opening its PR; each merge to `main` needs its own approval.
- Close each older CFT issue once its replacement exists.

Workflow notes: Refresh Radar Data now opens its PR (#27 merged), so the Actions permission is on. Security Rebuild skips quietly until the first stable release.

Strict-snap results (2026-10-10): Vulkan, llvmpipe and Mesa EGL GL work; NVIDIA EGL GL fails, which is harmless because wgpu picks Vulkan first. Local `snapcraft pack` is blocked by Docker's iptables rules (the owner declined the change), so test PR snaps from the `hookecho-snap` workflow artifact.

Working practice: iterate on branches with PRs. CI runs only on PRs into `main`, so stacked work goes onto the open PR's branch. Pushes to `main` trigger the candidate pipeline. **Ask before pushing.** Before pushing, run all of CI's gates locally: fmt, clippy, tests and `scripts/check-design-tokens.sh`.

Known issues and notes:
- Memory with a 20-frame loop is about 435 MB against a 300 MB budget: about 150 MB is the GPU driver, about 95 MB the decoded full volume, about 32 MB basemap decode transients. This is Phase 4 work. Use `HOOKECHO_MEM_TRACE=1` for a stage breakdown.
- Bench on the RTX 2080: cold start about 0.5 s, switch about 2 ms, 4K frame p99 about 2.7 ms.
- Always run manual smoke tests with `HOOKECHO_HOME=<scratch dir>`. Without it they write `~/.config/hookecho`.
- The contrast test models egui's gamma-space blending. Accent text is used only on plain glass.
- App screenshots: `./target/release/hookecho --screenshots target/shots`.
