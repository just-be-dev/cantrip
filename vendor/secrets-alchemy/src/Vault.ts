import * as ConfigProvider from "effect/ConfigProvider";
import * as Data from "effect/Data";
import * as Effect from "effect/Effect";
import type * as Layer from "effect/Layer";
import { type Client, defaultClient } from "./Client.ts";

/** What the deploy API accepts as a secret name. Anything else isn't in the vault. */
const SECRET_NAME = /^[A-Za-z_][A-Za-z0-9_.-]{0,127}$/u;

/**
 * A ConfigProvider over the project's vault. A Config path is the secret
 * name its segments make joined by `_` (as for the shell: `Config.String(
 * "STRIPE_KEY")` reads `STRIPE_KEY`). Each name is read once per process.
 * A name the vault lacks is absent, so earlier providers answer for it; a
 * preview's deploy reads nothing. Any other failure is a `SourceError`,
 * which fails the deploy rather than deploying without the value.
 */
export const vaultConfigProvider = (client: Client): ConfigProvider.ConfigProvider => {
  // Answers so far, by name; a failure isn't kept, so the next lookup asks again.
  const known = new Map<string, string | undefined>();
  let preview = false;
  const read = (name: string) =>
    known.has(name)
      ? Effect.succeed(known.get(name))
      : client.read([name]).pipe(
          Effect.map((values) => values[name]),
          Effect.catchTag("SecretsManagerError", (error) => {
            if (error.reason !== "PreviewStage")
              return Effect.fail(new ConfigProvider.SourceError({ message: error.message }));
            preview = true;
            return Effect.succeed(undefined);
          }),
          Effect.tap((value) => Effect.sync(() => known.set(name, value))),
        );
  return ConfigProvider.make((path) => {
    const name = path.join("_");
    if (preview || !SECRET_NAME.test(name)) return Effect.succeed(undefined);
    return Effect.map(read(name), (value) =>
      value === undefined ? undefined : ConfigProvider.makeValue(value),
    );
  });
};

/** A `secrets` entry that reads the project's vault. */
export class VaultSecretsProvider extends Data.TaggedClass(
  "@just-be/secrets-alchemy/SecretProvider::Vault",
)<{
  readonly layer: Layer.Layer<never>;
}> {}

/**
 * The project's vault as an Alchemy `secrets` entry. Later entries win, and
 * a name the vault lacks falls back to the ones before it:
 *
 * ```ts
 * secrets: [SecretsManager.fromVault()]
 * ```
 *
 * Without the runner's credentials (`alchemy dev`, a deploy from a laptop)
 * it contributes nothing.
 */
export const fromVault = (options: { readonly client?: Client | undefined } = {}) =>
  new VaultSecretsProvider({
    layer: ConfigProvider.layerAdd(
      Effect.sync(() => {
        const client = options.client ?? defaultClient();
        return client === undefined
          ? ConfigProvider.make(() => Effect.succeed(undefined))
          : vaultConfigProvider(client);
      }),
      { asPrimary: true },
    ),
  });
