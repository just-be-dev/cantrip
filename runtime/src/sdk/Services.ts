import type * as FileSystem from "effect/FileSystem"
import type * as HttpClient from "effect/http/HttpClient"
import type * as Path from "effect/Path"
import type * as ChildProcessSpawner from "effect/process/ChildProcessSpawner"
import type * as Scope from "effect/Scope"
import type * as Stdio from "effect/Stdio"
import type * as Terminal from "effect/Terminal"
import type { Lifecycle } from "./Daemon.ts"
import type { MenuBar } from "./MenuBar.ts"
import type { Ui } from "./Ui.ts"

/** The platform services the host provides to everything it runs. */
export type PlatformServices =
  | HttpClient.HttpClient
  | FileSystem.FileSystem
  | Path.Path
  | ChildProcessSpawner.ChildProcessSpawner

/**
 * Everything the host provides to a running spell. A spell's `run` effect and
 * its actions may require any of these without providing them.
 *
 * stdout is reserved for the app protocol, so `Stdio` and `Terminal` are not
 * offered. `console.log` and `Effect.log` go to the host log instead.
 */
export type SpellServices = Ui | PlatformServices

/**
 * Everything the host provides to a running daemon and its menu actions.
 * Daemons have no bar view, so they get `MenuBar` instead of `Ui`. `Scope`
 * is the daemon's own: finalizers added with it run when the daemon stops.
 * `Lifecycle` says why it's stopping (see `Daemon.isHandingOff`).
 */
export type DaemonServices = MenuBar | Lifecycle | Scope.Scope | PlatformServices

/**
 * What a CLI command's handlers get (see `Spell.command` / `Daemon.command`).
 * `Stdio`, `Terminal`, and `Console` are wired to the terminal that ran the
 * command, so `Console.log` prints there. A daemon's commands also get
 * `DaemonServices` and whatever its `layer` provides.
 */
export type CommandServices = PlatformServices | Stdio.Stdio | Terminal.Terminal
