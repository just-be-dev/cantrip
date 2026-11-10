/** `/download/Cantrip-<version>.zip`: one release's zip, streamed from R2. */
import type { APIRoute } from "astro";
import { download } from "../../lib/releases.ts";

export const prerender = false;

export const GET: APIRoute = ({ params }) => download(params.file ?? "");
