# Prompt: Hook Echo, a clean-room weather radar viewer in Rust

Paste everything below the line into Claude Code, started in an empty project folder.

---

I want to build **Hook Echo**, a weather radar viewer for storm chasers and weather enthusiasts, in Rust. It should reach **1:1 feature parity with RadarScope**, run on Linux first (Snap Store), and also ship for Windows and macOS.

My priorities, in order:
1. **Fast and light.** Instant startup, newest data on screen as soon as it exists, smooth on a laptop in a car with integrated graphics, low memory and battery use.
2. **No trouble for the user.** No account, no setup, sensible defaults, opens straight to useful radar.
3. **A clean, modern, premium look.** It must look like a polished consumer app, **not like something that belongs in a terminal** or a developer tool. A clean, uncluttered layout matters here: it helps people track storms.

## 1. Clean-room rules (non-negotiable)

- Do not decompile, disassemble, or inspect any RadarScope binary, package, network traffic, or private API. Do not search for or use leaked code.
- **Parity means features and workflow, not appearance.** Read RadarScope's public material (website, help docs, release notes, store listings, reviews, tutorial videos) to learn *what* it does. Our icons, color tables, styling and screens are original. Familiar layout conventions are fine; pixel copies are not.
- Don't use RadarScope's name, logo or text anywhere. The app is named **Hook Echo** (snap name `hookecho`, binary `hookecho`, desktop file ID `io.github.kz6fittycent.HookEcho`). The icon should be an original take on a hook-echo reflectivity shape.
- Record the public source for every data format and algorithm in `docs/PROVENANCE.md`.
- License: **GPL-3.0-or-later**. Allowed dependency licenses are MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, OFL (fonts) and Unicode, enforced by `cargo-deny` in CI. Keep `THIRD_PARTY_LICENSES.md` current.

## 2. Tech stack (decided)

- **Language:** Rust (stable), 2024 edition.
- **Rendering:** `wgpu`, for the radar, basemap, overlays and the glass-panel blur. Falls back to OpenGL on older GPUs.
- **UI shell:** `egui`/`eframe` on the wgpu backend, with **a fully custom design system** (see section 5). Stock egui widgets must never appear in the shipped UI.
- **Async and networking:** `tokio`, `reqwest` (rustls, not OpenSSL).
- **Decoding:** reuse the `nexrad` crates from crates.io if they're maintained, correct, and license-compatible. Otherwise write our own decoder. Either way, verify the output against MetPy or Py-ART on the same files.
- **Basemap:** no map engine and no tile server. Pre-process Natural Earth and US Census TIGER data (states, counties, highways, cities) into a compact bundled binary format at build time. Tessellate it with `lyon` and draw it with our wgpu renderer. Render text and labels with `glyphon`.
- **Config and cache:** `serde` + `toml`, using XDG directories (`directories` crate).

**Workspace layout.** Only `hookecho-app` knows about egui:

```
crates/
  hookecho-core     decoding, fetching, caching, app state, storm math (no UI, no GPU)
  hookecho-render   wgpu renderer: radar, basemap, overlays, blur; draws to any surface or offscreen
  hookecho-app      egui shell: design system, panels, palette, timeline, inspector
  hookecho-data     build-time tool that generates the bundled site list and basemap
```

## 3. Data sources (all free and public)

- **Level II, real-time:** `s3://unidata-nexrad-level2-chunks/` (anonymous). This is the primary live source.
- **Level II, archive:** `s3://unidata-nexrad-level2/`
- **Level III:** `s3://unidata-nexrad-level3/`
- **Formats:** NOAA ICD 2620002 (Level II, Message 31) and ICD 2620001 (Level III). Cite ICD sections in code comments for non-obvious details.
- **Warnings and watches:** `https://api.weather.gov/alerts/active`, with a descriptive User-Agent as the NWS requires.
- **SPC outlooks, watches, and mesoscale discussions:** public feeds from spc.noaa.gov.
- **Lightning:** GOES GLM on AWS Open Data, as the free substitute for commercial lightning networks.
- **Basemap:** Natural Earth and US Census TIGER, bundled.

Cache on disk, respect rate limits, use conditional requests, and never hammer an endpoint.

## 4. Performance budgets (measured, not guessed)

Add a `--bench` mode that measures each of these on bundled sample data and prints the results. CI fails if any regresses more than 10% from the committed baseline.

| Metric | Budget |
|---|---|
| Cold start to radar on screen (cached data) | < 1 s |
| New sweep chunk downloaded → drawn | < 1 s |
| Pan, zoom and loop, 4K, integrated GPU | 60 fps (frame time < 16 ms, p99) |
| Tilt or product switch | < 50 ms |
| Memory with a 20-frame loop loaded | < 300 MB |
| Idle CPU (nothing animating) | ~0% (redraw only on input, animation or new data) |
| Snap size | < 50 MB |

How to hit them:
- **Progressive sweeps:** draw each elevation sweep as soon as its chunks arrive, without waiting for the full volume scan. The UI must show which sweeps are in and which are still coming.
- Decode on background threads and **never block the UI thread** on disk or network.
- Upload each decoded sweep **once** as a GPU texture. The fragment shader does polar-to-screen mapping and color lookup, so loops, tilt switches and color-table changes never re-decode or re-upload.
- Keep a bounded LRU of GPU frames and evict by memory budget.
- On a slow or dropped connection, show cached data with a clear age warning, retry with backoff, and never freeze.

## 5. Design: it must not look like it belongs in a terminal

This section matters as much as the features. **egui looks like a developer tool by default.** Avoiding that look is the job here, so treat the defaults as a starting point to replace completely.

### Visual direction
- **Map first:** radar and basemap fill the entire window, edge to edge. No permanent toolbars, menu bar, or docked panels.
- **Floating glass panels:** compact panels hover over the map with rounded corners (12–16 px), soft shadows, a hairline border, and a **real backdrop blur** of the radar behind them. Since we own the wgpu renderer, render the scene to a texture, blur the regions under the panels, and composite. It must look like modern macOS or GNOME, not flat gray boxes.
- **Typography:** bundle **Inter** (OFL) as the only UI font, with a clear type scale (e.g. 12/13/15/17/22/28) and tabular figures for numbers, times and values. Never use egui's default font or monospace in the UI.
- **Icons:** one consistent, original-style icon set: **Lucide** or **Phosphor** (MIT) rendered as vector paths. No emoji and no text glyphs standing in for icons.
- **Color:** a restrained neutral palette, so the radar colors are the loudest thing on screen. One accent color. Dark theme by default (it's used at night in storms), a true light theme, and a follow-system option. All text must pass WCAG AA contrast.
- **Motion:** eased transitions of 150–200 ms for panels, sheets and hovers, and smooth zoom with momentum. Honor reduced-motion settings.
- **Generous spacing** on a 4/8 px grid. If a panel looks crowded, it holds too much.

### Layout
- **Top left:** site chip (name, distance, scan age) and product chip (e.g. "Reflectivity · 0.5°"). Click one to open a searchable picker.
- **Bottom:** a slim timeline with play/pause, a scrubber with scan ticks, speed, and a live/latest button. Scan age is colored green → amber → red as data gets stale.
- **Right:** a collapsible side sheet for layers (warnings, outlooks, lightning, storm tracks), color tables and settings. Hidden by default.
- **Inspector:** hover (or long-press on touch) shows a small floating card next to the cursor with value, units, height above radar level, and range/bearing from the radar and from me. Never a dialog.
- **Command palette (Ctrl+K):** reaches every site, product, layer, action and setting by typing. This keeps the main screen minimal.
- **`?`** shows a shortcuts cheat sheet. Every common action has a shortcut, shown in tooltips.
- Multi-pane (2 or 4) splits the map with thin dividers, not window chrome.
- If a new feature doesn't fit this layout cleanly, **stop and propose where it goes**. Never just add another button.

### Design system in code
- `hookecho-app/src/design/` holds **tokens** (color, spacing, radii, elevation, type scale, durations) and a **component library**: Chip, IconButton, GlassPanel, Sheet, Palette, Slider, Toggle, Segmented, Tooltip, Card. Every screen is built only from these.
- No hard-coded colors, sizes or fonts outside `design/`. Add a CI check (a simple grep) that enforces it.
- HiDPI and fractional scaling on Wayland and X11. The layout must still work at 900×600.
- Accessibility: visible keyboard focus everywhere, AccessKit labels, colorblind-safe default color tables with alternates.

### Design process (gates)
1. Before any feature work, write `docs/DESIGN.md` (tokens, layout, each panel and state), then build a **throwaway prototype of the main screen** with real radar from a bundled sample file, glass panels, the timeline and a fake inspector. Show me screenshots in dark and light themes. **Wait for my approval.** If it still reads as "dev tool", iterate until it doesn't.
2. At the end of every phase, generate screenshots with `--screenshots`, review them yourself against `docs/DESIGN.md` (alignment, spacing, contrast, clutter, consistency), fix what's off, then show me.

## 6. Feature parity

Create `docs/FEATURE_PARITY.md` before Phase 1 and show it to me. It's a table of every RadarScope feature from public sources, with columns: feature · free/Pro · source URL · our phase · status (planned / in progress / done / verified) · notes.

- Include every product, overlay, tool, setting and workflow detail you can find.
- For features that depend on paid or proprietary data, list the free substitute and how it differs. Never scrape paid feeds.
- Put a separate, lower-priority "beyond parity" list of gaps users report in other viewers (e.g. Supercell Wx issues and reviews) that RadarScope also has.
- Update the matrix in the same commit as any feature change. "Done" means implemented and tested; "verified" means I've checked it on real weather.

## 7. Phases (commit after each step, keep tests passing)

**Phase 0: Preflight and design gate**
- Evaluate the `nexrad` crates, then propose the decoder plan
- Write `CLAUDE.md` (plan, phases, status section you keep updated), `docs/FEATURE_PARITY.md`, and `docs/DESIGN.md`
- Build the main-screen prototype and get my approval (section 5)

**Phase 1: Core and CI skeleton**
- Level II decoding of all moments (REF, VEL, SW, ZDR, CC, KDP, PHI), with tests on small real sample files in `tests/data/` and comparisons against MetPy/Py-ART
- GPU radial rendering at full super-res, bundled basemap, site picker, tilt selection
- `--smoke-test`, `--screenshots`, `--bench`
- The Snap build and Rust CI from section 8, set up **now**, so GPU problems under strict confinement show up early

**Phase 2: Usable viewer**
- Real-time chunks with progressive sweep display, auto-refresh, scan-age indicators
- Animation loop, inspector, color tables (originals plus import of the public GRLevelX `.pal` format)
- Command palette, side sheet, light/dark/system themes, shortcuts

**Phase 3: Chaser features**
- NWS warnings and watches, SPC outlooks and MDs, GLM lightning
- Storm-relative velocity, Level III products (storm tracks, hail/meso/TVS markers)
- My location via GeoClue (D-Bus) on Linux and the platform APIs on Windows/macOS, plus a distance/bearing tool
- Multi-pane layouts

**Phase 4: Polish and release**
- Velocity dealiasing (document the algorithm and its source)
- Meet every performance budget on an integrated GPU
- The full release pipeline from section 8

**Phase 5: Close parity gaps**
- Work through every remaining `FEATURE_PARITY.md` row in the order I approve, then the "beyond parity" list

## 8. Distribution: Snap first, with the same workflows as Desktop Scriptures

Port the CI/CD pipeline from my existing project at `/home/james/git/Desktop-Scriptures/.github/workflows/`. **Read every workflow and every comment first.** The comments document real failures I've hit, and those fixes must carry over. Keep the structure, names, retries and behavior the same; swap the Python/PyInstaller/Qt steps for cargo.

**`snap/snapcraft.yaml`** (model it on `/home/james/git/Desktop-Scriptures/snap/snapcraft.yaml`):
- `base: core24`, `confinement: strict`, `grade: stable`, `license: GPL-3.0-or-later`, `compression: lzo`, amd64 to start
- `plugin: rust`, with the version taken from the workspace `Cargo.toml` via `craftctl set version=...`
- `extensions: [gnome]`; plugs: `desktop`, `desktop-legacy`, `wayland`, `x11`, `opengl`, `network`, `location-observe`
- Confirm wgpu works on a real strict install (Vulkan and the GL fallback), not just that the snap builds

**Workflows:**
1. **`snap-builds.yml`:** on push and PR to `main`, run `snapcore/action-build@v1` with `snapcraft-args: pack` (**not** `args`), then `snapcraft-review-action`. Add a Rust job: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo deny check`, the design-token grep check, and `--bench` against the baseline.
2. **`call-for-testing.yml`:** chained via `workflow_run` off a successful Snap Builds run on `main`. Upload to `candidate` (the bare risk level, **not** `latest/candidate`, because of the export-login macaroon). Open the call-for-testing issue with `/promote <rev> stable [done]` instructions, and capture `ghvmctl` screenshots in a clean VM, including the `exec "snap install"` workaround, the retries, the `screenshots` branch, and the failure comment.
3. **`promote-to-stable.yml`:** the `/promote` issue comment, limited to OWNER/MEMBER/COLLABORATOR, gated by the `promote-stable` environment, with the same exact-match regex.
4. **`security-rebuild.yml`:** daily `review-tools.snap-check-notices` against stable. Rebuild from the **release commit** (found from the `Cargo.toml` version history), never from `main`, with the same dry_run/force inputs and open-issue guard.
5. **`desktop-builds.yml`:** a Windows x64 Inno Setup installer and a macOS Apple Silicon `.dmg` (ad-hoc signed) from `cargo build --release`. Keep the same smoke tests, screenshots, checksums, test pre-releases (keep 3), `desktop-testing` issues, and a GitHub Release on `v*` tags that must match the `Cargo.toml` version.
6. **`refresh-radar-data.yml`** (replaces the citation refresh workflows): weekly, regenerates the bundled site list and basemap with `hookecho-data`, publishes it to a `radar-data` release the app checks for updates, then opens and auto-merges a PR. It refuses to publish if entries were lost.

**CLI flags the workflows rely on:**
- `--smoke-test`: start, load a bundled sample volume, render one frame offscreen, check that the config and cache dirs were created, and exit 0
- `--screenshots <dir>`: write numbered PNGs (`1-home.png`, `2-velocity.png`, …) from bundled data in both themes, so screenshots never depend on live weather
- `--bench`: section 4

**For me to do (list these in the README, don't do them yourself):** create the `SNAPCRAFT_STORE_CREDENTIALS` secret, create the `promote-stable` environment with required reviewers, and register the snap name (`snapcraft register hookecho`).

## 9. Working rules

- Small, reviewable commits with clear messages. Ask before pushing.
- Correctness over speed in the decoder. When unsure about a format detail, cite the ICD section rather than guessing.
- Fuzz the Level II and Level III parsers (`cargo-fuzz`) before Phase 2. No `unsafe` in `hookecho-core`.
- No telemetry, no accounts, and no paid services. Ask me before adding anything that phones home beyond the data sources above.
- Keep `CLAUDE.md`'s status section current so a fresh session can pick up cold.

Start with Phase 0.
