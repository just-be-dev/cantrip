#!/usr/bin/env bash
# Publish a Cantrip release to the website (cantrip.just-be.dev): build Cantrip.app, zip it, and
# upload it through the site's PUT /api/releases/<version>. The site then offers it at /download
# and through /install.sh.
#
#   CANTRIP_RELEASE_TOKEN=… scripts/release.sh            → build and publish
#   CANTRIP_RELEASE_TOKEN=… scripts/release.sh --no-build → publish build/Cantrip.app as it is
#
# The token is CANTRIP_RELEASE_TOKEN in the cantrip vault at https://secrets.just-be.dev (prod's
# deploy writes it there). CANTRIP_SITE picks another site, e.g. a branch preview or
# `alchemy dev`'s http://localhost:1337.
#
# The version is Info.plist's CFBundleShortVersionString plus the commit, e.g. 0.1.0-0da9964, so
# every commit can be released; bump Info.plist for a release worth naming.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
site="${CANTRIP_SITE:-https://cantrip.just-be.dev}"
site="${site%/}"
app="$root/build/Cantrip.app"
plist="$root/app/Info.plist"

: "${CANTRIP_RELEASE_TOKEN:?set CANTRIP_RELEASE_TOKEN (the cantrip vault at https://secrets.just-be.dev)}"

if [[ -n "$(git -C "$root" status --porcelain)" && -z "${CANTRIP_ALLOW_DIRTY:-}" ]]; then
  echo "✗ the working tree has changes; commit them first (or set CANTRIP_ALLOW_DIRTY=1)" >&2
  exit 1
fi

commit="$(git -C "$root" rev-parse --short HEAD)"
base_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")"
min_macos="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$plist")"
version="$base_version-$commit"

if [[ "${1:-}" != "--no-build" ]]; then
  "$root/scripts/bundle.sh"
fi
[[ -d "$app" ]] || { echo "✗ no $app; run scripts/bundle.sh" >&2; exit 1; }

arch="$(lipo -archs "$app/Contents/MacOS/Cantrip")"
[[ "$arch" == *" "* ]] && arch="universal"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
zip="$tmp/Cantrip-$version.zip"

echo "→ zipping Cantrip $version ($arch)"
ditto -c -k --sequesterRsrc --keepParent "$app" "$zip"
sha256="$(shasum -a 256 "$zip" | cut -d' ' -f1)"
echo "  $(du -h "$zip" | cut -f1) · sha256 $sha256"

echo "→ publishing to $site"
curl --fail-with-body -sS -X PUT "$site/api/releases/$version" \
  -H "authorization: Bearer $CANTRIP_RELEASE_TOKEN" \
  -H "content-type: application/zip" \
  -H "x-cantrip-sha256: $sha256" \
  -H "x-cantrip-commit: $(git -C "$root" rev-parse HEAD)" \
  -H "x-cantrip-arch: $arch" \
  -H "x-cantrip-min-macos: $min_macos" \
  --data-binary "@$zip"
echo
echo "✓ Cantrip $version is live: $site/download"
