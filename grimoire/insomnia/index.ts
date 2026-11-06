// Insomnia: keeps the Mac awake, lid closed too, with `pmset -a disablesleep`.
// A port of ~/Code/insomnia's menu bar app to a Cantrip daemon.
//
// Needs a one-time sudoers rule (Install Permissions… in the menu):
//   <you> ALL=(root) NOPASSWD: /usr/bin/pmset -a disablesleep 0, /usr/bin/pmset -a disablesleep 1
//
// Fails safe: when the daemon stops (Cantrip quits or crashes, Restart Host, or this
// directory is edited) the session finalizer restores normal sleep. A session saved in
// state.json resumes with its original deadline the next time Insomnia starts.
import * as Clock from "effect/Clock"
import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"
import * as Stream from "effect/Stream"
import * as SubscriptionRef from "effect/SubscriptionRef"
import { Daemon, MenuBar } from "cantrip"
import { insomniaCommand } from "./cli.ts"
import { type Actions, errorTitle, icons, type Notice, statusItem } from "./menu.ts"
import { batteryMonitorLayer, sleepControlLayer } from "./pmset.ts"
import { privilegeInstallerLayer } from "./privileges.ts"
import { PrivilegeInstaller } from "./service.ts"
import { SessionManager } from "./session.ts"
import { fileStateStoreLayer } from "./store.ts"

/** How often the menu bar title ("1h 12m") is refreshed. */
const renderInterval = "15 seconds"

const macosLayer = Layer.mergeAll(sleepControlLayer, batteryMonitorLayer, fileStateStoreLayer, privilegeInstallerLayer)

/** The session's lifetime is the daemon's scope, so its finalizer runs when the daemon stops. */
const insomniaLayer = SessionManager.layer.pipe(Layer.provideMerge(macosLayer))

interface ShownNotice {
  readonly notice: Notice
  /** The menu has been opened since the notice appeared. The next open clears it. */
  readonly seen: boolean
}

const program = Effect.gen(function*() {
  const manager = yield* SessionManager
  const installer = yield* PrivilegeInstaller
  const notice = yield* SubscriptionRef.make<ShownNotice | null>(null)

  /** Where Insomnia.app raised an NSAlert: a HUD now, and a row in the menu. */
  const report = (error: { readonly _tag: string; readonly message: string }) =>
    Effect.gen(function*() {
      yield* Effect.logWarning(`Insomnia: ${error.message}`)
      yield* SubscriptionRef.set(notice, { notice: { kind: error._tag, message: error.message }, seen: false })
      yield* MenuBar.hud(errorTitle(error._tag))
    })

  /** Runs a menu action. Failures are reported, never thrown at the host. */
  const act = <E extends { readonly _tag: string; readonly message: string }>(
    name: string,
    effect: Effect.Effect<void, E>,
    done?: string
  ) =>
    SubscriptionRef.set(notice, null).pipe(
      Effect.andThen(effect),
      Effect.andThen(done === undefined ? Effect.void : MenuBar.hud(done)),
      Effect.catch(report),
      Effect.withSpan(`Insomnia.${name}`)
    )

  const actions: Actions = {
    start: (seconds, label) =>
      act(
        "start",
        manager.start(seconds === null ? null : seconds * 1000),
        seconds === null ? "Keeping awake until stopped" : `Keeping awake for ${label.toLowerCase()}`
      ),
    stop: act("stop", manager.stop, "Normal sleep restored"),
    setThreshold: (percent) => act("setLowBatteryThreshold", manager.setLowBatteryThreshold(percent)),
    install: act("installPrivileges", installer.install.pipe(Effect.andThen(manager.refresh)), "Permissions installed")
  }

  const render = Effect.gen(function*() {
    const status = yield* manager.status
    const shown = yield* SubscriptionRef.get(notice)
    const now = yield* Clock.currentTimeMillis
    yield* MenuBar.render(statusItem(status, shown?.notice ?? null, now, actions))
  })

  // One consumer, so renders never interleave: on every status or notice change, and on a
  // timer for the remaining-time title.
  yield* Stream.mergeAll(
    [
      manager.changes.pipe(Stream.as(undefined)),
      SubscriptionRef.changes(notice).pipe(Stream.as(undefined)),
      Stream.tick(renderInterval)
    ],
    { concurrency: "unbounded" }
  ).pipe(Stream.runForEach(() => render), Effect.forkScoped)

  // Opening the menu re-reads pmset (the app's `Refresh`). A notice stays for one look.
  yield* MenuBar.opened.pipe(
    Stream.runForEach(() =>
      SubscriptionRef.update(notice, (shown) => (shown === null || shown.seen ? null : { ...shown, seen: true })).pipe(
        Effect.andThen(manager.refresh)
      )
    ),
    Effect.forkScoped
  )

  return yield* Effect.never
})

export default Daemon.summon({
  title: "Insomnia",
  icon: icons.idle,
  // Built once in the daemon's scope and shared by the menu and the `insomnia` command.
  layer: insomniaLayer,
  run: program,
  command: insomniaCommand
})
