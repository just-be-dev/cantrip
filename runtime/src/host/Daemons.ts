/**
 * Runs daemons: long-lived spells that own a menu bar item. Each one runs in
 * its own Scope, a child of the host's. The scope closes when the host shuts
 * down or when a reload replaces or removes the daemon; a daemon whose
 * file didn't change keeps running.
 */
import * as Cause from "effect/Cause"
import * as Context from "effect/Context"
import * as Effect from "effect/Effect"
import * as Exit from "effect/Exit"
import * as FileSystem from "effect/FileSystem"
import * as HttpClient from "effect/http/HttpClient"
import * as Layer from "effect/Layer"
import * as Path from "effect/Path"
import * as ChildProcessSpawner from "effect/process/ChildProcessSpawner"
import * as PubSub from "effect/PubSub"
import * as Scope from "effect/Scope"
import * as Semaphore from "effect/Semaphore"
import * as Stream from "effect/Stream"
import * as SubscriptionRef from "effect/SubscriptionRef"
import * as MenuBar from "../sdk/MenuBar.ts"
import type { DaemonServices } from "../sdk/Services.ts"
import type { Inbound, WireBarIcon, WireMenuEntry, WireStatusItem } from "./Protocol.ts"
import { type LoadedDaemon, Registry } from "./Registry.ts"
import { Wire } from "./Wire.ts"

type Handler = Effect.Effect<unknown, unknown, DaemonServices>

interface Running {
  readonly loaded: LoadedDaemon
  readonly scope: Scope.Closeable
  readonly opened: PubSub.PubSub<void>
  handlers: ReadonlyMap<string, Handler>
  shown: boolean
}

const logPath = `${process.env.HOME}/Library/Logs/Cantrip/host.log`

const clamp = (n: number) => Math.min(1, Math.max(0, n))

const ringIcon = (ring: MenuBar.Ring): WireBarIcon => ({
  kind: "ring",
  value: ring.value === null ? null : clamp(ring.value),
  tone: ring.tone ?? "normal",
  busy: ring.busy ?? false,
  pulse: ring.pulse ?? false,
  badge: ring.badge ?? false
})

const actionOf = (entry: MenuBar.Item): "none" | "callback" | "copy" | "open" => {
  if (entry.onSelect) return "callback"
  if (entry.copy !== undefined) return "copy"
  if (entry.open !== undefined) return "open"
  return "none"
}

const actionValue = (entry: MenuBar.Item, action: ReturnType<typeof actionOf>) => {
  if (action === "copy") return entry.copy
  if (action === "open") return entry.open
  return undefined
}

/** The handler of a failed daemon's "Restart" item. Recognized by identity; the host restarts it. */
const restartMarker: Handler = Effect.sync(() => {})

const encode = (
  title: string,
  fallbackIcon: string,
  item: MenuBar.StatusItem
): { readonly wire: WireStatusItem; readonly handlers: ReadonlyMap<string, Handler> } => {
  const handlers = new Map<string, Handler>()
  const entries = (prefix: string, list: ReadonlyArray<MenuBar.Entry>): Array<WireMenuEntry> =>
    list.map((entry, i): WireMenuEntry => {
      switch (entry._tag) {
        case "Separator":
          return { kind: "separator" }
        case "Header":
          return { kind: "header", title: entry.title }
        default: {
          const id = `${prefix}${i}`
          const gauge = entry.progress !== undefined
          const action = gauge ? "none" : actionOf(entry)
          if (action === "callback" && entry.onSelect) handlers.set(id, entry.onSelect)
          const submenu = entries(`${id}.`, entry.submenu ?? [])
          return {
            kind: "item",
            id,
            title: entry.title,
            subtitle: entry.subtitle,
            icon: entry.icon,
            accessory: entry.accessory,
            progress: gauge ? clamp(entry.progress ?? 0) : undefined,
            tone: entry.tone ?? "normal",
            checked: entry.checked ?? false,
            enabled: !(entry.disabled ?? (action === "none" && submenu.length === 0)),
            key: entry.key,
            action,
            value: actionValue(entry, action),
            submenu
          }
        }
      }
    })
  const icon = item.icon ?? fallbackIcon
  return {
    handlers,
    wire: {
      title,
      daemonIcon: fallbackIcon,
      icon: MenuBar.isRing(icon) ? ringIcon(icon) : { kind: "image", value: icon },
      label: item.title,
      tooltip: item.tooltip,
      menu: entries("", item.menu)
    }
  }
}

export class Daemons extends Context.Service<Daemons, {
  readonly handle: (message: Extract<Inbound, { readonly type: "menu" | "menuOpened" }>) => Effect.Effect<void>
}>()("cantrip/host/Daemons") {
  static readonly layer = Layer.effect(
    Daemons,
    Effect.gen(function*() {
      const wire = yield* Wire
      const registry = yield* Registry
      const hostScope = yield* Effect.scope
      const platform = yield* Effect.context<
        HttpClient.HttpClient | FileSystem.FileSystem | Path.Path | ChildProcessSpawner.ChildProcessSpawner
      >()
      const running = new Map<string, Running>()
      const live = (r: Running) => running.get(r.loaded.id) === r

      const menuBarFor = (r: Running): MenuBar.MenuBar["Service"] => {
        const { id, daemon } = r.loaded
        return MenuBar.MenuBar.of({
          render: (item) =>
            Effect.suspend(() => {
              if (!live(r)) return Effect.void
              const encoded = encode(daemon.title, daemon.icon ?? "sparkle", item)
              r.handlers = encoded.handlers
              r.shown = true
              return wire.send({ type: "status", daemon: id, item: encoded.wire })
            }),
          remove: Effect.suspend(() => {
            if (!live(r)) return Effect.void
            r.handlers = new Map()
            return wire.send({ type: "statusRemoved", daemon: id })
          }),
          opened: Stream.fromPubSub(r.opened),
          hud: (message) => wire.send({ type: "hud", message }),
          open: (target) => wire.send({ type: "open", target })
        })
      }

      /** Run an effect in a daemon's scope with every daemon service provided. */
      const fork = (r: Running, label: string, effect: Handler, onFailure: (cause: Cause.Cause<unknown>) => Effect.Effect<void>) =>
        effect.pipe(
          Effect.catchCause((cause) =>
            Cause.hasInterruptsOnly(cause) ? Effect.void : Effect.andThen(
              Effect.logError(`${label} failed`, cause),
              onFailure(cause)
            )
          ),
          Effect.provideService(MenuBar.MenuBar, menuBarFor(r)),
          Effect.provideService(Scope.Scope, r.scope),
          Effect.provideContext(platform),
          Effect.forkIn(r.scope),
          Effect.asVoid
        )

      /** A daemon whose `run` failed keeps its item, showing the error and a way to restart it. */
      const showFailure = (r: Running, cause: Cause.Cause<unknown>) =>
        menuBarFor(r).render({
          icon: "exclamationmark.triangle",
          tooltip: `${r.loaded.daemon.title} failed`,
          menu: [
            MenuBar.header(r.loaded.daemon.title),
            MenuBar.item("Failed", { subtitle: Cause.pretty(cause).split("\n")[0]?.slice(0, 200) }),
            MenuBar.separator,
            MenuBar.item(`Restart ${r.loaded.daemon.title}`, { onSelect: restartMarker }),
            MenuBar.item("Open Journal", { open: logPath })
          ]
        })

      const start = (loaded: LoadedDaemon) =>
        Effect.gen(function*() {
          const r: Running = {
            loaded,
            scope: yield* Scope.fork(hostScope),
            opened: yield* PubSub.unbounded<void>(),
            handlers: new Map(),
            shown: false
          }
          running.set(loaded.id, r)
          yield* Effect.logInfo(`Starting daemon ${loaded.id}`)
          // Show something right away unless the daemon renders within a moment
          // (avoids a flash of the placeholder on every start).
          yield* fork(
            r,
            "placeholder",
            Effect.sleep("300 millis").pipe(
              Effect.andThen(() => r.shown ? Effect.void : menuBarFor(r).render({ menu: [MenuBar.header(loaded.daemon.title)] }))
            ),
            () => Effect.void
          )
          yield* fork(r, `Daemon ${loaded.daemon.title}`, loaded.daemon.run, (cause) => showFailure(r, cause))
        })

      /** `replaced`: a new instance starts right after, so keep the item (and its place in the menu bar). */
      const stop = (r: Running, replaced = false) =>
        Effect.gen(function*() {
          if (live(r)) running.delete(r.loaded.id)
          yield* Effect.logInfo(`Stopping daemon ${r.loaded.id}`)
          yield* Scope.close(r.scope, Exit.void)
          yield* PubSub.shutdown(r.opened)
          if (!replaced && !running.has(r.loaded.id)) yield* wire.send({ type: "statusRemoved", daemon: r.loaded.id })
        })

      /** Starts, stops, and restarts happen one at a time, so a restart can't race a reload. */
      const lock = yield* Semaphore.make(1)

      const restart = (r: Running) =>
        Effect.gen(function*() {
          const current = yield* SubscriptionRef.get(registry.daemons)
          // A reload may have replaced or removed it while the user was choosing Restart.
          if (!live(r) || current.get(r.loaded.id) !== r.loaded) return
          yield* stop(r, true)
          yield* start(r.loaded)
        }).pipe(lock.withPermit)

      const reconcile = (next: ReadonlyMap<string, LoadedDaemon>) =>
        Effect.gen(function*() {
          for (const r of [...running.values()]) {
            const replacement = next.get(r.loaded.id)
            if (replacement?.daemon !== r.loaded.daemon) yield* stop(r, replacement !== undefined)
          }
          for (const loaded of next.values()) {
            if (!running.has(loaded.id)) yield* start(loaded)
          }
        })

      yield* Effect.addFinalizer(() =>
        Effect.forEach([...running.values()], (r) => stop(r), { concurrency: "unbounded", discard: true })
      )

      yield* SubscriptionRef.changes(registry.daemons).pipe(
        Stream.runForEach((next) => lock.withPermit(reconcile(next))),
        Effect.forkScoped
      )

      const handle = (message: Extract<Inbound, { readonly type: "menu" | "menuOpened" }>): Effect.Effect<void> =>
        Effect.suspend(() => {
          const r = running.get(message.daemon)
          if (!r) return Effect.void
          if (message.type === "menuOpened") return Effect.asVoid(PubSub.publish(r.opened, undefined))
          const handler = r.handlers.get(message.item)
          if (!handler) return Effect.logWarning(`No menu item ${message.item} in ${message.daemon}`)
          // Not in the daemon's scope: restarting closes that scope.
          if (handler === restartMarker) return restart(r)
          return fork(r, `${r.loaded.daemon.title} menu item`, handler, () =>
            wire.send({ type: "hud", message: `⚠︎ ${r.loaded.daemon.title}: action failed` }))
        })

      return Daemons.of({ handle })
    })
  )
}
