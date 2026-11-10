/**
 * A project's vault in the secrets manager, from its Alchemy stack
 * (`docs/project-vaults.md` in the secrets manager).
 *
 * - `fromVault()`: a `secrets` entry, so `Config.Redacted("NAME")` reads the
 *   vault at deploy time (prod only).
 * - `Secret`: write a value Alchemy owns into the vault (prod only).
 * - `ProjectVault()` and `binding()`: give a Worker a consumer binding to the
 *   vault; `Grants` says what it may read. In the Worker, use
 *   `@just-be/secrets-alchemy/runtime`.
 *
 * Works on Alchemy's core API only, so with either `alchemy/Cloudflare` or
 * `@alchemy.run/cloudflare`.
 */
import * as Effect from "effect/Effect";
import { type Client, defaultClient, type SecretsManagerError, type VaultInfo } from "./Client.ts";

export {
  credentialsFrom,
  makeClient,
  SecretsManagerError,
  TOKEN_VARIABLE,
  URL_VARIABLE,
  VaultInfo,
  type Client,
  type Credentials,
} from "./Client.ts";
export { Grants, type GrantsProps, providers, Secret, type SecretProps } from "./Resources.ts";
export { fromVault, VaultSecretsProvider, vaultConfigProvider } from "./Vault.ts";

declare global {
  // Alchemy sets this in deployed Workers; the stack's plan-time code never sees it.
  var __ALCHEMY_RUNTIME__: boolean | undefined;
}

/**
 * The project's vault and this stage's consumer, or `undefined` without the
 * runner's credentials (`alchemy dev`, a laptop) and in a deployed Worker.
 * Looked up once per process and consumer. A preview gets its own consumer,
 * which reads nothing unless an admin grants it. A failed lookup fails the
 * deploy (as a defect: a stack's program may only fail with config errors).
 */
export const ProjectVault = (
  options: { readonly consumer?: string; readonly client?: Client } = {},
): Effect.Effect<VaultInfo | undefined> => lookup(options).pipe(Effect.orDie);

const lookup = (options: {
  readonly consumer?: string;
  readonly client?: Client;
}): Effect.Effect<VaultInfo | undefined, SecretsManagerError> =>
  Effect.suspend(() => {
    if (globalThis.__ALCHEMY_RUNTIME__) return Effect.succeed(undefined);
    const client = options.client ?? defaultClient();
    return client === undefined ? Effect.succeed(undefined) : client.vault(options.consumer);
  });

/** What a consumer binding carries: its consumer and vault, fixed by the deploy. */
export interface ConsumerProps {
  readonly role: "consumer";
  readonly consumerId: string;
  readonly vaultId: string;
}

/**
 * A service binding to the secrets host, fixed to this stage's consumer and
 * the project's vault. Pass your Cloudflare module's `WorkerEntrypoint`:
 *
 * ```ts
 * env: { SECRETS: SecretsManager.binding(Cloudflare.WorkerEntrypoint, vault) }
 * ```
 *
 * The host lives in the secrets manager's stack, so the binding names its
 * script without managing it.
 */
export const binding = <B>(
  entrypoint: (worker: never, options: { readonly props: ConsumerProps }) => B,
  vault: VaultInfo,
): B =>
  entrypoint({ Type: "Cloudflare.Worker", workerName: vault.hostScript } as never, {
    props: { role: "consumer", consumerId: vault.consumerId, vaultId: vault.vaultId },
  });
