/**
 * The wire protocol between Cantrip.app (Swift) and this host: one JSON object
 * per line. The app writes to our stdin, and we write to stdout. Logs go to
 * stderr.
 *
 * Keep in sync with app/Sources/Cantrip/Protocol.swift.
 */
import * as Schema from "effect/Schema"

// ── App → Host ──────────────────────────────────────────────────────────────

export const Run = Schema.Struct({ type: Schema.tag("run"), session: Schema.String, command: Schema.String })
/** `depth` is the view's index in the session's stack (0 = the spell's first view). */
export const Query = Schema.Struct({
  type: Schema.tag("query"),
  session: Schema.String,
  depth: Schema.Int,
  text: Schema.String
})
export const InvokeAction = Schema.Struct({
  type: Schema.tag("action"),
  session: Schema.String,
  depth: Schema.Int,
  action: Schema.String
})
/** The user went back: the view at `depth` and everything above it is gone. */
export const Pop = Schema.Struct({ type: Schema.tag("pop"), session: Schema.String, depth: Schema.Int })
export const Close = Schema.Struct({ type: Schema.tag("close"), session: Schema.String })
export const Reload = Schema.Struct({ type: Schema.tag("reload") })
/** The user chose `item` (a `WireMenuItem.id`) in a daemon's menu, or from the bar's search. */
export const MenuSelect = Schema.Struct({ type: Schema.tag("menu"), daemon: Schema.String, item: Schema.String })
/** The user opened a daemon's menu. */
export const MenuOpened = Schema.Struct({ type: Schema.tag("menuOpened"), daemon: Schema.String })

export const Inbound = Schema.Union([Run, Query, InvokeAction, Pop, Close, Reload, MenuSelect, MenuOpened]).pipe(
  Schema.toTaggedUnion("type")
)
export type Inbound = typeof Inbound.Type

export const InboundLine = Schema.fromJsonString(Inbound)

// ── Host → App ──────────────────────────────────────────────────────────────

export interface WireCommand {
  readonly id: string
  readonly title: string
  readonly subtitle?: string | undefined
  readonly icon?: string | undefined
  readonly keywords: ReadonlyArray<string>
  readonly mode: "view" | "no-view"
}

export interface WireAction {
  readonly id: string
  readonly title: string
  readonly icon?: string | undefined
  /** `copy` and `open` are handled by the app using `value`; others call back here. */
  readonly kind: "callback" | "push" | "copy" | "open"
  readonly value?: string | undefined
}

export interface WireListItem {
  readonly id: string
  readonly title: string
  readonly subtitle?: string | undefined
  readonly icon?: string | undefined
  readonly accessories: ReadonlyArray<string>
  readonly keywords: ReadonlyArray<string>
  readonly actions: ReadonlyArray<WireAction>
}

export type WireView =
  | {
    readonly kind: "list"
    readonly placeholder?: string | undefined
    readonly filtering: boolean
    readonly emptyText?: string | undefined
    readonly items: ReadonlyArray<WireListItem>
  }
  | {
    readonly kind: "detail"
    readonly markdown: string
    readonly metadata: ReadonlyArray<{ readonly label: string; readonly value: string }>
    readonly actions: ReadonlyArray<WireAction>
  }

export type WireTone = "normal" | "warning" | "critical"

export type WireBarIcon =
  /** An `Icon` string: SF Symbol, emoji, or file path. */
  | { readonly kind: "image"; readonly value: string }
  /** `value` is 0–1, or null for "no data" (dashed). Swift animates between values. */
  | {
    readonly kind: "ring"
    readonly value: number | null
    readonly tone: WireTone
    readonly busy: boolean
    readonly pulse: boolean
    readonly badge: boolean
  }

export type WireMenuEntry =
  | { readonly kind: "separator" }
  | { readonly kind: "header"; readonly title: string }
  | {
    readonly kind: "item"
    /** Unique within the daemon's current menu; sent back in `menu`. */
    readonly id: string
    readonly title: string
    readonly subtitle?: string | undefined
    readonly icon?: string | undefined
    readonly accessory?: string | undefined
    /** 0–1. Makes the row a gauge (not clickable). */
    readonly progress?: number | undefined
    readonly tone: WireTone
    readonly checked: boolean
    readonly enabled: boolean
    readonly key?: string | undefined
    /** `callback` sends `menu` back; `copy` and `open` are handled by the app using `value`. */
    readonly action: "none" | "callback" | "copy" | "open"
    readonly value?: string | undefined
    readonly submenu: ReadonlyArray<WireMenuEntry>
  }

export interface WireStatusItem {
  /** The daemon's title and icon, for listing its menu actions in the bar's search. */
  readonly title: string
  readonly daemonIcon?: string | undefined
  readonly icon: WireBarIcon
  /** Text next to the icon. */
  readonly label?: string | undefined
  readonly tooltip?: string | undefined
  readonly menu: ReadonlyArray<WireMenuEntry>
}

export type Outbound =
  | { readonly type: "commands"; readonly commands: ReadonlyArray<WireCommand>; readonly errors: ReadonlyArray<string> }
  /**
   * Show `view` at `depth`: replace it if the app has a view there, append it
   * if `depth` is one past the top, otherwise ignore it (that view is gone).
   */
  | { readonly type: "render"; readonly session: string; readonly depth: number; readonly view: WireView }
  /** Remove the view at `depth` and everything above it. */
  | { readonly type: "pop"; readonly session: string; readonly depth: number }
  /** The spell closed itself; go back to the root search. */
  | { readonly type: "closed"; readonly session: string }
  | { readonly type: "loading"; readonly session: string; readonly loading: boolean }
  | {
    readonly type: "toast"
    readonly session: string
    readonly style: "success" | "failure" | "info"
    readonly title: string
    readonly message?: string | undefined
  }
  | { readonly type: "hud"; readonly message: string }
  | { readonly type: "hide" }
  | { readonly type: "clipboard"; readonly text: string }
  | { readonly type: "open"; readonly target: string }
  /** The session ended on its own. `rendered: false` means it never showed a view. */
  | { readonly type: "done"; readonly session: string; readonly rendered: boolean }
  | { readonly type: "error"; readonly session: string; readonly title: string; readonly message: string }
  /** Show or update `daemon`'s menu bar item. */
  | { readonly type: "status"; readonly daemon: string; readonly item: WireStatusItem }
  /** Take `daemon`'s menu bar item away. */
  | { readonly type: "statusRemoved"; readonly daemon: string }
