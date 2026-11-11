#!/usr/bin/env bash
# Publish a Cantrip release to the website (cantrip.just-be.dev): build Cantrip.app, zip it, and
# upload it through the site's PUT /api/releases/<version>. The site then offers it at /download
# and through /install.sh.
#
# You don't run this by hand: .github/workflows/release.yml runs it on GitHub Actions for every
# push to main that touches the app, and the site trusts that workflow's GitHub OIDC token.
#
#   scripts/release.sh            → build and publish
#   scripts/release.sh --no-build → publish build/Cantrip.app as it is
#
# The token: in GitHub Actions (with `id-token: write`), the run's OIDC token for the site; else
# CANTRIP_RELEASE_TOKEN, which only an `alchemy dev` site accepts (its RELEASE_TOKEN, in
# .alchemy/state). CANTRIP_SITE picks the site, e.g. http://localhost:1337 for `alchemy dev`.
#
# The version is Info.plist's CFBundleShortVersionString plus the commit, e.g. 0.1.0-0da9964, so
# every commit can be released; bump Info.plist for a release worth naming. Publishing a version
# that's already there changes nothing and succeeds.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
site="${CANTRIP_SITE:-https://cantrip.just-be.dev}"
site="${site%/}"
app="$root/build/Cantrip.app"
plist="$root/app/Info.plist"

token="${CANTRIP_RELEASE_TOKEN:-}"
if [[ -z "$token" && -n "${ACTIONS_ID_TOKEN_REQUEST_URL:-}" ]]; then
  echo "→ asking GitHub for an OIDC token for $site"
  token="$(curl -fsS -H "authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" \
    "$ACTIONS_ID_TOKEN_REQUEST_URL&audience=$site" | sed -E 's/.*"value":"([^"]+)".*/\1/')"
fi
[[ -n "$token" ]] || {
  echo "✗ no token: releases publish from GitHub Actions (.github/workflows/release.yml)," >&2
  echo "  or set CANTRIP_RELEASE_TOKEN for an alchemy dev site" >&2
  exit 1
}

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
status="$(curl -sS -o "$tmp/response" -w '%{http_code}' -X PUT "$site/api/releases/$version" \
  -H "authorization: Bearer $token" \
  -H "content-type: application/zip" \
  -H "x-cantrip-sha256: $sha256" \
  -H "x-cantrip-commit: $(git -C "$root" rev-parse HEAD)" \
  -H "x-cantrip-arch: $arch" \
  -H "x-cantrip-min-macos: $min_macos" \
  --data-binary "@$zip")"
cat "$tmp/response"
echo
case "$status" in
  201) echo "✓ Cantrip $version is live: $site/download" ;;
  409) echo "✓ Cantrip $version was already published" ;;
  *) echo "✗ publishing failed ($status)" >&2; exit 1 ;;
esac
