# Provenance

This file gives a public source for every data format, algorithm and bundled asset used by Hook Echo. Add an entry in the same commit that introduces the code.

## Data formats

| Item | Source | Used in |
|---|---|---|
| NEXRAD Level II (Archive II, Message 31, Message 1 legacy) | NOAA ROC, *Interface Control Document for the RDA/RPG*, ICD 2620002 (latest revision), <https://www.roc.noaa.gov/interface-control-documents.php> | via `nexrad-decode` (MIT) |
| NEXRAD Level II real-time chunk layout (`SITE/vol/YYYYMMDD-HHMMSS-seq-S/I/E`, volumes 1–999 in a ring; S = volume header + metadata record, I/E = one LDM record) | Unidata, *NEXRAD Level II real-time data on AWS*, <https://registry.opendata.aws/noaa-nexrad/>; verified against the live bucket 2026-10-10 | `hookecho-core::realtime`, `hookecho-core::live` |
| S3 ListObjectsV2 (anonymous) | AWS S3 API reference, *ListObjectsV2*, <https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html> | `hookecho-core::s3` |
| NEXRAD Level III products | NOAA ROC, *ICD for the RPG to Class 1 User*, ICD 2620001 | `hookecho-core::level3` (planned) |
| NEXRAD site list | NOAA NCEI HOMR, *nexrad-stations.txt*, <https://www.ncei.noaa.gov/access/homr/file/nexrad-stations.txt> | `hookecho-data sites` → `crates/hookecho-core/data/sites.tsv` |
| Archive II file layout (volume header, LDM bzip2 records, uncompressed 2432-byte segments) | NOAA ROC ICD 2620010 (Archive II/User); cross-checked against MetPy `metpy/io/nexrad.py` | `tools/trim_level2.py` |
| GRLevelX color table (`.pal`) | Gibson Ridge Software, *Color Table File Specification* (public), plus the extensions publicly documented in the commercial viewer's help center (Color4, SolidColor4, masks, indexed `???` units) | `crates/hookecho-core/src/pal.rs` (masks and indexed units not yet) |

## Algorithms

| Algorithm | Source | Used in |
|---|---|---|
| Beam height (4/3 effective-earth model) | Doviak & Zrnić, *Doppler Radar and Weather Observations*, 2nd ed., eq. 2.28b | prototype inspector; `hookecho-core` (planned) |
| Dual-pol noise mask: hide ZDR, PHI and CC where the co-located reflectivity is under 5 dBZ or missing | Original (Hook Echo). Rationale: dual-pol estimates degrade at low signal-to-noise ratio (Doviak & Zrnić, *Doppler Radar and Weather Observations*, 2nd ed., on estimator variance vs. SNR); Level II carries no SNR field, so reflectivity stands in | `Sweep::masked_by_reflectivity`, `model::ensure_upload` |
| "Little or no precipitation" test: drawn reflectivity area (km²) in runs of ≥ 4 gates along a radial | Original (Hook Echo); gate area from range × azimuth spacing × gate length | `Field::echo_area_km2`, `Model::no_precipitation` |
| Command palette fuzzy matching (in-order subsequence; word-start and run bonuses) | Original (Hook Echo) | `commands::fuzzy` |
| Storm-relative velocity (subtract the storm-motion component per azimuth) | NWS JetStream, *Storm Relative Velocity*; publicly described in the commercial viewer's SRV help article | planned (Phase 3) |
| Gaussian backdrop blur (separable, downsampled) | Standard technique; e.g. Rákos, *Efficient Gaussian blur with linear sampling* (2010) | prototype; `hookecho-render` |

## Verification references

| Item | Source | Used in |
|---|---|---|
| Level II gate-by-gate reference | MetPy 1.7.1 (BSD-3-Clause), `Level2File`, via `tools/verify/dump_metpy.py` | `tests/data/*.metpy.json`, `crates/hookecho-core/tests/level2_vs_metpy.rs` |

## Bundled assets

| Asset | License | Source |
|---|---|---|
| Inter typeface | OFL-1.1 | <https://github.com/rsms/inter> |
| Inter Tabular (derived: `tnum` frozen as default) | OFL-1.1 | Generated from Inter with `opentype-feature-freezer` (Apache-2.0) |
| Lucide icons | ISC | <https://github.com/lucide-icons/lucide> |
| US state, county and primary-road boundaries | Public domain (US Census Bureau) | Census Cartographic Boundary Files and TIGER/Line, <https://www.census.gov/geographies/mapping-files.html> |
| Natural Earth (coastlines, lakes, populated places) | Public domain | <https://www.naturalearthdata.com> |
| Sample Level II volume KTLX 2013-05-20 20:16:43 UTC (first 3 sweeps, `tests/data/`) | Public domain (NOAA) | `s3://unidata-nexrad-level2/2013/05/20/KTLX/KTLX20130520_201643_V06.gz`, trimmed by `tools/trim_level2.py` |
| Sample Level II volume KTLX 2024-04-28 03:13:12 UTC (first 3 sweeps, `tests/data/`) | Public domain (NOAA) | `s3://unidata-nexrad-level2/2024/04/28/KTLX/KTLX20240428_031312_V06`, trimmed by `tools/trim_level2.py` |
