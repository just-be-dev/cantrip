/**
 * Cantrip is a macOS app, so it has nothing to deploy. projects.just-be.dev
 * runs `alchemy deploy --stage <STAGE>` on every push, and this empty stack
 * makes those deploys succeed without creating anything. With no resources
 * there's nothing to track, so local state is enough.
 */
import * as Alchemy from "alchemy"
import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"

export default Alchemy.Stack(
  process.env.PROJECT_NAME?.trim() || "cantrip",
  { providers: Layer.empty, state: Alchemy.localState() },
  Effect.succeed({})
)
