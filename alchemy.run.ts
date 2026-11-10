/**
 * Cantrip's website, where the app is distributed from: an Astro site
 * (site/) on a Cloudflare Worker, and an R2 bucket of release zips.
 * projects.just-be.dev runs `alchemy deploy --stage <STAGE>` on every push:
 * main serves prod at https://cantrip.just-be.dev, other branches a preview.
 *
 * Releases are uploaded through the site (`PUT /api/releases/<version>`,
 * scripts/release.sh) with a token minted here. In prod that token is also
 * written to the project's vault as CANTRIP_RELEASE_TOKEN, where the release
 * script's operator finds it (secrets.just-be.dev).
 */
import * as Cloudflare from "@alchemy.run/cloudflare";
import * as SecretsManager from "@just-be/secrets-alchemy";
import * as Alchemy from "alchemy";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as path from "node:path";
import { host, isProd, name, placement, stage } from "./project.ts";

const state = Layer.unwrap(
  Effect.gen(function* () {
    const dev = yield* Alchemy.ALCHEMY_DEV.pipe(Effect.orDie);
    return dev ? Alchemy.localState() : Cloudflare.state();
  }),
);

export default Alchemy.Stack(
  name,
  {
    providers: Layer.mergeAll(Cloudflare.providers(), SecretsManager.providers()),
    state,
    // The vault wins over the shell; a name it lacks falls back to the shell.
    secrets: [Alchemy.Secrets.ProcessEnv(), SecretsManager.fromVault()],
  },
  Effect.gen(function* () {
    const releases = yield* Cloudflare.R2.Bucket("Releases", {
      // A preview's bucket goes with it, zips and all; prod's never does.
      forceDestroy: !isProd,
    });

    const releaseToken = yield* Alchemy.makeRandom("ReleaseToken");
    // Prod only (a preview's deploy writes nothing to the vault).
    yield* SecretsManager.Secret("ReleaseTokenSecret", {
      name: "CANTRIP_RELEASE_TOKEN",
      value: releaseToken,
    });

    const site = yield* Cloudflare.Website.Astro("Site", {
      // Absolute: `alchemy dev` runs the site in a child whose cwd is already
      // the root, which would resolve a relative "site" to site/site.
      rootDir: path.join(import.meta.dirname, "site"),
      compatibility: { date: "2026-10-04" },
      // No sessions: nothing on the site keeps per-visitor state.
      sessionKVBindingName: false,
      astro: isProd ? { site: `https://${host}` } : {},
      env: {
        RELEASES: releases,
        RELEASE_TOKEN: releaseToken,
      },
      ...placement(),
    });

    return { url: site.url, stage: stage ?? "local" };
  }),
);
