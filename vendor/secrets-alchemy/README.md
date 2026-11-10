# @just-be/secrets-alchemy

A project's vault in the secrets manager, from its Alchemy stack. The contract is the secrets manager's `docs/project-vaults.md`.

The source of truth is `packages/secrets-alchemy` in the secrets manager. Projects vendor a copy at `vendor/secrets-alchemy` with a `file:` dependency. Don't edit a vendored copy: change it here and copy it again.

It depends only on `alchemy` core and `effect`, so it works with `alchemy/Cloudflare` (beta.83) and `@alchemy.run/cloudflare` (beta.84).

## What the runner provides

projects.just-be.dev's runner sets `SECRETS_MANAGER_URL` and `SECRETS_MANAGER_TOKEN` (a deploy token for this project's stage, valid for an hour). Without them (`alchemy dev`, a deploy from a laptop) nothing reads or writes the vault: config falls back, `ProjectVault()` is `undefined`, and the resources record that they didn't apply and warn.

Only prod reads, writes and grants. A preview finds the vault and its own consumer (`project.<project>.<stage>`), which reads nothing unless an admin grants it.

## Use

```ts
import * as SecretsManager from "@just-be/secrets-alchemy";

export default Alchemy.Stack(
  name,
  {
    providers: Layer.mergeAll(Cloudflare.providers(), SecretsManager.providers()),
    state,
    // The vault wins over the shell; a name it lacks falls back to the shell.
    // (Unlisted, Alchemy adds the shell last, and the shell would win.)
    secrets: [Alchemy.Secrets.ProcessEnv(), SecretsManager.fromVault()],
  },
  Effect.gen(function* () {
    // Deploy-time: resolved from the vault in prod, else from the shell.
    const slack = yield* Config.Redacted("SLACK_SIGNING_SECRET");

    // Write a value Alchemy holds into the vault (prod only).
    const robot = yield* Cloudflare.Access.ServiceToken("Robot", {});
    yield* SecretsManager.Secret("RobotSecret", {
      name: "ROBOT_CLIENT_SECRET",
      value: robot.clientSecret,
    });

    // Runtime: grant this stage's Workers, then bind the vault.
    yield* SecretsManager.Grants("AppSecrets", { secrets: ["STRIPE_KEY"] });
    const vault = yield* SecretsManager.ProjectVault();
    const worker = yield* Cloudflare.Worker("App", {
      main: "./src/index.ts",
      env:
        vault === undefined
          ? {}
          : { SECRETS: SecretsManager.binding(Cloudflare.WorkerEntrypoint, vault) },
    });
    return { url: worker.url };
  }),
);
```

In the Worker:

```ts
import { readSecret } from "@just-be/secrets-alchemy/runtime";

const key = await readSecret(env.SECRETS, "STRIPE_KEY");
```

`readSecret` throws a `SecretsError` whose `reason` is the host's tag: `SecretNotGranted` (not granted or no such secret), `NotBound` (no binding, e.g. under `alchemy dev`), `SecretsUnavailable`, …

## Which to use

- **`readSecret` at runtime** keeps values out of Alchemy state and Worker settings, and revoking a grant applies on the next read. Prefer it for anything sensitive.
- **`fromVault()`** is for config the deploy itself needs, or for stacks that already read `Config.Redacted`. The values end up in Worker secret bindings and Alchemy state.
- **`Secret`** keeps its value in Alchemy state, like any Redacted prop. Use it for values Alchemy already holds; put hand-made values in through the admin site.

`Secret` writes only when the value differs from the active one. Removing it leaves the secret. `Grants` grants exactly its list, revokes what it drops, and revokes everything when removed. A grant needs the secret to exist: to grant one the stack writes, pass `secret.name` so the grant waits for the write. Both record the vault they applied to, and apply again after an admin replaces the project's vault.

List `Alchemy.Secrets.ProcessEnv()` before `fromVault()`. Alchemy adds the shell last when the list leaves it out, and then the shell wins over the vault.
