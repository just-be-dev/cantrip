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
import type * as Effect from "effect/Effect"
import type { DaemonServices } from "./Services.ts"
import type { Icon } from "./View.ts"

export const TypeId = "~cantrip/Daemon" as const

export interface Daemon {
  readonly [TypeId]: typeof TypeId
  readonly title: string
  /** The menu bar icon until the daemon renders one. */
  readonly icon?: Icon | undefined
  readonly run: Effect.Effect<unknown, unknown, DaemonServices>
}

export const summon = (daemon: Omit<Daemon, typeof TypeId>): Daemon => ({ [TypeId]: TypeId, ...daemon })

export const isDaemon = (u: unknown): u is Daemon => typeof u === "object" && u !== null && TypeId in u
