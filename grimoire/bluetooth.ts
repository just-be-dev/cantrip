// Find Bluetooth devices nearby and connect to them: headphones, speakers, keyboards.
// The list opens with your paired devices and keeps scanning while it's up, so a
// device in pairing mode shows up within a few seconds. ↵ pairs (if it's new) and
// connects; on a connected device it disconnects.
//
// It remembers every device connected while it's open. When one comes back in
// range it goes to the top as "Back in range" with ↵ to Reconnect, and
// Reconnect Bluetooth Device reconnects the last one without the list.
//
// It drives macOS's Bluetooth through blueutil (`brew install blueutil`). The first
// scan asks for Bluetooth access for Cantrip.
import { homedir } from "node:os"
import * as Config from "effect/Config"
import * as Effect from "effect/Effect"
import * as FiberHandle from "effect/FiberHandle"
import * as FileSystem from "effect/FileSystem"
import * as Path from "effect/Path"
import * as ChildProcess from "effect/process/ChildProcess"
import * as ChildProcessSpawner from "effect/process/ChildProcessSpawner"
import * as Ref from "effect/Ref"
import * as Schema from "effect/Schema"
import * as Stream from "effect/Stream"
import { Action, Spell, Ui, type View } from "cantrip"

/** How long each scan listens before the list updates. Name lookups add a little. */
const SCAN_SECONDS = 5
/** Asked for by old devices that want a PIN; anything newer ignores it. */
const LEGACY_PIN = "0000"

// GUI apps don't get Homebrew's bin on their PATH, so look in the usual places.
const BLUEUTIL = ["/opt/homebrew/bin/blueutil", "/usr/local/bin/blueutil"]
const BREW = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]

// MARK: blueutil

class BluetoothError extends Schema.TaggedError<BluetoothError>()("BluetoothError", {
  message: Schema.String
}) {}

const Device = Schema.Struct({
  address: Schema.String,
  name: Schema.NullOr(Schema.String),
  paired: Schema.Boolean,
  connected: Schema.Boolean
})
type Device = typeof Device.Type

const decodeDevices = Schema.decodeUnknownEffect(Schema.fromJsonString(Schema.Array(Device)))

const find = (candidates: ReadonlyArray<string>) =>
  Effect.gen(function*() {
    const fs = yield* FileSystem.FileSystem
    for (const path of candidates) {
      if (yield* fs.exists(path).pipe(Effect.orElseSucceed(() => false))) return path
    }
    return undefined
  })

interface Exec {
  readonly exitCode: number
  readonly stdout: string
  readonly stderr: string
}

/**
 * Runs blueutil to completion. `onStderr` sees each line it prints there as it
 * comes (pairing prompts) and may answer on its stdin.
 */
const exec = (
  bin: string,
  args: ReadonlyArray<string>,
  onStderr?: (line: string, answer: (text: string) => Effect.Effect<void>) => Effect.Effect<void>
) =>
  Effect.scoped(Effect.gen(function*() {
    const spawner = yield* ChildProcessSpawner.ChildProcessSpawner
    const handle = yield* spawner.spawn(ChildProcess.make(bin, [...args]))
    const answer = (text: string) =>
      Stream.make(Uint8Array.from(text, (c) => c.charCodeAt(0))).pipe(Stream.run(handle.stdin), Effect.ignore)
    const lines: Array<string> = []
    const [stdout, , exitCode] = yield* Effect.all([
      handle.stdout.pipe(Stream.decodeText, Stream.mkString),
      handle.stderr.pipe(
        Stream.decodeText,
        Stream.splitLines,
        Stream.runForEach((line) => {
          lines.push(line)
          return onStderr ? onStderr(line, answer) : Effect.void
        })
      ),
      handle.exitCode
    ], { concurrency: "unbounded" })
    const result: Exec = { exitCode, stdout, stderr: lines.join("\n") }
    return result
  })).pipe(Effect.mapError((e) => new BluetoothError({ message: e.message })))

/** Runs blueutil and fails with what it said if it exits non-zero. */
const blueutil = (bin: string, args: ReadonlyArray<string>, onStderr?: Parameters<typeof exec>[2]) =>
  exec(bin, args, onStderr).pipe(
    Effect.flatMap((r) =>
      r.exitCode === 0
        ? Effect.succeed(r.stdout)
        : Effect.fail(new BluetoothError({ message: r.stderr.trim() || `blueutil exited with ${r.exitCode}` }))
    )
  )

const list = (bin: string, args: ReadonlyArray<string>) =>
  blueutil(bin, [...args, "--format", "json"]).pipe(
    Effect.flatMap((out) => decodeDevices(out.trim() || "[]")),
    Effect.mapError((e) => e instanceof BluetoothError ? e : new BluetoothError({ message: String(e) }))
  )

// MARK: Remembered devices

/**
 * Every device connected while the spell was open, in `history.json`, so it can
 * be offered for a quick reconnect when it comes back in range.
 */
const historyFile = Config.String("CANTRIP_BLUETOOTH_HISTORY").pipe(
  Config.withDefault(`${homedir()}/Library/Application Support/Cantrip/bluetooth/history.json`)
)

const Remembered = Schema.Struct({
  address: Schema.String,
  name: Schema.NullOr(Schema.String),
  lastConnected: Schema.Number
})
type Remembered = typeof Remembered.Type
const HistoryJson = Schema.fromJsonString(Schema.Array(Remembered))
type History = ReadonlyMap<string, Remembered>

const loadHistory: Effect.Effect<History, never, FileSystem.FileSystem> = Effect.gen(function*() {
  const fs = yield* FileSystem.FileSystem
  const file = yield* historyFile
  if (!(yield* fs.exists(file))) return new Map()
  const entries = yield* Schema.decodeUnknownEffect(HistoryJson)(yield* fs.readFileString(file))
  return new Map(entries.map((r) => [r.address, r]))
}).pipe(
  Effect.catch((e) => Effect.as(Effect.logWarning("Ignoring unreadable Bluetooth history", String(e)), new Map()))
)

/** Written to a temp file and renamed, so a crash never leaves half a file. */
const saveHistory = (history: History) =>
  Effect.gen(function*() {
    const fs = yield* FileSystem.FileSystem
    const path = yield* Path.Path
    const file = yield* historyFile
    yield* fs.makeDirectory(path.dirname(file), { recursive: true })
    const text = yield* Schema.encodeEffect(HistoryJson)([...history.values()])
    yield* fs.writeFileString(`${file}.tmp`, text)
    yield* fs.rename(`${file}.tmp`, file)
  }).pipe(Effect.catch((e) => Effect.logWarning("Couldn't save Bluetooth history", String(e))))

const remember = (history: History, devices: ReadonlyArray<Device>, now: number): History => {
  const next = new Map(history)
  for (const d of devices) {
    next.set(d.address, { address: d.address, name: d.name ?? history.get(d.address)?.name ?? null, lastConnected: now })
  }
  return next
}

const ago = (then: number, now: number) => {
  const seconds = Math.round((then - now) / 1000)
  const format = new Intl.RelativeTimeFormat("en", { numeric: "auto" })
  for (const [unit, size] of [["day", 86400], ["hour", 3600], ["minute", 60]] as const) {
    if (Math.abs(seconds) >= size) return format.format(Math.round(seconds / size), unit)
  }
  return "just now"
}

// MARK: Connecting

type Notify = (toast: Ui.Toast) => Effect.Effect<void>

/** Pairs `d` first if it isn't yet, then connects. `stage` hears each step begin. */
const pairAndConnect = (bin: string, d: Device, notify: Notify, stage: (busy: Busy) => Effect.Effect<void>) =>
  Effect.gen(function*() {
    const name = nameOf(d)
    if (!d.paired) {
      yield* stage("Pairing…")
      yield* blueutil(bin, ["--pair", d.address, LEGACY_PIN], (line, answer) => {
        // Numeric comparison: say yes, and show the number so it can be checked.
        const confirm = /display number (\d+)/.exec(line)
        if (confirm) {
          return Effect.andThen(
            notify({ title: `Pairing code ${confirm[1]}`, message: `It should match the one on ${name}` }),
            answer("yes\n")
          )
        }
        const passkey = /Input (?:passkey|pin) (\S+)/i.exec(line)
        if (passkey) {
          return notify({ title: `Type ${passkey[1]} on ${name}`, message: "then press Return on it" })
        }
        return Effect.void
      })
    }
    yield* stage("Connecting…")
    yield* blueutil(bin, ["--connect", d.address])
  })

// MARK: The list

type Status = "new" | "paired" | "connected"
type Busy = "Pairing…" | "Connecting…" | "Disconnecting…"

interface State {
  readonly devices: ReadonlyMap<string, Device>
  /** Addresses found by a scan since the list opened: in range now. */
  readonly nearby: ReadonlySet<string>
  readonly history: History
  readonly busy: ReadonlyMap<string, Busy>
  readonly scanning: boolean
}

const status = (d: Device): Status => {
  if (d.connected) return "connected"
  return d.paired ? "paired" : "new"
}
const label: Record<Status, string> = { new: "New", paired: "Paired", connected: "Connected" }
const nameOf = (d: Pick<Device, "name">) => d.name?.trim() || "Unnamed device"

/** Where a device sits in the list: remembered ones back in range first. */
const rank = (d: Device, s: State) => {
  const remembered = s.history.has(d.address)
  if (d.connected) return 4
  if (remembered && s.nearby.has(d.address)) return 0
  if (!d.paired && s.nearby.has(d.address)) return 1
  if (remembered) return d.paired ? 2 : 5
  return 3
}

/** A best guess from the name; blueutil doesn't report the device class. */
const iconFor = (d: Pick<Device, "name">) => {
  const n = (d.name ?? "").toLowerCase()
  if (/airpods max/.test(n)) return "airpods.max"
  if (/airpods pro/.test(n)) return "airpodspro"
  if (/airpods/.test(n)) return "airpods"
  if (/beats|headphone|buds|wh-|wf-|bose|qc|jabra|sony/.test(n)) return "headphones"
  if (/speaker|homepod|soundlink|jbl|ue boom|sonos|soundbar/.test(n)) return "hifispeaker"
  if (/keyboard/.test(n)) return "keyboard"
  if (/mouse/.test(n)) return "computermouse"
  if (/trackpad/.test(n)) return "rectangle.and.hand.point.up.left"
  if (/controller|dualsense|dualshock|xbox|joy-con|gamepad/.test(n)) return "gamecontroller"
  if (/iphone|phone|pixel|galaxy/.test(n)) return "iphone"
  if (/ipad/.test(n)) return "ipad"
  if (/watch/.test(n)) return "applewatch"
  if (/mac|book/.test(n)) return "laptopcomputer"
  return "dot.radiowaves.left.and.right"
}

type Services = Ui.Ui | ChildProcessSpawner.ChildProcessSpawner | FileSystem.FileSystem | Path.Path

const browse = (bin: string): Effect.Effect<void, never, Services> =>
  Effect.scoped(Effect.gen(function*() {
    const ui = yield* Ui.Ui
    const fs = yield* FileSystem.FileSystem
    const path = yield* Path.Path
    const provide = <A, E>(effect: Effect.Effect<A, E, FileSystem.FileSystem | Path.Path>) =>
      effect.pipe(Effect.provideService(FileSystem.FileSystem, fs), Effect.provideService(Path.Path, path))

    const history = yield* provide(loadHistory)
    // Remembered devices are listed even before a scan finds them.
    const devices = new Map<string, Device>(
      [...history.values()].map((r) => [r.address, { ...r, paired: false, connected: false }])
    )
    const state = yield* Ref.make<State>({ devices, nearby: new Set(), history, busy: new Map(), scanning: false })
    const scanner = yield* FiberHandle.make()

    const render = Effect.gen(function*() {
      const s = yield* Ref.get(state)
      const now = Date.now()
      const sorted = [...s.devices.values()].sort((a, b) =>
        rank(a, s) - rank(b, s)
        || (s.history.get(b.address)?.lastConnected ?? 0) - (s.history.get(a.address)?.lastConnected ?? 0)
        || Number(a.name === null) - Number(b.name === null)
        || nameOf(a).localeCompare(nameOf(b))
      )
      yield* ui.render({
        _tag: "List",
        placeholder: s.scanning ? "Scanning for devices…" : "Search devices…",
        emptyText: s.scanning
          ? "Looking for devices. Put yours in pairing mode."
          : "No devices found",
        items: sorted.map((d) => item(d, s, now))
      })
    })

    const update = (f: (s: State) => State) => Effect.andThen(Ref.update(state, f), render)
    const setBusy = (address: string, busy: Busy | undefined) =>
      update((s) => {
        const next = new Map(s.busy)
        if (busy) next.set(address, busy)
        else next.delete(address)
        return { ...s, busy: next }
      })

    /**
     * Merges devices in. Paired lists are the truth for pairing and connection;
     * anything connected is remembered, however it got connected.
     */
    const merge = (found: ReadonlyArray<Device>, options?: { readonly nearby?: boolean }) =>
      Effect.gen(function*() {
        const before = yield* Ref.get(state)
        const devices = new Map(before.devices)
        for (const d of found) {
          const known = devices.get(d.address)
          devices.set(d.address, { ...d, name: d.name ?? known?.name ?? null })
        }
        const connected = found.filter((d) => d.connected)
        const history = connected.length > 0 ? remember(before.history, connected, Date.now()) : before.history
        const nearby = options?.nearby ? new Set([...before.nearby, ...found.map((d) => d.address)]) : before.nearby
        yield* update((s) => ({ ...s, devices, history, nearby }))
        if (history !== before.history) yield* provide(saveHistory(history))

        const back = found.filter((d) =>
          options?.nearby && !d.connected && before.history.has(d.address) && !before.nearby.has(d.address)
        )
        const first = back[0]
        if (first) {
          yield* ui.toast({
            style: "success",
            title: back.length === 1 ? `${nameOf(first)} is back` : `${back.length} devices you've used are back`,
            message: "Reconnect from the top of the list"
          })
        }
      })
    const refreshPaired = list(bin, ["--paired"]).pipe(Effect.flatMap((found) => merge(found)))

    const scanForever = Effect.gen(function*() {
      yield* update((s) => ({ ...s, scanning: true }))
      yield* ui.setLoading(true)
      yield* list(bin, ["--inquiry", String(SCAN_SECONDS)]).pipe(
        Effect.flatMap((found) => merge(found, { nearby: true })),
        Effect.andThen(refreshPaired),
        Effect.forever
      )
    }).pipe(
      Effect.catch((e) => ui.toast({ style: "failure", title: "Scan failed", message: e.message })),
      Effect.ensuring(Effect.andThen(ui.setLoading(false), update((s) => ({ ...s, scanning: false }))))
    )
    const startScanning = FiberHandle.run(scanner, scanForever)

    /**
     * Bluetooth won't pair or connect well while an inquiry runs, so stop
     * scanning for the length of `effect`, then pick it back up.
     */
    const withoutScanning = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
      FiberHandle.clear(scanner).pipe(Effect.andThen(effect), Effect.ensuring(startScanning))

    const connect = (d: Device) =>
      withoutScanning(Effect.gen(function*() {
        yield* pairAndConnect(bin, d, ui.toast, (busy) => setBusy(d.address, busy))
        yield* refreshPaired
        yield* ui.hud(`Connected to ${nameOf(d)}`)
      }).pipe(
        Effect.catch((e) =>
          Effect.andThen(
            ui.toast({ style: "failure", title: `Couldn't connect to ${nameOf(d)}`, message: e.message }),
            refreshPaired.pipe(Effect.ignore)
          )
        ),
        Effect.ensuring(setBusy(d.address, undefined))
      ))

    const disconnect = (d: Device) =>
      withoutScanning(Effect.gen(function*() {
        yield* setBusy(d.address, "Disconnecting…")
        yield* blueutil(bin, ["--disconnect", d.address])
        yield* refreshPaired
        yield* ui.toast({ style: "success", title: `Disconnected ${nameOf(d)}` })
      }).pipe(
        Effect.catch((e) => ui.toast({ style: "failure", title: `Couldn't disconnect ${nameOf(d)}`, message: e.message })),
        Effect.ensuring(setBusy(d.address, undefined))
      ))

    /** Drops it from history. A device that isn't paired or nearby leaves the list too. */
    const forget = (d: Device) =>
      Effect.gen(function*() {
        const s = yield* Ref.get(state)
        const history = new Map(s.history)
        history.delete(d.address)
        const devices = new Map(s.devices)
        if (!d.paired && !s.nearby.has(d.address)) devices.delete(d.address)
        yield* update((s) => ({ ...s, history, devices }))
        yield* provide(saveHistory(history))
        yield* ui.toast({ title: `Forgot ${nameOf(d)}` })
      })

    const rescan = Action.run("Scan Again", startScanning, { icon: "arrow.clockwise" })

    function item(d: Device, s: State, now: number): View.ListItem {
      const remembered = s.history.get(d.address)
      const nearby = s.nearby.has(d.address)
      const busy = s.busy.get(d.address)
      let main: Action.Action
      let tag: string
      if (d.connected) {
        main = Action.run("Disconnect", disconnect(d), { icon: "xmark.circle" })
        tag = label.connected
      } else if (remembered) {
        main = Action.run("Reconnect", connect(d), { icon: "arrow.triangle.2.circlepath" })
        if (nearby) tag = "Back in range"
        else tag = d.paired ? "Paired" : "Not nearby"
      } else {
        main = Action.run(d.paired ? "Connect" : "Pair and Connect", connect(d), { icon: "link" })
        tag = label[status(d)]
      }
      return {
        id: d.address,
        title: nameOf(d),
        subtitle: remembered && !d.connected
          ? `Last connected ${ago(remembered.lastConnected, now)}`
          : d.address,
        icon: iconFor(d),
        accessories: [busy ?? tag],
        keywords: ["bluetooth", tag.toLowerCase(), ...(remembered ? ["reconnect", "recent"] : []), d.address],
        actions: [
          main,
          Action.copy("Copy Address", d.address),
          ...(remembered ? [Action.run("Forget", forget(d), { icon: "trash" })] : []),
          rescan
        ]
      }
    }

    yield* render
    yield* ensurePower(bin)
    yield* refreshPaired.pipe(
      Effect.catch((e) => ui.toast({ style: "failure", title: "Couldn't list devices", message: e.message }))
    )
    yield* startScanning
    yield* Effect.never
  }))

const ensurePower = (bin: string) =>
  Effect.gen(function*() {
    const power = yield* blueutil(bin, ["--power"])
    if (power.trim() === "1") return
    yield* blueutil(bin, ["--power", "1"])
    yield* Ui.toast({ style: "success", title: "Bluetooth turned on" })
  }).pipe(Effect.catch((e) => Ui.toast({ style: "failure", title: "Couldn't turn Bluetooth on", message: e.message })))

// MARK: Quick reconnect

/**
 * Reconnects the device you used most recently that isn't connected now,
 * without showing the list. Paired ones are tried first, newest first; it moves
 * on when one isn't around.
 */
const reconnectLast = (bin: string) =>
  Effect.gen(function*() {
    const history = yield* loadHistory
    if (history.size === 0) {
      return yield* Ui.hud("No Bluetooth devices to reconnect yet")
    }
    yield* ensurePower(bin)
    const paired = new Map((yield* list(bin, ["--paired"])).map((d) => [d.address, d]))
    const candidates = [...history.values()]
      .filter((r) => !paired.get(r.address)?.connected)
      .sort((a, b) => b.lastConnected - a.lastConnected)
      .slice(0, 3)
    if (candidates.length === 0) return yield* Ui.hud("Everything you've used is connected")
    const ui = yield* Ui.Ui
    const notify: Notify = (t) => ui.hud(t.message ? `${t.title}: ${t.message}` : t.title)
    for (const r of candidates) {
      const d = paired.get(r.address) ?? { ...r, paired: false, connected: false }
      const ok = yield* pairAndConnect(bin, d, notify, () => Effect.void).pipe(
        Effect.as(true),
        Effect.orElseSucceed(() => false)
      )
      if (ok) {
        yield* saveHistory(remember(history, [d], Date.now()))
        return yield* Ui.hud(`Connected to ${nameOf(d)}`)
      }
    }
    yield* Ui.hud(`Couldn't reach ${candidates.map(nameOf).join(", ")}`)
  }).pipe(Effect.catch((e) => Ui.hud(`Bluetooth: ${e.message}`)))

// MARK: Without blueutil

const missing = (then: (bin: string) => Effect.Effect<void, never, Services>) =>
  Effect.gen(function*() {
    const brew = yield* find(BREW)
    const install = brew
      ? [Action.run(
        "Install with Homebrew",
        Effect.gen(function*() {
          yield* Ui.setLoading(true)
          yield* Ui.toast({ title: "Installing blueutil…" })
          const result = yield* exec(brew, ["install", "blueutil"]).pipe(Effect.ensuring(Ui.setLoading(false)))
          const bin = yield* find(BLUEUTIL)
          if (result.exitCode !== 0 || !bin) {
            return yield* Ui.toast({ style: "failure", title: "Install failed", message: result.stderr.trim() })
          }
          yield* then(bin)
        }).pipe(Effect.catch((e) => Ui.toast({ style: "failure", title: "Install failed", message: e.message }))),
        { icon: "arrow.down.circle" }
      )]
      : []
    yield* Ui.detail({
      markdown: [
        "# Bluetooth needs blueutil",
        "",
        "This spell talks to macOS's Bluetooth through [blueutil](https://github.com/toy/blueutil), a small command line tool.",
        "",
        brew ? "Press ↵ to install it with Homebrew." : "Install it with Homebrew:",
        "",
        "```sh",
        "brew install blueutil",
        "```"
      ].join("\n"),
      actions: [
        ...install,
        Action.copy("Copy Install Command", "brew install blueutil"),
        Action.open("Open blueutil on GitHub", "https://github.com/toy/blueutil")
      ]
    })
  })

export default Spell.make({
  title: "Connect Bluetooth Device",
  subtitle: "Pair and connect headphones, speakers, keyboards…",
  icon: "dot.radiowaves.left.and.right",
  keywords: ["bluetooth", "pair", "headphones", "airpods", "speaker", "keyboard", "mouse", "audio"],
  run: Effect.gen(function*() {
    const bin = yield* find(BLUEUTIL)
    yield* bin ? browse(bin) : missing(browse)
  })
})

export const reconnect = Spell.make({
  title: "Reconnect Bluetooth Device",
  subtitle: "Connect the device you used last",
  icon: "arrow.triangle.2.circlepath",
  keywords: ["bluetooth", "reconnect", "headphones", "airpods", "speaker", "last", "recent"],
  run: Effect.gen(function*() {
    const bin = yield* find(BLUEUTIL)
    if (!bin) return yield* missing(reconnectLast)
    yield* Ui.hide
    yield* reconnectLast(bin)
  })
})
