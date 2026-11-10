/** `/download`: the latest release's zip, by redirect so the URL names the version. */
import type { APIRoute } from "astro";
import { latest } from "../../lib/releases.ts";

export const prerender = false;

export const GET: APIRoute = async ({ redirect }) => {
  const release = await latest();
  if (release === undefined) return new Response("No release yet", { status: 404 });
  const response = redirect(`/download/${release.file}`, 302);
  response.headers.set("cache-control", "no-store");
  return response;
};
