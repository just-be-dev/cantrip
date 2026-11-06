/**
 * Declarative views a spell can show. Swift draws these natively; actions
 * that need code (`Action.run`, `Action.push`) call back into the spell.
 */
import type * as Effect from "effect/Effect"
import type { SpellServices } from "./Services.ts"

/**
 * An SF Symbol name (`"doc.on.clipboard"`), a single emoji (`"🔮"`),
 * or an absolute file path (shows that file's Finder icon).
 */
export type Icon = string

export type Action =
  | {
    readonly _tag: "Run"
    readonly title: string
    readonly icon?: Icon | undefined
    readonly effect: Effect.Effect<unknown, unknown, SpellServices>
  }
  | {
    readonly _tag: "Copy"
    readonly title: string
    readonly icon?: Icon | undefined
    readonly text: string
  }
  | {
    readonly _tag: "Open"
    readonly title: string
    readonly icon?: Icon | undefined
    readonly target: string
  }
  | {
    readonly _tag: "Push"
    readonly title: string
    readonly icon?: Icon | undefined
    /** Runs with a `Ui` bound to the new view, so it can render and react like a spell's `run`. */
    readonly program: Effect.Effect<unknown, unknown, SpellServices>
  }

export interface ListItem {
  /** Stable id. Defaults to the item's position. */
  readonly id?: string | undefined
  readonly title: string
  readonly subtitle?: string | undefined
  readonly icon?: Icon | undefined
  /** Short labels shown on the right edge of the row. */
  readonly accessories?: ReadonlyArray<string> | undefined
  /** Extra words that native filtering matches on. */
  readonly keywords?: ReadonlyArray<string> | undefined
  /** The first action runs on ↵, the second on ⌘↵, all of them under ⌘K. */
  readonly actions?: ReadonlyArray<Action> | undefined
}

export interface List {
  readonly _tag: "List"
  readonly items: ReadonlyArray<ListItem>
  readonly placeholder?: string | undefined
  /**
   * `true` (default): Swift fuzzy-filters `items` as the user types.
   * `false`: the spell owns filtering and reads `Ui.query` itself.
   */
  readonly filtering?: boolean | undefined
  readonly emptyText?: string | undefined
}

export interface Detail {
  readonly _tag: "Detail"
  readonly markdown: string
  readonly metadata?: ReadonlyArray<{ readonly label: string; readonly value: string }> | undefined
  readonly actions?: ReadonlyArray<Action> | undefined
}

export type View = List | Detail

export const list = (options: Omit<List, "_tag">): List => ({ _tag: "List", ...options })

export const detail = (options: Omit<Detail, "_tag">): Detail => ({ _tag: "Detail", ...options })

export const isView = (u: unknown): u is View =>
  typeof u === "object" && u !== null && "_tag" in u && (u._tag === "List" || u._tag === "Detail")
