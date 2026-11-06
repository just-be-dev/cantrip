// A file can export several spells. Each export becomes its own command.
// These run shell commands through Effect's ChildProcessSpawner.
import * as Effect from "effect/Effect"
import * as ChildProcess from "effect/process/ChildProcess"
import * as ChildProcessSpawner from "effect/process/ChildProcessSpawner"
import { Action, Spell, Ui } from "incant"

const osascript = (script: string) =>
  ChildProcessSpawner.ChildProcessSpawner.use((spawner) =>
    spawner.string(ChildProcess.make("osascript", ["-e", script]))
  )

export const toggleDarkMode = Spell.make({
  title: "Toggle Dark Mode",
  icon: "circle.lefthalf.filled",
  keywords: ["appearance", "theme", "light"],
  mode: "no-view",
  run: Effect.gen(function*() {
    yield* osascript(
      `tell application "System Events" to tell appearance preferences to set dark mode to not dark mode`
    )
    const dark = yield* osascript(
      `tell application "System Events" to tell appearance preferences to get dark mode`
    )
    yield* Ui.hud(dark.trim() === "true" ? "Dark mode on" : "Light mode on")
  })
})

export const emptyTrash = Spell.make({
  title: "Empty Trash",
  icon: "trash",
  mode: "no-view",
  run: Effect.gen(function*() {
    yield* osascript(`tell application "Finder" to empty trash`)
    yield* Ui.hud("Trash emptied")
  })
})

export const ipAddress = Spell.make({
  title: "IP Address",
  subtitle: "Show local and public addresses",
  icon: "network",
  run: Effect.gen(function*() {
    const spawner = yield* ChildProcessSpawner.ChildProcessSpawner
    yield* Ui.detail({ markdown: "# IP Address\n\nLooking up…" })
    const local = yield* spawner.string(ChildProcess.make("ipconfig", ["getifaddr", "en0"])).pipe(
      Effect.map((s) => s.trim()),
      Effect.orElseSucceed(() => "unknown")
    )
    const publicIp = yield* Effect.tryPromise(() => fetch("https://api.ipify.org").then((r) => r.text())).pipe(
      Effect.orElseSucceed(() => "unknown")
    )
    yield* Ui.detail({
      markdown: `# IP Address\n\n**Local:** \`${local}\`\n\n**Public:** \`${publicIp}\``,
      metadata: [{ label: "Interface", value: "en0" }],
      actions: [
        Action.copy("Copy Local IP", local),
        Action.copy("Copy Public IP", publicIp)
      ]
    })
  })
})
