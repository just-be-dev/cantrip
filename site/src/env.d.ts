/// <reference types="astro/client" />
/// <reference types="@cloudflare/workers-types" />

/** The Worker's bindings, given by the root alchemy.run.ts. */
declare namespace Cloudflare {
  interface Env {
    /** Release zips and latest.json (src/lib/releases.ts). */
    RELEASES: R2Bucket;
    /**
     * `owner/repo` whose release workflow, on main, may publish with its GitHub OIDC token
     * (src/pages/api/releases/[version].ts). Unset: no one may.
     */
    RELEASE_GITHUB_REPOSITORY?: string;
    /** `alchemy dev` only: a shared token for `PUT /api/releases/<version>`, as there's no GitHub. */
    RELEASE_TOKEN?: string;
  }
}
