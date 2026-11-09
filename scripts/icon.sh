#!/usr/bin/env bash
# Render Cantrip's icon (app/examples/icon.rs, drawn with Rune) into app/Cantrip.icns.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

(cd "$root/app" && cargo build --release --quiet --example icon)
"$root/app/target/release/examples/icon" --snap "$work/icon.png" --scale 2 >/dev/null 2>&1

set_dir="$work/Cantrip.iconset"
mkdir -p "$set_dir"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$work/icon.png" --out "$set_dir/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$work/icon.png" --out "$set_dir/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$set_dir" -o "$root/app/Cantrip.icns"
echo "✓ app/Cantrip.icns"
