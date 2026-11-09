/**
 * A daemon's menu bar item: an icon (and optional title) in the macOS menu
 * bar with a menu under it. The app draws it; the daemon describes it.
 *
 * ```ts
 * yield* MenuBar.render({
 *   icon: MenuBar.ring(0.62),
 *   tooltip: "62% left",
 *   menu: [
 *     MenuBar.header("Claude Usage"),
 *     MenuBar.item("Current session", { accessory: "62% left", progress: 0.62, subtitle: "Resets in 2h" }),
 *     MenuBar.separator,
 *     MenuBar.item("Refresh", { key: "r", onSelect: refresh })
 *   ]
 * })
 * ```
 */
import * as Context from "effect/Context"
import type * as Effect from "effect/Effect"
import * as Stream from "effect/Stream"
import type { DaemonServices } from "./Services.ts"
import type { Icon } from "./View.ts"

/** How alarming something is. `"normal"` follows the system (template) color. */
export type Tone = "normal" | "warning" | "critical"

/**
 * A ring that fills clockwise from 12 o'clock. The app animates between values,
 * so render as often as you like.
 */
export interface Ring {
  readonly _tag: "Ring"
  /** How full the ring is, 0–1. `null` draws a dashed, empty ring (no data yet). */
  readonly value: number | null
  readonly tone?: Tone | undefined
  /** A segment orbits the ring, e.g. while fetching. */
  readonly busy?: boolean | undefined
  /** The ring fades in and out to get attention. */
  readonly pulse?: boolean | undefined
  /** A dot in the middle, e.g. the last refresh failed. */
  readonly badge?: boolean | undefined
}

export interface Item {
  readonly _tag: "Item"
  readonly title: string
  /** A second, smaller line under the title. */
  readonly subtitle?: string | undefined
  readonly icon?: Icon | undefined
  /** Short text on the right. Only shown on rows with `progress`. */
  readonly accessory?: string | undefined
  /** Turns the row into a gauge: a bar filled 0–1 under the title. Gauge rows aren't clickable. */
  readonly progress?: number | undefined
  readonly tone?: Tone | undefined
  /** A checkmark in front of the title. */
  readonly checked?: boolean | undefined
  /** Grayed out. Defaults to `false` for rows that do something, `true` for plain text rows. */
  readonly disabled?: boolean | undefined
  /** A ⌘-key shortcut while the menu is open, e.g. `"r"`. */
  readonly key?: string | undefined
  /** Runs in the daemon when the item is chosen, from the menu or from the bar's search. */
  readonly onSelect?: Effect.Effect<unknown, unknown, DaemonServices> | undefined
  /** Copy this text when chosen. The app handles it directly. */
  readonly copy?: string | undefined
  /** Open this URL or path when chosen. The app handles it directly. */
  readonly open?: string | undefined
  readonly submenu?: ReadonlyArray<Entry> | undefined
}

export type Entry = Item | { readonly _tag: "Separator" } | { readonly _tag: "Header"; readonly title: string }

export interface StatusItem {
  /** An SF Symbol, emoji, or file path (see `Icon`), or a `ring`. Defaults to the daemon's icon. */
  readonly icon?: Icon | Ring | undefined
  /** Text next to the icon, e.g. `"1h 12m"`. Keep it short. */
  readonly title?: string | undefined
  readonly tooltip?: string | undefined
  readonly menu: ReadonlyArray<Entry>
}

/**
 * The running daemon's handle on its menu bar item. Each daemon has one item;
 * it appears on the first `render` and goes away when the daemon stops.
 */
export class MenuBar extends Context.Service<MenuBar, {
  /** Show or update the item. */
  readonly render: (item: StatusItem) => Effect.Effect<void>
  /** Take the item out of the menu bar until the next `render`. */
  readonly remove: Effect.Effect<void>
  /** Emits each time the user opens the menu. The menu shows what was last rendered and updates live. */
  readonly opened: Stream.Stream<void>
  /** Flash a small message over the screen. */
  readonly hud: (message: string) => Effect.Effect<void>
  /** Open a URL, file, or app with the default handler. */
  readonly open: (target: string) => Effect.Effect<void>
}>()("cantrip/MenuBar") {}

export const render = (item: StatusItem) => MenuBar.use((m) => m.render(item))
export const remove = MenuBar.use((m) => m.remove)
export const opened = Stream.unwrap(MenuBar.useSync((m) => m.opened))
export const hud = (message: string) => MenuBar.use((m) => m.hud(message))
export const open = (target: string) => MenuBar.use((m) => m.open(target))

export const ring = (value: number | null, options?: Omit<Ring, "_tag" | "value">): Ring => ({
  _tag: "Ring",
  value,
  ...options
})

export const item = (title: string, options?: Omit<Item, "_tag" | "title">): Item => ({
  _tag: "Item",
  title,
  ...options
})

export const separator: Entry = { _tag: "Separator" }

/** A small gray section title. */
export const header = (title: string): Entry => ({ _tag: "Header", title })

export const isRing = (u: unknown): u is Ring => typeof u === "object" && u !== null && "_tag" in u && u._tag === "Ring"
