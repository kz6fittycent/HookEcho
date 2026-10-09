#!/bin/sh
# Downloads the Level II sample volumes used by the prototype and tests.
# Public NOAA data from the Unidata AWS Open Data bucket (see docs/PROVENANCE.md).
set -eu
cd "$(dirname "$0")"
base=https://unidata-nexrad-level2.s3.amazonaws.com
for key in 2013/05/20/KTLX/KTLX20130520_201643_V06.gz; do
  f=$(basename "$key")
  [ -f "$f" ] || curl -fsSL -o "$f" "$base/$key"
done
