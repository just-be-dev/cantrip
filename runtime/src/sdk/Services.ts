import type * as FileSystem from "effect/FileSystem"
import type * as HttpClient from "effect/http/HttpClient"
import type * as Path from "effect/Path"
import type * as ChildProcessSpawner from "effect/process/ChildProcessSpawner"
import type { Ui } from "./Ui.ts"

/**
 * Everything the host provides to a running spell. A spell's `run` effect and
 * its actions may require any of these without providing them.
 *
 * stdout is reserved for the app protocol, so `Stdio` and `Terminal` are not
 * offered. `console.log` and `Effect.log` go to the host log instead.
 */
export type SpellServices =
  | Ui
  | HttpClient.HttpClient
  | FileSystem.FileSystem
  | Path.Path
  | ChildProcessSpawner.ChildProcessSpawner
