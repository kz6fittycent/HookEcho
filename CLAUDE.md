# Hook Echo: working notes for Claude

Hook Echo is a clean-room weather radar viewer in Rust that aims for 1:1 feature/workflow parity with the leading commercial viewer.

- **Platforms:** Linux first (Snap Store), then Windows and macOS.
- **License:** GPL-3.0-or-later.
- **The full brief** is `docs/KICKOFF.md`. Read it before any non-trivial change. It is the source of truth for rules, stack, budgets, design and phases.

## Non-negotiables (short form, details in KICKOFF)

- **Clean room.** No decompiling or inspecting the original's binaries, traffic or private APIs, and no leaked code. Learn features only from public material. Never use the original's name, logo or text.
- **Every data format and algorithm** gets a public source in `docs/PROVENANCE.md`.
- **Dependency licenses:** MIT / Apache-2.0 / BSD / ISC / Zlib / MPL-2.0 / OFL / Unicode only, enforced by `cargo-deny`. Exceptions only with the owner's approval (see `docs/DECODER_PLAN.md`). Keep `THIRD_PARTY_LICENSES.md` current.
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

**Phase 0, waiting at the design gate.**

Done:
- Evaluated the `nexrad` crates. Proposal in `docs/DECODER_PLAN.md`: use them for L2 behind an adapter, write our own L3. **Pending approval**, including the `bzip2-1.0.6` license exception.
- Wrote `docs/FEATURE_PARITY.md` (about 110 rows, every row sourced) and `docs/DESIGN.md`.
- Built the prototype in `prototype/` (see `prototype/README.md`). Screenshots: `prototype/screenshots/`.

Next:
- The owner reviews the screenshots, DESIGN.md, FEATURE_PARITY.md and DECODER_PLAN.md.
- Iterate on the prototype until approved, then start Phase 1.
- Phase 1 must begin by reading every workflow and comment in `/home/james/git/Desktop-Scriptures/.github/workflows/` and its `snap/snapcraft.yaml` (KICKOFF §8).

Open questions for the owner:
- License allowlist: add `bzip2-1.0.6`?
- KDP: Level III product (proposed) versus Level II-derived?
- Rows marked **Ask first** in FEATURE_PARITY.md (geocoding, mPING, Spotter Network, satellite basemap).
