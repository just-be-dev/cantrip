// The menu bar item, as data. A port of Insomnia.app's StatusMenu.swift; pure apart from the
// `onSelect` effects it is handed.
import type * as Effect from "effect/Effect"
import { type DaemonServices, MenuBar } from "cantrip"
import type { EndReason, Percent, Status } from "./model.ts"

/** A failed action, shown as a row at the top of the menu until it's dismissed. */
export interface Notice {
  /** The error's `_tag`. */
  readonly kind: string
  readonly message: string
}

type Run = Effect.Effect<unknown, never, DaemonServices>

export interface Actions {
  readonly start: (seconds: number | null, label: string) => Run
  readonly stop: Run
  readonly setThreshold: (percent: Percent | null) => Run
  readonly install: Run
}

export const durations: ReadonlyArray<readonly [label: string, seconds: number | null]> = [
  ["Until Stopped", null],
  ["15 Minutes", 15 * 60],
  ["30 Minutes", 30 * 60],
  ["1 Hour", 60 * 60],
  ["2 Hours", 2 * 60 * 60],
  ["4 Hours", 4 * 60 * 60],
  ["8 Hours", 8 * 60 * 60]
]

export const thresholds: ReadonlyArray<Percent | null> = [null, 10, 20, 30, 50]

export const icons = {
  active: "cup.and.heat.waves.fill",
  idle: "moon.zzz.fill",
  warning: "exclamationmark.triangle"
} as const

// Formatting

const remainingSeconds = (endsAt: number, now: number) => Math.max(0, Math.ceil((endsAt - now) / 1000))
const remainingMinutes = (endsAt: number, now: number) => Math.ceil(remainingSeconds(endsAt, now) / 60)

/** Menu bar title: "1h 12m", "42m", "<1m". */
export const shortRemaining = (endsAt: number, now: number): string => {
  const minutes = remainingMinutes(endsAt, now)
  if (minutes < 1) return "<1m"
  return minutes >= 60 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${minutes}m`
}

/** "1 hr 12 min", "2 hr", "42 min", "less than a minute". */
export const longRemaining = (endsAt: number, now: number): string => {
  const minutes = remainingMinutes(endsAt, now)
  if (minutes >= 60) {
    const h = Math.floor(minutes / 60)
    const m = minutes % 60
    return m === 0 ? `${h} hr` : `${h} hr ${m} min`
  }
  return minutes < 1 ? "less than a minute" : `${minutes} min`
}

/** Short local time, e.g. "3:45 PM". */
export const clock = (epochMillis: number): string =>
  new Date(epochMillis).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })

export const describeReason = (reason: EndReason): string => {
  switch (reason) {
    case "Expired":
      return "ended on schedule"
    case "LowBattery":
      return "stopped for low battery"
    case "External":
      return "ended by another app"
    case "Shutdown":
      return "ended when Insomnia quit"
    case "Stopped":
      return "stopped"
  }
}

export const errorTitle = (kind: string): string => {
  switch (kind) {
    case "BatteryTooLow":
      return "Battery too low to keep awake"
    case "PrivilegesMissing":
      return "Insomnia needs permission"
    case "PrivilegeInstallFailed":
      return "Couldn't install permissions"
    case "PowerCommandFailed":
      return "Couldn't change sleep settings"
    case "StateStoreError":
      return "Couldn't save Insomnia's state"
    default:
      return "Insomnia ran into a problem"
  }
}

/** The first line of the menu, also used as the tooltip. */
export const statusLine = (status: Status, now: number): string => {
  const { session } = status
  if (session._tag === "Active") {
    return session.endsAt === null
      ? "Awake until stopped"
      : `Awake · ${longRemaining(session.endsAt, now)} left (until ${clock(session.endsAt)})`
  }
  return status.sleepDisabled ? "Sleep is disabled outside Insomnia" : "Sleeping normally"
}

// The item

export const statusItem = (
  status: Status,
  notice: Notice | null,
  now: number,
  actions: Actions
): MenuBar.StatusItem => {
  const active = status.session._tag === "Active"
  const granted = status.privileges === "Granted"
  const outside = !active && status.sleepDisabled

  const icon = notice !== null || outside ? icons.warning : active ? icons.active : icons.idle
  const title = status.session._tag === "Active" && status.session.endsAt !== null
    ? shortRemaining(status.session.endsAt, now)
    : undefined

  const menu: Array<MenuBar.Entry> = []

  if (notice !== null) {
    menu.push(
      MenuBar.item(errorTitle(notice.kind), { subtitle: notice.message, icon: icons.warning, tone: "warning" }),
      MenuBar.separator
    )
  }

  menu.push(MenuBar.item(statusLine(status, now), outside ? { tone: "warning" } : undefined))
  if (status.battery !== null) {
    const { percent, onAC } = status.battery
    menu.push(MenuBar.item(`Battery ${percent}% · ${onAC ? "on power adapter" : "on battery"}`))
  }
  if (!active && status.lastEnded !== null) {
    menu.push(
      MenuBar.item(`Last session ${describeReason(status.lastEnded.reason)} at ${clock(status.lastEnded.at)}`)
    )
  }

  menu.push(MenuBar.separator)

  // The title stays "Keep Awake" while a session runs (the app said "Keep Awake For") so the
  // search bar's "Keep Awake › 1 Hour" entries don't change name.
  menu.push(
    MenuBar.item("Keep Awake", {
      disabled: !granted,
      submenu: durations.map(([label, seconds]) =>
        MenuBar.item(label, { disabled: !granted, onSelect: actions.start(seconds, label) })
      )
    })
  )
  if (active) {
    menu.push(MenuBar.item("Stop Keeping Awake", { key: "s", onSelect: actions.stop }))
  } else if (status.sleepDisabled) {
    menu.push(MenuBar.item("Restore Normal Sleep", { onSelect: actions.stop }))
  }

  menu.push(MenuBar.separator)

  menu.push(
    MenuBar.item("Low Battery Stop", {
      submenu: thresholds.map((threshold) =>
        MenuBar.item(threshold === null ? "Off" : `At ${threshold}%`, {
          checked: threshold === status.settings.lowBatteryThreshold,
          onSelect: actions.setThreshold(threshold)
        })
      )
    })
  )
  if (!granted) {
    menu.push(MenuBar.item("Install Permissions…", { onSelect: actions.install }))
  }

  return { icon, title, tooltip: `Insomnia: ${statusLine(status, now)}`, menu }
}
