// Domain: schemas, invariants, and errors. No platform imports (no Bun, node:*, or process code).
import * as Schema from "effect/Schema"

/** Whole-number percentage, 0–100. */
export const Percent = Schema.Int.check(Schema.isBetween({ minimum: 0, maximum: 100 }))
export type Percent = typeof Percent.Type

/** Epoch milliseconds. */
export const EpochMillis = Schema.Int.check(Schema.isGreaterThanOrEqualTo(0))

export const Settings = Schema.Struct({
  /** End the session when on battery at or below this level. `null` turns the safety stop off. */
  lowBatteryThreshold: Schema.NullOr(Percent)
})
export type Settings = typeof Settings.Type

export const defaultSettings: Settings = { lowBatteryThreshold: 20 }

export const Idle = Schema.TaggedStruct("Idle", {})
export const Active = Schema.TaggedStruct("Active", {
  startedAt: EpochMillis,
  /** `null` keeps the Mac awake until stopped. */
  endsAt: Schema.NullOr(EpochMillis)
})
export const Session = Schema.Union([Idle, Active])
export type Session = typeof Session.Type
export type ActiveSession = typeof Active.Type

export const idle: Session = { _tag: "Idle" }

export const EndReason = Schema.Literals([
  "Stopped",
  "Expired",
  "LowBattery",
  /** Something else turned `disablesleep` off while a session was running. */
  "External",
  /** The daemon stopped (Cantrip quit, Restart Host, or this file was edited). */
  "Shutdown"
])
export type EndReason = typeof EndReason.Type

export const Ended = Schema.Struct({ reason: EndReason, at: EpochMillis })
export type Ended = typeof Ended.Type

export const Battery = Schema.Struct({
  percent: Percent,
  onAC: Schema.Boolean
})
export type Battery = typeof Battery.Type

export const Privileges = Schema.Literals(["Granted", "Missing"])
export type Privileges = typeof Privileges.Type

/** Everything the menu needs to render Insomnia's state. */
export const Status = Schema.Struct({
  session: Session,
  lastEnded: Schema.NullOr(Ended),
  settings: Settings,
  /** Last observed `SleepDisabled` from pmset. Can be true while Idle if something else set it. */
  sleepDisabled: Schema.Boolean,
  /** `null` on Macs without an internal battery or when it couldn't be read. */
  battery: Schema.NullOr(Battery),
  privileges: Privileges
})
export type Status = typeof Status.Type

/** What survives a restart. Same schema as Insomnia.app's `state.json`. */
export const PersistedState = Schema.Struct({
  settings: Settings,
  session: Session,
  lastEnded: Schema.NullOr(Ended)
})
export type PersistedState = typeof PersistedState.Type

export const initialPersistedState: PersistedState = {
  settings: defaultSettings,
  session: idle,
  lastEnded: null
}

/** True when the low-battery safety stop should end (or refuse) a session. */
export const isBatteryTooLow = (battery: Battery | null, settings: Settings): boolean =>
  battery !== null &&
  !battery.onAC &&
  settings.lowBatteryThreshold !== null &&
  battery.percent <= settings.lowBatteryThreshold

export const isExpired = (session: Session, now: number): boolean =>
  session._tag === "Active" && session.endsAt !== null && session.endsAt <= now

// Errors

export class PrivilegesMissing extends Schema.TaggedError<PrivilegesMissing>()("PrivilegesMissing", {}) {
  override get message() {
    return "Insomnia can't change pmset without a password. Choose Install Permissions… in the Insomnia menu."
  }
}

export class PowerCommandFailed extends Schema.TaggedError<PowerCommandFailed>()("PowerCommandFailed", {
  command: Schema.String,
  detail: Schema.String
}) {
  override get message() {
    return `${this.command} failed: ${this.detail}`
  }
}

export class BatteryTooLow extends Schema.TaggedError<BatteryTooLow>()("BatteryTooLow", {
  percent: Percent,
  threshold: Percent
}) {
  override get message() {
    return `Battery is at ${this.percent}%, at or below the ${this.threshold}% safety stop. Plug in or lower the threshold.`
  }
}

export class StateStoreError extends Schema.TaggedError<StateStoreError>()("StateStoreError", {
  detail: Schema.String
}) {
  override get message() {
    return `Couldn't save Insomnia state: ${this.detail}`
  }
}

export class PrivilegeInstallFailed extends Schema.TaggedError<PrivilegeInstallFailed>()("PrivilegeInstallFailed", {
  detail: Schema.String
}) {
  override get message() {
    return `Couldn't install permissions: ${this.detail}`
  }
}
