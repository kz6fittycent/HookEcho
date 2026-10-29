# Provenance

This file gives a public source for every data format, algorithm and bundled asset used by Hook Echo. Add an entry in the same commit that introduces the code.

## Data formats

| Item | Source | Used in |
|---|---|---|
| NEXRAD Level II (Archive II, Message 31, Message 1 legacy) | NOAA ROC, *Interface Control Document for the RDA/RPG*, ICD 2620002 (latest revision), <https://www.roc.noaa.gov/interface-control-documents.php> | via `nexrad-decode` (MIT) |
| NEXRAD Level II real-time chunk layout (`SITE/vol/YYYYMMDD-HHMMSS-seq-S/I/E`, volumes 1–999 in a ring; S = volume header + metadata record, I/E = one LDM record) | Unidata, *NEXRAD Level II real-time data on AWS*, <https://registry.opendata.aws/noaa-nexrad/>; verified against the live bucket 2026-10-10 | `hookecho-core::realtime`, `hookecho-core::live` |
| S3 ListObjectsV2 (anonymous) | AWS S3 API reference, *ListObjectsV2*, <https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html> | `hookecho-core::s3` |
| NEXRAD Level III products (WMO/AWIPS text header, Message Header and Product Description Blocks, bzip2 symbology block, Digital Radial Data Array packet 16) | NOAA ROC, *ICD for the RPG to Class 1 User*, ICD 2620001 Rev AD (Build 24.0), §3.3, Figures 3-3, 3-6 and 3-11c, <https://www.roc.noaa.gov/public-documents/icds/2620001AD.pdf> | `hookecho-core::level3` |
| TDWR Level III products 180/182/186 (base reflectivity, base velocity, long-range reflectivity; 150 m and 300 m gates) | NOAA ROC, *ICD for the SPG to AWIPS Class 1 User*, ICD 2620063 (Rev E, Build 12.0); gate spacing cross-checked against MetPy 1.7.1's product table (90 and 416 km over 592 and 1390 bins). The ROC site was unreachable when the decoder was written (2026-10-28): re-check the gate spacing against 2620063 itself | `hookecho-core::level3` |
| TDWR Level III files (`DAL_TZ0_YYYY_MM_DD_HH_MM_SS`: three-letter site, product, volume time) | Unidata, `s3://unidata-nexrad-level3/` (approved in Phase 0), checked 2026-10-28: about 180 files per product per day for TDAL and TDFW | `tests/data/level3/` |
| NWS warnings (active alerts as GeoJSON: `event`, `senderName`, `areaDesc`, `headline`, `description`, `instruction`, `sent`/`ends`/`expires`, Polygon/MultiPolygon geometry; `status=actual` and `event=` filters; ETag and `If-None-Match`; a descriptive User-Agent) | National Weather Service, *API Web Service* documentation, <https://www.weather.gov/documentation/services-web-api>, and the OpenAPI spec at <https://api.weather.gov/openapi.json>; verified against the live API 2026-10-29. Approved by the owner 2026-10-28 | `hookecho-core::alerts`; sample answer in `tests/data/alerts/` (US Government work, public domain) |
| Warning colors | NWS, *Weather.gov Help - Hazards Map*, <https://www.weather.gov/help-map>. Exception: flash flood uses the table's flood-warning green (00FF00) instead of its dark red, per DESIGN.md §2.4 | `hookecho-app::design::hazards` |
| SPC convective outlooks (GeoJSON per day and kind, `dayNotlk_{cat,torn,wind,hail,prob}.nolyr.geojson`: `LABEL`, `LABEL2`, `stroke`, `fill`, `VALID_ISO`/`EXPIRE_ISO`/`ISSUE_ISO`, `FORECASTER`) | NOAA Storm Prediction Center, public products at <https://www.spc.noaa.gov/products/outlook/>; format read from the files themselves, 2026-10-29. Endpoint approved by the owner 2026-10-29 | `hookecho-core::spc`; samples in `tests/data/spc/` (US Government work, public domain) |
| SPC active watches and MDs (`products/watch/ActiveWW.kmz`, `products/md/ActiveMD.kmz`: KML `NetworkLink`s to one KMZ per product; text in the first `<pre>` of `wwNNNN.html` / `mdNNNN.html`) | NOAA SPC, <https://www.spc.noaa.gov/products/watch/> and <https://www.spc.noaa.gov/products/md/>; older active lists from the Internet Archive for samples | `hookecho-core::spc` |
| KML (`NetworkLink`, `Placemark`, `Polygon`/`outerBoundaryIs`/`coordinates` as lon,lat[,alt]) | OGC, *KML 2.2*, OGC 07-147r2, <https://www.ogc.org/standard/kml/> | `hookecho-core::spc::kml_rings` |
| Zip local file headers (stored and deflated entries) | PKWARE, *APPNOTE.TXT*, §4.3.7, <https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT>; deflate by `flate2` | `hookecho-core::spc::unzip_first` |
| Watch colors (tornado watch FFFF00, severe thunderstorm watch DB7093) | NWS *Hazards Map* table, as for warnings. MDs have no NWS color: our own light blue, dashed | `hookecho-app::design::hazards` |
| METARs: the AWC cache file (`data/cache/metars.cache.csv.gz`: `station_id`, `observation_time`, `latitude`/`longitude`, `temp_c`, `dewpoint_c`, `wind_*`, `visibility_statute_mi` with "10+", `altim_in_hg`, `three_hr_pressure_tendency_mb`, `wx_string`, `flight_category`) and the Data API (`api/data/metar?ids=&hours=&format=json`: `obsTime`, `temp`, `dewp`, `wdir`/`wspd`/`wgst`, `visib`, `altim` in hPa, `presTend`, `name`) | NOAA Aviation Weather Center, *Data API*, <https://aviationweather.gov/data/api/>; verified against both 2026-10-29. Endpoint approved by the owner 2026-10-29 | `hookecho-core::obs`; samples in `tests/data/metar/` (US Government work, public domain) |
| CSV (quoted fields, `""` escapes) | IETF RFC 4180, <https://www.rfc-editor.org/rfc/rfc4180> | `hookecho-core::obs` |
| Relative humidity from temperature and dewpoint (Magnus form, a = 17.625, b = 243.04 °C) | O. A. Alduchov and R. E. Eskridge, "Improved Magnus form approximation of saturation vapor pressure", *J. Appl. Meteor.* 35 (1996) 601–609 | `hookecho-core::obs::relative_humidity` |
| Station model and wind barbs (shaft toward where the wind comes from; pennant 50 kt, full feather 10, half 5; feathers on the clockwise side in the Northern Hemisphere) | NWS, *How to read a surface weather map* / JetStream "Station model", <https://www.weather.gov/jetstream/stationmodel> | `hookecho-app::stations` |
| Pressure trend wording (steady within 0.5 hPa over 3 hours) | Our own threshold; the 3-hour window follows the METAR 5appp tendency group (FMH-1, §12.7.2) | `hookecho-app::stations` |
| Point in polygon (even-odd rule) | Standard ray-casting test; W. R. Franklin, *PNPOLY*, <https://wrfranklin.org/Research/Short_Notes/pnpoly.html> | `hookecho-core::alerts::Alert::contains` |
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
| Level III (TDWR) reference: position, time, elevation, data scaling, radial angles, SHA-256 of the raw codes | MetPy 1.7.1 (BSD-3-Clause), `Level3File`, via `tools/verify/dump_metpy_level3.py` | `tests/data/level3/*.metpy.json`, `crates/hookecho-core/tests/level3_vs_metpy.rs` |

## Bundled assets

| Asset | License | Source |
|---|---|---|
| Inter typeface | OFL-1.1 | <https://github.com/rsms/inter> |
| Inter Tabular (derived: `tnum` frozen as default) | OFL-1.1 | Generated from Inter with `opentype-feature-freezer` (Apache-2.0) |
| Lucide icons | ISC | <https://github.com/lucide-icons/lucide> |
| US state, county and primary-road boundaries | Public domain (US Census Bureau) | Census Cartographic Boundary Files and TIGER/Line, <https://www.census.gov/geographies/mapping-files.html> |
| US places (names, locations, land area) | Public domain (US Census Bureau) | 2024 Gazetteer Files, places, <https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2024_Gazetteer/2024_Gaz_place_national.zip> |
| US place population (label ranking) | Public domain (US Census Bureau) | Vintage 2024 population estimates, incorporated places (SUMLEV 162), <https://www2.census.gov/programs-surveys/popest/datasets/2020-2024/cities/totals/sub-est2024.csv>; build-time only (`hookecho-data basemap`), never fetched by the app |
| Natural Earth (coastlines, lakes, populated places) | Public domain | <https://www.naturalearthdata.com> |
| Sample Level II volume KTLX 2013-05-20 20:16:43 UTC (first 3 sweeps, `tests/data/`) | Public domain (NOAA) | `s3://unidata-nexrad-level2/2013/05/20/KTLX/KTLX20130520_201643_V06.gz`, trimmed by `tools/trim_level2.py` |
| Sample Level II volume KTLX 2024-04-28 03:13:12 UTC (first 3 sweeps, `tests/data/`) | Public domain (NOAA) | `s3://unidata-nexrad-level2/2024/04/28/KTLX/KTLX20240428_031312_V06`, trimmed by `tools/trim_level2.py` |
