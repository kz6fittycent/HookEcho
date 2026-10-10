# Hook Echo: working notes for Claude

Hook Echo is a clean-room weather radar viewer in Rust that aims for 1:1 feature/workflow parity with the leading commercial viewer.

- **Platforms:** Linux first (Snap Store), then Windows and macOS.
- **License:** GPL-3.0-or-later.
- **The full brief** is `docs/KICKOFF.md`. Read it before any non-trivial change. It is the source of truth for rules, stack, budgets, design and phases.

## Non-negotiables (short form, details in KICKOFF)

- **Clean room.** No decompiling or inspecting the original's binaries, traffic or private APIs, and no leaked code. Learn features only from public material. Never use the original's name, logo or text.
- **Every data format and algorithm** gets a public source in `docs/PROVENANCE.md`.
- **Dependency licenses:** MIT / Apache-2.0 / BSD / ISC / Zlib / MPL-2.0 / OFL / Unicode, plus the approved exceptions `bzip2-1.0.6` and `BSL-1.0`, enforced by `cargo-deny`. Any other exception needs the owner's approval. Keep `THIRD_PARTY_LICENSES.md` current.
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

**Phase 1 is complete locally (2026-10-10), except the snap build and GPU test, which are waiting on CI.** Nothing has been pushed, and the repo has no GitHub remote yet.

Phase 0 was approved: design, the `bzip2-1.0.6` exception, and KDP from Level III. `BSL-1.0` was approved on 2026-10-10.

Phase 1 checklist:
- [x] Read every Desktop-Scriptures workflow and comment. The lessons are in `docs/CI_NOTES.md`.
- [x] Workspace; cargo-deny (`deny.toml`, scoped to shipping targets; `ttf-parser` advisory ignored with a reason)
- [x] hookecho-core: Level II for all moments, matching MetPy exactly; fuzzed (`fuzz/`, regressions in `tests/data/fuzz-regressions/`)
- [x] hookecho-data: `sites` and `basemap`
- [x] hookecho-render, hookecho-app (main screen, pickers, inspector, `--smoke-test`, `--screenshots`, `--bench`)
- [x] THIRD_PARTY_LICENSES.md (`scripts/gen-licenses.sh`), `bench/baseline.json` (RTX 2080 and llvmpipe on the dev machine)
- [x] `snap/snapcraft.yaml`, six workflows, `packaging/`, README with the owner's one-time setup
- [x] FEATURE_PARITY.md updated for Phase 1
- [ ] **Snap build and strict-install GPU test (Vulkan + GL fallback): deferred to CI** (owner's choice). The local build is blocked because Docker's iptables `FORWARD DROP` cuts off LXD's network. The owner declined the DOCKER-USER rule change. Do this once the repo is on GitHub.
- [ ] After the first green CI run: add the CI adapter's `bench-result.json` to `bench/baseline.json`.

Phase 2 follow-ups found during the Phase 1 review:
- Mask dual-pol products (CC, ZDR, PHI) where reflectivity or SNR is low; clear-air noise is chaotic on CC.
- Rank place labels by Census population estimates instead of land area.
- Upstream: nexrad-data rc.7 panics on input shorter than 24 bytes (our adapter guards it). **Ask the owner before filing an issue.**

Known issues and notes:
- Memory with a 20-frame loop is about 435 MB against a 300 MB budget: about 150 MB is the GPU driver, about 95 MB the decoded full volume, about 32 MB basemap decode transients. This is Phase 4 work. Use `HOOKECHO_MEM_TRACE=1` for a stage breakdown.
- Bench on the RTX 2080: cold start about 0.5 s, switch about 2 ms, 4K frame p99 about 2.7 ms.
- Place labels rank by land area, so large rural places outrank towns. Switch to Census population estimates (a free file).
- Always run manual smoke tests with `HOOKECHO_HOME=<scratch dir>`. Without it they write `~/.config/hookecho`.
- The contrast test models egui's gamma-space blending. Accent text is used only on plain glass.
- App screenshots: `./target/release/hookecho --screenshots target/shots`.
