/**
 * Publishes a release: `PUT /api/releases/<version>` with the zipped
 * Cantrip.app as the body, `Authorization: Bearer <token>`, and
 *
 *   x-cantrip-sha256       the zip's SHA-256 (R2 refuses a body that doesn't match)
 *   x-cantrip-commit       the commit it was built from
 *   x-cantrip-arch         e.g. arm64
 *   x-cantrip-min-macos    Info.plist's LSMinimumSystemVersion
 *
 * The zip goes to `releases/Cantrip-<version>.zip` and latest.json then
 * points at it. scripts/release.sh is the client; the release workflow
 * (.github/workflows/release.yml) runs it on every push to main.
 *
 * The token is the workflow's GitHub OIDC token (see lib/github-oidc.ts):
 * only that workflow, on main of the repo the stack names, can publish, and
 * nothing secret is stored. Under `alchemy dev` there's no GitHub, so a
 * local stack binds RELEASE_TOKEN instead and that is accepted.
 *
 * A version that's already published answers 409 and changes nothing.
 */
import type { APIRoute } from "astro";
import { env } from "cloudflare:workers";
import { verify } from "../../../lib/github-oidc.ts";
import { latestKey, releaseKey, type Release } from "../../../lib/releases.ts";

export const prerender = false;

const digest = async (text: string) =>
  new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)));

/** Compares digests of the two tokens in constant time, so the check leaks neither. */
const sameToken = async (given: string, expected: string) => {
  const [a, b] = await Promise.all([digest(given), digest(expected)]);
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i]! ^ b[i]!;
  return diff === 0;
};

/** Who's publishing: the commit they built, or why they may not. */
const authorize = async (
  request: Request,
): Promise<{ ok: true; commit?: string } | { ok: false; error: string }> => {
  const given = request.headers.get("authorization")?.replace(/^Bearer\s+/i, "") ?? "";
  if (!given) return { ok: false, error: "missing token" };
  if (env.RELEASE_TOKEN) {
    return (await sameToken(given, env.RELEASE_TOKEN))
      ? { ok: true }
      : { ok: false, error: "bad token" };
  }
  if (!env.RELEASE_GITHUB_REPOSITORY) return { ok: false, error: "publishing is off here" };
  const result = await verify(given, {
    audience: new URL(request.url).origin,
    repository: env.RELEASE_GITHUB_REPOSITORY,
    workflow: ".github/workflows/release.yml",
    ref: "refs/heads/main",
  });
  return result.ok ? { ok: true, commit: result.claims.sha } : result;
};

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body, null, 2), {
    status,
    headers: { "content-type": "application/json" },
  });

export const PUT: APIRoute = async ({ request, params }) => {
  const auth = await authorize(request);
  if (!auth.ok) return json({ error: `unauthorized: ${auth.error}` }, 401);

  const version = params.version ?? "";
  if (!/^[\w.+-]+$/.test(version)) return json({ error: "bad version" }, 400);

  const sha256 = request.headers.get("x-cantrip-sha256")?.toLowerCase() ?? "";
  if (!/^[0-9a-f]{64}$/.test(sha256)) return json({ error: "x-cantrip-sha256 is required" }, 400);
  if (request.body === null) return json({ error: "empty body" }, 400);

  const file = `Cantrip-${version}.zip`;
  if ((await env.RELEASES.head(releaseKey(file))) !== null) {
    return json({ error: `${version} is already published` }, 409);
  }
  const object = await env.RELEASES.put(releaseKey(file), request.body, {
    sha256,
    httpMetadata: { contentType: "application/zip" },
  });
  if (object === null) return json({ error: "upload failed" }, 500);

  const release: Release = {
    version,
    file,
    sha256,
    size: object.size,
    arch: request.headers.get("x-cantrip-arch") ?? "arm64",
    minimumSystemVersion: request.headers.get("x-cantrip-min-macos") ?? "15.0",
    // From GitHub's signed claims when there are some, not the request's say-so.
    commit: auth.commit ?? request.headers.get("x-cantrip-commit") ?? "",
    publishedAt: new Date().toISOString(),
  };
  await env.RELEASES.put(latestKey, JSON.stringify(release, null, 2), {
    httpMetadata: { contentType: "application/json" },
  });
  return json(release, 201);
};
