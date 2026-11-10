/** `/latest.json`: the release on offer (src/lib/releases.ts `Release`), or 404 before the first. */
import type { APIRoute } from "astro";
import { latest } from "../lib/releases.ts";

export const prerender = false;

export const GET: APIRoute = async () => {
  const release = await latest();
  return release === undefined
    ? new Response(JSON.stringify({ error: "no release yet" }), {
        status: 404,
        headers: { "content-type": "application/json" },
      })
    : new Response(JSON.stringify(release, null, 2), {
        headers: { "content-type": "application/json", "cache-control": "no-store" },
      });
};
