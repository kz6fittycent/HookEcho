# Feature parity matrix

Hook Echo targets 1:1 *feature and workflow* parity with the commercial viewer described in the public sources below. Appearance, icons, color tables and wording are our own.

**Status values:**

- **planned:** not started.
- **in progress:** being built.
- **done:** implemented and tested.
- **verified:** the owner has checked it on real weather.

**Tier column** (as described in public material):

- **Base:** included with the paid base app.
- **T1 / T2:** Pro Tier One / Two.
- **3P:** needs a third-party account (Spotter Network, AllisonHouse, WeatherOps).
- **Mixed:** public sources disagree on the tier, or it changed between versions.

Every Hook Echo feature is free. The tier column only records how the original gates the feature.

**Phase column:** the KICKOFF phase where we build it. **P5** means "close parity gaps" (order to be approved).

Sources (all public, read 2026-10-09):

[getting-started]: https://radarscope.zendesk.com/hc/en-us/articles/8970695921170-Getting-Started
[upgrading]: https://radarscope.zendesk.com/hc/en-us/articles/8991862852754-Upgrading-Your-RadarScope-Experience
[appstore]: https://apps.apple.com/us/app/radarscope/id288419283
[radar-list]: https://radarscope.zendesk.com/hc/en-us/articles/8990487931794-Radar-List
[data-overview]: https://radarscope.zendesk.com/hc/en-us/articles/4411519606802-Radar-Data-Overview
[palettes]: https://radarscope.zendesk.com/hc/en-us/articles/4411527535634-Color-Palettes
[pal-tech]: https://radarscope.zendesk.com/hc/en-us/articles/11683929347602-Color-Palette-Reference-Technical-Guide
[pal-import]: https://radarscope.zendesk.com/hc/en-us/articles/4642221122834-Importing-Custom-Color-Palettes
[providers]: https://radarscope.zendesk.com/hc/en-us/articles/4411492912146-Data-Provider-Settings
[locations]: https://radarscope.zendesk.com/hc/en-us/articles/8990554970002-Saved-Location-Markers
[radar-legend]: https://radarscope.zendesk.com/hc/en-us/articles/8988864893202-Radar-Legends
[product-legend]: https://radarscope.zendesk.com/hc/en-us/articles/8989474231058-Product-Legend
[refl]: https://radarscope.zendesk.com/hc/en-us/articles/4411528274194-Reflectivity-Products
[vel]: https://radarscope.zendesk.com/hc/en-us/articles/4411511450514-Velocity-Products
[srv]: https://radarscope.zendesk.com/hc/en-us/articles/6392750778514-Super-Res-Storm-Relative-Velocity
[dualpol]: https://radarscope.zendesk.com/hc/en-us/articles/4411535488018-Dual-Polarization-Products
[precip]: https://radarscope.zendesk.com/hc/en-us/articles/4411535395602-Precipitation-Products
[other]: https://radarscope.zendesk.com/hc/en-us/articles/4411519694354-Other-Products
[classic]: https://radarscope.zendesk.com/hc/en-us/articles/4411511509138-Classic-Products
[tdwr]: https://radarscope.zendesk.com/hc/en-us/articles/4411535574546-TDWR-Products
[intl-au]: https://radarscope.zendesk.com/hc/en-us/articles/4411519739922-Australian-Products
[intl-ca]: https://radarscope.zendesk.com/hc/en-us/articles/4411519781650-Canadian-Products
[intl-fi]: https://radarscope.zendesk.com/hc/en-us/articles/4411519812498-Finnish-Products
[intl-de]: https://radarscope.zendesk.com/hc/en-us/articles/4411519837074-German-Products
[layers]: https://radarscope.zendesk.com/hc/en-us/articles/8990755805970-Layers
[bulletins]: https://radarscope.zendesk.com/hc/en-us/articles/8989939553682-Weather-Bulletins
[tools]: https://radarscope.zendesk.com/hc/en-us/articles/8989847357970-Map-Interaction-Tools
[shortcuts]: https://radarscope.zendesk.com/hc/en-us/articles/8991008565906-Shortcuts
[sharing]: https://radarscope.zendesk.com/hc/en-us/articles/8990587809810-Sharing-Content
[advanced]: https://radarscope.zendesk.com/hc/en-us/articles/8991096219922-Advanced-Settings
[spotters]: https://radarscope.zendesk.com/hc/en-us/articles/8990529831314-Spotter-Locations
[accounts]: https://radarscope.zendesk.com/hc/en-us/articles/4411527453202-Accounts
[whatsnew57]: https://radarscope.zendesk.com/hc/en-us/articles/39202500581778-What-s-New-in-Radarscope-5-6-and-5-7
[widget]: https://radarscope.zendesk.com/hc/en-us/articles/8991265059346-Home-Screen-Widget
[wearables]: https://radarscope.zendesk.com/hc/en-us/articles/8991326899090-Wearable-Devices
[not-updating]: https://radarscope.zendesk.com/hc/en-us/articles/4411219111314-Radar-Not-Updating
[scwx]: https://github.com/dpaulat/supercell-wx/issues

## 1. Radar sites and data access

| Feature | Tier | Source | Phase | Status | Notes |
|---|---|---|---|---|---|
| Single-site radar in native radial (polar) format | Base | [appstore] | P1 | done | GPU polar-to-screen in the fragment shader |
| NEXRAD sites incl. Guam, South Korea, Okinawa | Base | [providers] | P1 | done | All exist in the Unidata Level II buckets |
| Pick a radar by clicking its map marker ("radar selection mode") | Base | [getting-started] | P1 | done | Our version: site markers appear when zoomed out or while the site picker is open |
| Searchable radar list grouped by state/region | Base | [radar-list] | P1 | done | Site chip → searchable picker; also in the command palette |
| Favorite radars (add, remove, reorder) | Base | [radar-list] | P2 | planned | Pinned at the top of the site picker |
| Radar status dot (receiving / late) | Base | [radar-list] | P2 | planned | From the newest chunk/volume age per site; one cheap S3 listing, cached |
| Radar detail with latest operator status message | Base | [radar-list] [not-updating] | P3 | planned | Level III Free Text Message (FTM) product, ICD 2620001 |
| Nearest radar from my location | Base | [getting-started] | P3 | in progress | Done: radar list nearest-first from the map center with distances; a home point ("Set home", then click the map) that a first run opens near; "Nearest radar" command. Next: GeoClue location |
| Auto radar selection as location changes (toggle) | Base | [advanced] | P3 | planned | |
| Show TDWR sites (off by default) | Base | [advanced] [tdwr] | P5 | done | Listed in the radar picker ("· airport") and on the map. On by default (the owner asked for them); side sheet → Layers → "Airport radars (TDWR)" turns them off (a favorite TDWR stays). 44 of 47 published on 2026-10-28; TJBQ, TJRV and TPBI had no scans |
| Show research radars (off by default) | Base | [advanced] [whatsnew57] | P5 | planned | Free public research feeds only; list TBD |
| Data update timed to the VCP schedule (no wasteful polling) | Base | [data-overview] | P2 | in progress | Polls the chunk bucket every 4 s with `start-after` (only new keys come back); state changes only, so the idle app is never woken. Predicting chunk times from the VCP is a later refinement |
| Multiple data providers with automatic fallback | Base | [providers] | P4 | planned | Free substitutes: Unidata chunks → NOAA NODD `noaa-nexrad-level2` → NWS Level III. No paid feeds |
| Third provider (Amazon) | T2 | [providers] [appstore] | P1 | planned | AWS Open Data is our *primary* source |
| Archive: Level II back to 1991 | T2 | [upgrading] [appstore] | P5 | planned | `unidata-nexrad-level2` / NODD |
| Archive: Level III (super-res 30 days, non-super-res since 2020) | T2 | [data-overview] | P5 | planned | `unidata-nexrad-level3`; depth depends on bucket retention |
| Archive date/time picker from the product timestamp | T2 | [product-legend] | P5 | planned | Click the scan-time label in the timeline |
| Canadian radars (MSC) | Base | [intl-ca] | P5 | planned | MSC Datamart open data (ODIM HDF5). New decoder |
| Australian radars (BoM) | Base | [intl-au] | P5 | planned | Needs a confirmed *free* volume-data feed; otherwise drop it with a note |
| German radars (DWD) | Base | [intl-de] | P5 | planned | DWD Open Data (ODIM HDF5) |
| Finnish radars (FMI) | Base | [intl-fi] | P5 | planned | FMI Open Data (ODIM HDF5) |

## 2. Radar products

| Feature | Tier | Source | Phase | Status | Notes |
|---|---|---|---|---|---|
| Super-res reflectivity, 4 lowest tilts (default product) | Base | [refl] | P1 | done | We use Level II super-res directly (0.25 km × 0.5°) |
| Level II reflectivity, all tilts | T2 | [refl] | P1 | done | Free |
| Super-res velocity, 4 tilts | Base | [vel] | P1 | done | Level II |
| Level II velocity, all tilts | T2 | [vel] | P1 | done | |
| Super-res and Level II spectrum width | Base/T2 | [vel] | P1 | done | |
| Differential reflectivity (ZDR), standard and super-res | Base | [dualpol] | P1 | done | Level II moment |
| Correlation coefficient (CC/RHO), standard and super-res | Base | [dualpol] | P1 | done | Level II moment |
| Differential phase (PHI) | Base | [palettes] | P1 | done | Level II moment |
| Specific differential phase (KDP) | Base | [dualpol] | P3 | planned | **Not a Level II moment.** Decided: Level III product 163 (owner, 2026-10-09) |
| Hydrometeor classification (HC) | Base | [dualpol] | P3 | planned | Level III product 165 |
| Hybrid hydrometeor classification (HHC) | Base | [dualpol] | P3 | planned | Level III product 177 |
| Super-res storm-relative velocity | Base | [srv] [appstore] | P3 | planned | L2 velocity minus the storm-motion component per azimuth. Motion from the L3 storm-motion metadata or a user-set vector |
| Classic storm-relative velocity | Base | [classic] | P3 | planned | Level III product 56 |
| Digital reflectivity / digital velocity (L3) | Base | [refl] [vel] | P3 | planned | Products 94/99 |
| Composite reflectivity | Base | [classic] | P3 | planned | L3 product 37/38, or computed from the L2 volume on the GPU |
| Digital VIL | Base | [other] | P3 | planned | Product 134 |
| Enhanced echo tops (with "topped" range) | Base | [other] [pal-tech] | P3 | planned | Product 135; the TOPPED mask in palettes |
| Precipitation: DAA, DSA/DTA, DOD, DSD, storm total, one-hour, storm total (classic) | Base | [precip] [classic] | P3 | planned | Products 170, 172, 173, 174, 175, 78, 80 |
| Precipitation depiction (rain/mix/snow) | Base | [refl] | P5 | planned | **Proprietary** (radar plus surface obs). Free substitute: color reflectivity by MRMS `PrecipFlag` or by the HC class. It will differ from the original |
| TDWR long-range and short-range reflectivity, velocity | Base | [tdwr] | P5 | done | Pulled forward by the owner (2026-10-28). Shown live and in the loop, one scan about every 6 minutes (`hookecho-core::tdwr`): short-range reflectivity (TZ0) and velocity (TV0), plus long-range reflectivity (TZL, 225 nm) as a second sweep, chosen with side sheet → Layers → "Airport radar long range" |
| Tilt selection | Base | [product-legend] | P1 | done | Product chip → picker; `[`/`]` keys |
| Quick product/tilt grid ("product picker" shortcut) | Base | [shortcuts] | P2 | planned | Product chip opens a grid: product × tilt |
| Progressive sweep display while the volume is still scanning | — | KICKOFF | P2 | done | Real-time chunks decoded as they arrive; the previous volume shows under a sweep still arriving; sweep strip shows in / arriving / pending tilts |
| Velocity dealiasing | — | KICKOFF, [scwx] #290 | P4 | planned | Algorithm and source documented in PROVENANCE.md |
| Forecast models (8 models incl. RRFS, HRRR), run/hour pickers | T2 | [whatsnew57] [appstore] | P5 | planned | NOAA NODD GRIB2 on AWS (free). Large scope: needs a GRIB2 decoder |
| MRMS national mosaic products | T2 | [upgrading] [whatsnew57] | P5 | planned | `noaa-mrms-pds` on AWS (free) |
| GOES satellite layers | T2 | [upgrading] | P5 | done | GOES-East (`noaa-goes19` on AWS, owner-approved) CONUS imagery under the radar: visible (band 1, 1 km), infrared (band 13, enhanced colors for cold tops), water vapor (band 9). Layers → Satellite. Reprojected per pixel on the GPU from the ABI fixed grid. Not yet: band 2 (0.5 km visible, 65 MB a scan), GOES-West, a satellite loop |
| RTMA surface analysis | T2 | [appstore] | P5 | planned | NODD (free) |
| Surface observations | T2 | [appstore] | P5 | done | aviationweather.gov METARs (owner-approved 2026-10-29): station plots (temperature, dewpoint, wind barb) when zoomed in; click one to pin it to the station panel. Layers → Surface observations |
| ProbSevere | T2 | [appstore] | P5 | planned | NOAA/CIMSS public feed; confirm terms |
| Soundings and hodographs | T2 | [upgrading] [appstore] | P5 | planned | Model soundings from NODD GRIB2; RAOB from a free NOAA source |

## 3. Map layers and overlays

| Feature | Tier | Source | Phase | Status | Notes |
|---|---|---|---|---|---|
| Base map: states, counties, coastlines | Base | [layers] | P1 | done | Bundled Natural Earth + TIGER, lyon-tessellated |
| Cities and towns (≈32k), decluttered by zoom | Base | [layers] | P1 | done | Census places ranked by 2024 population estimates (census-designated places: a low guess from land area); label collision on the GPU (glyphon) |
| Roads: interstates and US highways | Base | [layers] | P1 | in progress | TIGER primary roads (interstates and other limited-access highways) bundled; the rest of the US highways need TIGER secondary roads |
| Detailed roads and road labels | T2 | [layers] [providers] | P5 | planned | Paid (Mapbox) in the original. Substitute: TIGER secondary roads bundled at higher zoom |
| Satellite imagery base map | T2 | [layers] | P5 | planned | Paid (Mapbox) in the original. Free option needs a tile source, which conflicts with "no tile server". **Ask first** |
| NWS warnings: tornado, severe t-storm, flash flood, special marine, snow squall, extreme wind | Base | [bulletins] | P3 | done | api.weather.gov alerts (`hookecho-core::alerts`), 60 s polling with ETag/If-None-Match, last answer cached on disk. NWS map colors (`design/hazards.rs`), heavier outline for tornado and extreme wind. Click a warning for its full text (card below the site chip; overlapping warnings stack). No requests while switched off |
| Hurricane, tropical storm, fire, dust storm, storm surge warnings | Base | [appstore] | P3 | planned | Same feed |
| Pulsing polygons for emergencies and PDS | Base | [providers] | P3 | planned | From the NWS alert parameters/text; honor reduced-motion |
| Warning type filter | Base | [bulletins] | P3 | done | Side sheet → Layers → Warnings: master switch plus one per type |
| Tornado and severe thunderstorm watches (US) | Mixed | [bulletins] [appstore] | P3 | done | SPC active watch outlines (yellow tornado, pale violet red severe; heavier for PDS) with the watch text on click. Layers → Watches |
| Canadian warnings and watches; Australian warnings | Base | [bulletins] [advanced] | P5 | planned | ECCC GeoMet (free); BoM (check terms) |
| Special weather statements | T1 | [upgrading] [bulletins] | P3 | planned | NWS alerts |
| SPC mesoscale discussions | Mixed | [upgrading] [bulletins] | P3 | done | Dashed outlines with the full discussion text on click. Layers → Mesoscale discussions |
| SPC convective outlooks: categorical, tornado, hail, wind | Mixed | [shortcuts] [appstore] | P3 | done | Days 1–2 categorical, tornado, wind, hail; day 3 categorical and any-severe. SPC's colors, translucent over the radar; click for the risk there. Layers → Outlook. Not yet: conditional-intensity hatching |
| SPC fire weather outlooks | T1 | [appstore] | P5 | planned | spc.noaa.gov |
| WPC excessive rainfall outlook | T1 | [appstore] | P5 | planned | wpc.ncep.noaa.gov |
| Local storm reports, with type filter | Mixed | [bulletins] | P5 | planned | NWS LSR products / IEM (free) |
| Lightning, animated with the loop, fading over 30 min | T1 | [layers] [providers] | P3 | planned | **Substitute:** GOES GLM (total lightning, ~8 km pixel, satellite). It differs from the original's ground-network cloud-to-ground strikes: no CG/IC split, lower spatial precision |
| Storm tracks (SCIT) with 15-min cross-lines and cell detail | Base | [layers] | P3 | planned | Level III product 58 |
| Hail, mesocyclone, TVS markers | — | KICKOFF | P3 | planned | Level III products 59, 141, 61 |
| Derived contours: hail probability, hail size, azimuthal shear (24 h) | T2 | [layers] [shortcuts] | P5 | planned | Proprietary in the original. Substitute: MRMS MESH, POSH and rotation tracks (free) |
| mPING reports layer, with filter | Base | [bulletins] | P5 | planned | Needs an mPING API key (free, but an account). **Ask first** |
| Spotter Network reports and locations | 3P | [spotters] [bulletins] | P5 | planned | Needs a user account. **Ask first** (phones home) |
| Custom location markers layer | Base | [locations] | P3 | planned | |
| WeatherOps enterprise assets layer | 3P | [layers] [accounts] | — | won't do | Enterprise service. Substitute: placefile/GeoJSON import (beyond parity) |
| Notification badge: count of active bulletins, colored by highest priority | Base | [bulletins] | P3 | planned | Shown in the top-left cluster; opens the bulletins list |
| Bulletin list and detail, with raw-text toggle | Base | [bulletins] [appstore] | P3 | planned | |
| Layer quick-toggle grid | Base | [shortcuts] | P2 | in progress | Side sheet (`L`) → Layers: county lines, highways, place names (saved). Also toggled from the command palette. Bulletin/outlook/lightning layers join in P3 |

## 4. Tools and interaction

| Feature | Tier | Source | Phase | Status | Notes |
|---|---|---|---|---|---|
| Pan, zoom, double-click zoom, keyboard zoom | Base | [getting-started] [shortcuts] | P1 | done | Smooth zoom with momentum |
| Prefer zoom over scroll on trackpads (toggle) | Base | [advanced] | P2 | planned | |
| Inspector: value under the pointer, with direction-to-radar indicator | T1 | [tools] [upgrading] | P2 | in progress | Hover card with value, beam height ARL, range/bearing from the radar (Phase 1). Distance from me arrives with location (P3); pin with `I` |
| Distance tool with beam height, snapping to radar/spotters/reports | Base | [tools] | P3 | planned | |
| Draw tool (freehand annotations) | Base | [tools] | P5 | planned | |
| My location (GPS/platform location) | Base | [getting-started] | P3 | planned | GeoClue (D-Bus) on Linux |
| Saved locations: add by search or current position, rename, reorder, delete | Base | [locations] | P3 | planned | Geocoding needs a free service. **Ask first** (phones home); offline fallback: Census places gazetteer |
| Import/export locations as JSON (≤ 250) | Base | [locations] | P3 | planned | Same `name/lat/lon` shape, for interoperability |
| Loop animation | Base | [getting-started] | P2 | done | Earlier volumes at the current tilt from the real-time bucket (only the chunks up to that tilt), newest first; plays oldest → live with a longer hold on live |
| Loop length: 10 / 30 / 50 frames | Base/T1/T2 | [appstore] [upgrading] | P2 | done | Side sheet → Settings → Loop (saved; default 10) |
| Previous/next frame; jump to first/last | Base | [getting-started] | P2 | done | `,` `.` step; Shift+`,`/Home first; Shift+`.`/End live |
| Loop scrubber | Base | [shortcuts] | P2 | done | Ticks spaced by real scan time; click or drag to scrub |
| Loop speed | Base | [advanced] | P2 | done | ½× / 1× / 2× on the timeline (saved) |
| Color bar with value on hover | Base | [product-legend] | P2 | done | Hover the legend: the value and units under the pointer, with a marker on the bar |
| Product time plus "updated" time; red when stale | Base | [product-legend] | P2 | in progress | Scan age + green/amber/red dot vs the VCP volume time, local sweep time, offline notice; the explicit "updated at" time comes with the timeline |
| Radar legend: VCP number and clear-air / precipitation mode | Base | [radar-legend] | P2 | planned | In the site chip detail |
| Dual-pane (2 products side by side) | T1 | [upgrading] [getting-started] | P3 | planned | Shared camera |
| Quad-pane | T1 | [getting-started] [appstore] | P3 | planned | |
| Keyboard shortcuts | Base | [shortcuts] | P2 | done | `?` opens a grouped cheat sheet (built from the same table as the command palette's keycaps); tooltips show keys |
| Share/export a still image, with default caption text | Base | [sharing] [advanced] | P4 | planned | PNG export + copy to clipboard; caption = site/product/time |
| Share/export the loop as GIF/video | Base | [sharing] [scwx] #414 | P5 | planned | GIF/APNG first; video needs an encoder license check |
| Submit mPING report | Base | [sharing] | P5 | planned | **Ask first** (outbound) |
| Submit Spotter Network report; report my location every 2 min | 3P | [sharing] [advanced] | P5 | planned | **Ask first** (account + outbound) |
| URL scheme / deep links | Base | [appstore] | P5 | planned | `hookecho://site/KTLX?product=...` |

## 5. Settings and appearance

| Feature | Tier | Source | Phase | Status | Notes |
|---|---|---|---|---|---|
| Built-in alternate palettes per product, incl. colorblind-friendly ones | Base | [palettes] | P2 | done | Our own originals: Standard (default, colorblind-aware), Classic, Classic red/green velocity, Grayscale. Click the legend to choose; the choice persists per product |
| Import custom palettes in the GRLevelX `.pal` format with extensions (Color, Color4, SolidColor, SolidColor4, RF, Scale, Units, masks RAIN/MIX/SNOW/TOPPED, indexed `???` units) | T1 | [pal-tech] [pal-import] | P2 / P3 | in progress | Drop a `.pal` on the window. Done: Product, Units (speed units imply Scale), Scale, Offset, RF, Color/Color4 (with second color), SolidColor/SolidColor4; parser fuzzed (`pal_parse`); unknown statements ignored. Imported palettes can be removed (✕ in the palette list). Moved to P3: masks RAIN/MIX/SNOW/TOPPED and indexed `???` units, which need Level III hydrometeor classes and echo tops |
| Restore default palettes | Base | [palettes] | P2 | done | Pick "Hook Echo Standard" in the legend's palette list |
| Theme: dark (default), light, follow system | Base | [advanced] | P2 | done | Side sheet → Settings → Appearance (saved) |
| Font scale for map labels | Base | [advanced] | P2 | done | Side sheet → Settings → Map text: S / M / L / XL (saved); also in the command palette |
| Metric units toggle | Base | [advanced] | P2 | done | Side sheet → Settings: distance (mi/ft or km/m) and wind speed (kt, mph, km/h, m/s) separately; also in the command palette (saved) |
| UTC time display mode | Base | [appstore] | P2 | done | Side sheet → Settings → Clock: Local or UTC (archived scans always show the UTC date) |
| Prevent the screen from sleeping while open | Base | [advanced] | P4 | planned | Desktop: the idle-inhibit portal (snap `desktop` plug) |
| International products: choose countries | Base | [advanced] | P5 | planned | |
| External display "TV mirroring" (full-res map, no chrome) | Base | [advanced] | P5 | planned | Desktop analog: presentation/fullscreen mode (`F`) |
| Default share caption toggle | Base | [advanced] | P4 | planned | |

## 6. Platform and account features (not applicable or substituted)

| Feature | Tier | Source | Phase | Status | Notes |
|---|---|---|---|---|---|
| Accounts, subscriptions, cross-device sharing | T2 | [accounts] | — | won't do | No accounts by design |
| Home-screen / lock-screen widgets | Base | [widget] | — | won't do | Mobile-only. Possible later: a GNOME/KDE panel applet |
| Apple Watch / Wear OS companion apps | Base | [wearables] | — | won't do | Mobile-only |
| CarPlay / Apple TV | Base | [whatsnew57] [appstore] | — | won't do | Platform-specific; our "car" story is a laptop with integrated graphics |
| AllisonHouse as an alternate (paid) provider | 3P | [providers] | — | won't do | Paid feed |

## 7. Beyond parity (lower priority)

Requests users raise against other viewers (mainly [Supercell Wx issues][scwx]) that the original *also* lacks or limits:

| Feature | Source | Phase | Status | Notes |
|---|---|---|---|---|
| GRLevelX-style placefiles | [scwx] #614, #88 | P5+ | planned | Big ecosystem for chasers |
| GeoJSON / KML / shapefile overlays | [scwx] #638, #655, #514 | P5+ | planned | |
| Vertical cross-section tool | [scwx] #403 | P5+ | planned | We have the full L2 volume on the GPU |
| Hide dual-pol noise: CC, ZDR and PHI only where there's echo (on by default; side sheet → Layers, command palette) | Owner review (Phase 1 follow-up) | P2 | done | Co-located reflectivity ≥ 5 dBZ; the inspector follows the same rule |
| "Command center": a station panel beside the radar with temperature, dewpoint, pressure and its trend, wind and gusts, visibility, plus nearby observations on the map | Owner request (2026-10-10) | P5 | in progress | Done: the station panel (bottom left, the station nearest home or the map center, or a pinned one) with temperature, dew point and RH, wind and gusts, pressure with its 3-hour trend, visibility and weather; station plots on the map. Next: more stations at once, history graphs |
| Command palette (Ctrl+K or the search button): every site, product, layer, action, setting and palette by typing, fuzzy matched and grouped | KICKOFF §5 | P2 | done | Enter runs the top match; arrows move; Esc closes |
| Says so when there's little or no precipitation in range (clear air), so a blank map doesn't look broken | Owner feedback (CFT #8) | P2 | done | Under 20 km² of drawn reflectivity in runs of 1 km or more, so clutter speckle doesn't count |
| Rotational velocity / NROT readout in the inspector | [scwx] #160, #206 | P5+ | planned | |
| Space weather: Kp index, solar wind, X-ray flux and flares, SWPC alerts, aurora forecast | Owner request (2026-10-28) | P5 | done | NOAA SWPC JSON (owner-approved 2026-10-29). Menu → "Space weather": R/S/G now, Kp now and a 3-day past/3-day forecast Kp chart, solar wind (speed, density, Bz, Bt), X-ray class and latest flare, the 3-day outlook, latest alerts. Layers → Aurora forecast: the OVATION grid on the map. Not yet: an X-ray flux chart |
| Radar mosaic: the latest reflectivity of every radar on screen, nearest radar wins, then single-radar mode on a storm | Owner question (2026-10-28) | P5+ | planned | Reflectivity only (velocity can't be mosaicked). Uses data we already fetch, or NOAA MRMS (new endpoint) |
| VAD wind profile (VWP) display | [scwx] #383 | P5+ | planned | Level III product 48 |
| Audible alerts / TTS for new warnings near me; "follow" an alert | [scwx] #352, #637, #617, #581 | P5+ | planned | Local only, no push service |
| Per-pane independent site/time (not only product) | [scwx] #77, #183 | P5+ | planned | |
| Inspector values on all panes at once | [scwx] #616 | P5+ | planned | |
| Reflectivity gate filter / low-dBZ threshold slider | [scwx] #480 | P5+ | planned | Shader uniform, essentially free |
| Map-wide multi-radar mosaic from Level II | [scwx] #78 | P5+ | planned | Beyond MRMS; our own GPU mosaic |
| Export the raw Level II file currently viewed | [scwx] #176 | P5+ | planned | |
| GPSD network connection for in-car GPS | [scwx] #367 | P5+ | planned | Useful for chasers without GeoClue |
| Advanced drawing tools (shapes, text, persistence) | [scwx] #590, #80 | P5+ | planned | |
| Offline cache of recent site data (cold start without network) | [scwx] #613 | P2 | done | Chunk disk cache (512 MB cap); a site reopens on its last cached volume before any network answer. Side sheet → Settings → Data shows its size and clears it |
