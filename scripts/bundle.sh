#!/usr/bin/env bash
# Build Cantrip.app: the Swift binary, the bun runtime, and the example spells.
#
#   scripts/bundle.sh            → build/Cantrip.app
#   scripts/bundle.sh --install  → also copy to ~/Applications and launch it
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/build"
app="$out/Cantrip.app"

echo "→ swift build (release)"
(cd "$root/app" && swift build -c release --arch arm64 >/dev/null)
bin="$(cd "$root/app" && swift build -c release --arch arm64 --show-bin-path)/Cantrip"

echo "→ assembling $app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/Cantrip"
cp "$root/app/Info.plist" "$app/Contents/Info.plist"
# Point the app at the bun pinned in mise.toml; a GUI launch would otherwise get mise's global one.
bun_path="$(mise which bun -C "$root" 2>/dev/null)" || bun_path="$(command -v bun)"
/usr/libexec/PlistBuddy -c "Add :CantripBun string $bun_path" "$app/Contents/Info.plist"
echo "  bun: $bun_path ($("$bun_path" --version))"

echo "→ staging runtime (production deps)"
rsync -a --delete --exclude node_modules --exclude '*.log' "$root/runtime/" "$app/Contents/Resources/runtime/"
(cd "$app/Contents/Resources/runtime" && "$bun_path" install --production --frozen-lockfile --ignore-scripts >/dev/null)
rsync -a --delete --exclude node_modules "$root/grimoire/" "$app/Contents/Resources/grimoire/"

echo "→ signing (ad hoc)"
codesign --force --deep --sign - "$app" >/dev/null 2>&1

echo "✓ $app"

if [[ "${1:-}" == "--install" ]]; then
  dest="$HOME/Applications/Cantrip.app"
  # An update is a restart, not a quit: let daemons hand their state over
  # (e.g. Insomnia keeps its session) when the old host shuts down.
  CANTRIP_NO_LAUNCH=1 "$HOME/.config/cantrip/bin/cantrip" handoff >/dev/null 2>&1 || true
  pkill -x Cantrip 2>/dev/null || true
  mkdir -p "$HOME/Applications"
  rm -rf "$dest"
  cp -R "$app" "$dest"
  open "$dest"
  echo "✓ installed to $dest and launched"
fi
