#!/usr/bin/env bash
# Render the website's screenshots, demo video and icon from the app's headless scenes
# (app/src/fixtures.rs) into site/public/media/. Re-run after the bar's look changes.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/site/public/media"
bin="$root/app/target/release/Cantrip"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$out"

echo "→ cargo build (release)"
(cd "$root/app" && cargo build --release --quiet)

echo "→ stills (the root search in each theme)"
for pair in root-arcane:arcane root:sigil; do
  scene="${pair%%:*}" name="${pair##*:}"
  "$bin" --scene "$scene" --crop shell --snap "$tmp/$name.png" --at 1 >/dev/null 2>&1
  cwebp -quiet -q 88 "$tmp/$name.png" -o "$out/$name.webp"
done

echo "→ demo video (opening, then a couple of searches)"
cat >"$tmp/demo.json" <<'EOF'
{"events": [
  {"t": 0.1, "focus": true},
  {"t": 1.2, "type": "type", "text": "uuid", "every": 0.16},
  {"t": 2.9, "type": "key", "name": "Backspace"},
  {"t": 3.05, "type": "key", "name": "Backspace"},
  {"t": 3.2, "type": "key", "name": "Backspace"},
  {"t": 3.35, "type": "key", "name": "Backspace"},
  {"t": 3.7, "type": "type", "text": "party", "every": 0.14},
  {"t": 5.6, "type": "mark", "name": "end"}
]}
EOF
"$bin" --scene opening-arcane --script "$tmp/demo.json" --video "$tmp/demo.mp4" >/dev/null 2>&1
# The window is 1696×1136 px at 2×; the shell sits 88 px in, 1520×960.
crop="crop=1520:960:88:88"
ffmpeg -y -v error -i "$tmp/demo.mp4" -vf "$crop" -an -c:v libx264 -preset slow -crf 24 \
  -pix_fmt yuv420p -movflags +faststart "$out/demo.mp4"
ffmpeg -y -v error -i "$tmp/demo.mp4" -vf "$crop" -an -c:v libvpx-vp9 -b:v 0 -crf 36 \
  -row-mt 1 "$out/demo.webm"

echo "→ icon"
sips -s format png -z 256 256 "$root/app/Cantrip.icns" --out "$out/icon.png" >/dev/null
sips -s format png -z 64 64 "$root/app/Cantrip.icns" --out "$root/site/public/favicon.png" >/dev/null

ls -lh "$out" "$root/site/public/favicon.png"
