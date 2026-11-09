#!/usr/bin/env bash
# Build Cantrip.app: the Rust (Rune) binary, its icon, bun, the spell runtime, and the example
# spells. The app is self-contained: it runs the bun inside it (the version pinned in mise.toml).
#
#   scripts/bundle.sh            → build/Cantrip.app
#   scripts/bundle.sh --install  → also install it to /Applications and launch it
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/build"
app="$out/Cantrip.app"
dest="/Applications/Cantrip.app"

echo "→ cargo build (release)"
(cd "$root/app" && cargo build --release --quiet)
bin="$root/app/target/release/Cantrip"

echo "→ assembling $app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/bun"
cp "$bin" "$app/Contents/MacOS/Cantrip"
cp "$root/app/Info.plist" "$app/Contents/Info.plist"
cp "$root/app/Cantrip.icns" "$app/Contents/Resources/Cantrip.icns"

# The bun pinned in mise.toml, inside the app, so it runs without mise (or any bun) installed.
bun_path="$(mise which bun -C "$root" 2>/dev/null)" || bun_path="$(command -v bun)"
cp "$bun_path" "$app/Contents/Resources/bun/bun"
echo "  bun: $("$app/Contents/Resources/bun/bun" --version) (from $bun_path)"

echo "→ staging runtime (production deps)"
rsync -a --delete --exclude node_modules --exclude '*.log' "$root/runtime/" "$app/Contents/Resources/runtime/"
(cd "$app/Contents/Resources/runtime" && "$app/Contents/Resources/bun/bun" install --production --frozen-lockfile --ignore-scripts >/dev/null)
rsync -a --delete --exclude node_modules "$root/grimoire/" "$app/Contents/Resources/grimoire/"

echo "→ signing (ad hoc)"
codesign --force --deep --sign - "$app" >/dev/null 2>&1

echo "✓ $app"

if [[ "${1:-}" == "--install" ]]; then
  # An update is a restart, not a quit: let daemons hand their state over
  # (e.g. Insomnia keeps its session) when the old host shuts down.
  CANTRIP_NO_LAUNCH=1 "$HOME/.config/cantrip/bin/cantrip" handoff >/dev/null 2>&1 || true
  pkill -x Cantrip 2>/dev/null || true
  rm -rf "$dest"
  cp -R "$app" "$dest"
  # Earlier builds installed to ~/Applications; one Cantrip is enough.
  rm -rf "$HOME/Applications/Cantrip.app"
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$dest" >/dev/null 2>&1 || true
  open "$dest"
  echo "✓ installed to $dest and launched"
fi
