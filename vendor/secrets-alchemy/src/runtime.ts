/**
 * Reading a granted secret at runtime, in a Worker that binds the project's
 * vault (`SecretsManager.binding`). No Alchemy or Effect imports, so any
 * Worker can use it.
 */

/** Alchemy's RPC reports an expected failure as this envelope rather than rejecting. */
const RPC_ERROR = "~alchemy/rpc/error";

/**
 * Reading failed. `reason` is the secrets host's tag: `SecretNotGranted`
 * (not granted, or no such secret: indistinguishable on purpose),
 * `BindingMisconfigured`, `InvalidRequest`, `SecretsUnavailable`, or
 * `NotBound` when the Worker has no binding (e.g. under `alchemy dev`).
 */
export class SecretsError extends Error {
  override readonly name = "SecretsError";
  constructor(readonly reason: string) {
    super(`Secrets manager: ${reason}`);
  }
}

interface VaultBinding {
  readonly get: (name: string) => Promise<unknown>;
  readonly hasHash: (digest: string) => Promise<unknown>;
}

const isBinding = (value: unknown): value is VaultBinding =>
  typeof value === "object" &&
  value !== null &&
  typeof (value as { get?: unknown }).get === "function";

const settle = async (call: () => Promise<unknown>) => {
  let result: unknown;
  try {
    result = await call();
  } catch {
    throw new SecretsError("SecretsUnavailable");
  }
  const envelope = result as { readonly _tag?: unknown; readonly error?: { _tag?: unknown } };
  if (envelope?._tag === RPC_ERROR)
    throw new SecretsError(String(envelope.error?._tag ?? "SecretsUnavailable"));
  return result;
};

/** The current value of a granted secret. Revoking the grant takes effect on the next call. */
export const readSecret = async (binding: unknown, name: string): Promise<string> => {
  if (!isBinding(binding)) throw new SecretsError("NotBound");
  const value = await settle(() => binding.get(name));
  if (typeof value !== "string") throw new SecretsError("SecretsUnavailable");
  return value;
};

/**
 * Whether a secret granted with hash inspection holds the value whose
 * SHA-256 is `digest` (`sha256:<64 lowercase hex>`). Never log the digest.
 */
export const hasSecretHash = async (binding: unknown, digest: string): Promise<boolean> => {
  if (!isBinding(binding)) throw new SecretsError("NotBound");
  const found = await settle(() => binding.hasHash(digest));
  if (typeof found !== "boolean") throw new SecretsError("SecretsUnavailable");
  return found;
};
