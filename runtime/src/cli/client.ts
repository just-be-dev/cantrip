/**
 * The client behind every command link (`~/.local/bin/insomnia` → the shim →
 * here). Plain bun, no Effect, so it starts fast. Connects to the running
 * host's socket, sends argv, streams stdio both ways, and exits with the
 * command's code. See src/host/Commands.ts for the frame format.
 *
 * argv: [bun, client.ts, <link name>, ...args]
 */
const socketPath = process.env.CANTRIP_SOCKET
const [name = "cantrip", ...args] = process.argv.slice(2)

const fail = (message: string, code: number): never => {
  process.stderr.write(`${name}: ${message}\n`)
  process.exit(code)
}

if (!socketPath) fail("CANTRIP_SOCKET isn't set; run this through a command link", 64)

let buffer = ""
let interrupted = false
/** Bytes the socket hasn't taken yet. Bun's `write` may take only part of a frame. */
let pending = new Uint8Array()
const encoder = new TextEncoder()

// `cmd | head` closes our stdout early; exit quietly like other tools do.
process.stdout.on("error", (error: NodeJS.ErrnoException) => {
  if (error.code === "EPIPE") process.exit(141)
})

const connect = () =>
  Bun.connect({
    unix: socketPath!,
    socket: {
      data: (_socket, chunk) => {
        buffer += chunk.toString()
        let newline = buffer.indexOf("\n")
        while (newline >= 0) {
          const line = buffer.slice(0, newline)
          buffer = buffer.slice(newline + 1)
          newline = buffer.indexOf("\n")
          if (!line) continue
          let frame:
            | { readonly type: "out"; readonly fd: 1 | 2; readonly data: string }
            | { readonly type: "exit"; readonly code: number }
          try {
            frame = JSON.parse(line)
          } catch {
            fail(`unreadable reply from Cantrip: ${line.slice(0, 120)}`, 70)
            return
          }
          if (frame.type === "out") {
            ;(frame.fd === 2 ? process.stderr : process.stdout).write(Buffer.from(frame.data, "base64"))
          } else {
            process.exitCode = frame.code
            // Let stdout drain before exiting.
            process.stdout.write("", () => process.exit(frame.code))
          }
        }
      },
      drain: (socket) => flush(socket),
      close: () => {
        if (process.exitCode === undefined) fail("Cantrip stopped while the command was running", 69)
      },
      error: (_socket, error) => fail(String(error), 69)
    }
  })

/** If Cantrip isn't running, launch it (in the background) and wait for its socket. */
const connectOrLaunch = async () => {
  try {
    return await connect()
  } catch {
    if (process.env.CANTRIP_NO_LAUNCH) return fail("Cantrip isn't running", 69)
    Bun.spawnSync(["open", "-g", "-b", "dev.just-be.cantrip"], { stdout: "ignore", stderr: "ignore" })
    for (let i = 0; i < 50; i++) {
      await Bun.sleep(200)
      try {
        return await connect()
      } catch {
        // not up yet
      }
    }
    return fail("Cantrip isn't running, and couldn't be started", 69)
  }
}

function flush(socket: { write: (data: Uint8Array) => number }) {
  while (pending.length > 0) {
    const written = socket.write(pending)
    if (written <= 0) return
    pending = pending.subarray(written)
  }
}

const socket = await connectOrLaunch()
const send = (frame: unknown) => {
  const bytes = encoder.encode(`${JSON.stringify(frame)}\n`)
  const next = new Uint8Array(pending.length + bytes.length)
  next.set(pending)
  next.set(bytes, pending.length)
  pending = next
  flush(socket)
}
/** Resolves once everything queued has been handed to the socket (backpressure for stdin). */
const drained = async () => {
  while (pending.length > 0) await Bun.sleep(5)
}

send({
  type: "run",
  name,
  args,
  columns: process.stdout.columns,
  rows: process.stdout.rows,
  tty: { stdin: process.stdin.isTTY === true, stdout: process.stdout.isTTY === true },
  env: { NO_COLOR: process.env.NO_COLOR, FORCE_COLOR: process.env.FORCE_COLOR, TERM: process.env.TERM }
})

// Ctrl-C asks the host to interrupt the command; a second one gives up.
process.on("SIGINT", () => {
  if (interrupted) process.exit(130)
  interrupted = true
  send({ type: "interrupt" })
})

// Forward piped stdin. A terminal's stdin is only read if it's not a TTY, so
// commands don't swallow keystrokes they never asked for.
if (process.stdin.isTTY) {
  send({ type: "stdinEnd" })
} else {
  ;(async () => {
    for await (const chunk of process.stdin) {
      send({ type: "stdin", data: Buffer.from(chunk).toString("base64") })
      await drained()
    }
    send({ type: "stdinEnd" })
  })()
}
