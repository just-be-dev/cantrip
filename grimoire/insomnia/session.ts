// The keep-awake session. Ported from Insomnia's SessionManager (src/session/application.ts).
// Domain only: talks to the Mac through the contracts in service.ts.
import * as Clock from "effect/Clock"
import * as Context from "effect/Context"
import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"
import * as Schedule from "effect/Schedule"
import * as Semaphore from "effect/Semaphore"
import type * as Stream from "effect/Stream"
import * as SubscriptionRef from "effect/SubscriptionRef"
import {
  BatteryTooLow,
  type EndReason,
  idle,
  isBatteryTooLow,
  isExpired,
  type Percent,
  type PowerCommandFailed,
  type PrivilegesMissing,
  type Session,
  type StateStoreError,
  type Status
} from "./model.ts"
import { BatteryMonitor, SleepControl, StateStore } from "./service.ts"

/** How often expiry is checked. Cheap: it only reads the clock. */
export const expiryCheckInterval = "1 second"
/** How often pmset is polled for battery, privileges, and outside changes to `disablesleep`. */
export const powerPollInterval = "30 seconds"

export type StartError = PrivilegesMissing | PowerCommandFailed | BatteryTooLow | StateStoreError
export type StopError = PrivilegesMissing | PowerCommandFailed | StateStoreError

/**
 * Owns the keep-awake session: starts and stops it, ends it on expiry or low battery,
 * notices outside changes, recovers it after a restart, and restores sleep on shutdown.
 *
 * The layer's scope is the session's lifetime: closing it restores sleep if a session is Active.
 */
export class SessionManager extends Context.Service<SessionManager, {
  readonly status: Effect.Effect<Status>
  /** Current status, then every change. */
  readonly changes: Stream.Stream<Status>
  /** Keep the Mac awake, lid closed included. `null` duration means until stopped. Replaces a running session. */
  readonly start: (durationMillis: number | null) => Effect.Effect<void, StartError>
  /** End the session and restore normal sleep. Also clears a `disablesleep` set by something else. */
  readonly stop: Effect.Effect<void, StopError>
  readonly setLowBatteryThreshold: (
    percent: Percent | null
  ) => Effect.Effect<void, StateStoreError | PrivilegesMissing | PowerCommandFailed>
  /** Re-read pmset now instead of waiting for the next poll. */
  readonly refresh: Effect.Effect<void>
}>()("insomnia/SessionManager") {
  static readonly layer = Layer.effect(
    SessionManager,
    Effect.gen(function*() {
      const sleep = yield* SleepControl
      const batteryMonitor = yield* BatteryMonitor
      const store = yield* StateStore
      const lock = yield* Semaphore.make(1)

      /** `null` when pmset couldn't be read, so a read failure never looks like an outside change. */
      const readSleepDisabled = sleep.isDisabled.pipe(
        Effect.tapError((error) => Effect.logWarning("Couldn't read SleepDisabled", error)),
        Effect.orElseSucceed((): boolean | null => null)
      )

      const persisted = yield* store.load
      const ref = yield* SubscriptionRef.make<Status>({
        session: persisted.session,
        lastEnded: persisted.lastEnded,
        settings: persisted.settings,
        sleepDisabled: (yield* readSleepDisabled) ?? false,
        battery: yield* batteryMonitor.read,
        privileges: yield* sleep.privileges
      })

      /** Writes pmset, and records missing permissions as soon as sudo asks for a password. */
      const setSleepDisabled = (disabled: boolean) =>
        sleep.setDisabled(disabled).pipe(
          Effect.tapError((error) =>
            error._tag === "PrivilegesMissing"
              ? SubscriptionRef.update(ref, (s) => ({ ...s, privileges: "Missing" as const }))
              : Effect.void
          )
        )

      const save = (status: Status) =>
        store.save({ settings: status.settings, session: status.session, lastEnded: status.lastEnded })

      /** Restore sleep, then record the session as ended. Leaves the session running if pmset fails. */
      const endSession = Effect.fn("SessionManager.endSession")(function*(reason: EndReason, at?: number) {
        const current = yield* SubscriptionRef.get(ref)
        yield* setSleepDisabled(false)
        const now = yield* Clock.currentTimeMillis
        const next: Status = {
          ...current,
          session: idle,
          lastEnded: current.session._tag === "Active" ? { reason, at: at ?? now } : current.lastEnded,
          sleepDisabled: false
        }
        yield* SubscriptionRef.set(ref, next)
        yield* save(next)
        if (current.session._tag === "Active") yield* Effect.logInfo(`Session ended: ${reason}`)
      })

      /** Ends the session for a background reason, logging failures so the next check retries. */
      const endInBackground = (reason: EndReason, at?: number) =>
        endSession(reason, at).pipe(
          Effect.tapError((error) => Effect.logError(`Couldn't end session (${reason}); will retry`, error)),
          Effect.ignore
        )

      const checkExpiry = lock.withPermit(
        Effect.gen(function*() {
          const { session } = yield* SubscriptionRef.get(ref)
          const now = yield* Clock.currentTimeMillis
          if (session._tag === "Active" && isExpired(session, now)) {
            yield* endInBackground("Expired", session.endsAt ?? now)
          }
        })
      )

      const poll = lock.withPermit(
        Effect.gen(function*() {
          const sleepDisabled = yield* readSleepDisabled
          const battery = yield* batteryMonitor.read
          const privileges = yield* sleep.privileges
          const current = yield* SubscriptionRef.updateAndGet(ref, (status) => ({
            ...status,
            sleepDisabled: sleepDisabled ?? status.sleepDisabled,
            battery,
            privileges
          }))
          if (current.session._tag !== "Active") return
          if (isBatteryTooLow(battery, current.settings)) {
            yield* endInBackground("LowBattery")
          } else if (sleepDisabled === false) {
            // Someone ran `pmset -a disablesleep 0` (or Amphetamine ended its own session).
            const now = yield* Clock.currentTimeMillis
            const next: Status = { ...current, session: idle, lastEnded: { reason: "External", at: now } }
            yield* SubscriptionRef.set(ref, next)
            yield* save(next).pipe(Effect.ignore)
            yield* Effect.logWarning("disablesleep was turned off outside Insomnia; session ended")
          }
        })
      )

      // Recover a session that survived a hard kill, a crash, or a reload.
      yield* lock.withPermit(
        Effect.gen(function*() {
          const current = yield* SubscriptionRef.get(ref)
          if (current.session._tag !== "Active") return
          const now = yield* Clock.currentTimeMillis
          if (isExpired(current.session, now)) {
            yield* endInBackground("Expired", current.session.endsAt ?? now)
          } else if (isBatteryTooLow(current.battery, current.settings)) {
            yield* endInBackground("LowBattery")
          } else {
            yield* Effect.logInfo("Resuming session from before restart")
            yield* setSleepDisabled(true).pipe(
              Effect.andThen(SubscriptionRef.update(ref, (s) => ({ ...s, sleepDisabled: true }))),
              Effect.tapError((error) => Effect.logError("Couldn't resume session", error)),
              Effect.ignore
            )
          }
        })
      )

      // Registered before the loops start so it runs after they're interrupted.
      // Only restores what Insomnia turned on: an Idle session leaves an outside `disablesleep` alone.
      yield* Effect.addFinalizer(() =>
        lock.withPermit(
          Effect.gen(function*() {
            const { session } = yield* SubscriptionRef.get(ref)
            if (session._tag !== "Active") return
            yield* endSession("Shutdown").pipe(
              Effect.tapError((error) => Effect.logError("Couldn't restore sleep on shutdown", error)),
              Effect.ignore
            )
          })
        )
      )

      yield* Effect.forkScoped(checkExpiry.pipe(Effect.repeat(Schedule.spaced(expiryCheckInterval))))
      yield* Effect.forkScoped(poll.pipe(Effect.repeat(Schedule.spaced(powerPollInterval))))

      const start = Effect.fn("SessionManager.start")(function*(durationMillis: number | null) {
        // Uninterruptible: if the daemon stops mid-start, the shutdown finalizer must
        // see either the finished Active session (and restore sleep) or no session at all.
        yield* lock.withPermit(
          Effect.uninterruptible(Effect.gen(function*() {
            const previous = yield* SubscriptionRef.get(ref)
            const battery = yield* batteryMonitor.read
            const { lowBatteryThreshold } = previous.settings
            if (battery !== null && lowBatteryThreshold !== null && isBatteryTooLow(battery, previous.settings)) {
              yield* SubscriptionRef.set(ref, { ...previous, battery })
              return yield* new BatteryTooLow({ percent: battery.percent, threshold: lowBatteryThreshold })
            }
            const now = yield* Clock.currentTimeMillis
            const session: Session = {
              _tag: "Active",
              startedAt: now,
              endsAt: durationMillis === null ? null : now + Math.max(0, Math.round(durationMillis))
            }
            const next: Status = { ...previous, session, battery, sleepDisabled: true }
            // Persist first so a crash between these steps is recovered (and cleaned up) on restart.
            yield* save(next)
            yield* setSleepDisabled(true).pipe(Effect.tapError(() => save(previous).pipe(Effect.ignore)))
            yield* SubscriptionRef.set(ref, next)
            yield* Effect.logInfo(
              durationMillis === null
                ? "Session started (until stopped)"
                : `Session started for ${Math.round(durationMillis / 60_000)}m`
            )
          }))
        )
      })

      const stop = lock.withPermit(endSession("Stopped")).pipe(Effect.withSpan("SessionManager.stop"))

      const setLowBatteryThreshold = Effect.fn("SessionManager.setLowBatteryThreshold")(function*(
        percent: Percent | null
      ) {
        yield* lock.withPermit(
          Effect.gen(function*() {
            const next = yield* SubscriptionRef.updateAndGet(ref, (status) => ({
              ...status,
              settings: { ...status.settings, lowBatteryThreshold: percent }
            }))
            yield* save(next)
            if (next.session._tag === "Active" && isBatteryTooLow(next.battery, next.settings)) {
              yield* endSession("LowBattery")
            }
          })
        )
      })

      return {
        status: SubscriptionRef.get(ref),
        changes: SubscriptionRef.changes(ref),
        start,
        stop,
        setLowBatteryThreshold,
        refresh: poll
      }
    })
  )
}
