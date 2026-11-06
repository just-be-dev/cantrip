// The `insomnia` command. It runs inside the Insomnia daemon (see index.ts), so
// `insomnia start 1h` drives the same session as the menu: one SessionManager,
// never a second one next to it.
import * as Argument from "effect/cli/Argument"
import * as Command from "effect/cli/Command"
import * as Clock from "effect/Clock"
import * as Console from "effect/Console"
import * as Effect from "effect/Effect"
import * as Option from "effect/Option"
import { MenuBar } from "cantrip"
import { clock, describeReason, longRemaining, statusLine } from "./menu.ts"
import type { Percent, Status } from "./model.ts"
import { SUDOERS_PATH } from "./privileges.ts"
import { PrivilegeInstaller } from "./service.ts"
import { SessionManager } from "./session.ts"

/** "1h", "30m", "1h30m", "90" (minutes). `null` means until stopped. */
export const parseDuration = (input: string): number | null | undefined => {
  const text = input.trim().toLowerCase()
  if (text === "" || text === "forever" || text === "until-stopped") return null
  if (/^\d+$/.test(text)) return Number(text) * 60
  const match = /^(?:(\d+)h)?(?:(\d+)m)?$/.exec(text)
  if (!match || (match[1] === undefined && match[2] === undefined)) return undefined
  const seconds = Number(match[1] ?? 0) * 3600 + Number(match[2] ?? 0) * 60
  return seconds > 0 ? seconds : undefined
}

const describeDuration = (seconds: number) => {
  const h = Math.floor(seconds / 3600)
  const m = Math.round((seconds % 3600) / 60)
  return [h > 0 ? `${h} hr` : "", m > 0 ? `${m} min` : ""].filter(Boolean).join(" ")
}

const describeSession = ({ session }: Status, now: number): string => {
  if (session._tag === "Idle") return "idle"
  if (session.endsAt === null) return "active (until stopped)"
  return `active, ${longRemaining(session.endsAt, now)} left (until ${clock(session.endsAt)})`
}

export const statusText = (status: Status, now: number): string => {
  const { session, battery, settings } = status
  const sessionText = describeSession(status, now)
  const lines = [
    statusLine(status, now),
    "",
    `sleep:        ${status.sleepDisabled ? "DISABLED (lid close won't sleep)" : "normal"}`,
    `session:      ${sessionText}`,
    `battery:      ${battery === null ? "n/a" : `${battery.percent}%${battery.onAC ? " (AC)" : ""}`}`,
    `safety stop:  ${settings.lowBatteryThreshold === null ? "off" : `${settings.lowBatteryThreshold}%`}`,
    `permissions:  ${status.privileges === "Granted" ? "installed" : "missing (run `insomnia install`)"}`
  ]
  if (session._tag === "Idle" && status.lastEnded) {
    lines.push(`last session: ${describeReason(status.lastEnded.reason)} at ${clock(status.lastEnded.at)}`)
  }
  return lines.join("\n")
}

const printStatus = Effect.gen(function*() {
  const manager = yield* SessionManager
  yield* manager.refresh
  yield* Console.log(statusText(yield* manager.status, yield* Clock.currentTimeMillis))
})

const status = Command.make("status", {}, () => printStatus).pipe(
  Command.withDescription("Show sleep, session, battery, and permission state")
)

const start = Command.make(
  "start",
  {
    duration: Argument.String("duration").pipe(
      Argument.withDescription("How long: 30m, 1h, 1h30m, or minutes (default: until stopped)"),
      Argument.optional
    )
  },
  ({ duration }) =>
    Effect.gen(function*() {
      const text = Option.getOrElse(duration, () => "")
      const seconds = parseDuration(text)
      if (seconds === undefined) {
        return yield* Effect.fail(new Error(`Can't read "${text}" as a duration. Try 30m, 1h, or 1h30m.`))
      }
      yield* (yield* SessionManager).start(seconds === null ? null : seconds * 1000)
      const message = seconds === null ? "Keeping awake until stopped" : `Keeping awake for ${describeDuration(seconds)}`
      yield* MenuBar.hud(message)
      yield* Console.log(message)
    })
).pipe(Command.withDescription("Keep the Mac awake, lid closed too"))

const stopHandler = Effect.gen(function*() {
  yield* (yield* SessionManager).stop
  yield* MenuBar.hud("Normal sleep restored")
  yield* Console.log("Normal sleep restored")
})

const stop = Command.make("stop", {}, () => stopHandler).pipe(
  Command.withDescription("End the session and restore normal sleep")
)

const restore = Command.make("restore", {}, () => stopHandler).pipe(
  Command.withDescription("Restore normal sleep, even if something else disabled it")
)

const threshold = Command.make(
  "threshold",
  {
    percent: Argument.String("percent").pipe(Argument.withDescription("Battery percent (0–100), or \"off\""))
  },
  ({ percent }) =>
    Effect.gen(function*() {
      const value = percent === "off" ? null : Number(percent.replace(/%$/, ""))
      if (value !== null && (!Number.isInteger(value) || value < 0 || value > 100)) {
        return yield* Effect.fail(new Error(`"${percent}" isn't a percent (0–100) or "off"`))
      }
      yield* (yield* SessionManager).setLowBatteryThreshold(value as Percent | null)
      yield* Console.log(value === null ? "Low Battery Stop is off" : `Low Battery Stop at ${value}%`)
    })
).pipe(Command.withDescription("On battery, end the session at or below this level"))

const install = Command.make("install", {}, () =>
  Effect.gen(function*() {
    yield* (yield* PrivilegeInstaller).install
    yield* (yield* SessionManager).refresh
    yield* Console.log(`Installed ${SUDOERS_PATH}`)
  })).pipe(
    Command.withDescription("Allow passwordless `pmset -a disablesleep 0|1` (asks for an admin password once)")
  )

export const insomniaCommand = Command.make("insomnia", {}, () => printStatus).pipe(
  Command.withDescription("Keep your Mac awake with the lid closed, safely. With no subcommand, shows status."),
  Command.withSubcommands([status, start, stop, restore, threshold, install])
)
