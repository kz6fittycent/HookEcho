# Phase 0 main-screen prototype (throwaway)

This prototype answers one question for the design gate: does the main screen read as a premium consumer app rather than a dev tool, with real radar under real glass? It is **not** the start of `hookecho-app`. Phase 1 rebuilds everything properly in the workspace crates.

```sh
../assets/samples/fetch.sh                     # one-time: the KTLX 2013-05-20 sample volume
cargo run --release -- --screenshots screenshots
cargo run --release                            # window: drag to pan, scroll to zoom,
                                               # V velocity, T theme, I pin inspector
cargo test --release                           # WCAG AA contrast check of the glass text tokens
```

## What's real

- **Radar:** Level II super-res 0.5° reflectivity and velocity, decoded with `nexrad-data`/`nexrad-model`. The raw bytes are uploaded once as `R8Uint` textures; polar→screen mapping and the color lookup run in the fragment shader (`src/scene.wgsl`).
- **Basemap:** Census counties, states and primary roads, stroked with lyon. Lines are extruded in the shader, so their width is constant in pixels at every zoom.
- **Backdrop blur:** downsampled separable Gaussian with tone compression (DESIGN.md §2.6). egui draws the blurred texture inside each rounded glass panel.
- **Inspector values:** read from the actual sweep (value, beam height, range/bearing).
- **Fonts and icons:** Inter and its tabular-figures instance, and Lucide icons. No stock egui widgets.

## What's staged

- **Scan age:** the sample is a 2013 archive file presented as "live", so the "1 min ago" age is fixed text.
- **Loop:** the timeline's frame ticks are the real KTLX scan times before 20:16:43 UTC, but only the last frame is loaded.
- **Me marker:** fixed in Norman.
- **Inspector position:** pinned to the debris-ball maximum for the screenshots.
- **Sweep strip:** the "11 in, 1 arriving, 2 pending" state in the product chip is illustrative.
- **Map labels** are drawn by egui here; the real renderer uses glyphon on the GPU.
- **The 4/3-earth slant-range correction** is approximated by `1/cos(elevation)` in the shader.
