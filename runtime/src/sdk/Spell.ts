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
import type * as Effect from "effect/Effect"
import type { SpellServices } from "./Services.ts"
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
}

export const make = (spell: Omit<Spell, typeof TypeId>): Spell => ({ [TypeId]: TypeId, ...spell })

export const isSpell = (u: unknown): u is Spell => typeof u === "object" && u !== null && TypeId in u
