/**
 * CLI commands for spells and daemons. A spell or daemon with a `command`
 * (an `effect/cli` Command) gets a link in `~/.local/bin` named after it.
 * Running that link runs the command *in this host*: a small bun client
 * (`src/cli/client.ts`) connects to a Unix socket, sends argv, streams stdio
 * both ways, and exits with the command's exit code. A daemon's command runs
 * inside the daemon, so it sees the same live state as the menu.
 *
 *   ~/.local/bin/insomnia ─symlink─▶ ~/.config/cantrip/bin/cantrip (shim)
 *        └─▶ bun client.ts ──unix socket──▶ host ──▶ Command.runWith(...)
 *
 * Socket frames are JSON lines.
 *   client → host: { type: "run", name, args, columns, rows, tty, env }
 *                  { type: "stdin", data: base64 } | { type: "stdinEnd" } | { type: "interrupt" }
 *   host → client: { type: "out", fd: 1 | 2, data: base64 } | { type: "exit", code }
 */
import * as NodePath from "node:path"
import { formatWithOptions } from "node:util"
import * as Cause from "effect/Cause"
import * as CliError from "effect/cli/CliError"
import * as CliCommand from "effect/cli/Command"
import * as TerminalCapabilities from "effect/cli/TerminalCapabilities"
import * as Console from "effect/Console"
import * as Context from "effect/Context"
import * as Effect from "effect/Effect"
import * as Exit from "effect/Exit"
import * as Fiber from "effect/Fiber"
import * as FileSystem from "effect/FileSystem"
import * as HttpClient from "effect/http/HttpClient"
import * as Layer from "effect/Layer"
import * as Option from "effect/Option"
import * as Path from "effect/Path"
import * as ChildProcessSpawner from "effect/process/ChildProcessSpawner"
import * as Queue from "effect/Queue"
import * as Sink from "effect/Sink"
import * as Stdio from "effect/Stdio"
import * as Stream from "effect/Stream"
import * as SubscriptionRef from "effect/SubscriptionRef"
import * as Terminal from "effect/Terminal"
import { DaemonUnavailable, Daemons } from "./Daemons.ts"
import { GrimoireDir, type LoadedCommand, Registry, runtimeDir } from "./Registry.ts"

/** Where command links go. Must be on your PATH. */
const linkDir = process.env.CANTRIP_BIN_DIR ?? `${process.env.HOME}/.local/bin`

interface RunRequest {
  readonly type: "run"
  readonly name: string
  readonly args: ReadonlyArray<string>
  readonly columns?: number
  readonly rows?: number
  readonly tty?: { readonly stdin?: boolean; readonly stdout?: boolean }
  readonly env?: { readonly NO_COLOR?: string; readonly FORCE_COLOR?: string; readonly TERM?: string }
}

type ClientFrame =
  | RunRequest
  | { readonly type: "stdin"; readonly data: string }
  | { readonly type: "stdinEnd" }
  | { readonly type: "interrupt" }

/** One connected client: buffered writes (Bun sockets can take partial writes) and its stdin. */
interface Connection {
  buffer: string
  pending: Uint8Array
  closed: boolean
  fiber?: Fiber.Fiber<unknown>
  stdin?: Queue.Queue<Uint8Array, Cause.Done>
  socket: Bun.Socket<Connection>
}

const encoder = new TextEncoder()

const flush = (c: Connection) => {
  while (c.pending.length > 0 && !c.closed) {
    const written = c.socket.write(c.pending)
    if (written <= 0) return
    c.pending = c.pending.subarray(written)
  }
}

const send = (c: Connection, frame: unknown) => {
  if (c.closed) return
  const bytes = encoder.encode(`${JSON.stringify(frame)}\n`)
  const next = new Uint8Array(c.pending.length + bytes.length)
  next.set(c.pending)
  next.set(bytes, c.pending.length)
  c.pending = next
  flush(c)
}

const out = (c: Connection, fd: 1 | 2, chunk: string | Uint8Array) =>
  send(c, { type: "out", fd, data: Buffer.from(typeof chunk === "string" ? encoder.encode(chunk) : chunk).toString("base64") })

/** A Stdio sink that writes to the client's stdout or stderr. */
const sinkTo = (c: Connection, fd: 1 | 2) =>
  // biome-ignore lint/complexity/noForEach lint/suspicious/useIterableCallbackReturn: Effect's Sink.forEach, not Array#forEach
  Sink.forEach((chunk: string | Uint8Array): Effect.Effect<void> => Effect.sync(() => out(c, fd, chunk)))

/** A `Console` that prints to the client's terminal instead of the host log. */
const makeConsole = (c: Connection, colors: boolean): Console.Console => {
  const line = (args: ReadonlyArray<unknown>) => `${formatWithOptions({ colors }, ...args)}\n`
  const counts = new Map<string, number>()
  const timers = new Map<string, number>()
  const elapsed = (label: string) => `${label}: ${(performance.now() - (timers.get(label) ?? performance.now())).toFixed(3)}ms`
  return {
    assert: (condition, ...args) => {
      if (!condition) out(c, 2, line(["Assertion failed:", ...args]))
    },
    clear: () => {},
    count: (label = "default") => {
      const n = (counts.get(label) ?? 0) + 1
      counts.set(label, n)
      out(c, 1, `${label}: ${n}\n`)
    },
    countReset: (label = "default") => void counts.delete(label),
    debug: (...args) => out(c, 1, line(args)),
    dir: (item) => out(c, 1, `${Bun.inspect(item, { colors })}\n`),
    dirxml: (...args) => out(c, 1, line(args)),
    error: (...args) => out(c, 2, line(args)),
    group: (...args) => args.length > 0 && out(c, 1, line(args)),
    groupCollapsed: (...args) => args.length > 0 && out(c, 1, line(args)),
    groupEnd: () => {},
    info: (...args) => out(c, 1, line(args)),
    log: (...args) => out(c, 1, line(args)),
    table: (data, properties) => out(c, 1, `${Bun.inspect.table(data, properties as Array<string>, { colors })}\n`),
    time: (label = "default") => void timers.set(label, performance.now()),
    timeEnd: (label = "default") => {
      out(c, 1, `${elapsed(label)}\n`)
      timers.delete(label)
    },
    timeLog: (label = "default", ...args) => out(c, 1, line([elapsed(label), ...args])),
    trace: (...args) => out(c, 2, line(args)),
    warn: (...args) => out(c, 2, line(args))
  }
}

export class Commands extends Context.Service<Commands, {}>()("cantrip/host/Commands") {
  static readonly layer = Layer.effect(
    Commands,
    Effect.gen(function*() {
      const registry = yield* Registry
      const daemons = yield* Daemons
      const fs = yield* FileSystem.FileSystem
      const path = yield* Path.Path
      const grimoire = yield* GrimoireDir
      const platform = yield* Effect.context<
        HttpClient.HttpClient | FileSystem.FileSystem | Path.Path | ChildProcessSpawner.ChildProcessSpawner
      >()

      const socketPath = path.join(grimoire, ".cantrip.sock")
      const shimDir = path.join(path.dirname(grimoire), "bin")
      const shim = path.join(shimDir, "cantrip")

      // The shim every link points at. Rewritten on each start, since bun or
      // the app's runtime may have moved.
      const quote = (s: string) => `'${s.replaceAll("'", `'\\''`)}'`
      yield* fs.makeDirectory(shimDir, { recursive: true })
      yield* fs.writeFileString(
        shim,
        [
          "#!/bin/sh",
          "# Written by Cantrip. Runs a spell's CLI command in the running Cantrip host.",
          `CANTRIP_SOCKET=${quote(socketPath)} exec ${quote(process.execPath)} ${
            quote(NodePath.join(runtimeDir, "src/cli/client.ts"))
          } "$(basename "$0")" "$@"`,
          ""
        ].join("\n")
      )
      yield* fs.chmod(shim, 0o755)

      /**
       * Make `linkDir` hold exactly one symlink to the shim per command (plus
       * `cantrip`). Never touches files that aren't our links.
       */
      const syncLinks = (commands: ReadonlyMap<string, LoadedCommand>) =>
        Effect.gen(function*() {
          yield* fs.makeDirectory(linkDir, { recursive: true })
          const wanted = new Set(["cantrip", ...commands.keys()])
          for (const name of yield* fs.readDirectory(linkDir)) {
            const at = path.join(linkDir, name)
            const target = yield* fs.readLink(at).pipe(Effect.orElseSucceed(() => undefined))
            if (target === shim && !wanted.has(name)) {
              yield* fs.remove(at)
              yield* Effect.logInfo(`Unlinked command ${name}`)
            }
          }
          for (const name of wanted) {
            const at = path.join(linkDir, name)
            const target = yield* fs.readLink(at).pipe(Effect.orElseSucceed(() => undefined))
            if (target === shim) continue
            if (target !== undefined || (yield* fs.exists(at))) {
              yield* Effect.logWarning(`Not linking command ${name}: ${at} already exists`)
              continue
            }
            yield* fs.symlink(shim, at)
            yield* Effect.logInfo(`Linked command ${name} → ${at}`)
          }
        }).pipe(Effect.catchCause((cause) => Effect.logError("Could not link commands", cause)))

      yield* SubscriptionRef.changes(registry.commands).pipe(Stream.runForEach(syncLinks), Effect.forkScoped)

      /** `cantrip` on its own: what's available. */
      const listing = (commands: ReadonlyMap<string, LoadedCommand>) => {
        const rows = [...commands.values()].sort((a, b) => a.name.localeCompare(b.name))
        if (rows.length === 0) return "No spell or daemon has a command yet.\n"
        const width = Math.max(...rows.map((r) => r.name.length))
        return [
          "Commands from your grimoire:",
          "",
          ...rows.map((r) => `  ${r.name.padEnd(width)}  ${r.owner.title} (${r.owner.kind})`),
          "",
          "Run one with `<name> --help`, or `cantrip <name> …`.",
          ""
        ].join("\n")
      }

      /** Runs one request to completion and reports the exit code. */
      const invoke = (c: Connection, request: RunRequest, stdin: Queue.Queue<Uint8Array, Cause.Done>) =>
        Effect.gen(function*() {
          const commands = yield* SubscriptionRef.get(registry.commands)
          let name = request.name
          let args = request.args
          if (name === "cantrip") {
            const [first, ...rest] = args
            if (first === undefined || first.startsWith("-")) {
              out(c, 1, listing(commands))
              return 0
            }
            name = first
            args = rest
          }
          const entry = commands.get(name)
          if (!entry) {
            out(c, 2, `cantrip: no spell or daemon has a command named "${name}". Run \`cantrip\` to list them.\n`)
            return 127
          }

          const env = request.env ?? {}
          const stdoutIsTerminal = request.tty?.stdout === true
          const stdinIsTerminal = request.tty?.stdin === true
          const canColor = env.TERM !== "dumb" && env.NO_COLOR === undefined &&
            (stdoutIsTerminal || (env.FORCE_COLOR !== undefined && env.FORCE_COLOR !== "0"))
          const stdinStream = Stream.fromQueue(stdin)
          const lines = stdinStream.pipe(Stream.decodeText(), Stream.splitLines)

          const program = CliCommand.runWith(entry.command, { version: entry.owner.title })(args).pipe(
            Effect.provideService(
              Stdio.Stdio,
              Stdio.make({
                args: Effect.succeed(args),
                stdinIsTerminal: Effect.succeed(stdinIsTerminal),
                stdoutIsTerminal: Effect.succeed(stdoutIsTerminal),
                stderrIsTerminal: Effect.succeed(stdoutIsTerminal),
                stdout: () => sinkTo(c, 1),
                stderr: () => sinkTo(c, 2),
                stdin: stdinStream
              })
            ),
            Effect.provideService(
              Terminal.Terminal,
              Terminal.make({
                columns: Effect.succeed(request.columns ?? 80),
                rows: Effect.succeed(request.rows ?? 24),
                size: Effect.die("Terminal.size isn't supported in Cantrip commands"),
                readInput: Effect.die("Interactive input isn't supported in Cantrip commands"),
                readLine: lines.pipe(
                  Stream.runHead,
                  Effect.flatMap((line) => line._tag === "Some" ? Effect.succeed(line.value) : Effect.fail(new Terminal.QuitError()))
                ),
                display: (text) => Effect.sync(() => out(c, 1, text))
              })
            ),
            Effect.provideService(TerminalCapabilities.TerminalCapabilities, {
              canColor,
              canAnimate: false,
              canPrompt: false
            }),
            Effect.provideService(Console.Console, makeConsole(c, canColor))
          )

          const exit = yield* (
            entry.owner.kind === "daemon"
              ? daemons.runIn(entry.owner.id, program)
              // The command's requirements are erased in the registry; `Spell.command` checked them.
              : (program.pipe(Effect.scoped, Effect.provideContext(platform)) as Effect.Effect<void, unknown>)
          ).pipe(Effect.exit)

          if (Exit.isSuccess(exit)) return 0
          if (Cause.hasInterruptsOnly(exit.cause)) return 130
          const error = Cause.findErrorOption(exit.cause)
          if (error._tag === "Some") {
            const e = error.value
            if (e instanceof DaemonUnavailable) {
              out(c, 2, `${name}: ${e.message}\n`)
              return 69
            }
            if (CliError.isCliError(e)) return e._tag === "ShowHelp" && e.errors.length === 0 ? 0 : 2
            // Command.run already printed errors that carry a user message.
            if (typeof e === "object" && e !== null && CliError.getUserMessage(e) !== undefined) return 1
            out(c, 2, `${name}: ${e instanceof Error ? e.message : formatWithOptions({ colors: false }, e)}\n`)
            return 1
          }
          yield* Effect.logError(`Command ${name} crashed`, exit.cause)
          out(c, 2, `${name}: ${Cause.pretty(exit.cause)}\n`)
          return 70
        }).pipe(Effect.withSpan("Commands.invoke"))

      const services = yield* Effect.context<never>()
      const runFork = Effect.runForkWith(services)
      const runSync = Effect.runSyncWith(services)

      const onFrame = (c: Connection, frame: ClientFrame) => {
        switch (frame.type) {
          case "run": {
            if (c.fiber) return
            const stdin = runSync(Queue.unbounded<Uint8Array, Cause.Done>())
            c.stdin = stdin
            c.fiber = runFork(
              invoke(c, frame, stdin).pipe(
                Effect.onExit((exit) =>
                  Effect.sync(() => {
                    if (Exit.isSuccess(exit)) send(c, { type: "exit", code: exit.value })
                    else send(c, { type: "exit", code: Cause.hasInterruptsOnly(exit.cause) ? 130 : 70 })
                  })
                ),
                Effect.catchCause((cause) => Effect.logError("Command failed", cause))
              )
            )
            return
          }
          case "stdin":
            if (c.stdin) Queue.offerUnsafe(c.stdin, Buffer.from(frame.data, "base64"))
            return
          case "stdinEnd":
            if (c.stdin) runSync(Queue.end(c.stdin))
            return
          case "interrupt":
            if (c.fiber) runFork(Fiber.interrupt(c.fiber))
            return
        }
      }

      // Another host (or a crashed one) may have left a socket file behind.
      yield* fs.remove(socketPath).pipe(Effect.ignore)
      /** Identity of the socket file we created, so shutdown never deletes a newer host's. */
      const inode = (p: string) => fs.stat(p).pipe(Effect.map((info) => Option.getOrUndefined(info.ino)), Effect.orElseSucceed(() => undefined))
      const server = yield* Effect.acquireRelease(
        Effect.sync(() =>
          Bun.listen<Connection>({
            unix: socketPath,
            socket: {
              open: (socket) => {
                socket.data = { buffer: "", pending: new Uint8Array(), closed: false, socket }
              },
              data: (socket, chunk) => {
                const c = socket.data
                c.buffer += chunk.toString()
                let newline = c.buffer.indexOf("\n")
                while (newline >= 0) {
                  const line = c.buffer.slice(0, newline)
                  c.buffer = c.buffer.slice(newline + 1)
                  if (line.trim()) {
                    try {
                      onFrame(c, JSON.parse(line) as ClientFrame)
                    } catch (error) {
                      console.error("Bad CLI frame", error)
                    }
                  }
                  newline = c.buffer.indexOf("\n")
                }
              },
              drain: (socket) => flush(socket.data),
              close: (socket) => {
                const c = socket.data
                c.closed = true
                // The client went away (Ctrl-C twice, closed terminal): stop its command.
                if (c.fiber) runFork(Fiber.interrupt(c.fiber))
              },
              error: (_socket, error) => console.error("CLI socket error", error)
            }
          })
        ),
        (server) =>
          Effect.gen(function*() {
            server.stop(true)
            const now = yield* inode(socketPath)
            if (now !== undefined && now === ours) yield* fs.remove(socketPath).pipe(Effect.ignore)
          })
      )
      const ours = yield* inode(socketPath)
      yield* Effect.logInfo(`CLI commands listening on ${server.unix}`)

      return Commands.of({})
    })
  )
}
