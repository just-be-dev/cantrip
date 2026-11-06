/**
 * The `Ui` service is the running spell's handle on the command bar.
 */
import * as Context from "effect/Context"
import * as Duration from "effect/Duration"
import * as Effect from "effect/Effect"
import * as Stream from "effect/Stream"
import type { SpellServices } from "./Services.ts"
import * as View from "./View.ts"

export interface Toast {
  readonly style?: "success" | "failure" | "info" | undefined
  readonly title: string
  readonly message?: string | undefined
}

/**
 * Each view in a spell's stack gets its own `Ui`. `render`, `pop`, and `query`
 * act on that view even after other views have been pushed above it.
 */
export class Ui extends Context.Service<Ui, {
  /** Show `view` as this `Ui`'s view, or update it in place. */
  readonly render: (view: View.View) => Effect.Effect<void>
  /**
   * Push a view above this one. A program runs with a `Ui` for the new view and
   * is interrupted when the user goes back. Esc returns here.
   */
  readonly push: (view: View.View | Effect.Effect<unknown, unknown, SpellServices>) => Effect.Effect<void>
  /** Close this view and everything above it. Popping the first view closes the spell. */
  readonly pop: Effect.Effect<void>
  /** This view's search text: emits the current value, then every change. */
  readonly query: Stream.Stream<string>
  /** Show or hide the loading bar. */
  readonly setLoading: (loading: boolean) => Effect.Effect<void>
  /** A message in the bar's footer. */
  readonly toast: (toast: Toast) => Effect.Effect<void>
  /** Hide the bar and flash a small message over the screen. */
  readonly hud: (message: string) => Effect.Effect<void>
  /** Hide the bar. */
  readonly hide: Effect.Effect<void>
  /** Put text on the system clipboard. */
  readonly copy: (text: string) => Effect.Effect<void>
  /** Open a URL, file, or app with the default handler. */
  readonly open: (target: string) => Effect.Effect<void>
}>()("incant/Ui") {}

export const render = (view: View.View) => Ui.use((ui) => ui.render(view))
export const push = (view: View.View | Effect.Effect<unknown, unknown, SpellServices>) =>
  Ui.use((ui) => ui.push(view))
export const pop = Ui.use((ui) => ui.pop)
export const setLoading = (loading: boolean) => Ui.use((ui) => ui.setLoading(loading))
export const toast = (toast: Toast) => Ui.use((ui) => ui.toast(toast))
export const hud = (message: string) => Ui.use((ui) => ui.hud(message))
export const hide = Ui.use((ui) => ui.hide)
export const copy = (text: string) => Ui.use((ui) => ui.copy(text))
export const open = (target: string) => Ui.use((ui) => ui.open(target))
export const query = Stream.unwrap(Ui.useSync((ui) => ui.query))

/** Render a static list. Swift filters it as the user types. */
export const list = (options: Omit<View.List, "_tag">) => render(View.list(options))

/** Render a markdown detail view. */
export const detail = (options: Omit<View.Detail, "_tag">) => render(View.detail(options))

/**
 * A list whose items come from `search(query)`. It reruns after typing pauses
 * for `debounce` (default 120ms), and a new keystroke cancels the old search.
 * Search failures show in the footer, and the list stays up.
 */
export const searchList = <E>(options: {
  readonly search: (query: string) => Effect.Effect<ReadonlyArray<View.ListItem>, E, SpellServices>
  readonly placeholder?: string | undefined
  readonly emptyText?: string | undefined
  readonly debounce?: Duration.Input | undefined
}): Effect.Effect<void, never, SpellServices> =>
  Effect.gen(function*() {
    const ui = yield* Ui
    const show = (items: ReadonlyArray<View.ListItem>) =>
      ui.render(View.list({
        items,
        placeholder: options.placeholder,
        emptyText: options.emptyText,
        filtering: false
      }))

    yield* show([])
    yield* ui.query.pipe(
      Stream.debounce(options.debounce ?? Duration.millis(120)),
      Stream.switchMap((q) =>
        Stream.fromEffect(
          Effect.gen(function*() {
            yield* ui.setLoading(true)
            const items = yield* options.search(q)
            yield* show(items)
          }).pipe(
            Effect.catchCause((cause) => ui.toast({ style: "failure", title: "Search failed", message: String(cause) })),
            Effect.ensuring(ui.setLoading(false))
          )
        )
      ),
      Stream.runDrain
    )
  })
