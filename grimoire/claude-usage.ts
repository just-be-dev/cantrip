// A daemon: how much of your Claude plan you have left, as a ring in the menu bar.
// It uses Claude Code's login (the "Claude Code-credentials" Keychain item) and the
// endpoint behind Claude Code's /usage. When the token expires it refreshes it and
// saves it back the way Claude Code does, so the two stay in sync.
import * as Command from "effect/cli/Command"
import * as Flag from "effect/cli/Flag"
import * as Clock from "effect/Clock"
import * as Console from "effect/Console"
import * as Context from "effect/Context"
import * as Duration from "effect/Duration"
import * as Effect from "effect/Effect"
import * as HttpClient from "effect/http/HttpClient"
import * as HttpClientRequest from "effect/http/HttpClientRequest"
import * as HttpClientResponse from "effect/http/HttpClientResponse"
import * as ChildProcess from "effect/process/ChildProcess"
import * as ChildProcessSpawner from "effect/process/ChildProcessSpawner"
import * as Layer from "effect/Layer"
import * as Result from "effect/Result"
import * as Schema from "effect/Schema"
import * as Stream from "effect/Stream"
import * as SubscriptionRef from "effect/SubscriptionRef"
import { userInfo } from "node:os"
import { Daemon, type DaemonServices, MenuBar } from "cantrip"

const SERVICE = "Claude Code-credentials"
const SECURITY = "/usr/bin/security"
const USAGE_URL = "https://api.anthropic.com/api/oauth/usage"
const TOKEN_URL = "https://platform.claude.com/v1/oauth/token"
const CLIENT_ID = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
const DEFAULT_SCOPES = [
  "user:profile",
  "user:inference",
  "user:sessions:claude_code",
  "user:mcp_servers",
  "user:file_upload",
  "user:plugins"
]
// Cloudflare rejects requests with no recognizable User-Agent.
const USER_AGENT = "claude-usage-menubar/1.0 (claude-cli compatible)"

/** The usage endpoint is rate limited; 5 minutes is plenty for a glanceable gauge. */
const POLL_MS = 5 * 60_000
/** Opening the menu refreshes, but not more often than this. */
const MIN_MANUAL_MS = 30_000
/** How often to check the wall clock (timers pause during sleep) and re-render relative times. */
const TICK_MS = 30_000

// MARK: Errors

class CredentialsNotFound extends Schema.TaggedError<CredentialsNotFound>()("CredentialsNotFound", {}) {}
class CredentialsMalformed extends Schema.TaggedError<CredentialsMalformed>()("CredentialsMalformed", {}) {}
class CredentialsWriteFailed extends Schema.TaggedError<CredentialsWriteFailed>()("CredentialsWriteFailed", {
  stderr: Schema.String
}) {}
class NotLoggedIn extends Schema.TaggedError<NotLoggedIn>()("NotLoggedIn", {}) {}
class HttpError extends Schema.TaggedError<HttpError>()("HttpError", {
  status: Schema.Number,
  body: Schema.String
}) {}

const describe = (error: Effect.Error<typeof fetchSnapshot>): string => {
  switch (error._tag) {
    case "CredentialsNotFound":
      return "Claude Code credentials not found in Keychain."
    case "CredentialsMalformed":
      return "Claude Code credentials are in an unexpected format."
    case "CredentialsWriteFailed":
      return `Couldn't save refreshed token: ${error.stderr.trim()}`
    case "NotLoggedIn":
      return "Not signed in. Run `claude` in a terminal and use /login."
    case "HttpError":
      return `Request failed (${error.status}): ${error.body.slice(0, 160)}`
    case "TimeoutError":
      return "The request timed out."
    default:
      return error.message
  }
}

// MARK: Keychain

/** Runs `security`, optionally feeding it stdin, and collects its output and exit status. */
const security = Effect.fn("security")(function*(args: ReadonlyArray<string>, input?: string) {
  const spawner = yield* ChildProcessSpawner.ChildProcessSpawner
  return yield* Effect.scoped(Effect.gen(function*() {
    const handle = yield* spawner.spawn(ChildProcess.make(SECURITY, args, {
      stdin: input === undefined ? "ignore" : Stream.succeed(new TextEncoder().encode(input))
    }))
    const [stdout, stderr, status] = yield* Effect.all([
      Stream.mkString(Stream.decodeText(handle.stdout)),
      Stream.mkString(Stream.decodeText(handle.stderr)),
      handle.exitCode
    ], { concurrency: "unbounded" })
    return { status: Number(status), stdout, stderr }
  }))
})

const JsonObject = Schema.Record(Schema.String, Schema.Unknown)
const decodeJsonObject = Schema.decodeUnknownEffect(Schema.fromJsonString(JsonObject))

const OAuth = Schema.Struct({
  accessToken: Schema.String,
  refreshToken: Schema.optional(Schema.NullOr(Schema.String)),
  /** Epoch ms; missing or 0 means unknown. */
  expiresAt: Schema.optional(Schema.NullOr(Schema.Number)),
  scopes: Schema.optional(Schema.NullOr(Schema.Array(Schema.String)))
})
const decodeCredentials = Schema.decodeUnknownEffect(Schema.Struct({ claudeAiOauth: OAuth }))

interface Credentials {
  readonly accessToken: string
  readonly refreshToken: string | undefined
  readonly expiresAt: number | undefined
  readonly scopes: ReadonlyArray<string>
}

/** The stored JSON as-is, so unknown keys round-trip untouched. */
const readRaw = Effect.gen(function*() {
  const result = yield* security(["find-generic-password", "-s", SERVICE, "-w"])
  if (result.status !== 0) return yield* new CredentialsNotFound()
  return yield* decodeJsonObject(result.stdout.replace(/[\r\n]+$/, "")).pipe(
    Effect.mapError(() => new CredentialsMalformed())
  )
})

const readCredentials = Effect.gen(function*() {
  const { claudeAiOauth: oauth } = yield* readRaw.pipe(
    Effect.flatMap(decodeCredentials),
    Effect.catchTag("SchemaError", () => Effect.fail(new CredentialsMalformed()))
  )
  return {
    accessToken: oauth.accessToken,
    refreshToken: oauth.refreshToken ?? undefined,
    expiresAt: oauth.expiresAt && oauth.expiresAt > 0 ? oauth.expiresAt : undefined,
    scopes: oauth.scopes ?? []
  } satisfies Credentials
})

/** The account of the existing item, from `"acct"<blob>="…"` in `security`'s attribute dump. */
const existingAccount = security(["find-generic-password", "-s", SERVICE]).pipe(
  Effect.map((result) =>
    result.status === 0 ? /"acct"<blob>="([^"]*)"/.exec(result.stdout)?.[1] : undefined
  )
)

/** Writes new tokens back, preserving every other field Claude Code stores. */
const saveTokens = Effect.fn("saveTokens")(function*(tokens: {
  readonly accessToken: string
  readonly refreshToken: string | undefined
  readonly expiresAt: number
  readonly scopes: ReadonlyArray<string> | undefined
}) {
  const raw = yield* readRaw
  const previous = raw.claudeAiOauth
  const oauth: Record<string, unknown> = typeof previous === "object" && previous !== null && !Array.isArray(previous)
    ? { ...previous }
    : {}
  oauth.accessToken = tokens.accessToken
  if (tokens.refreshToken !== undefined) oauth.refreshToken = tokens.refreshToken
  oauth.expiresAt = Math.trunc(tokens.expiresAt)
  if (tokens.scopes && tokens.scopes.length > 0) oauth.scopes = tokens.scopes
  const hex = Buffer.from(JSON.stringify({ ...raw, claudeAiOauth: oauth }), "utf8").toString("hex")
  const account = (yield* existingAccount) ?? userInfo().username
  // Same as Claude Code: `security -i` reads the command from stdin, so the secret
  // never appears in an argument list.
  const result = yield* security(
    ["-i"],
    `add-generic-password -U -a "${account}" -s "${SERVICE}" -X "${hex}"\n`
  )
  if (result.status !== 0) return yield* new CredentialsWriteFailed({ stderr: result.stderr })
})

// MARK: API

const TokenResponse = Schema.Struct({
  access_token: Schema.String,
  expires_in: Schema.optional(Schema.NullOr(Schema.Number)),
  refresh_token: Schema.optional(Schema.NullOr(Schema.String)),
  scope: Schema.optional(Schema.NullOr(Schema.String))
})

// Uninterruptible (bounded by the timeout): a rotated refresh token must reach the
// Keychain even if the daemon stops mid-refresh, or Claude Code needs a new /login.
const refreshTokens = Effect.fn("refreshTokens")(function*(creds: Credentials) {
  if (!creds.refreshToken) return yield* new NotLoggedIn()
  const client = yield* HttpClient.HttpClient
  const response = yield* HttpClientRequest.post(TOKEN_URL).pipe(
    HttpClientRequest.setHeader("User-Agent", USER_AGENT),
    HttpClientRequest.bodyJsonUnsafe({
      grant_type: "refresh_token",
      refresh_token: creds.refreshToken,
      client_id: CLIENT_ID,
      scope: (creds.scopes.length > 0 ? creds.scopes : DEFAULT_SCOPES).join(" ")
    }),
    client.execute,
    Effect.timeout("30 seconds")
  )
  if (response.status !== 200) {
    // 400/401 here means the refresh token is dead: the user must log in again.
    if (response.status === 400 || response.status === 401) return yield* new NotLoggedIn()
    const body = yield* response.text.pipe(Effect.orElseSucceed(() => ""))
    return yield* new HttpError({ status: response.status, body })
  }
  const token = yield* HttpClientResponse.schemaBodyJson(TokenResponse)(response).pipe(
    Effect.mapError(() => new HttpError({ status: response.status, body: "Unexpected token response" }))
  )
  const expiresAt = (yield* Clock.currentTimeMillis) + (token.expires_in ?? 3600) * 1000
  const refreshToken = token.refresh_token ?? undefined
  yield* saveTokens({
    accessToken: token.access_token,
    refreshToken,
    expiresAt,
    scopes: token.scope?.split(" ").filter((s) => s !== "")
  })
  return { ...creds, accessToken: token.access_token, refreshToken: refreshToken ?? creds.refreshToken, expiresAt }
}, Effect.uninterruptible)

const fetchUsage = Effect.fn("fetchUsage")(function*(token: string) {
  const client = yield* HttpClient.HttpClient
  const body = yield* HttpClientRequest.get(USAGE_URL).pipe(
    HttpClientRequest.bearerToken(token),
    HttpClientRequest.setHeaders({
      "anthropic-beta": "oauth-2025-04-20",
      Accept: "application/json",
      "User-Agent": USER_AGENT
    }),
    client.execute,
    Effect.flatMap((response) =>
      Effect.flatMap(response.text, (text) =>
        response.status === 200 ? Effect.succeed(text) : Effect.fail(new HttpError({ status: response.status, body: text }))
      )
    ),
    Effect.timeout("20 seconds")
  )
  const root = yield* decodeJsonObject(body).pipe(
    Effect.mapError(() => new HttpError({ status: 200, body: "Unexpected usage response" }))
  )
  return parseUsage(root, yield* Clock.currentTimeMillis)
})

const expiringSoon = (creds: Credentials, now: number) => creds.expiresAt === undefined || creds.expiresAt < now + 60_000

/** Fetches usage, refreshing (and saving) the OAuth token when needed. */
const fetchSnapshot = Effect.gen(function*() {
  const stored = yield* readCredentials
  const creds = expiringSoon(stored, yield* Clock.currentTimeMillis) ? yield* refreshTokens(stored) : stored
  return yield* fetchUsage(creds.accessToken).pipe(
    Effect.catchTag("HttpError", (error) =>
      error.status === 401 ? Effect.gen(function*() {
        // Revoked or rotated elsewhere: re-read (Claude Code may have refreshed), then refresh.
        const latest = yield* readCredentials
        if (latest.accessToken !== creds.accessToken && !expiringSoon(latest, yield* Clock.currentTimeMillis)) {
          return yield* fetchUsage(latest.accessToken)
        }
        const fresh = yield* refreshTokens(latest)
        return yield* fetchUsage(fresh.accessToken)
      }) : Effect.fail(error))
  )
})

// MARK: Parsing

export interface UsageWindow {
  readonly id: string
  readonly label: string
  /** Percent left, 0–100. */
  readonly remaining: number
  readonly resetsAt: number | undefined
}

export interface ExtraUsage {
  readonly monthlyLimit: number | undefined
  readonly usedCredits: number | undefined
  readonly utilization: number | undefined
  readonly currency: string | undefined
}

export interface Snapshot {
  readonly windows: ReadonlyArray<UsageWindow>
  readonly extra: ExtraUsage | undefined
  readonly fetchedAt: number
}

const KNOWN: Record<string, readonly [label: string, order: number]> = {
  five_hour: ["Current session (5h)", 0],
  seven_day: ["Weekly · all models", 1],
  seven_day_opus: ["Weekly · Opus", 2],
  seven_day_sonnet: ["Weekly · Sonnet", 3],
  seven_day_oauth_apps: ["Weekly · OAuth apps", 4]
}

const WindowJson = Schema.Struct({
  utilization: Schema.Number,
  resets_at: Schema.optional(Schema.NullOr(Schema.String)),
  display_name: Schema.optional(Schema.NullOr(Schema.String))
})
const decodeWindow = Schema.decodeUnknownOption(WindowJson)

const ExtraJson = Schema.Struct({
  is_enabled: Schema.Literal(true),
  monthly_limit: Schema.optional(Schema.NullOr(Schema.Number)),
  used_credits: Schema.optional(Schema.NullOr(Schema.Number)),
  utilization: Schema.optional(Schema.NullOr(Schema.Number)),
  currency: Schema.optional(Schema.NullOr(Schema.String))
})
const decodeExtra = Schema.decodeUnknownOption(ExtraJson)

const prettify = (key: string) =>
  key.split("_").filter((w) => w !== "").map((w) => w[0]!.toUpperCase() + w.slice(1).toLowerCase()).join(" ")

const parseDate = (value: string | null | undefined) => {
  const ms = value ? Date.parse(value) : NaN
  return Number.isNaN(ms) ? undefined : ms
}

const clampPercent = (n: number) => Math.max(0, Math.min(100, n))

/**
 * Lenient: any top-level object with a numeric `utilization` is a window, and so is
 * each element of a list (e.g. `model_scoped`), so new windows show up on their own.
 */
export const parseUsage = (root: Readonly<Record<string, unknown>>, now: number): Snapshot => {
  const windows: Array<UsageWindow & { readonly order: number }> = []
  for (const [key, value] of Object.entries(root)) {
    if (key === "extra_usage") continue
    if (Array.isArray(value)) {
      value.forEach((element, idx) => {
        const w = decodeWindow(element)
        if (w._tag === "None") return
        windows.push({
          id: `${key}.${idx}`,
          label: `Weekly · ${w.value.display_name ?? `${prettify(key)} ${idx + 1}`}`,
          remaining: clampPercent(100 - w.value.utilization),
          resetsAt: parseDate(w.value.resets_at),
          order: 60 + idx
        })
      })
    } else {
      const w = decodeWindow(value)
      if (w._tag === "None") continue
      const known = KNOWN[key]
      windows.push({
        id: key,
        label: known?.[0] ?? prettify(key),
        remaining: clampPercent(100 - w.value.utilization),
        resetsAt: parseDate(w.value.resets_at),
        order: known?.[1] ?? 50
      })
    }
  }
  windows.sort((a, b) => a.order - b.order || a.label.localeCompare(b.label))

  const extra = decodeExtra(root.extra_usage)
  return {
    windows: windows.map(({ order: _, ...w }) => w),
    extra: extra._tag === "None" ? undefined : {
      monthlyLimit: extra.value.monthly_limit ?? undefined,
      usedCredits: extra.value.used_credits ?? undefined,
      utilization: extra.value.utilization ?? undefined,
      currency: extra.value.currency ?? undefined
    },
    fetchedAt: now
  }
}

// MARK: Formatting

/** The window you'll hit first: the one with the least left. */
const tightest = (snapshot: Snapshot | undefined) =>
  snapshot?.windows.reduce<UsageWindow | undefined>((min, w) => (!min || w.remaining < min.remaining ? w : min), undefined)

const toneFor = (remaining: number): MenuBar.Tone => {
  if (remaining <= 10) return "critical"
  if (remaining <= 25) return "warning"
  return "normal"
}

const span = (seconds: number, units: ReadonlyArray<readonly [string, number]>) => {
  let rest = Math.floor(seconds)
  const parts: Array<string> = []
  for (const [suffix, size] of units) {
    const n = Math.floor(rest / size)
    rest -= n * size
    if (n > 0) parts.push(`${n}${suffix}`)
  }
  return parts.length > 0 ? parts.join(" ") : `<1${units[units.length - 1]![0]}`
}

/** "in 2h 10m · 3:40 PM", "in 3d 4h · Tue 3:40 PM", or "now". */
export const resetText = (resetsAt: number, now: number): string => {
  const seconds = (resetsAt - now) / 1000
  if (seconds <= 0) return "now"
  const long = seconds >= 86_400
  const relative = long ? span(seconds, [["d", 86_400], ["h", 3600]]) : span(seconds, [["h", 3600], ["m", 60]])
  const absolute = new Date(resetsAt).toLocaleString(undefined, {
    ...(long ? { weekday: "short" } : {}),
    hour: "numeric",
    minute: "2-digit"
  })
  return `in ${relative} · ${absolute}`
}

/** "just now", "3 min ago", "2 hr ago", "1 day ago". */
export const agoText = (then: number, now: number): string => {
  const minutes = Math.floor((now - then) / 60_000)
  if (minutes < 1) return "just now"
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} hr ago`
  const days = Math.floor(hours / 24)
  return `${days} ${days === 1 ? "day" : "days"} ago`
}

/** The API reports credits in minor units (cents). */
const money = (cents: number, currency: string) => {
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(cents / 100)
  } catch {
    return `${(cents / 100).toFixed(2)} ${currency}`
  }
}

const extraSummary = (extra: ExtraUsage) => {
  const code = (extra.currency ?? "USD").toUpperCase()
  if (extra.usedCredits !== undefined && extra.monthlyLimit !== undefined) {
    return `${money(extra.usedCredits, code)} / ${money(extra.monthlyLimit, code)}`
  }
  if (extra.usedCredits !== undefined) return `${money(extra.usedCredits, code)} used`
  return extra.utilization === undefined ? "Enabled" : `${Math.trunc(extra.utilization)}% used`
}

// MARK: View

export interface State {
  readonly snapshot: Snapshot | undefined
  readonly error: string | undefined
  readonly loading: boolean
  /** When the last fetch started (epoch ms), successful or not. */
  readonly lastAttempt: number
}

export const view = (
  state: State,
  now: number,
  refresh: Effect.Effect<unknown, unknown, DaemonServices>
): MenuBar.StatusItem => {
  const { snapshot, error, loading } = state
  const tight = tightest(snapshot)
  const tone = tight ? toneFor(tight.remaining) : "normal"
  const rows: Array<MenuBar.Entry> = []

  if (snapshot) {
    if (snapshot.windows.length === 0) rows.push(MenuBar.item("No plan limits reported for this account."))
    for (const w of snapshot.windows) {
      rows.push(MenuBar.item(w.label, {
        progress: w.remaining / 100,
        accessory: `${Math.round(w.remaining)}% left`,
        subtitle: w.resetsAt === undefined ? undefined : `Resets ${resetText(w.resetsAt, now)}`,
        tone: toneFor(w.remaining)
      }))
    }
    const extra = snapshot.extra
    if (extra) {
      rows.push(
        extra.utilization === undefined
          ? MenuBar.item("Extra usage", { subtitle: extraSummary(extra) })
          : MenuBar.item("Extra usage", {
            progress: Math.min(extra.utilization, 100) / 100,
            accessory: extraSummary(extra)
          })
      )
    }
  } else if (!error) {
    rows.push(MenuBar.item("Loading…"))
  }
  if (error) rows.push(MenuBar.item(error, { icon: "exclamationmark.triangle.fill", tone: "warning" }))

  return {
    icon: MenuBar.ring(tight ? tight.remaining / 100 : null, {
      tone,
      busy: loading,
      pulse: tone === "critical",
      badge: error !== undefined
    }),
    tooltip: tight
      ? `Claude: ${Math.round(tight.remaining)}% left · ${tight.label}`
      : `Claude usage: ${error ?? "loading…"}`,
    menu: [
      MenuBar.header("Claude Usage"),
      ...rows,
      MenuBar.separator,
      ...(snapshot ? [MenuBar.item(`Updated ${agoText(snapshot.fetchedAt, now)}`)] : []),
      MenuBar.item(loading ? "Refreshing…" : "Refresh", { key: "r", disabled: loading, onSelect: refresh })
    ]
  }
}

// MARK: Shared state

/**
 * The usage state, shared by the menu and the `claude-usage` command. Built once
 * per daemon run (as its `layer`), so both see the same snapshot and a refresh
 * from either is never doubled up.
 */
export class Usage extends Context.Service<Usage, {
  readonly state: SubscriptionRef.SubscriptionRef<State>
  /** Fetches unless a fetch is running, or (unless forced) one started under 30s ago. */
  readonly refresh: (force: boolean) => Effect.Effect<void>
  /** Waits until no fetch is running. */
  readonly settled: Effect.Effect<State>
}>()("claude-usage/Usage") {
  static readonly layer = Layer.effect(
    Usage,
    Effect.gen(function*() {
      const state = yield* SubscriptionRef.make<State>({
        snapshot: undefined,
        error: undefined,
        loading: false,
        lastAttempt: 0
      })
      const services = yield* Effect.context<HttpClient.HttpClient | ChildProcessSpawner.ChildProcessSpawner>()

      const refresh = (force: boolean): Effect.Effect<void> =>
        Effect.gen(function*() {
          const now = yield* Clock.currentTimeMillis
          const start = yield* SubscriptionRef.modify(state, (s): readonly [boolean, State] =>
            s.loading || (!force && now - s.lastAttempt < MIN_MANUAL_MS)
              ? [false, s]
              : [true, { ...s, loading: true, lastAttempt: now }])
          if (!start) return
          const result = yield* fetchSnapshot.pipe(Effect.result, Effect.provideContext(services))
          if (Result.isFailure(result)) yield* Effect.logWarning("Claude usage refresh failed", result.failure)
          // On failure keep the last snapshot; the badge says it's stale.
          yield* SubscriptionRef.update(state, (s) =>
            Result.isSuccess(result)
              ? { ...s, loading: false, snapshot: result.success, error: undefined }
              : { ...s, loading: false, error: describe(result.failure) })
        }).pipe(
          // If interrupted mid-fetch, don't leave the spinner on.
          Effect.onInterrupt(() => SubscriptionRef.update(state, (s) => ({ ...s, loading: false })))
        )

      const settled = SubscriptionRef.changes(state).pipe(
        Stream.filter((s) => !s.loading),
        Stream.runHead,
        Effect.flatMap((s) => s._tag === "Some" ? Effect.succeed(s.value) : SubscriptionRef.get(state))
      )

      return Usage.of({ state, refresh, settled })
    })
  )
}

// MARK: Command

const bar = (remaining: number, width = 10) => {
  const filled = Math.round((Math.max(0, Math.min(100, remaining)) / 100) * width)
  return `${"█".repeat(filled)}${"░".repeat(width - filled)}`
}

export const usageText = (state: State, now: number): string => {
  const { snapshot, error } = state
  if (!snapshot) return `Claude usage: ${error ?? "no data yet"}`
  const lines = [`Claude usage · updated ${agoText(snapshot.fetchedAt, now)}`, ""]
  if (snapshot.windows.length === 0) lines.push("No plan limits reported for this account.")
  const width = Math.max(0, ...snapshot.windows.map((w) => w.label.length), snapshot.extra ? "Extra usage".length : 0)
  for (const w of snapshot.windows) {
    const left = `${Math.round(w.remaining)}% left`.padStart(9)
    const reset = w.resetsAt === undefined ? "" : `  resets ${resetText(w.resetsAt, now)}`
    lines.push(`${w.label.padEnd(width)}  ${bar(w.remaining)} ${left}${reset}`)
  }
  if (snapshot.extra) lines.push(`${"Extra usage".padEnd(width)}  ${extraSummary(snapshot.extra)}`)
  if (error) lines.push("", `Last refresh failed: ${error}`)
  return lines.join("\n")
}

const usageCommand = Command.make(
  "claude-usage",
  {
    json: Flag.Boolean("json").pipe(Flag.withDefault(false), Flag.withDescription("Print the raw snapshot as JSON")),
    refresh: Flag.Boolean("refresh").pipe(Flag.withDefault(false), Flag.withDescription("Fetch now, even if the data is fresh"))
  },
  ({ json, refresh }) =>
    Effect.gen(function*() {
      const usage = yield* Usage
      // Same rule as opening the menu: data under 30s old is used as is.
      yield* usage.refresh(refresh)
      const state = yield* usage.settled
      const now = yield* Clock.currentTimeMillis
      if (json) {
        yield* Console.log(JSON.stringify({ ...state.snapshot, error: state.error ?? null }, null, 2))
      } else {
        yield* Console.log(usageText(state, now))
      }
      if (!state.snapshot) return yield* Effect.fail(new Error(state.error ?? "no usage data"))
    })
).pipe(Command.withDescription("Show how much of your Claude plan limits you have left"))

// MARK: Daemon

export default Daemon.summon({
  title: "Claude Usage",
  icon: "gauge.with.dots.needle.33percent",
  layer: Usage.layer,
  command: usageCommand,
  run: Effect.gen(function*() {
    const usage = yield* Usage

    const render = Effect.gen(function*() {
      const now = yield* Clock.currentTimeMillis
      yield* MenuBar.render(view(yield* SubscriptionRef.get(usage.state), now, usage.refresh(true)))
    })

    // Every state change (from the menu, the timer, or the command) redraws.
    yield* SubscriptionRef.changes(usage.state).pipe(Stream.runForEach(() => render), Effect.forkChild)
    yield* MenuBar.opened.pipe(
      Stream.runForEach(() => Effect.andThen(render, usage.refresh(false))),
      Effect.forkChild
    )

    // Poll the wall clock rather than trusting a 5-minute timer, which pauses during sleep.
    const tick = Effect.gen(function*() {
      const { lastAttempt } = yield* SubscriptionRef.get(usage.state)
      const now = yield* Clock.currentTimeMillis
      if (now - lastAttempt >= POLL_MS || now < lastAttempt) yield* usage.refresh(true)
      else yield* render
      const due = (yield* SubscriptionRef.get(usage.state)).lastAttempt + POLL_MS - (yield* Clock.currentTimeMillis)
      yield* Effect.sleep(Duration.millis(Math.max(1000, Math.min(TICK_MS, due))))
    })
    return yield* Effect.forever(tick)
  })
})
