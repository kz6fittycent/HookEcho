#!/bin/sh
# Downloads the Level II sample volumes used by the prototype and tests.
# Public NOAA data from the Unidata AWS Open Data bucket (see docs/PROVENANCE.md).
set -eu
cd "$(dirname "$0")"
base=https://unidata-nexrad-level2.s3.amazonaws.com
# The Moore volume the app bundles lives in assets/bundled/ (committed).
for key in 2024/04/28/KTLX/KTLX20240428_031312_V06; do
  f=$(basename "$key")
  [ -f "$f" ] || curl -fsSL -o "$f" "$base/$key"
done
