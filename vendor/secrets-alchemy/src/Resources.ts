import { isResolved } from "alchemy/Diff";
import * as Provider from "alchemy/Provider";
import { Resource } from "alchemy/Resource";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Redacted from "effect/Redacted";
import { type Client, defaultClient, SecretsManagerError } from "./Client.ts";

/**
 * Resources a project's stack declares against its vault. Both act only in a
 * prod deploy with the runner's credentials. In a preview they change
 * nothing; without credentials (`alchemy dev`, a laptop) they change nothing
 * and warn. Either way they record that they didn't apply, so the next
 * deploy that can apply them does.
 */

/** A secret whose value this stack owns. */
export interface SecretProps {
  /** The secret's name in the vault. Defaults to the resource's id. */
  readonly name?: string;
  /**
   * The value. It's kept in this stack's Alchemy state like any Redacted
   * prop, so prefer a value Alchemy already holds, such as a token another
   * resource minted.
   */
  readonly value: Redacted.Redacted<string>;
}

export type Secret = Resource<
  "SecretsManager.Secret",
  SecretProps,
  {
    name: string;
    /** Whether the vault holds this value: false in a preview or without credentials. */
    written: boolean;
    /** The vault written to; empty when not written. A new project vault is written again. */
    vaultId: string;
    /** The secret's revision after the write; 0 when not written. */
    revision: number;
  }
>;

/**
 * Write a value into the project's vault, as the secret `name`. A redeploy
 * with the same value writes nothing; a changed value is a new version.
 * Removing the resource leaves the secret in the vault.
 */
export const Secret = Resource<Secret>("SecretsManager.Secret");

/** Secrets a stage's Workers may read at runtime through their consumer binding. */
export interface GrantsProps {
  /** Exactly these secrets; each must exist in the vault. */
  readonly secrets: ReadonlyArray<string>;
  /** Names one of the stage's consumers when a stack has more than one. */
  readonly consumer?: string;
  /** Also let the Worker check a value by its SHA-256 (`hasHash`). */
  readonly hashInspection?: boolean;
}

export type Grants = Resource<
  "SecretsManager.Grants",
  GrantsProps,
  {
    consumerId: string;
    consumer: string | null;
    /** What's granted now: empty when not written. */
    secrets: ReadonlyArray<string>;
    hashInspection: boolean;
    written: boolean;
    /** The vault the grants are in; empty when not written. A new project vault is granted again. */
    vaultId: string;
  }
>;

/**
 * Grant the stage's consumer exactly `secrets`: grants the ones it adds and
 * revokes the ones it drops, and revokes them all when removed. Prod only.
 */
export const Grants = Resource<Grants>("SecretsManager.Grants");

const valueOf = (value: unknown) =>
  Redacted.isRedacted(value) ? Redacted.value(value as Redacted.Redacted<string>) : value;

const sameSet = (a: ReadonlyArray<string>, b: ReadonlyArray<string>) =>
  a.length === b.length && [...a].sort().join("\n") === [...b].sort().join("\n");

/** A preview's deploy may not write: that's "not applied", not a failure. */
const unlessPreview = <A>(
  effect: Effect.Effect<A, SecretsManagerError>,
): Effect.Effect<A | undefined, SecretsManagerError> =>
  effect.pipe(
    Effect.catchTag("SecretsManagerError", (error) =>
      error.reason === "PreviewStage" ? Effect.succeed(undefined) : Effect.fail(error),
    ),
  );

const noCredentials = (what: string) =>
  Effect.logWarning(
    `SecretsManager: no SECRETS_MANAGER_URL/SECRETS_MANAGER_TOKEN, so ${what} wasn't applied. The platform's runner sets them; a deploy that has them applies it.`,
  );

/**
 * The providers for {@link Secret} and {@link Grants}. Add them to the
 * stack's `providers`. `client` is for tests; by default each operation uses
 * the runner's credentials.
 */
export const providers = (options: { readonly client?: () => Client | undefined } = {}) => {
  const clientOf = options.client ?? defaultClient;

  /**
   * Whether what's recorded is still what the vault holds: written, and to
   * the project's current vault (an admin may have deleted it, and the next
   * lookup maps a new one). Without credentials there's nothing to compare.
   */
  const stillApplied = (output: { readonly written: boolean; readonly vaultId: string }) =>
    Effect.gen(function* () {
      const client = clientOf();
      if (client === undefined) return true;
      if (!output.written) return false;
      return (yield* client.vault()).vaultId === output.vaultId;
    });

  const secret = Provider.succeed(Secret, {
    stables: ["name"],
    diff: Effect.fn(function* ({ id, olds, news, output }) {
      if (!isResolved(news)) return undefined;
      const changed =
        (news.name ?? id) !== (output?.name ?? olds?.name ?? id) ||
        valueOf(olds?.value) !== valueOf(news.value);
      const applied = output === undefined || (yield* stillApplied(output));
      return changed || !applied ? { action: "update" as const } : { action: "noop" as const };
    }),
    read: ({ output }) => Effect.succeed(output),
    reconcile: Effect.fn(function* ({ id, news }) {
      const name = news.name ?? id;
      const unwritten = { name, written: false, vaultId: "", revision: 0 };
      const client = clientOf();
      if (client === undefined) {
        yield* noCredentials(`the secret ${name}`);
        return unwritten;
      }
      const { vaultId } = yield* client.vault();
      const put = yield* unlessPreview(client.put(name, news.value));
      return put === undefined
        ? unwritten
        : { name, written: true, vaultId, revision: put.revision };
    }),
    // The secret stays in the vault: other stages or an admin may rely on it.
    delete: () => Effect.void,
  });

  const grants = Provider.succeed(Grants, {
    diff: Effect.fn(function* ({ news, output }) {
      if (!isResolved(news) || output === undefined) return undefined;
      const changed =
        !sameSet(output.secrets, news.secrets) ||
        (news.consumer ?? null) !== output.consumer ||
        (news.hashInspection ?? false) !== output.hashInspection;
      const applied = yield* stillApplied(output);
      return (output.written && changed) || !applied
        ? { action: "update" as const }
        : { action: "noop" as const };
    }),
    read: ({ output }) => Effect.succeed(output),
    reconcile: Effect.fn(function* ({ news, output }) {
      const consumer = news.consumer ?? null;
      const hashInspection = news.hashInspection ?? false;
      // Not applied here: keep what was recorded (it's still what the vault
      // holds), so a deploy that can apply it compares against the truth.
      const unchanged = output ?? {
        consumerId: "",
        consumer,
        secrets: [],
        hashInspection,
        written: false,
        vaultId: "",
      };
      const client = clientOf();
      if (client === undefined) {
        yield* noCredentials("the Workers' grants");
        return unchanged;
      }
      const { vaultId } = yield* client.vault();
      // Grants recorded in a vault that's since been replaced went with it.
      const before = output?.written && output.vaultId === vaultId ? output : undefined;
      // Moving to another consumer: the old one loses everything first.
      if (before !== undefined && before.consumer !== consumer && before.secrets.length > 0)
        yield* unlessPreview(
          client.grants({
            consumer: before.consumer ?? undefined,
            grant: [],
            revoke: before.secrets,
          }),
        );
      const dropped =
        before !== undefined && before.consumer === consumer
          ? before.secrets.filter((name) => !news.secrets.includes(name))
          : [];
      const granted = yield* unlessPreview(
        client.grants({
          consumer: consumer ?? undefined,
          grant: news.secrets,
          revoke: dropped,
          hashInspection,
        }),
      );
      return granted === undefined
        ? unchanged
        : {
            consumerId: granted.consumerId,
            consumer,
            secrets: [...news.secrets],
            hashInspection,
            written: true,
            vaultId,
          };
    }),
    delete: Effect.fn(function* ({ output }) {
      if (!output.written || output.secrets.length === 0) return;
      const client = clientOf();
      // Fail closed: dropping the resource without revoking would leave grants nothing tracks.
      if (client === undefined)
        return yield* new SecretsManagerError({
          reason: "NoCredentials",
          message:
            "Secrets manager: revoking the Workers' grants needs SECRETS_MANAGER_URL and SECRETS_MANAGER_TOKEN. Remove the Grants in a deploy the platform runs.",
        });
      if ((yield* client.vault()).vaultId !== output.vaultId) return;
      yield* unlessPreview(
        client.grants({
          consumer: output.consumer ?? undefined,
          grant: [],
          revoke: output.secrets,
        }),
      );
    }),
  });

  return Layer.mergeAll(secret, grants);
};
