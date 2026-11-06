/**
 * Line-delimited JSON over stdio to Incant.app.
 */
import * as Context from "effect/Context"
import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"
import * as Schema from "effect/Schema"
import * as Stream from "effect/Stream"
import { InboundLine, type Inbound, type Outbound } from "./Protocol.ts"

export class Wire extends Context.Service<Wire, {
  readonly send: (message: Outbound) => Effect.Effect<void>
  /** Decoded app messages. Lines that don't decode are logged and skipped. */
  readonly incoming: Stream.Stream<Inbound>
}>()("incant/host/Wire") {
  static readonly layer = Layer.sync(Wire, () => {
    // Keep the real stdout for the protocol. Everything that logs through
    // `console` (spells, Effect's logger) goes to stderr so it can't corrupt it.
    const stdout = process.stdout
    redirectConsoleToStderr()

    const decode = Schema.decodeUnknownEffect(InboundLine)

    return Wire.of({
      send: (message) => Effect.sync(() => void stdout.write(`${JSON.stringify(message)}\n`)),
      incoming: Stream.fromReadableStream({
        evaluate: () => Bun.stdin.stream(),
        onError: (cause) => cause
      }).pipe(
        Stream.decodeText(),
        Stream.splitLines,
        Stream.filter((line) => line.trim().length > 0),
        Stream.mapEffect((line) =>
          decode(line).pipe(
            Effect.map((message): Inbound | undefined => message),
            Effect.catch((error) =>
              Effect.as(Effect.logWarning("Ignoring malformed message", line, error.message), undefined)
            )
          )
        ),
        Stream.filter((message): message is Inbound => message !== undefined),
        Stream.orDie
      )
    })
  })
}

const redirectConsoleToStderr = () => {
  const write = (...args: ReadonlyArray<unknown>) => {
    process.stderr.write(`${args.map((a) => (typeof a === "string" ? a : Bun.inspect(a))).join(" ")}\n`)
  }
  console.log = write
  console.info = write
  console.debug = write
  console.warn = write
  console.error = write
  console.trace = write
}
