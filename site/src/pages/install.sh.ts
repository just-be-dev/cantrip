/**
 * `curl -fsSL https://cantrip.just-be.dev/install.sh | sh`
 *
 * The script is rendered per request with the latest release's file and
 * SHA-256 baked in. curl doesn't quarantine what it downloads, so the
 * ad-hoc-signed app opens without a Gatekeeper prompt.
 */
import type { APIRoute } from "astro";
import { latest, type Release } from "../lib/releases.ts";

export const prerender = false;

const quote = (value: string) => `'${value.replaceAll("'", `'\\''`)}'`;

const script = (origin: string, release: Release) => `#!/bin/sh
# Installs Cantrip ${release.version} to /Applications (or ~/Applications, or $CANTRIP_INSTALL_DIR)
# and opens it.
# Source: ${origin}/install.sh
set -eu

url=${quote(`${origin}/download/${release.file}`)}
sha256=${quote(release.sha256)}
version=${quote(release.version)}
arch=${quote(release.arch)}
min_macos=${quote(release.minimumSystemVersion)}

say() { printf '\\033[35m✦\\033[0m %s\\n' "$*"; }
fail() { printf '\\033[31m✗\\033[0m %s\\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail "Cantrip is a macOS app."
[ "$(uname -m)" = "$arch" ] || fail "Cantrip $version is built for $arch Macs; this one is $(uname -m)."
macos="$(sw_vers -productVersion)"
[ "$(printf '%s\\n%s\\n' "$min_macos" "$macos" | sort -V | head -n1)" = "$min_macos" ] \\
  || fail "Cantrip needs macOS $min_macos or later; this Mac has $macos."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "Downloading Cantrip $version"
curl -fL --progress-bar "$url" -o "$tmp/Cantrip.zip"
echo "$sha256  $tmp/Cantrip.zip" | shasum -a 256 -c - >/dev/null || fail "Checksum mismatch; try again."
ditto -x -k "$tmp/Cantrip.zip" "$tmp"

dir="\${CANTRIP_INSTALL_DIR:-/Applications}"
[ -w "$dir" ] || { dir="$HOME/Applications"; mkdir -p "$dir"; }
dest="$dir/Cantrip.app"

if pgrep -x Cantrip >/dev/null 2>&1; then
  say "Handing over from the running Cantrip"
  # An update is a restart: let daemons hand their state over (see scripts/bundle.sh).
  CANTRIP_NO_LAUNCH=1 "$HOME/.config/cantrip/bin/cantrip" handoff >/dev/null 2>&1 || true
  pkill -x Cantrip 2>/dev/null || true
  sleep 1
fi

say "Installing to $dest"
rm -rf "$dest"
mv "$tmp/Cantrip.app" "$dest"
xattr -dr com.apple.quarantine "$dest" 2>/dev/null || true
open "$dest"
say "Cantrip $version is ready. Press ⌥ Space to cast."
`;

export const GET: APIRoute = async ({ url }) => {
  const release = await latest();
  const body =
    release === undefined
      ? `#!/bin/sh\necho "No Cantrip release has been published yet." >&2\nexit 1\n`
      : script(url.origin, release);
  return new Response(body, {
    headers: { "content-type": "text/x-shellscript; charset=utf-8", "cache-control": "no-store" },
  });
};
