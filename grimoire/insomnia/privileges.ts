// The sudoers rule and its one-time admin install. Same rule and path as Insomnia.app, so an
// existing /etc/sudoers.d/insomnia keeps working unchanged.
import { userInfo } from "node:os"
import * as Effect from "effect/Effect"
import * as FileSystem from "effect/FileSystem"
import * as Layer from "effect/Layer"
import * as Path from "effect/Path"
import { ChildProcessSpawner } from "effect/process/ChildProcessSpawner"
import { PrivilegeInstallFailed } from "./model.ts"
import { disableSleepArgs, exec, PMSET } from "./pmset.ts"
import { PrivilegeInstaller } from "./service.ts"
import { stateDirConfig } from "./store.ts"

export const SUDOERS_PATH = "/etc/sudoers.d/insomnia"

const USERNAME = /^[a-z_][a-z0-9_.-]*$/i

/** One rule, two exact commands: nothing else `pmset` can do becomes passwordless. */
export const sudoersRule = (username: string): string => {
  if (!USERNAME.test(username)) throw new Error(`Refusing unusual username: ${username}`)
  const command = (disabled: boolean) => [PMSET, ...disableSleepArgs(disabled)].join(" ")
  return [
    "# Installed by insomnia. Lets this user toggle `pmset disablesleep` without a password.",
    `# Remove with \`sudo rm ${SUDOERS_PATH}\`.`,
    `${username} ALL=(root) NOPASSWD: ${command(false)}, ${command(true)}`,
    ""
  ].join("\n")
}

/** AppleScript string literal. */
const appleScriptString = (value: string) => `"${value.replaceAll("\\", "\\\\").replaceAll("\"", "\\\"")}"`

/** Single-quoted shell word. */
const shellQuote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`

/** Runs a shell script as root behind the standard macOS admin prompt (Touch ID or password). */
const runAsAdmin = Effect.fn("privileges.runAsAdmin")(function*(script: string) {
  const result = yield* exec("/usr/bin/osascript", [
    "-e",
    `do shell script ${appleScriptString(script)} with administrator privileges`
  ]).pipe(Effect.mapError((error) => new PrivilegeInstallFailed({ detail: error.message })))
  if (result.exitCode !== 0) {
    const detail = result.stderr.trim()
    return yield* new PrivilegeInstallFailed({
      detail: /User canceled|-128/.test(detail) ? "cancelled" : detail || `exit ${result.exitCode}`
    })
  }
})

export const privilegeInstallerLayer = Layer.effect(
  PrivilegeInstaller,
  Effect.gen(function*() {
    const fs = yield* FileSystem.FileSystem
    const path = yield* Path.Path
    const spawner = yield* ChildProcessSpawner
    const stateDir = yield* stateDirConfig

    const install = Effect.scoped(
      Effect.gen(function*() {
        const rule = yield* Effect.try({
          try: () => sudoersRule(userInfo().username),
          catch: (error) => new PrivilegeInstallFailed({ detail: String(error) })
        })
        // Stage under our own state dir rather than $TMPDIR, which a launcher can leave stale.
        yield* fs.makeDirectory(stateDir, { recursive: true })
        const dir = yield* fs.makeTempDirectoryScoped({ directory: stateDir, prefix: "sudoers-" })
        const file = path.join(dir, "sudoers")
        yield* fs.writeFileString(file, rule)
        // visudo validates before install, so a bad file can never lock out sudo.
        yield* runAsAdmin(
          [
            `/usr/sbin/visudo -cf ${shellQuote(file)}`,
            `/usr/bin/install -m 0440 -o root -g wheel ${shellQuote(file)} ${SUDOERS_PATH}`
          ].join(" && ")
        )
      })
    ).pipe(
      Effect.catchTag("PlatformError", (error) => Effect.fail(new PrivilegeInstallFailed({ detail: error.message }))),
      Effect.provideService(ChildProcessSpawner, spawner),
      Effect.withSpan("PrivilegeInstaller.install")
    )

    return PrivilegeInstaller.of({ install })
  })
)
