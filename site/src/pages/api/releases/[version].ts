/**
 * Publishes a release: `PUT /api/releases/<version>` with the zipped
 * Cantrip.app as the body, `Authorization: Bearer <RELEASE_TOKEN>`, and
 *
 *   x-cantrip-sha256       the zip's SHA-256 (R2 refuses a body that doesn't match)
 *   x-cantrip-commit       the commit it was built from
 *   x-cantrip-arch         e.g. arm64
 *   x-cantrip-min-macos    Info.plist's LSMinimumSystemVersion
 *
 * The zip goes to `releases/Cantrip-<version>.zip` and latest.json then
 * points at it. scripts/release.sh is the client.
 */
import type { APIRoute } from "astro";
import { env } from "cloudflare:workers";
import { latestKey, releaseKey, type Release } from "../../../lib/releases.ts";

export const prerender = false;

const digest = async (text: string) =>
  new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)));

/** Compares digests of the two tokens in constant time, so the check leaks neither. */
const authorized = async (request: Request) => {
  const expected = env.RELEASE_TOKEN;
  if (!expected) return false;
  const given = request.headers.get("authorization")?.replace(/^Bearer\s+/i, "") ?? "";
  const [a, b] = await Promise.all([digest(given), digest(expected)]);
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i]! ^ b[i]!;
  return diff === 0;
};

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body, null, 2), {
    status,
    headers: { "content-type": "application/json" },
  });

export const PUT: APIRoute = async ({ request, params }) => {
  if (!(await authorized(request))) return json({ error: "unauthorized" }, 401);

  const version = params.version ?? "";
  if (!/^[\w.+-]+$/.test(version)) return json({ error: "bad version" }, 400);

  const sha256 = request.headers.get("x-cantrip-sha256")?.toLowerCase() ?? "";
  if (!/^[0-9a-f]{64}$/.test(sha256)) return json({ error: "x-cantrip-sha256 is required" }, 400);
  if (request.body === null) return json({ error: "empty body" }, 400);

  const file = `Cantrip-${version}.zip`;
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
    commit: request.headers.get("x-cantrip-commit") ?? "",
    publishedAt: new Date().toISOString(),
  };
  await env.RELEASES.put(latestKey, JSON.stringify(release, null, 2), {
    httpMetadata: { contentType: "application/json" },
  });
  return json(release, 201);
};
