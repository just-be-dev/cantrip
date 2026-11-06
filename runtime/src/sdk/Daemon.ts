/**
 * A daemon is a long-running companion that lives in the menu bar. It starts
 * when the host starts (or when its file is added), and runs until the host
 * stops or its file changes. Use it for things you want to glance at or that
 * must keep running when the bar is closed.
 *
 * ```ts
 * import * as Effect from "effect/Effect"
 * import { Daemon, MenuBar } from "cantrip"
 *
 * export const clock = Daemon.summon({
 *   title: "Clock",
 *   icon: "clock",
 *   run: Effect.gen(function*() {
 *     while (true) {
 *       yield* MenuBar.render({ title: new Date().toLocaleTimeString(), menu: [] })
 *       yield* Effect.sleep("1 second")
 *     }
 *   })
 * })
 * ```
 *
 * `run` runs in a scope that closes when the daemon stops, so
 * `Effect.addFinalizer` / `acquireRelease` clean up on quit, on Restart
 * Daemon, and before a changed file is loaded again. Files that didn't change
 * keep their daemons running across reloads.
 *
 * Menu actions (`MenuBar.item(…, { onSelect })`) run in the same scope, and
 * are also offered in the bar's search.
 */
import type * as Command from "effect/cli/Command"
import type * as Effect from "effect/Effect"
import type * as Layer from "effect/Layer"
import type { CommandServices, DaemonServices } from "./Services.ts"
import type { Icon } from "./View.ts"

export const TypeId = "~cantrip/Daemon" as const

/** What `summon` takes. `Provides` is what `layer` builds; `run`, menu actions, and `command` can use it. */
export interface Options<Provides> {
  readonly title: string
  /** The menu bar icon until the daemon renders one. */
  readonly icon?: Icon | undefined
  /**
   * Services the daemon shares with its CLI command, built once in the
   * daemon's scope (so their finalizers run when it stops).
   */
  readonly layer?: Layer.Layer<Provides, unknown, DaemonServices> | undefined
  readonly run: Effect.Effect<unknown, unknown, DaemonServices | Provides>
  /**
   * A CLI command (`effect/cli`), linked into `~/.local/bin` under the
   * command's name. It runs inside this daemon, so it sees the same live
   * state as the menu. See `Spell.command`.
   */
  readonly command?: Command.Command<any, any, any, unknown, CommandServices | DaemonServices | Provides> | undefined
}

/** A summoned daemon. The host erases `Provides`; `summon` is where it's checked. */
export interface Daemon {
  readonly [TypeId]: typeof TypeId
  readonly title: string
  readonly icon?: Icon | undefined
  readonly layer?: Layer.Layer<any, unknown, DaemonServices> | undefined
  readonly run: Effect.Effect<unknown, unknown, any>
  readonly command?: Command.Command<any, any, any, unknown, any> | undefined
}

export const summon = <Provides = never>(daemon: Options<Provides>): Daemon => ({ [TypeId]: TypeId, ...daemon })

export const isDaemon = (u: unknown): u is Daemon => typeof u === "object" && u !== null && TypeId in u
