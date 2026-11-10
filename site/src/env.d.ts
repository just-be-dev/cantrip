/// <reference types="astro/client" />
/// <reference types="@cloudflare/workers-types" />

/** The Worker's bindings, given by the root alchemy.run.ts. */
declare namespace Cloudflare {
  interface Env {
    /** Release zips and latest.json (src/lib/releases.ts). */
    RELEASES: R2Bucket;
    /** Bearer token for `PUT /api/releases/<version>`; scripts/release.sh sends it. */
    RELEASE_TOKEN: string;
  }
}
