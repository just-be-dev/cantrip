/**
 * One session per spell run. A session is a stack of frames, one per view.
 * Each frame owns a Scope (its fibers), its search text, and the action
 * handlers of whatever it last rendered. Popping a frame closes its scope.
 * Closing the session closes every frame.
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
import * as Scope from "effect/Scope"
import * as Stream from "effect/Stream"
import * as SubscriptionRef from "effect/SubscriptionRef"
import type { SpellServices } from "../sdk/Services.ts"
import { Ui } from "../sdk/Ui.ts"
import * as View from "../sdk/View.ts"
import type { Inbound, WireAction, WireView } from "./Protocol.ts"
import { Registry } from "./Registry.ts"
import { Wire } from "./Wire.ts"

/** Everything the app sends except daemon menu events, which go to `Daemons`. */
export type SessionMessage = Exclude<Inbound, { readonly type: "menu" | "menuOpened" | "handoff" }>

interface Frame {
  readonly scope: Scope.Closeable
  readonly query: SubscriptionRef.SubscriptionRef<string>
  readonly ui: Ui["Service"]
  handlers: ReadonlyMap<string, View.Action>
  shown: boolean
}

interface Session {
  readonly id: string
  readonly title: string
  readonly scope: Scope.Closeable
  readonly frames: Array<Frame>
}

const wireKind = (action: View.Action): Pick<WireAction, "kind" | "value"> => {
  switch (action._tag) {
    case "Run":
      return { kind: "callback" }
    case "Push":
      return { kind: "push" }
    case "Copy":
      return { kind: "copy", value: action.text }
    case "Open":
      return { kind: "open", value: action.target }
  }
}

const encode = (view: View.View): { readonly wire: WireView; readonly handlers: ReadonlyMap<string, View.Action> } => {
  const handlers = new Map<string, View.Action>()
  const actions = (prefix: string, list: ReadonlyArray<View.Action> | undefined) =>
    (list ?? []).map((action, i): WireAction => {
      const id = `${prefix}.${i}`
      handlers.set(id, action)
      return { id, title: action.title, icon: action.icon, ...wireKind(action) }
    })
  switch (view._tag) {
    case "List":
      return {
        handlers,
        wire: {
          kind: "list",
          placeholder: view.placeholder,
          filtering: view.filtering ?? true,
          emptyText: view.emptyText,
          items: view.items.map((item, i) => ({
            id: item.id ?? String(i),
            title: item.title,
            subtitle: item.subtitle,
            icon: item.icon,
            accessories: item.accessories ?? [],
            keywords: item.keywords ?? [],
            actions: actions(String(i), item.actions)
          }))
        }
      }
    case "Detail":
      return {
        handlers,
        wire: {
          kind: "detail",
          markdown: view.markdown,
          metadata: view.metadata ?? [],
          actions: actions("detail", view.actions)
        }
      }
  }
}

export class Sessions extends Context.Service<Sessions, {
  readonly handle: (message: SessionMessage) => Effect.Effect<void>
}>()("cantrip/host/Sessions") {
  static readonly layer = Layer.effect(
    Sessions,
    Effect.gen(function*() {
      const wire = yield* Wire
      const registry = yield* Registry
      const platform = yield* Effect.context<
        HttpClient.HttpClient | FileSystem.FileSystem | Path.Path | ChildProcessSpawner.ChildProcessSpawner
      >()
      const sessions = new Map<string, Session>()
      const live = (session: Session) => sessions.get(session.id) === session

      const reportFailure = (session: Session, cause: Cause.Cause<unknown>) =>
        Cause.hasInterruptsOnly(cause) ? Effect.void : Effect.gen(function*() {
          yield* Effect.logError(`Spell ${session.title} failed`, cause)
          yield* wire.send({
            type: "error",
            session: session.id,
            title: `${session.title} failed`,
            message: Cause.pretty(cause)
          })
        })

      /** Run an effect in a frame's scope with every spell service provided. */
      const fork = (session: Session, frame: Frame, effect: Effect.Effect<unknown, unknown, SpellServices>) =>
        effect.pipe(
          Effect.catchCause((cause) => reportFailure(session, cause)),
          Effect.provideService(Ui, frame.ui),
          Effect.provideContext(platform),
          Effect.forkIn(frame.scope),
          Effect.asVoid
        )

      const closeSession = (session: Session) =>
        Effect.suspend(() => {
          if (live(session)) sessions.delete(session.id)
          return Scope.close(session.scope, Exit.void)
        })

      /** Drop the frame at `depth` and everything above it. */
      const truncate = (session: Session, depth: number) =>
        Effect.suspend(() => {
          const removed = session.frames.splice(depth)
          return Effect.forEach(removed.reverse(), (f) => Scope.close(f.scope, Exit.void), { discard: true })
        })

      const pushFrame = (session: Session) =>
        Effect.gen(function*() {
          const depth = session.frames.length
          const frame: Frame = {
            scope: yield* Scope.fork(session.scope),
            query: yield* SubscriptionRef.make(""),
            ui: makeUi(session, () => depth),
            handlers: new Map(),
            shown: false
          }
          session.frames.push(frame)
          return frame
        })

      /** Push a program above `depth`: replace anything there and run it in a new frame. */
      const push = (session: Session, depth: number, program: Effect.Effect<unknown, unknown, SpellServices>) =>
        Effect.gen(function*() {
          if (!live(session) || session.frames.length <= depth) return
          if (session.frames.length > depth + 1) {
            yield* truncate(session, depth + 1)
            yield* wire.send({ type: "pop", session: session.id, depth: depth + 1 })
          }
          const frame = yield* pushFrame(session)
          yield* fork(session, frame, program)
        })

      function makeUi(session: Session, depthOf: () => number): Ui["Service"] {
        const frameAt = () => session.frames[depthOf()]
        return Ui.of({
          render: (view) =>
            Effect.suspend(() => {
              const frame = frameAt()
              if (!frame || !live(session)) return Effect.void
              const encoded = encode(view)
              frame.handlers = encoded.handlers
              frame.shown = true
              return wire.send({ type: "render", session: session.id, depth: depthOf(), view: encoded.wire })
            }),
          push: (view) => Effect.suspend(() => push(session, depthOf(), View.isView(view) ? frameRender(view) : view)),
          pop: Effect.suspend(() => {
            const depth = depthOf()
            if (!live(session) || session.frames.length <= depth) return Effect.void
            if (depth === 0) return Effect.andThen(wire.send({ type: "closed", session: session.id }), closeSession(session))
            return Effect.andThen(wire.send({ type: "pop", session: session.id, depth }), truncate(session, depth))
          }),
          query: Stream.unwrap(Effect.sync(() => {
            const frame = frameAt()
            return frame ? SubscriptionRef.changes(frame.query) : Stream.empty
          })),
          setLoading: (loading) => wire.send({ type: "loading", session: session.id, loading }),
          toast: (t) =>
            wire.send({ type: "toast", session: session.id, style: t.style ?? "info", title: t.title, message: t.message }),
          hud: (message) => wire.send({ type: "hud", message }),
          hide: wire.send({ type: "hide" }),
          copy: (text) => wire.send({ type: "clipboard", text }),
          open: (target) => wire.send({ type: "open", target })
        })
      }

      const frameRender = (view: View.View): Effect.Effect<void, never, Ui> => Ui.use((ui) => ui.render(view))

      const run = Effect.fn("Sessions.run")(function*(id: string, command: string) {
        const loaded = yield* registry.get(command)
        if (!loaded) {
          return yield* wire.send({ type: "error", session: id, title: "Unknown spell", message: command })
        }
        const session: Session = { id, title: loaded.spell.title, scope: yield* Scope.make(), frames: [] }
        sessions.set(id, session)
        const root = yield* pushFrame(session)
        yield* fork(
          session,
          root,
          loaded.spell.run.pipe(
            Effect.onExit((exit) => {
              // A spell that finished without ever showing a view is done: the app can hide.
              if (Exit.isFailure(exit) || root.shown || !live(session)) return Effect.void
              return Effect.andThen(wire.send({ type: "done", session: id, rendered: false }), closeSession(session))
            })
          )
        )
      })

      const action = Effect.fn("Sessions.action")(function*(session: Session, depth: number, actionId: string) {
        const frame = session.frames[depth]
        const handler = frame?.handlers.get(actionId)
        if (!frame || !handler) return yield* Effect.logWarning(`No action ${actionId} at ${session.id}/${depth}`)
        switch (handler._tag) {
          case "Run":
            return yield* fork(session, frame, handler.effect)
          case "Push":
            return yield* push(session, depth, handler.program)
          case "Copy":
          case "Open":
            return // the app does these itself
        }
      })

      const withSession = (id: string, f: (session: Session) => Effect.Effect<void>) =>
        Effect.suspend(() => {
          const session = sessions.get(id)
          return session ? f(session) : Effect.void
        })

      const handle = (message: SessionMessage): Effect.Effect<void> => {
        switch (message.type) {
          case "run":
            return run(message.session, message.command)
          case "query":
            return withSession(message.session, (s) => {
              const frame = s.frames[message.depth]
              return frame ? SubscriptionRef.set(frame.query, message.text) : Effect.void
            })
          case "action":
            return withSession(message.session, (s) => action(s, message.depth, message.action))
          case "pop":
            return withSession(message.session, (s) =>
              message.depth === 0 ? closeSession(s) : truncate(s, message.depth))
          case "close":
            return withSession(message.session, closeSession)
          case "reload":
            return registry.reload
        }
      }

      yield* Effect.addFinalizer(() => Effect.forEach([...sessions.values()], closeSession, { discard: true }))

      return Sessions.of({ handle })
    })
  )
}
