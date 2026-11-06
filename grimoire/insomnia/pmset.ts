// macOS adapters for SleepControl and BatteryMonitor, plus the pure pmset / `sudo -l` parsers.
// Only ever writes `sudo -n /usr/bin/pmset -a disablesleep 0|1`: that's all the sudoers rule allows.
import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"
import * as ChildProcess from "effect/process/ChildProcess"
import { ChildProcessSpawner } from "effect/process/ChildProcessSpawner"
import * as Stream from "effect/Stream"
import { type Battery, PowerCommandFailed, type Privileges, PrivilegesMissing } from "./model.ts"
import { BatteryMonitor, SleepControl } from "./service.ts"

export const PMSET = "/usr/bin/pmset"
export const SUDO = "/usr/bin/sudo"

/** The exact argument lists the sudoers rule allows. Keep in sync with `sudoersRule`. */
export const disableSleepArgs = (disabled: boolean) => ["-a", "disablesleep", disabled ? "1" : "0"] as const

// Process helper

export interface ExecResult {
  readonly exitCode: number
  readonly stdout: string
  readonly stderr: string
}

/** Runs a command to completion and collects its output. Never uses a shell. */
export const exec = Effect.fn("insomnia.exec")(function*(command: string, args: ReadonlyArray<string>) {
  const spawner = yield* ChildProcessSpawner
  return yield* Effect.scoped(
    Effect.gen(function*() {
      const handle = yield* spawner.spawn(ChildProcess.make(command, [...args]))
      const [stdout, stderr, exitCode] = yield* Effect.all(
        [
          handle.stdout.pipe(Stream.decodeText, Stream.mkString),
          handle.stderr.pipe(Stream.decodeText, Stream.mkString),
          handle.exitCode
        ],
        { concurrency: "unbounded" }
      )
      const result: ExecResult = { exitCode, stdout, stderr }
      return result
    })
  )
})

export const describe = (command: string, args: ReadonlyArray<string>) => [command, ...args].join(" ")

// Parsers (pure, checked against real pmset output in _test.ts)

/** Reads `SleepDisabled` from `pmset -g`. A missing line means sleep is enabled. */
export const parseSleepDisabled = (pmsetG: string): boolean => {
  const match = /^\s*SleepDisabled\s+(\d+)\s*$/m.exec(pmsetG)
  return match !== null && match[1] !== "0"
}

/** Reads the internal battery from `pmset -g batt`. `null` when the Mac has none. */
export const parseBattery = (pmsetBatt: string): Battery | null => {
  const percent = /InternalBattery-\d+.*?\t(\d{1,3})%/.exec(pmsetBatt)
  if (percent?.[1] === undefined) return null
  const value = Number(percent[1])
  if (!Number.isInteger(value) || value < 0 || value > 100) return null
  return { percent: value, onAC: /drawing from 'AC Power'/.test(pmsetBatt) }
}

/**
 * True when `sudo -n -l` lists a NOPASSWD rule covering both exact disablesleep commands.
 * `sudo -l <cmd>` alone isn't enough: it succeeds for `(ALL) ALL`, which still asks for a password.
 * Without a terminal, sudo wraps rules at 80 columns onto deeper-indented lines, so rules are
 * rejoined first: a rule starts with `(runas)`, anything else continues the previous rule.
 */
export const parseSudoList = (sudoList: string): boolean => {
  const lines = sudoList.split("\n")
  const header = lines.findIndex((line) => line.includes("may run the following commands"))
  const rules: Array<string> = []
  for (const line of header === -1 ? [] : lines.slice(header + 1)) {
    const trimmed = line.trim()
    if (trimmed.length === 0) continue
    if (trimmed.startsWith("(") || rules.length === 0) rules.push(trimmed)
    else rules[rules.length - 1] += ` ${trimmed}`
  }
  const passwordless = rules
    .flatMap((line) => {
      const index = line.indexOf("NOPASSWD:")
      return index === -1 ? [] : line.slice(index + "NOPASSWD:".length).split(",")
    })
    .map((command) => command.trim().replace(/\s+/g, " "))
  const covered = (command: string) => passwordless.includes(command) || passwordless.includes("ALL")
  return [true, false].every((disabled) => covered([PMSET, ...disableSleepArgs(disabled)].join(" ")))
}

const isPasswordRequired = (stderr: string) =>
  /password is required|a terminal is required|not allowed to execute|may not run sudo/i.test(stderr)

// Adapters

export const sleepControlLayer = Layer.effect(
  SleepControl,
  Effect.gen(function*() {
    const spawner = yield* ChildProcessSpawner
    const run = (command: string, args: ReadonlyArray<string>) =>
      exec(command, args).pipe(
        Effect.provideService(ChildProcessSpawner, spawner),
        Effect.mapError((error) => new PowerCommandFailed({ command: describe(command, args), detail: error.message }))
      )

    const isDisabled = Effect.gen(function*() {
      const result = yield* run(PMSET, ["-g"])
      if (result.exitCode !== 0) {
        return yield* new PowerCommandFailed({
          command: "pmset -g",
          detail: result.stderr.trim() || `exit ${result.exitCode}`
        })
      }
      return parseSleepDisabled(result.stdout)
    })

    const setDisabled = Effect.fn("SleepControl.setDisabled")(function*(disabled: boolean) {
      const args = ["-n", PMSET, ...disableSleepArgs(disabled)]
      const result = yield* run(SUDO, args)
      if (result.exitCode === 0) return
      if (isPasswordRequired(result.stderr)) return yield* new PrivilegesMissing()
      return yield* new PowerCommandFailed({
        command: describe("sudo", args),
        detail: result.stderr.trim() || `exit ${result.exitCode}`
      })
    })

    const privileges: Effect.Effect<Privileges> = run(SUDO, ["-n", "-l"]).pipe(
      Effect.map((result): Privileges => result.exitCode === 0 && parseSudoList(result.stdout) ? "Granted" : "Missing"),
      Effect.orElseSucceed((): Privileges => "Missing")
    )

    return SleepControl.of({ isDisabled, setDisabled, privileges })
  })
)

export const batteryMonitorLayer = Layer.effect(
  BatteryMonitor,
  Effect.gen(function*() {
    const spawner = yield* ChildProcessSpawner
    const read = exec(PMSET, ["-g", "batt"]).pipe(
      Effect.provideService(ChildProcessSpawner, spawner),
      Effect.map((result) => (result.exitCode === 0 ? parseBattery(result.stdout) : null)),
      Effect.tapError((error) => Effect.logWarning("Couldn't read battery", error)),
      Effect.orElseSucceed((): Battery | null => null)
    )
    return BatteryMonitor.of({ read })
  })
)
