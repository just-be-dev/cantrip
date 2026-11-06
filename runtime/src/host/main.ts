/**
 * The Cantrip spell host. Cantrip.app starts this as one long-lived process
 * and talks to it over stdio (see Protocol.ts).
 *
 * Shutdown is graceful either way it happens: when stdin closes (the app quit
 * or crashed) the program finishes, and on SIGTERM/SIGINT (Restart Host)
 * the runtime interrupts it. Both close the layer scope, which stops every
 * session and daemon and runs their finalizers before the process exits.
 */
import * as BunRuntime from "@effect/platform-bun/BunRuntime"
import * as BunServices from "@effect/platform-bun/BunServices"
import * as Effect from "effect/Effect"
import * as FetchHttpClient from "effect/http/FetchHttpClient"
import * as Layer from "effect/Layer"
import * as Stream from "effect/Stream"
import { Commands } from "./Commands.ts"
import { Daemons } from "./Daemons.ts"
import { Registry } from "./Registry.ts"
import { Sessions } from "./Sessions.ts"
import { Wire } from "./Wire.ts"

/** Keep in sync with `engines.bun` in package.json and the pin in mise.toml. */
const MinimumBun = "1.4.4"

const main = Effect.gen(function*() {
  const wire = yield* Wire
  const sessions = yield* Sessions
  const daemons = yield* Daemons
  yield* Effect.logInfo(`Cantrip host ready (bun ${Bun.version})`)
  if (Bun.semver.order(Bun.version, MinimumBun) < 0) {
    yield* Effect.logWarning(`bun ${Bun.version} is older than ${MinimumBun}; update bun or set "bun" in config.json`)
  }
  // Each message is handled on its own fiber so a slow `run` never blocks input.
  yield* wire.incoming.pipe(
    Stream.runForEach((message) =>
      Effect.forkDetach(
        message.type === "menu" || message.type === "menuOpened" ? daemons.handle(message) : sessions.handle(message)
      )
    )
  )
  yield* Effect.logInfo("stdin closed; shutting down")
})

const Platform = Layer.mergeAll(BunServices.layer, FetchHttpClient.layer)

const Host = Layer.mergeAll(Sessions.layer, Commands.layer.pipe(Layer.provideMerge(Daemons.layer))).pipe(
  Layer.provideMerge(Registry.layer),
  Layer.provideMerge(Wire.layer),
  Layer.provide(Platform)
)

main.pipe(
  Effect.provide(Host),
  // Finalizers have run by now; don't wait on stray handles (sockets, timers).
  Effect.ensuring(Effect.sync(() => process.exit(0))),
  BunRuntime.runMain
)
