// The Cloudflare adapter is injected by Cloudflare.Website.Astro in the root
// alchemy.run.ts; don't declare one here. `site` is set per stage there too.
import { defineConfig } from "astro/config";

export default defineConfig({
  // Pages render in the Worker (the home page shows the latest release);
  // anything that never changes opts into `export const prerender = true`.
  output: "server",
});
