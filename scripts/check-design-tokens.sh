#!/usr/bin/env bash
# KICKOFF §5: no hard-coded colors, sizes or fonts outside
# crates/hookecho-app/src/design/. Every screen uses design tokens and the
# component library. Run by CI (snap-builds.yml); exits non-zero on a hit.
#
# What counts as a violation in app code outside design/:
#   - color construction: Color32::from_*, Color32::<NAMED> other than
#     WHITE/TRANSPARENT (used as texture tints), rgb(/rgba( helpers, hex
#     color literals like 0x1B1F26 or "#1B1F26"
#   - font construction: FontId::new, FontFamily::
#   - literal sizes in sizing APIs: vec2(<num>, CornerRadius::same(<num>,
#     Stroke::new(<num>, circle_*(…, <num>, egui::Margin, .rounding(
# A line may opt out with a trailing `// design-ok: <reason>`.
set -euo pipefail
cd "$(dirname "$0")/.."
src=crates/hookecho-app/src
pattern='Color32::from_|Color32::(BLACK|RED|GREEN|BLUE|YELLOW|GRAY|DARK_GRAY|LIGHT_GRAY|LIGHT_BLUE|GOLD|KHAKI|BROWN|DARK_RED|DARK_GREEN|DARK_BLUE)\b|\brgba?\(|0x[0-9A-Fa-f]{6}\b|"#[0-9A-Fa-f]{6}|FontId::new|FontFamily::|vec2\(\s*[0-9]|CornerRadius::same\(\s*[0-9]|Stroke::new\(\s*[0-9]|circle_(filled|stroke)\([^,]+,\s*[0-9]|egui::Margin|\.rounding\('
hits=$(grep -rnE "$pattern" "$src" --include='*.rs' | grep -v "^$src/design/" | grep -v 'design-ok:' || true)
if [[ -n "$hits" ]]; then
  echo "Design-token check failed: literal colors/sizes/fonts outside $src/design/:"
  echo "$hits"
  echo
  echo "Move the value into design/tokens.rs (or a component), or mark a deliberate exception with '// design-ok: <reason>'."
  exit 1
fi
echo "Design-token check passed."
