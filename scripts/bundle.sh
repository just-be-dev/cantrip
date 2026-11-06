#!/usr/bin/env bash
# Build Incant.app: the Swift binary, the bun runtime, and the example spells.
#
#   scripts/bundle.sh            → build/Incant.app
#   scripts/bundle.sh --install  → also copy to ~/Applications and launch it
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/build"
app="$out/Incant.app"

echo "→ swift build (release)"
(cd "$root/app" && swift build -c release --arch arm64 >/dev/null)
bin="$(cd "$root/app" && swift build -c release --arch arm64 --show-bin-path)/Incant"

echo "→ assembling $app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/Incant"
cp "$root/app/Info.plist" "$app/Contents/Info.plist"

echo "→ staging runtime (production deps)"
rsync -a --delete --exclude node_modules --exclude '*.log' "$root/runtime/" "$app/Contents/Resources/runtime/"
(cd "$app/Contents/Resources/runtime" && bun install --production --frozen-lockfile --ignore-scripts >/dev/null)
rsync -a --delete --exclude node_modules "$root/spells/" "$app/Contents/Resources/spells/"

echo "→ signing (ad hoc)"
codesign --force --deep --sign - "$app" >/dev/null 2>&1

echo "✓ $app"

if [[ "${1:-}" == "--install" ]]; then
  dest="$HOME/Applications/Incant.app"
  pkill -x Incant 2>/dev/null || true
  mkdir -p "$HOME/Applications"
  rm -rf "$dest"
  cp -R "$app" "$dest"
  open "$dest"
  echo "✓ installed to $dest and launched"
fi
