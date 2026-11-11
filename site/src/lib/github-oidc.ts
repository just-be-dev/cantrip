/**
 * Verifies a GitHub Actions OIDC token: the identity GitHub gives a workflow
 * run (`id-token: write`), signed by GitHub. Publishing a release needs one
 * from the release workflow on main, so no secret is stored anywhere.
 *
 * https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/about-security-hardening-with-openid-connect
 */
const issuer = "https://token.actions.githubusercontent.com";
const jwksUrl = `${issuer}/.well-known/jwks`;

export interface Expected {
  /** The audience the workflow asked for: the site's origin. */
  readonly audience: string;
  /** `owner/repo`. */
  readonly repository: string;
  /** The workflow file, e.g. `.github/workflows/release.yml`. */
  readonly workflow: string;
  /** e.g. `refs/heads/main`. */
  readonly ref: string;
}

export interface Claims {
  readonly iss: string;
  readonly aud: string | string[];
  readonly exp: number;
  readonly nbf?: number;
  readonly repository: string;
  readonly ref: string;
  readonly sha: string;
  readonly job_workflow_ref: string;
  readonly run_id: string;
}

const decoder = new TextDecoder();

const base64url = (text: string) =>
  Uint8Array.from(atob(text.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));

const part = <A>(text: string): A => JSON.parse(decoder.decode(base64url(text))) as A;

type Jwk = JsonWebKey & { kid: string };
let keys: { at: number; set: Jwk[] } | undefined;

const key = async (kid: string) => {
  const fresh = async () => {
    const response = await fetch(jwksUrl);
    if (!response.ok) throw new Error(`GitHub JWKS: ${response.status}`);
    const set = ((await response.json()) as { keys: Jwk[] }).keys;
    keys = { at: Date.now(), set };
    return set;
  };
  const stale = keys === undefined || Date.now() - keys.at > 60 * 60 * 1000;
  let jwk = (stale ? await fresh() : keys!.set).find((k) => k.kid === kid);
  // GitHub rotates keys: an unknown kid may just be newer than the cache.
  if (jwk === undefined && !stale) jwk = (await fresh()).find((k) => k.kid === kid);
  if (jwk === undefined) throw new Error(`unknown signing key ${kid}`);
  return crypto.subtle.importKey("jwk", jwk, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, [
    "verify",
  ]);
};

/** The token's claims if it's valid and from the expected workflow, else an error message. */
export const verify = async (
  token: string,
  expected: Expected,
): Promise<{ ok: true; claims: Claims } | { ok: false; error: string }> => {
  const [header, payload, signature] = token.split(".");
  if (!header || !payload || !signature) return { ok: false, error: "not a JWT" };
  try {
    const { alg, kid } = part<{ alg: string; kid: string }>(header);
    if (alg !== "RS256") return { ok: false, error: `unexpected alg ${alg}` };
    const valid = await crypto.subtle.verify(
      "RSASSA-PKCS1-v1_5",
      await key(kid),
      base64url(signature),
      new TextEncoder().encode(`${header}.${payload}`),
    );
    if (!valid) return { ok: false, error: "bad signature" };

    const claims = part<Claims>(payload);
    const now = Date.now() / 1000;
    const audiences = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
    const workflow = `${expected.repository}/${expected.workflow}@${expected.ref}`;
    if (claims.iss !== issuer) return { ok: false, error: "wrong issuer" };
    if (!audiences.includes(expected.audience)) return { ok: false, error: "wrong audience" };
    if (claims.exp < now - 30 || (claims.nbf ?? 0) > now + 30) return { ok: false, error: "expired" };
    if (claims.repository !== expected.repository) return { ok: false, error: "wrong repository" };
    if (claims.ref !== expected.ref) return { ok: false, error: `not from ${expected.ref}` };
    if (claims.job_workflow_ref !== workflow) return { ok: false, error: `not from ${workflow}` };
    return { ok: true, claims };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
};
