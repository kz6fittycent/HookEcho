#!/usr/bin/env bash
# Regenerates THIRD_PARTY_LICENSES.md with cargo-about (KICKOFF §1).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo about generate --workspace --fail scripts/about.hbs -o THIRD_PARTY_LICENSES.md
echo "wrote THIRD_PARTY_LICENSES.md"
