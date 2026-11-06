// Checks for the pure parsers (real pmset / sudo output, ported from Insomnia's pmset.test.ts),
// the menu formatting, and the session's safety rules against an in-memory Mac.
// Run from the repo root: `mise exec -- bun test ./grimoire/insomnia/_test.ts`
// The leading `_` keeps Cantrip from loading it; it needs nothing beyond bun.
import { describe, expect, test } from "bun:test"
import * as Effect from "effect/Effect"
import * as Layer from "effect/Layer"
import { parseDuration } from "./cli.ts"
import { longRemaining, shortRemaining } from "./menu.ts"
import {
  type Battery,
  initialPersistedState,
  type PersistedState,
  PowerCommandFailed,
  type Privileges,
  PrivilegesMissing
} from "./model.ts"
import { parseBattery, parseSleepDisabled, parseSudoList } from "./pmset.ts"
import { sudoersRule } from "./privileges.ts"
import { BatteryMonitor, SleepControl, StateStore } from "./service.ts"
import { SessionManager } from "./session.ts"

const pmsetG = (sleepDisabledLine: string) =>
  [
    "System-wide power settings:",
    sleepDisabledLine,
    "Currently in use:",
    " standby              1",
    " Sleep On Power Button 1",
    " hibernatefile        /var/vm/sleepimage",
    " powernap             1",
    " networkoversleep     0",
    " disksleep            10",
    " sleep                1 (sleep prevented by powerd, Amphetamine)",
    " hibernatemode        3",
    " ttyskeepawake        1",
    " displaysleep         2",
    " tcpkeepalive         1",
    " lowpowermode         0",
    " womp                 0"
  ].join("\n")

describe("parseSleepDisabled", () => {
  test("reads SleepDisabled 1 and 0", () => {
    expect(parseSleepDisabled(pmsetG(" SleepDisabled\t\t1"))).toBe(true)
    expect(parseSleepDisabled(pmsetG(" SleepDisabled\t\t0"))).toBe(false)
  })
  test("treats a missing SleepDisabled line as normal sleep", () => {
    expect(parseSleepDisabled(pmsetG(""))).toBe(false)
  })
  test("isn't fooled by other settings containing 'sleep 1'", () => {
    expect(parseSleepDisabled(pmsetG(" displaysleep 1"))).toBe(false)
  })
})

describe("parseBattery", () => {
  test("reads a discharging laptop", () => {
    expect(
      parseBattery(
        "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=22085731)\t95%; discharging; 6:59 remaining present: true\n"
      )
    ).toEqual({ percent: 95, onAC: false })
  })
  test("reads a laptop on AC, including 100%", () => {
    expect(
      parseBattery(
        "Now drawing from 'AC Power'\n -InternalBattery-0 (id=22085731)\t100%; charged; 0:00 remaining present: true\n"
      )
    ).toEqual({ percent: 100, onAC: true })
    expect(
      parseBattery("Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t7%; charging; (no estimate) present: true\n")
    ).toEqual({ percent: 7, onAC: true })
  })
  test("returns null on a Mac without a battery", () => {
    expect(parseBattery("Now drawing from 'AC Power'\n")).toBeNull()
  })
  test("ignores UPS and accessory batteries", () => {
    expect(parseBattery("Now drawing from 'AC Power'\n -CP1500 (id=1)\t100%; charging present: true\n")).toBeNull()
  })
})

describe("sudoersRule", () => {
  test("allows exactly the two disablesleep commands", () => {
    const rule = sudoersRule("just-be")
    expect(rule).toContain(
      "just-be ALL=(root) NOPASSWD: /usr/bin/pmset -a disablesleep 0, /usr/bin/pmset -a disablesleep 1"
    )
    expect(rule.split("\n").filter((line) => line.includes("NOPASSWD"))).toHaveLength(1)
  })
  test("refuses usernames that could inject sudoers syntax", () => {
    expect(() => sudoersRule("evil ALL=(ALL) NOPASSWD: ALL #")).toThrow()
    expect(() => sudoersRule("a,b")).toThrow()
    expect(() => sudoersRule("")).toThrow()
  })
})

describe("parseSudoList", () => {
  const listing = (...rules: Array<string>) =>
    [
      "Matching Defaults entries for just-be on host:",
      "    env_reset, lecture_file=/etc/sudo_lecture, !log_allowed",
      "",
      "User just-be may run the following commands on host:",
      ...rules.map((rule) => `    ${rule}`)
    ].join("\n")
  test("treats (ALL) ALL as needing a password", () => {
    expect(parseSudoList(listing("(ALL) ALL", "(root) NOPASSWD: /usr/local/sbin/momadmin.sh"))).toBe(false)
  })
  test("accepts insomnia's rule", () => {
    expect(
      parseSudoList(
        listing("(ALL) ALL", "(root) NOPASSWD: /usr/bin/pmset -a disablesleep 0, /usr/bin/pmset -a disablesleep 1")
      )
    ).toBe(true)
  })
  test("rejoins rules sudo wrapped at 80 columns (real output without a tty)", () => {
    const wrapped = [
      "User just-be may run the following commands on G3YQ4F3926:",
      "    (ALL) ALL",
      "    (root) NOPASSWD: /usr/bin/pmset -a disablesleep 0, /usr/bin/pmset -a",
      "        disablesleep 1",
      "    (root) NOPASSWD: /usr/local/sbin/momadmin.sh",
      ""
    ].join("\n")
    expect(parseSudoList(wrapped)).toBe(true)
  })
  test("needs both on and off to be passwordless", () => {
    expect(parseSudoList(listing("(root) NOPASSWD: /usr/bin/pmset -a disablesleep 1"))).toBe(false)
  })
  test("accepts a blanket NOPASSWD: ALL", () => {
    expect(parseSudoList(listing("(ALL) NOPASSWD: ALL"))).toBe(true)
  })
})

describe("remaining time", () => {
  const now = 1_000_000
  test("rounds up to the minute", () => {
    expect(shortRemaining(now + 72 * 60_000, now)).toBe("1h 12m")
    expect(shortRemaining(now + 42 * 60_000 - 1, now)).toBe("42m")
    expect(shortRemaining(now + 1000, now)).toBe("1m")
    expect(shortRemaining(now, now)).toBe("<1m")
    expect(shortRemaining(now - 5000, now)).toBe("<1m")
  })
  test("long form", () => {
    expect(longRemaining(now + 120 * 60_000, now)).toBe("2 hr")
    expect(longRemaining(now + 72 * 60_000, now)).toBe("1 hr 12 min")
    expect(longRemaining(now + 5 * 60_000, now)).toBe("5 min")
    expect(longRemaining(now, now)).toBe("less than a minute")
  })
})

// Session safety rules, against an in-memory Mac.

interface Mac {
  sleepDisabled: boolean
  privileges: Privileges
  pmsetBroken: boolean
  battery: Battery | null
  persisted: PersistedState
  /** Every pmset write and state save, in order. */
  log: Array<string>
}

const makeMac = (initial: Partial<Mac> = {}): Mac => ({
  sleepDisabled: false,
  privileges: "Granted",
  pmsetBroken: false,
  battery: { percent: 80, onAC: false },
  persisted: initialPersistedState,
  log: [],
  ...initial
})

const macLayer = (mac: Mac) =>
  Layer.mergeAll(
    Layer.succeed(SleepControl, {
      isDisabled: Effect.suspend(() =>
        mac.pmsetBroken
          ? Effect.fail(new PowerCommandFailed({ command: "pmset -g", detail: "broken" }))
          : Effect.succeed(mac.sleepDisabled)
      ),
      setDisabled: (disabled) =>
        Effect.suspend((): Effect.Effect<void, PrivilegesMissing | PowerCommandFailed> => {
          if (mac.privileges === "Missing") return Effect.fail(new PrivilegesMissing())
          if (mac.pmsetBroken) return Effect.fail(new PowerCommandFailed({ command: "pmset", detail: "broken" }))
          mac.log.push(`pmset ${disabled ? 1 : 0}`)
          mac.sleepDisabled = disabled
          return Effect.void
        }),
      privileges: Effect.sync(() => mac.privileges)
    }),
    Layer.succeed(BatteryMonitor, { read: Effect.sync(() => mac.battery) }),
    Layer.succeed(StateStore, {
      load: Effect.sync(() => mac.persisted),
      save: (state) =>
        Effect.sync(() => {
          mac.log.push(`save ${state.session._tag}${state.lastEnded ? ` ${state.lastEnded.reason}` : ""}`)
          mac.persisted = state
        })
    })
  )

/** Runs `body` with a manager, then closes its scope (the daemon stopping). */
const withManager = <A, E>(mac: Mac, body: (manager: SessionManager["Service"]) => Effect.Effect<A, E>) =>
  Effect.runPromise(
    SessionManager.use(body).pipe(Effect.provide(SessionManager.layer.pipe(Layer.provide(macLayer(mac)))))
  )

describe("SessionManager", () => {
  test("saves before disabling sleep, restores sleep before saving the end", async () => {
    const mac = makeMac()
    await withManager(mac, (m) => Effect.andThen(m.start(60_000), m.stop))
    expect(mac.log).toEqual(["save Active", "pmset 1", "pmset 0", "save Idle Stopped"])
  })

  test("shutdown restores sleep for an Active session", async () => {
    const mac = makeMac()
    await withManager(mac, (m) => m.start(null))
    expect(mac.log).toEqual(["save Active", "pmset 1", "pmset 0", "save Idle Shutdown"])
    expect(mac.sleepDisabled).toBe(false)
  })

  test("shutdown leaves an outside disablesleep alone", async () => {
    const mac = makeMac({ sleepDisabled: true })
    const status = await withManager(mac, (m) => m.status)
    expect(status.sleepDisabled).toBe(true)
    expect(mac.log).toEqual([])
    expect(mac.sleepDisabled).toBe(true)
  })

  test("a pmset read failure is not an outside change", async () => {
    const mac = makeMac()
    const status = await withManager(mac, (m) =>
      Effect.gen(function*() {
        yield* m.start(null)
        mac.pmsetBroken = true
        yield* m.refresh
        const status = yield* m.status
        mac.pmsetBroken = false
        return status
      }))
    expect(status.session._tag).toBe("Active")
  })

  test("disablesleep turned off elsewhere ends the session without writing pmset", async () => {
    const mac = makeMac()
    const status = await withManager(mac, (m) =>
      Effect.gen(function*() {
        yield* m.start(null)
        mac.sleepDisabled = false
        yield* m.refresh
        return yield* m.status
      }))
    expect(status.session._tag).toBe("Idle")
    expect(status.lastEnded?.reason).toBe("External")
    expect(mac.log).toEqual(["save Active", "pmset 1", "save Idle External"])
  })

  test("low battery: refuses to start at the threshold, stops a running session", async () => {
    const mac = makeMac({ battery: { percent: 20, onAC: false } })
    const refused = await withManager(mac, (m) => Effect.flip(m.start(null)))
    expect(refused._tag).toBe("BatteryTooLow")
    expect(mac.log).toEqual([])

    const running = makeMac({ battery: { percent: 21, onAC: false } })
    const status = await withManager(running, (m) =>
      Effect.gen(function*() {
        yield* m.start(null)
        running.battery = { percent: 20, onAC: false }
        yield* m.refresh
        return yield* m.status
      }))
    expect(status.lastEnded?.reason).toBe("LowBattery")
    expect(running.sleepDisabled).toBe(false)
  })

  test("recovers an unexpired session with its original deadline", async () => {
    const endsAt = Date.now() + 3_600_000
    const mac = makeMac({
      persisted: { ...initialPersistedState, session: { _tag: "Active", startedAt: 1, endsAt } }
    })
    const status = await withManager(mac, (m) => m.status)
    expect(status.session).toEqual({ _tag: "Active", startedAt: 1, endsAt })
    expect(mac.log.slice(0, 1)).toEqual(["pmset 1"])
  })

  test("an expired saved session restores sleep on startup", async () => {
    const endsAt = Date.now() - 1000
    const mac = makeMac({
      sleepDisabled: true,
      persisted: { ...initialPersistedState, session: { _tag: "Active", startedAt: 1, endsAt } }
    })
    const status = await withManager(mac, (m) => m.status)
    expect(status.session._tag).toBe("Idle")
    expect(status.lastEnded).toEqual({ reason: "Expired", at: endsAt })
    expect(mac.log).toEqual(["pmset 0", "save Idle Expired"])
  })

  test("missing permissions show up as soon as sudo asks for a password", async () => {
    const mac = makeMac()
    const status = await withManager(mac, (m) =>
      Effect.gen(function*() {
        mac.privileges = "Missing"
        const error = yield* Effect.flip(m.start(null))
        expect(error._tag).toBe("PrivilegesMissing")
        return yield* m.status
      }))
    expect(status.privileges).toBe("Missing")
    expect(status.session._tag).toBe("Idle")
    // The optimistic save is rolled back.
    expect(mac.persisted.session._tag).toBe("Idle")
  })
})

describe("insomnia start durations", () => {
  test("reads hours, minutes, both, bare minutes, and until-stopped", () => {
    expect(parseDuration("")).toBe(null)
    expect(parseDuration("until-stopped")).toBe(null)
    expect(parseDuration("30m")).toBe(1800)
    expect(parseDuration("1h")).toBe(3600)
    expect(parseDuration("1h30m")).toBe(5400)
    expect(parseDuration("90")).toBe(5400)
    expect(parseDuration("0m")).toBe(undefined)
    expect(parseDuration("soon")).toBe(undefined)
  })
})
