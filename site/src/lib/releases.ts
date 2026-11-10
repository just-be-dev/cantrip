/**
 * Cantrip's releases, kept in the R2 bucket bound as `RELEASES`:
 *
 *   releases/Cantrip-<version>.zip   one zipped Cantrip.app per release
 *   latest.json                      the {@link Release} the site offers
 *
 * `scripts/release.sh` builds and uploads a release through
 * `PUT /api/releases/<version>` (see src/pages/api/releases/[version].ts).
 */
import { env } from "cloudflare:workers";

export interface Release {
  /** e.g. `0.1.0-0da9964`: Info.plist's version and the commit it was built from. */
  readonly version: string;
  /** The zip's key under `releases/`, e.g. `Cantrip-0.1.0-0da9964.zip`. */
  readonly file: string;
  readonly sha256: string;
  readonly size: number;
  /** `arm64`: the app and the bun inside it are built for Apple silicon. */
  readonly arch: string;
  /** Info.plist's LSMinimumSystemVersion. */
  readonly minimumSystemVersion: string;
  readonly commit: string;
  /** ISO 8601. */
  readonly publishedAt: string;
}

export const latestKey = "latest.json";
export const releaseKey = (file: string) => `releases/${file}`;

/** The release on offer, or `undefined` before the first one is published. */
export const latest = async (): Promise<Release | undefined> => {
  const object = await env.RELEASES.get(latestKey);
  return object === null ? undefined : ((await object.json()) as Release);
};

/** A release's zip as a download response, or 404. */
export const download = async (file: string): Promise<Response> => {
  if (!/^Cantrip-[\w.+-]+\.zip$/.test(file)) return new Response("Not found", { status: 404 });
  const object = await env.RELEASES.get(releaseKey(file));
  if (object === null) return new Response("Not found", { status: 404 });
  const headers = new Headers();
  object.writeHttpMetadata(headers);
  headers.set("etag", object.httpEtag);
  headers.set("content-length", String(object.size));
  headers.set("content-type", "application/zip");
  headers.set("content-disposition", `attachment; filename="${file}"`);
  // A release's file never changes once published.
  headers.set("cache-control", "public, max-age=31536000, immutable");
  return new Response(object.body, { headers });
};

export const formatSize = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;
