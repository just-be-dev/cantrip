/**
 * What projects.just-be.dev offers this stack (from the platform's vite
 * template). The deploy runner sets these variables; without them (plain
 * `alchemy dev` or `alchemy deploy`) the stack deploys an ordinary Worker on
 * workers.dev.
 *
 * - PROJECT_NAME  the repo name; names the stack and the prod Worker
 * - STAGE         `prod` for main, `b-<slug>-<hash>` for other branches
 * - PROJECT_HOST  `<project>.<zone>`, the hostname prod claims here
 * - PROJECT_ZONE  the zone that hostname is in
 * - SECRETS_MANAGER_URL, SECRETS_MANAGER_TOKEN  the project's vault (prod reads and writes it)
 *
 * Return `{ url }` from the stack: it's how the platform learns where each
 * stage serves.
 */
const env = (name: string) => process.env[name]?.trim() || undefined;

export const name = env("PROJECT_NAME") ?? "cantrip";
export const stage = env("STAGE");
export const isProd = stage === "prod";
const zone = env("PROJECT_ZONE") ?? "just-be.dev";
export const host = env("PROJECT_HOST") ?? `${name}.${zone}`;

/**
 * Worker name and, in prod, the hostname. The custom domain gives the
 * hostname DNS and a certificate; the route is there too because the zone's
 * `*.just-be.dev/*` wildcard route outranks custom domains.
 */
export const placement = () => {
  if (stage === undefined) return {};
  if (!isProd) return { name: `${name}-${stage}` };
  return { name, domain: host, routes: [{ pattern: `${host}/*`, zoneName: zone }] };
};
