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

**Phase 2 in progress (2026-10-10).** Public repo: https://github.com/kz6fittycent/HookEcho. Phase 1 is complete.

- Phase 0 was approved: design, the `bzip2-1.0.6` exception, and KDP from Level III.
- `BSL-1.0` was approved on 2026-10-10, then dropped the same day along with Windows.
- **Linux only** (owner, 2026-10-10).
- The snap is registered and the store secret exists. Each push to `main` uploads a candidate and opens a call-for-testing issue. Revision 1 was sample-only; revision 2 brought live radar (CFT #8); revision 3 is PR #7.

Phase 2 done:
- Real-time chunks from `unidata-nexrad-level2-chunks`, progressive sweeps, offline chunk cache, scan-age status (PR #5).
- Loop and timeline, plain-language product picker, color palettes and GRLevelX `.pal` import (drag and drop; snap `home` plug), a "little or no rain" notice for clear air (PR #7).
- Side sheet: Layers / Colors / Settings (branch `phase2/side-sheet`).

Phase 2 still to do:
- Command palette (Ctrl+K) plus the search button top right; the `?` shortcuts sheet.
- `.pal` masks and indexed units; removing an imported palette; legend hover value.
- Separate velocity units; font scale.
- Mask dual-pol products (CC, ZDR, PHI) where reflectivity or SNR is low.
- Rank place labels by Census population estimates instead of land area.
- Upstream: nexrad-data rc.7 panics on input shorter than 24 bytes (our adapter guards it). **Ask the owner before filing an issue.**

Strict-snap results (2026-10-10): Vulkan, llvmpipe and Mesa EGL GL work; NVIDIA EGL GL fails, which is harmless because wgpu picks Vulkan first. Local `snapcraft pack` is blocked by Docker's iptables rules (the owner declined the change), so test PR snaps from the `hookecho-snap` workflow artifact.

Working practice: iterate on branches with PRs. CI runs only on PRs into `main`, so stacked work goes onto the open PR's branch. Pushes to `main` trigger the candidate pipeline. **Ask before pushing.** Before pushing, run all of CI's gates locally: fmt, clippy, tests and `scripts/check-design-tokens.sh`.

Known issues and notes:
- Memory with a 20-frame loop is about 435 MB against a 300 MB budget: about 150 MB is the GPU driver, about 95 MB the decoded full volume, about 32 MB basemap decode transients. This is Phase 4 work. Use `HOOKECHO_MEM_TRACE=1` for a stage breakdown.
- Bench on the RTX 2080: cold start about 0.5 s, switch about 2 ms, 4K frame p99 about 2.7 ms.
- Place labels rank by land area, so large rural places outrank towns. Switch to Census population estimates (a free file).
- Always run manual smoke tests with `HOOKECHO_HOME=<scratch dir>`. Without it they write `~/.config/hookecho`.
- The contrast test models egui's gamma-space blending. Accent text is used only on plain glass.
- App screenshots: `./target/release/hookecho --screenshots target/shots`.
