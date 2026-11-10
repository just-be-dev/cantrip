import * as Effect from "effect/Effect";
import * as Redacted from "effect/Redacted";
import * as Schema from "effect/Schema";

/**
 * The secrets manager's deploy API (`docs/project-vaults.md` in the secrets
 * manager), as a deploy calls it. The projects platform's runner sets the
 * two variables below; without them there's no client, and the vault
 * contributes nothing.
 */

/** The secrets manager's Api URL. */
export const URL_VARIABLE = "SECRETS_MANAGER_URL";
/** The deploy token: one project's stage, for an hour. A secret. */
export const TOKEN_VARIABLE = "SECRETS_MANAGER_TOKEN";

/**
 * A deploy API call failed. `reason` is the API's error tag
 * (`DeployUnauthenticated`, `PreviewStage`, `SecretNotFound`, …) or
 * `Unreachable`. Never carries a value or the token.
 */
export class SecretsManagerError extends Schema.TaggedError<SecretsManagerError>()(
  "SecretsManagerError",
  { reason: Schema.String, message: Schema.String },
) {}

/** The project's vault, as a deploy of one stage sees it. */
export const VaultInfo = Schema.Struct({
  project: Schema.String,
  stage: Schema.String,
  vaultId: Schema.String,
  consumerId: Schema.String,
  /** Prod: reads, writes and grants are allowed. A preview may only look the vault up. */
  writable: Schema.Boolean,
  /** The secrets host's Worker script, which a consumer binding names. */
  hostScript: Schema.String,
});
export type VaultInfo = typeof VaultInfo.Type;

const ReadReply = Schema.Struct({ values: Schema.Record(Schema.String, Schema.String) });
const PutReply = Schema.Struct({ changed: Schema.Boolean, revision: Schema.Number });
const GrantsReply = Schema.Struct({ consumerId: Schema.String });
const ErrorReply = Schema.Struct({ error: Schema.String });

export interface Credentials {
  readonly url: string;
  readonly token: Redacted.Redacted<string>;
}

/** The credentials the runner set, from `env` (the process environment by default). */
export const credentialsFrom = (
  env: Readonly<Record<string, string | undefined>> = globalThis.process?.env ?? {},
): Credentials | undefined => {
  const url = env[URL_VARIABLE]?.trim();
  const token = env[TOKEN_VARIABLE]?.trim();
  if (!url || !token) return undefined;
  return { url, token: Redacted.make(token) };
};

const describe: Record<string, string> = {
  DeployUnauthenticated: "the deploy token is missing, unknown or expired",
  PreviewStage: "only a prod deploy may read, write or grant",
  SecretNotFound: "the vault has no secret by that name",
  RetiredFingerprint: "that value was retired by a rotation and can't be used again",
  InvalidSecretValue: "the value is empty, too large, or not well-formed text",
  InvalidRequest: "the request was malformed",
  WriteConflict: "the secret kept changing; retry the deploy",
  SecretsUnavailable: "the secrets manager couldn't complete the call",
  Unreachable: "the secrets manager didn't answer",
};

const fail = (reason: string, what: string) =>
  new SecretsManagerError({
    reason,
    message: `Secrets manager: ${what}: ${describe[reason] ?? reason}.`,
  });

export type Fetch = (input: string, init: RequestInit) => Promise<Response>;

/**
 * A client for one deploy's credentials. `fetch` is the global one unless a
 * test supplies another.
 */
export const makeClient = (credentials: Credentials, fetchImpl: Fetch = (i, r) => fetch(i, r)) => {
  const base = credentials.url.replace(/\/+$/u, "");
  const call = <S extends Schema.Top & { readonly DecodingServices: never }>(
    what: string,
    method: "GET" | "POST" | "PUT",
    path: string,
    reply: S,
    body?: unknown,
  ): Effect.Effect<S["Type"], SecretsManagerError> =>
    Effect.gen(function* () {
      const response = yield* Effect.tryPromise({
        try: () =>
          fetchImpl(`${base}${path}`, {
            method,
            headers: {
              authorization: `Bearer ${Redacted.value(credentials.token)}`,
              ...(body === undefined ? {} : { "content-type": "application/json" }),
            },
            ...(body === undefined ? {} : { body: JSON.stringify(body) }),
          }),
        catch: () => fail("Unreachable", what),
      });
      const json = yield* Effect.tryPromise({
        try: () => response.json() as Promise<unknown>,
        catch: () => fail("SecretsUnavailable", what),
      });
      if (!response.ok) {
        const error = Schema.decodeUnknownOption(ErrorReply)(json);
        return yield* fail(error._tag === "Some" ? error.value.error : "SecretsUnavailable", what);
      }
      return yield* Schema.decodeUnknownEffect(reply)(json).pipe(
        Effect.mapError(() => fail("SecretsUnavailable", what)),
      );
    });

  /** Lookups so far, by consumer label: one per client and consumer. */
  const vaults = new Map<string, VaultInfo>();

  return {
    /** The project's vault and this stage's consumer. Looked up once per client and consumer. */
    vault: (consumer?: string) =>
      Effect.suspend(() => {
        const known = vaults.get(consumer ?? "");
        if (known !== undefined) return Effect.succeed(known);
        return call(
          "looking up the project's vault",
          "GET",
          `/api/deploy/vault${consumer === undefined ? "" : `?consumer=${encodeURIComponent(consumer)}`}`,
          VaultInfo,
        ).pipe(Effect.tap((info) => Effect.sync(() => void vaults.set(consumer ?? "", info))));
      }),
    read: (names: ReadonlyArray<string>) =>
      Effect.map(
        call("reading from the project's vault", "POST", "/api/deploy/read", ReadReply, { names }),
        (reply) => reply.values,
      ),
    put: (name: string, value: Redacted.Redacted<string>) =>
      call(
        `writing ${name} to the project's vault`,
        "PUT",
        `/api/deploy/secrets/${encodeURIComponent(name)}`,
        PutReply,
        { value: Redacted.value(value) },
      ),
    grants: (input: {
      readonly consumer?: string | undefined;
      readonly grant: ReadonlyArray<string>;
      readonly revoke: ReadonlyArray<string>;
      readonly hashInspection?: boolean | undefined;
    }) =>
      call("granting the project's Workers", "POST", "/api/deploy/grants", GrantsReply, {
        ...(input.consumer === undefined ? {} : { consumer: input.consumer }),
        grant: input.grant,
        revoke: input.revoke,
        ...(input.hashInspection === undefined ? {} : { hashInspection: input.hashInspection }),
      }),
  };
};

export type Client = ReturnType<typeof makeClient>;

/** The client for this process's credentials, or undefined without them. */
let shared: { readonly key: string; readonly client: Client } | undefined;

/** The client for this process's credentials (one per URL and token), or undefined without them. */
export const defaultClient = (): Client | undefined => {
  const credentials = credentialsFrom();
  if (credentials === undefined) return undefined;
  const key = `${credentials.url}\n${Redacted.value(credentials.token)}`;
  if (shared?.key !== key) shared = { key, client: makeClient(credentials) };
  return shared.client;
};
