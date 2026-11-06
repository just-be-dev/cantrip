/**
 * A spell is one command in the bar: metadata plus an Effect to run.
 *
 * ```ts
 * import * as Effect from "effect/Effect"
 * import { Spell, Ui } from "cantrip"
 *
 * export default Spell.make({
 *   title: "Generate UUID",
 *   icon: "number",
 *   mode: "no-view",
 *   run: Effect.gen(function*() {
 *     const id = crypto.randomUUID()
 *     yield* Ui.copy(id)
 *     yield* Ui.hud(`Copied ${id}`)
 *   })
 * })
 * ```
 */
import type * as Command from "effect/cli/Command"
import type * as Effect from "effect/Effect"
import type { CommandServices, SpellServices } from "./Services.ts"
import type { Icon } from "./View.ts"

export const TypeId = "~cantrip/Spell" as const

export interface Spell {
  readonly [TypeId]: typeof TypeId
  readonly title: string
  readonly subtitle?: string | undefined
  readonly icon?: Icon | undefined
  readonly keywords?: ReadonlyArray<string> | undefined
  /**
   * `"view"` (default): the bar stays open and shows what the spell renders.
   * `"no-view"`: the bar hides right away and the spell runs in the background.
   */
  readonly mode?: "view" | "no-view" | undefined
  readonly run: Effect.Effect<unknown, unknown, SpellServices>
  /**
   * A CLI command (`effect/cli`) to go with the spell. Cantrip links it into
   * `~/.local/bin` under the command's name, and running it there runs the
   * handler in the host, printing to your terminal:
   *
   * ```ts
   * command: Command.make("uuid", { count: Flag.integer("count").pipe(Flag.withDefault(1)) }, ({ count }) =>
   *   Effect.forEach(Array.from({ length: count }), () => Console.log(crypto.randomUUID())))
   * ```
   */
  readonly command?: Command.Command<any, any, any, unknown, CommandServices> | undefined
}

export const make = (spell: Omit<Spell, typeof TypeId>): Spell => ({ [TypeId]: TypeId, ...spell })

export const isSpell = (u: unknown): u is Spell => typeof u === "object" && u !== null && TypeId in u
