# Hook Echo design

Status: **approved at the Phase 0 design gate (2026-10-09).** Changes need a note here.

Hook Echo should feel like a calm, premium consumer app that gets out of the way of the weather. The radar is the loudest thing on screen. Everything else is quiet, compact, and floats.

## 1. Principles

1. **Map first.** Radar and basemap fill the window edge to edge. There's no menu bar, toolbar or docked panel. Chrome floats over the map and only takes the space it needs.
2. **One glance, one answer.** The always-visible chrome answers four questions: *which radar*, *which product*, *how old*, *where in the loop*. Everything else is one click, one key, or Ctrl+K away.
3. **Quiet chrome, loud data.** UI colors are low-saturation neutrals with one accent. Saturated color is reserved for radar data, warnings, and the scan-age status.
4. **Glass, not boxes.** Panels show a real blur of the radar beneath them, so the user never loses the storm behind a panel.
5. **Motion explains, never decorates.** Every animation is 150–200 ms and eased. With reduced motion, animations become instant cross-fades.
6. **Crowded means wrong.** If a panel feels busy, it holds too much. Move things to the side sheet or the command palette instead of shrinking them.

## 2. Tokens

All values live in `crates/hookecho-app/src/design/tokens.rs`. **No literal colors, sizes or font names outside `design/`**; CI greps for it.

### 2.1 Spacing (4/8 px grid)

| Token | px | Use |
|---|---|---|
| `space.1` | 4 | Icon-to-label gap, hairline offsets |
| `space.2` | 8 | Inner padding of chips, gaps between chips |
| `space.3` | 12 | Panel inner padding (compact) |
| `space.4` | 16 | Panel inner padding, window margin |
| `space.5` | 24 | Section gaps in sheets |
| `space.6` | 32 | Large separations, palette top offset |

The window margin (the gap between floating panels and the window edge) is `space.4`, or `space.3` below 1000 px width.

### 2.2 Radii

| Token | px | Use |
|---|---|---|
| `radius.sm` | 8 | Small controls: segmented items, keycaps, list-row highlight |
| `radius.md` | 12 | Chips, timeline bar, inspector card, tooltips |
| `radius.lg` | 16 | Side sheet, command palette, pickers |
| `radius.full` | 999 | Pills: status dots, toggle tracks |

### 2.3 Type scale

The only UI font is **Inter** (OFL-1.1), bundled. **Inter Tabular** is a derived instance with `tnum` frozen as the default, because egui can't apply OpenType features. It's used for every number, time and value, so digits don't jitter as they change.

| Token | Size / line | Weight | Use |
|---|---|---|---|
| `type.caption` | 12 / 16 | 500 | Secondary labels, units, tick labels, keycaps |
| `type.body` | 13 / 18 | 400 | Default UI text, list rows |
| `type.label` | 15 / 20 | 600 | Chip titles, panel titles |
| `type.title` | 17 / 22 | 600 | Sheet titles, palette input |
| `type.value` | 22 / 28 | 600, tabular | Inspector main value |
| `type.display` | 28 / 34 | 700 | Empty states, onboarding only |

Letter-spacing is −0.01 em at 17 px and up. There's no all-caps text except 12 px eyebrow labels (+0.04 em).

### 2.4 Color

Every text/background pair is checked against WCAG AA in a unit test (`design::tokens::tests::contrast`). Body text must reach ≥ 4.5:1 and large text or icons ≥ 3:1, both measured against the glass surface composited over the *darkest and lightest* plausible map backdrop.

**Dark theme (default):**

| Token | Value | Notes |
|---|---|---|
| `map.land` | `#15181D` | Basemap land, near-black blue-gray |
| `map.water` | `#0D1014` | |
| `map.border.state` | `#FFFFFF` @ 38% | 1.25 px |
| `map.border.county` | `#FFFFFF` @ 12% | 0.75 px; hidden when zoomed far out |
| `map.road` | `#8FA3BF` @ 28% | 1 px |
| `map.label` | `#E6E9EE` @ 80% | Halo `map.land` @ 85% |
| `glass.fill` | `#1B1F26` @ 66% | Drawn over the tone-compressed blurred backdrop (§2.6) |
| `glass.stroke` | `#FFFFFF` @ 9% | Hairline (1 physical px) |
| `glass.highlight` | `#FFFFFF` @ 5% | 1 px inner top edge, the "lit glass" cue |
| `glass.shadow` | `#000000` @ 45%, blur 24, y 8 | `elevation.2` |
| `text.primary` | `#F3F5F8` | |
| `text.secondary` | `#CDD2DA` | |
| `text.tertiary` | `#B3BAC4` | Units, tick labels. Hierarchy comes mostly from size and weight, because AA on glass leaves little room for dim text |
| `accent` | `#5C9DFF` | Focus rings, selected state, live button, scrubber head |
| `accent.on` | `#0B1A33` | Text/icon on an accent fill |
| `state.hover` | `#FFFFFF` @ 6% | |
| `state.pressed` | `#FFFFFF` @ 10% | |
| `state.selected` | `accent` @ 18% | |
| `status.fresh` | `#3CCF8E` | Scan age < 1.5 × the expected volume time |
| `status.aging` | `#F2B640` | |
| `status.stale` | `#FF6363` | Scan age > 3 × expected, or the feed is down |

**Light theme:**

| Token | Value |
|---|---|
| `map.land` | `#ECEEF1` |
| `map.water` | `#D9DEE5` |
| `map.border.state` | `#1D2430` @ 45% |
| `map.border.county` | `#1D2430` @ 14% |
| `map.road` | `#4F6480` @ 30% |
| `map.label` | `#1D2430` @ 85% (halo `map.land`) |
| `glass.fill` | `#FFFFFF` @ 70% |
| `glass.stroke` | `#0B1220` @ 10% |
| `glass.highlight` | `#FFFFFF` @ 60% |
| `glass.shadow` | `#0B1220` @ 16%, blur 24, y 8 |
| `text.primary` | `#121722` |
| `text.secondary` | `#4A5466` |
| `text.tertiary` | `#626C7D` |
| `accent` | `#2167E0` |
| `accent.on` | `#FFFFFF` |
| `status.fresh / aging / stale` | `#13985C` / `#B87800` / `#D93636` |

Radar colors are *data*, not theme. They're the same in both themes. Only the basemap underneath and the radar's alpha (0.92 dark, 0.88 light) change.

**Hazard colors** follow common NWS conventions (tornado red, severe-thunderstorm amber, flash-flood green, and so on) so they read instantly. Those values live in `design/hazards.rs`.

### 2.5 Elevation

| Token | Use |
|---|---|
| `elevation.1` | Chips, timeline: shadow blur 12, y 4, 60% of `glass.shadow` alpha |
| `elevation.2` | Inspector, tooltips, pickers |
| `elevation.3` | Command palette, side sheet: blur 40, y 16 |

### 2.6 Backdrop blur

Gaussian blur, σ ≈ 18 px at 1× scale. It's computed on a ¼-resolution copy of the scene (two separable passes), so the target is ≤ 0.3 ms at 4K on integrated graphics (to be measured in `--bench`). Saturation is ×1.25 in dark mode and ×1.05 in light, so radar color still glows through the glass.

**Tone compression** (learned in the Phase 0 prototype): the blurred backdrop is squeezed into [0, k] in dark mode (k = 0.16) or [1 − k, 1] in light mode (k = 0.35), in linear light, before the glass tint goes on top. Without it, no tint opacity keeps text at AA over a bright 60 dBZ core without making the glass look like a solid box. With it, the `text_contrast_meets_aa` test can prove AA against the worst possible backdrop, and the radar still shows through as colored light.

### 2.7 Motion

| Token | ms | Easing | Use |
|---|---|---|---|
| `motion.fast` | 150 | `cubic-bezier(0.2, 0, 0, 1)` | Hover, press, focus ring, tooltips |
| `motion.base` | 200 | `cubic-bezier(0.2, 0, 0, 1)` | Panels, sheets, pickers open/close |
| `motion.exit` | 150 | `cubic-bezier(0.3, 0, 1, 1)` | Things leaving |
| `motion.zoom` | — | Critically damped spring | Map zoom and fling, ~250 ms settle |

Reduced motion (an `org.gnome.desktop.interface enable-animations=false` / `prefers-reduced-motion` equivalent, or the in-app setting) turns every transition into a 100 ms opacity fade. It also disables fling momentum and pulsing polygons, which become a thicker static outline.

### 2.8 Icons

**Lucide** (ISC), drawn from SVG paths at 1.75 px stroke. Sizes are 16 (in chips and rows) and 20 (icon buttons). Icons are always `text.secondary`, becoming `text.primary` on hover and `accent` when selected. There's no emoji, and no text glyphs standing in for icons.

## 3. Layout

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ┌────────────────────────┐ ┌───────────────────────┐            ┌─────┐  │
│ │ ◉ KTLX  Oklahoma City  │ │ Reflectivity · 0.5° ▾ │            │ ≡ ⌕ │  │
│ │   12 mi · ● 1 min ago  │ └───────────────────────┘            └─────┘  │
│ └────────────────────────┘                                               │
│                                                                          │
│                        RADAR + BASEMAP (full bleed)                       │
│                                                                          │
│                               ┌──────────────────┐                       │
│                         ✛ ────│ 62.5 dBZ         │  inspector card       │
│                               │ 4,100 ft ARL     │                       │
│                               │ 14.2 mi 214° KTLX│                       │
│                               │  9.8 mi 251° me  │                       │
│                               └──────────────────┘                       │
│                                                       ┌────────────────┐ │
│                                                       │ legend (dBZ)   │ │
│ ┌────────────────────────────────────────────────────┐└────────────────┘ │
│ │ ▶  ⟨scan ticks ·······|········⟩  20:16  1×  ●LIVE  │                   │
│ └────────────────────────────────────────────────────┘                   │
└──────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Top-left cluster

- **Site chip.**
  - Line 1: a radar icon, the site ID in `type.label`, and the city in `text.secondary`.
  - Line 2: distance from me (if known), then a status dot with the scan age ("1 min ago", tabular). The dot and age text use `status.*`.
  - Click (or `S`) opens the **site picker**.
- **Product chip.** The same height as the site chip, so the row reads as one band.
  - Line 1: "Reflectivity 0.5°" (tilt in tabular) plus a chevron.
  - Line 2: the source ("Super-res · Level II") and the **sweep strip**, one small dot per tilt in the VCP: filled = in, accent ring = arriving, hollow = pending.
  - Click (or `P`) opens the **product picker**.
- **Bulletins badge** (Phase 3). Appears only when bulletins exist. It's a small round chip with a count, colored by the highest-priority bulletin (warning > watch > statement > discussion > report).

### 3.2 Top-right cluster

Two buttons: a labeled **Menu** button (`menu` icon plus the word "Menu", Ctrl+K), which opens the command palette, and the **side sheet** icon button (`layers` icon, key `L`). The palette's button carries a label because a bare magnifier didn't read as the app's menu (owner, 2026-10-28). In multi-pane mode a **pane layout** button joins them.

### 3.3 Bottom: timeline

A single glass bar, centered, max width 720 px. It's 48 px tall, with 8 px gaps between groups. Left to right:

1. Play/pause icon button (`Space`).
2. **Scrubber.**
   - One tick per frame, spaced by *real scan time*, so gaps in data are visible.
   - The current frame has a 3 px accent head.
   - Hovering a tick shows a tooltip with the time.
   - Drag to scrub; `,` `.` step one frame; Shift jumps to first/last.
3. Frame time (`type.label`, tabular, local or UTC per settings).
4. Speed segmented control (½× 1× 2×), shown on hover/focus, otherwise just the current value.
5. **Latest** button. It shows a pulse dot when there's new data you aren't viewing. Clicking it jumps to the newest scan and stops the loop.

The **progressive sweep** state lives in the scrubber. The newest tick is drawn as a partially filled ring while its volume is still arriving. The product picker's tilt column shows which tilts are *in*, *arriving* (animated fill, or a static half-fill with reduced motion), or *pending* (hollow).

### 3.4 Legend

A compact glass card bottom-right showing the color bar for the current product. The bar is 12 px tall with 4–6 value ticks and units in `text.tertiary`. Hovering the bar shows the value under the pointer. It collapses to a 4 px strip during loop playback if the window is under 1000 px wide.

### 3.5 Inspector

A floating card that follows the pointer, offset 16 px right and below. It flips to stay on screen. No click is needed (hover on desktop, long-press on touch). Contents:

- Value with units (`type.value`, tabular), then the category if any (for example "Hail/Rain" for HC).
- Beam height above radar level (ft or m).
- Range and bearing from the radar (with the site ID) and from me (if location is known).
- An arrow glyph pointing toward the radar.

It appears after 120 ms of rest and hides during drag/pan. Pressing `I` pins it to the map location until the next click.

### 3.6 Side sheet (right)

Hidden by default. It slides in from the right (`motion.base`), 360 px wide, full height minus the window margin, `radius.lg`. A segmented header picks the section: **Layers · Colors · Settings**.

- **Layers:** grouped toggles (Bulletins, Outlooks, Lightning, Storm attributes, Map), each with a short description and per-layer filter.
- **Colors:** per-product palette list with previews, an **Import .pal…** action and **Restore defaults**.
- **Settings:** Appearance (theme, font scale, reduced motion), Units, Time (local/UTC), Loop (frames, speed), Data (providers, cache size), Location, About/licenses.

The map stays interactive while the sheet is open. The sheet is dismissed with `Esc` or by clicking the button again.

### 3.7 Pickers

A picker drops down from its chip (`radius.lg`, `elevation.2`), max 420 px tall, and includes a search field.

- **Site picker:** Favorites first, then Nearest (if location is known), then by state. Each row shows ID, city, distance and status dot. A star toggles a favorite.
- **Product picker:** a two-column grid of product groups (Reflectivity, Velocity, Dual-pol, Derived, Precip, Storm) × tilts. Arrow keys move through it.

### 3.8 Command palette (Ctrl+K)

A centered card, 560 px wide, offset `space.6` from the top, `elevation.3`. The input row uses `type.title`. Under it, a segmented row of group tabs (All · Radars · Products · Layers · Actions · Settings) lets people browse without knowing what to type (owner, 2026-10-29); the Radars tab lists every radar. Results are grouped (Radars, Products, Layers, Actions, Settings), each row showing an icon, a label, a secondary hint and a right-aligned shortcut keycap. Fuzzy matching. `Enter` runs the selected row; `Esc` closes.

### 3.9 Shortcuts sheet (`?`)

A centered glass card with a 2-column list of keycaps and actions, grouped. Tooltips everywhere show their shortcut keycap.

### 3.10 Multi-pane

Two (side by side, or stacked when taller than wide) or four (2×2) panes. Panes are separated by a 1 px `glass.stroke` divider with a 6 px grab zone. The camera is shared. Each pane has its own **product chip** in its top-left; the site chip appears once, in pane 1. The timeline and legend are shared (the legend shows the focused pane's product).

## 4. States

| State | Treatment |
|---|---|
| **First launch (no location, no cache)** | Opens on a CONUS view with site markers and a centered glass card: "Choose a radar" + search field + "Use my location". It dismisses itself as soon as a site is picked |
| **Loading first sweep** | Site chip shows a spinner instead of the dot. The map shows the basemap and the site's range ring at 50% alpha. No blocking overlay |
| **Progressive scan** | See §3.3. Lower tilts are viewable as soon as they arrive |
| **Live, fresh** | Green dot + age (age text stays `text.secondary`, so the status color never has to carry small text at AA) |
| **Aging / stale** | Amber or red dot and age. When stale, a one-line notice appears in the site chip: "No new data from KTLX for 14 min" |
| **Offline** | The site chip shows a `wifi-off` icon + "Offline · showing cached 20:16". Retries with backoff silently; never blocks interaction |
| **Archive mode** | The scan time in the timeline becomes an accent-colored date chip ("May 20, 2013 · 20:16 UTC"). The Latest button reads "Return to live" |
| **Loop playing** | Play icon becomes pause; the inspector hides; the legend may collapse (§3.4) |
| **Error decoding a file** | Non-modal toast bottom-center, auto-dismiss 5 s, with "Details" |
| **Focus (keyboard)** | 2 px `accent` ring outside the element radius, offset 2 px, always visible on keyboard navigation |

## 5. Responsiveness

- **Minimum 900×600.** Below 1000 px wide, the window margin drops to 12, the legend docks into the timeline's right end, and the product chip text truncates the product name before the tilt.
- HiDPI and fractional scaling: all tokens are logical px and the renderer uses the surface scale factor. Hairlines are 1 *physical* px.
- Touch (where available): long-press = inspector; two-finger pan/zoom.

## 6. Accessibility

- AccessKit labels on every component, from the component library, so they can't be forgotten.
- Visible keyboard focus everywhere; full keyboard reachability through the palette.
- Colorblind-safe default color tables (§7), with alternates.
- All text meets WCAG AA (§2.4 test).

## 7. Radar color tables (original)

Our own tables are defined as control points in `hookecho-render/src/palettes/`:

- **Reflectivity "Hook Echo Standard":** cool blue-green at low dBZ, warm yellow → orange → red at the convective range, and magenta → white above 65 dBZ. It's tuned so the 40–60 dBZ structure (hooks, notches, BWERs) has the most luminance contrast. Low values (< 10 dBZ) are transparent by default; an "Expert" variant shows everything.
- **Velocity "Hook Echo Diverging":** inbound teal → near-black at 0 → outbound coral. It's colorblind-safe (blue/orange axis, not red/green), with saturated extremes and a purple range-folded color.
- **Alternates for every product:** perceptually uniform options derived from the matplotlib `viridis`, `cividis`, `inferno` and `plasma` maps (CC0), and a "Classic 16-level" look for reflectivity.

## 8. Component library

Located in `hookecho-app/src/design/components/`. Every screen is built only from these components; stock egui widgets never appear.

| Component | Notes |
|---|---|
| `GlassPanel` | Backdrop blur region + fill + hairline + highlight + shadow; the base of everything floating |
| `Chip` | Glass, `radius.md`, icon + 1–2 lines; hover/press/focus states |
| `IconButton` | 36×36 hit target, 20 px icon, tooltip with shortcut |
| `Sheet` | Side sheet container with a segmented header |
| `Palette` | Command palette; also powers pickers' search and lists |
| `Slider` | Thin track, 14 px thumb, tabular value |
| `Toggle` | 32×18 pill |
| `Segmented` | Glass track, selected item filled `state.selected` |
| `Tooltip` | Small glass, `type.caption`, keycap support |
| `Card` | Inspector and notices |
| `Keycap` | 12 px, `radius.sm`, used in tooltips, palette and the cheat sheet |
| `Timeline` | Composite of the above plus the scrubber |
