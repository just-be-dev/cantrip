// Find Bluetooth devices nearby and connect to them: headphones, speakers, keyboards.
// The list opens with your paired devices and keeps scanning while it's up, so a
// device in pairing mode shows up within a few seconds. ↵ pairs (if it's new) and
// connects; on a connected device it disconnects.
//
// It remembers every device connected while it's open. When one comes back in
// range it goes to the top as "Back in range" with ↵ to Reconnect, and
// Reconnect Bluetooth Device reconnects the last one without the list.
//
// Nothing to install: it talks to macOS's IOBluetooth framework through
// JavaScript for Automation (`osascript -l JavaScript`, part of macOS). The first
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

/** How long each scan listens before the list updates. */
const SCAN_SECONDS = 5
/** How long to wait for a device to answer a connection. */
const CONNECT_SECONDS = 15
/** How long to wait for pairing, which may need a code typed on the device. */
const PAIR_SECONDS = 60

// MARK: IOBluetooth

/**
 * Runs in `osascript -l JavaScript` with IOBluetooth bridged in. Results go to
 * stdout; `console.log` goes to stderr, which carries pairing prompts as they
 * come (`confirm N`, `passkey N`). A thrown error exits non-zero.
 */
const HELPER = String.raw`
ObjC.import("IOBluetooth")
ObjC.import("Foundation")

function run(argv) {
  const [command, arg, seconds] = argv
  const spin = (s) => $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(s))
  const waitFor = (done, s) => {
    const until = Date.now() + s * 1000
    while (!done() && Date.now() < until) spin(0.1)
    return done()
  }
  const describe = (d) => ({
    address: ObjC.unwrap(d.addressString),
    // Unnamed devices report their address as their name.
    name: /^([0-9a-f]{2}[:-]){5}[0-9a-f]{2}$/i.test(ObjC.unwrap(d.name) ?? "") ? null : ObjC.unwrap(d.name) ?? null,
    paired: d.isPaired,
    connected: d.isConnected
  })
  const list = (devices) => {
    const out = []
    for (let i = 0; i < devices.count; i++) out.push(describe(devices.objectAtIndex(i)))
    return JSON.stringify(out)
  }
  const device = () => {
    const d = $.IOBluetoothDevice.deviceWithAddressString(arg)
    if (!d || d.isNil()) throw new Error("No device " + arg)
    return d
  }

  switch (command) {
    case "power":
      return String($.IOBluetoothHostController.defaultController.powerState)
    case "power-on": {
      ObjC.bindFunction("IOBluetoothPreferenceSetControllerPowerState", ["void", ["int"]])
      $.IOBluetoothPreferenceSetControllerPowerState(1)
      if (!waitFor(() => $.IOBluetoothHostController.defaultController.powerState === 1, 5)) {
        throw new Error("Bluetooth didn't turn on")
      }
      return ""
    }
    case "paired":
      return list($.IOBluetoothDevice.pairedDevices)
    case "inquiry": {
      const inquiry = $.IOBluetoothDeviceInquiry.inquiryWithDelegate(null)
      inquiry.updateNewDeviceNames = true
      inquiry.start
      spin(Number(seconds))
      inquiry.stop
      return list(inquiry.foundDevices)
    }
    case "connect": {
      const d = device()
      if (d.isConnected) return ""
      const result = d.openConnection(null)
      if (result !== 0) throw new Error("Couldn't start a connection (" + result + ")")
      if (!waitFor(() => d.isConnected, Number(seconds))) throw new Error("It didn't answer. Is it on and nearby?")
      return ""
    }
    case "disconnect": {
      const d = device()
      if (!d.isConnected) return ""
      d.closeConnection
      if (!waitFor(() => !d.isConnected, 10)) throw new Error("It's still connected")
      return ""
    }
    case "pair": {
      const d = device()
      if (d.isPaired) return ""
      let finished = null
      ObjC.registerSubclass({
        name: "CantripPairing",
        methods: {
          // Numeric comparison: accept, and report the number so it can be checked.
          "devicePairingUserConfirmationRequest:numericValue:": {
            types: ["void", ["id", "unsigned int"]],
            implementation: (sender, n) => {
              console.log("confirm " + String(n).padStart(6, "0"))
              sender.replyUserConfirmation(true)
            }
          },
          "devicePairingUserPasskeyNotification:passkey:": {
            types: ["void", ["id", "unsigned int"]],
            implementation: (sender, n) => console.log("passkey " + String(n).padStart(6, "0"))
          },
          "devicePairingPINCodeRequest:": {
            types: ["void", ["id"]],
            implementation: (sender) => {
              finished = "This device wants a PIN code; pair it in System Settings"
              sender.stop
            }
          },
          "devicePairingFinished:error:": {
            types: ["void", ["id", "int"]],
            implementation: (sender, error) => {
              finished = error === 0 ? "ok" : "Pairing failed (" + error + ")"
            }
          }
        }
      })
      const pairing = $.IOBluetoothDevicePair.pairWithDevice(d)
      pairing.delegate = $.CantripPairing.alloc.init
      if (pairing.start !== 0) throw new Error("Couldn't start pairing")
      waitFor(() => finished !== null || d.isPaired, Number(seconds))
      pairing.stop
      if (d.isPaired) return ""
      throw new Error(finished && finished !== "ok" ? finished : "It didn't pair. Is it in pairing mode?")
    }
    default:
      throw new Error("Unknown command " + command)
  }
}
`

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

/** osascript reports a thrown error as `…: execution error: Error: Error: <message> (-2700)`. */
const errorMessage = (stderr: string) => {
  const line = stderr.split("\n").findLast((l) => l.includes("execution error")) ?? stderr
  return line.replace(/^.*execution error: (?:Error: )*/, "").replace(/ \(-?\d+\)$/, "").trim()
}

/**
 * Runs one helper command to completion and returns its stdout. `onStderr`
 * sees each line it logs as it comes (pairing prompts).
 */
const bluetooth = (args: ReadonlyArray<string>, onStderr?: (line: string) => Effect.Effect<void>) =>
  Effect.scoped(Effect.gen(function*() {
    const spawner = yield* ChildProcessSpawner.ChildProcessSpawner
    const handle = yield* spawner.spawn(ChildProcess.make("/usr/bin/osascript", ["-l", "JavaScript", "-e", HELPER, ...args]))
    const lines: Array<string> = []
    const [stdout, , exitCode] = yield* Effect.all([
      handle.stdout.pipe(Stream.decodeText, Stream.mkString),
      handle.stderr.pipe(
        Stream.decodeText,
        Stream.splitLines,
        Stream.runForEach((line) => {
          lines.push(line)
          return onStderr ? onStderr(line) : Effect.void
        })
      ),
      handle.exitCode
    ], { concurrency: "unbounded" })
    if (exitCode === 0) return stdout
    return yield* new BluetoothError({ message: errorMessage(lines.join("\n")) || `osascript exited with ${exitCode}` })
  })).pipe(Effect.catchTag("PlatformError", (e) => Effect.fail(new BluetoothError({ message: e.message }))))

const list = (args: ReadonlyArray<string>) =>
  bluetooth(args).pipe(
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
const pairAndConnect = (d: Device, notify: Notify, stage: (busy: Busy) => Effect.Effect<void>) =>
  Effect.gen(function*() {
    const name = nameOf(d)
    if (!d.paired) {
      yield* stage("Pairing…")
      yield* bluetooth(["pair", d.address, String(PAIR_SECONDS)], (line) => {
        const confirm = /^confirm (\d+)/.exec(line)
        if (confirm) {
          return notify({ title: `Pairing code ${confirm[1]}`, message: `It should match the one on ${name}` })
        }
        const passkey = /^passkey (\d+)/.exec(line)
        if (passkey) {
          return notify({ title: `Type ${passkey[1]} on ${name}`, message: "then press Return on it" })
        }
        return Effect.void
      })
    }
    yield* stage("Connecting…")
    yield* bluetooth(["connect", d.address, String(CONNECT_SECONDS)])
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

/** A best guess from the name; the Bluetooth device class is too coarse to pick an icon. */
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

const browse: Effect.Effect<void, never, Services> =
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
    const refreshPaired = list(["paired"]).pipe(Effect.flatMap((found) => merge(found)))

    const scanForever = Effect.gen(function*() {
      yield* update((s) => ({ ...s, scanning: true }))
      yield* ui.setLoading(true)
      yield* list(["inquiry", "", String(SCAN_SECONDS)]).pipe(
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
        yield* pairAndConnect(d, ui.toast, (busy) => setBusy(d.address, busy))
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
        yield* bluetooth(["disconnect", d.address])
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
    yield* ensurePower
    yield* refreshPaired.pipe(
      Effect.catch((e) => ui.toast({ style: "failure", title: "Couldn't list devices", message: e.message }))
    )
    yield* startScanning
    yield* Effect.never
  }))

const ensurePower =
  Effect.gen(function*() {
    const power = yield* bluetooth(["power"])
    if (power.trim() === "1") return
    yield* bluetooth(["power-on"])
    yield* Ui.toast({ style: "success", title: "Bluetooth turned on" })
  }).pipe(Effect.catch((e) => Ui.toast({ style: "failure", title: "Couldn't turn Bluetooth on", message: e.message })))

// MARK: Quick reconnect

/**
 * Reconnects the device you used most recently that isn't connected now,
 * without showing the list. Paired ones are tried first, newest first; it moves
 * on when one isn't around.
 */
const reconnectLast =
  Effect.gen(function*() {
    const history = yield* loadHistory
    if (history.size === 0) {
      return yield* Ui.hud("No Bluetooth devices to reconnect yet")
    }
    yield* ensurePower
    const paired = new Map((yield* list(["paired"])).map((d) => [d.address, d]))
    const candidates = [...history.values()]
      .filter((r) => !paired.get(r.address)?.connected)
      .sort((a, b) => b.lastConnected - a.lastConnected)
      .slice(0, 3)
    if (candidates.length === 0) return yield* Ui.hud("Everything you've used is connected")
    const ui = yield* Ui.Ui
    const notify: Notify = (t) => ui.hud(t.message ? `${t.title}: ${t.message}` : t.title)
    for (const r of candidates) {
      const d = paired.get(r.address) ?? { ...r, paired: false, connected: false }
      const ok = yield* pairAndConnect(d, notify, () => Effect.void).pipe(
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

export default Spell.make({
  title: "Connect Bluetooth Device",
  subtitle: "Pair and connect headphones, speakers, keyboards…",
  icon: "dot.radiowaves.left.and.right",
  keywords: ["bluetooth", "pair", "headphones", "airpods", "speaker", "keyboard", "mouse", "audio"],
  run: browse
})

export const reconnect = Spell.make({
  title: "Reconnect Bluetooth Device",
  subtitle: "Connect the device you used last",
  icon: "arrow.triangle.2.circlepath",
  keywords: ["bluetooth", "reconnect", "headphones", "airpods", "speaker", "last", "recent"],
  mode: "no-view",
  run: reconnectLast
})
