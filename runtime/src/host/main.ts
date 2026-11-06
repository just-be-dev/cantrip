/**
 * The Cantrip spell host. Cantrip.app starts this as one long-lived process
 * and talks to it over stdio (see Protocol.ts).
 */
import * as BunRuntime from "@effect/platform-bun/BunRuntime"
import * as BunServices from "@effect/platform-bun/BunServices"
import * as Effect from "effect/Effect"
import * as FetchHttpClient from "effect/http/FetchHttpClient"
import * as Layer from "effect/Layer"
import * as Stream from "effect/Stream"
import { Registry } from "./Registry.ts"
import { Sessions } from "./Sessions.ts"
import { Wire } from "./Wire.ts"

/** Keep in sync with `engines.bun` in package.json and the pin in mise.toml. */
const MinimumBun = "1.4.4"

const Main = Layer.effectDiscard(
  Effect.gen(function*() {
    const wire = yield* Wire
    const sessions = yield* Sessions
    yield* Effect.logInfo(`Cantrip host ready (bun ${Bun.version})`)
    if (Bun.semver.order(Bun.version, MinimumBun) < 0) {
      yield* Effect.logWarning(`bun ${Bun.version} is older than ${MinimumBun}; update bun or set "bun" in config.json`)
    }
    // Each message is handled on its own fiber so a slow `run` never blocks input.
    yield* wire.incoming.pipe(
      Stream.runForEach((message) => Effect.forkDetach(sessions.handle(message))),
      // stdin closed: the app quit, so exit.
      Effect.andThen(Effect.sync(() => process.exit(0))),
      Effect.forkScoped
    )
  })
)

const Platform = Layer.mergeAll(BunServices.layer, FetchHttpClient.layer)

const App = Main.pipe(
  Layer.provide(Sessions.layer),
  Layer.provide(Registry.layer),
  Layer.provide(Wire.layer),
  Layer.provide(Platform)
)

BunRuntime.runMain(Layer.launch(App))
