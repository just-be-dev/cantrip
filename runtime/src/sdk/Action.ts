/**
 * Things the user can do with a list item or detail view.
 * The first action runs on ↵, the second on ⌘↵, all of them under ⌘K.
 */
import type * as Effect from "effect/Effect"
import type { SpellServices } from "./Services.ts"
import * as Ui from "./Ui.ts"
import { type Action, type Icon, isView, type View } from "./View.ts"

export type { Action } from "./View.ts"

interface Options {
  readonly icon?: Icon | undefined
}

/** Run an effect in the spell. It is interrupted when the spell closes. */
export const run = (
  title: string,
  effect: Effect.Effect<unknown, unknown, SpellServices>,
  options?: Options
): Action => ({ _tag: "Run", title, effect, icon: options?.icon })

/** Copy text to the clipboard, hide the bar, and show a HUD. The app handles it directly. */
export const copy = (title: string, text: string, options?: Options): Action => ({
  _tag: "Copy",
  title,
  text,
  icon: options?.icon ?? "doc.on.doc"
})

/** Open a URL, file, or app with the default handler. The app handles it directly. */
export const open = (title: string, target: string, options?: Options): Action => ({
  _tag: "Open",
  title,
  target,
  icon: options?.icon ?? "arrow.up.forward.app"
})

/**
 * Push a new view on top. Pass a `View` for something static, or a program
 * (`Ui.searchList(...)`, an `Effect.gen` that calls `Ui.render`) for a live
 * view. The program's `Ui` targets the pushed view, and it is interrupted when
 * the user goes back.
 */
export const push = (
  title: string,
  view: View | Effect.Effect<unknown, unknown, SpellServices>,
  options?: Options
): Action => ({
  _tag: "Push",
  title,
  program: isView(view) ? Ui.render(view) : view,
  icon: options?.icon ?? "arrow.right"
})
