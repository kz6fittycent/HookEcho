# Provenance

This file gives a public source for every data format, algorithm and bundled asset used by Hook Echo. Add an entry in the same commit that introduces the code.

## Data formats

| Item | Source | Used in |
|---|---|---|
| NEXRAD Level II (Archive II, Message 31, Message 1 legacy) | NOAA ROC, *Interface Control Document for the RDA/RPG*, ICD 2620002 (latest revision), <https://www.roc.noaa.gov/interface-control-documents.php> | via `nexrad-decode` (MIT) |
| NEXRAD Level II real-time chunk layout | Unidata, *NEXRAD Level II real-time data on AWS*, <https://registry.opendata.aws/noaa-nexrad/> and the `unidata-nexrad-level2-chunks` bucket README | via `nexrad-data` (MIT) |
| NEXRAD Level III products | NOAA ROC, *ICD for the RPG to Class 1 User*, ICD 2620001 | `hookecho-core::level3` (planned) |
| GRLevelX color table (`.pal`) | Gibson Ridge Software, *Color Table File Specification* (public), plus the extensions publicly documented in the commercial viewer's help center (Color4, SolidColor4, masks, indexed `???` units) | planned (Phase 2) |

## Algorithms

| Algorithm | Source | Used in |
|---|---|---|
| Beam height (4/3 effective-earth model) | Doviak & Zrnić, *Doppler Radar and Weather Observations*, 2nd ed., eq. 2.28b | prototype inspector; `hookecho-core` (planned) |
| Storm-relative velocity (subtract the storm-motion component per azimuth) | NWS JetStream, *Storm Relative Velocity*; publicly described in the commercial viewer's SRV help article | planned (Phase 3) |
| Gaussian backdrop blur (separable, downsampled) | Standard technique; e.g. Rákos, *Efficient Gaussian blur with linear sampling* (2010) | prototype; `hookecho-render` |

## Bundled assets

| Asset | License | Source |
|---|---|---|
| Inter typeface | OFL-1.1 | <https://github.com/rsms/inter> |
| Inter Tabular (derived: `tnum` frozen as default) | OFL-1.1 | Generated from Inter with `opentype-feature-freezer` (Apache-2.0) |
| Lucide icons | ISC | <https://github.com/lucide-icons/lucide> |
| US state, county and primary-road boundaries | Public domain (US Census Bureau) | Census Cartographic Boundary Files and TIGER/Line, <https://www.census.gov/geographies/mapping-files.html> |
| Natural Earth (coastlines, lakes, populated places) | Public domain | <https://www.naturalearthdata.com> |
| Sample Level II volume KTLX 2013-05-20 20:16:43 UTC | Public domain (NOAA) | `s3://unidata-nexrad-level2/2013/05/20/KTLX/KTLX20130520_201643_V06.gz` |
