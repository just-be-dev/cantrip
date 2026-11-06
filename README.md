# Cantrip 🔮

A command bar for macOS: press **⌥ Space**, type, hit ↵. The UI is native
Swift (AppKit + SwiftUI). The commands, called **spells**, are
[Effect](https://effect.website) programs that run on [Bun](https://bun.sh).

```text
┌────────────── Cantrip.app (Swift) ─────────────┐        ┌──── bun host ─────┐
│ hotkey · panel · fuzzy search · apps · views   │ stdio  │ loads spells      │
│ draws List / Detail natively, copy/open/HUD    │◀──────▶│ runs Effect fibers│
└────────────────────────────────────────────────┘  JSON  └───────────────────┘
                                                   lines   ~/.config/cantrip/grimoire/*.ts
```

## Quick start

```sh
mise install            # bun 1.4.4 (pinned in mise.toml)
mise run install        # runtime deps
mise run dev            # run from source; the bar opens at launch
mise run install-app    # build Cantrip.app → ~/Applications and launch it
```

Your spells live in your **grimoire**, `~/.config/cantrip/grimoire/`. On first
launch Cantrip creates it and copies in the examples from
[`grimoire/`](grimoire). Edit them or add files there, and
they reload as soon as you save.

The installed app turns on **Launch at Login** the first time it runs. Toggle
it from the 🪄 menu bar icon. The menu also has:

| Item | Does |
|-|-|
| Open Grimoire | opens your spells folder |
| Open Journal | opens the daemon's log |
| Reload Grimoire (⌘R) | loads new and changed spells (saving a file does this too) |
| Restart Daemon | restarts the background bun process that runs spells, which stops any running spell |

The bar's search finds the same commands.

## Keys

| Key | Does |
|-----|------|
| ⌥ Space | open or close the bar |
| ↑ ↓ / ⌃N ⌃P | move the selection |
| ↵ / ⌘↵ | first / second action |
| ⌘K | every action for the selection |
| Esc | close actions → go back one view → clear the search → hide |
| ⌘R | Reload Grimoire (from the root search) |

The bar hides when it loses focus. Reopen it within 90 seconds and you're
right where you left off; after that it starts fresh.

## Writing a spell

A spell is a `.ts` file in your grimoire, `~/.config/cantrip/grimoire/`. Every exported
`Spell.make(...)` becomes a command. A directory with an `index.ts` works too.
Names starting with `_` are skipped, which is handy for shared helpers.

You don't need to install anything. Cantrip links `cantrip` and `effect` into
`node_modules/` and writes a `tsconfig.json`, so your editor has types.

```ts
import * as Effect from "effect/Effect"
import { Action, Spell, Ui } from "cantrip"

// No view: the bar hides at once, the spell runs, and a HUD flashes.
export const uuid = Spell.make({
  title: "Generate UUID",
  icon: "number",              // SF Symbol, emoji, or a file path
  mode: "no-view",
  run: Effect.gen(function*() {
    const id = crypto.randomUUID()
    yield* Ui.copy(id)
    yield* Ui.hud(`Copied ${id}`)
  })
})

// A list that Swift filters as you type.
export const fruit = Spell.make({
  title: "Fruit",
  run: Ui.list({
    items: ["Apple", "Banana"].map((name) => ({
      title: name,
      actions: [
        Action.copy("Copy", name),
        Action.push("Details", Ui.detail({ markdown: `# ${name}` })),
        Action.run("Shout", Ui.toast({ title: `${name}!`, style: "success" }))
      ]
    }))
  })
})

// A list that searches as you type: debounced, and each keystroke cancels the last search.
export const search = Spell.make({
  title: "Search Things",
  run: Ui.searchList({
    placeholder: "Search…",
    search: (query) => Effect.succeed([{ title: `You typed ${query}` }])
  })
})
```

### The model

- **`run` is an Effect.** It may require any of `SpellServices`: `Ui`,
  `HttpClient`, `FileSystem`, `Path`, or `ChildProcessSpawner`. The host
  provides them all.
- **Each view gets its own `Ui`.** `Ui.render` updates *that* view, even after
  something has been pushed on top of it. `Ui.query` is a `Stream` of that
  view's search text.
- **Views are data.** `View.list` and `View.detail` describe what to show, and
  Swift draws it. `Action.copy` and `Action.open` run in Swift with no round
  trip. `Action.run(effect)` and `Action.push(program)` call back into your spell.
- **Pushed views are programs.** `Action.push("Open", someEffect)` runs
  `someEffect` with a `Ui` bound to the new view, so it can loop, poll, or
  search. Esc interrupts it.
- **Scopes follow the UI.** Closing a view closes its scope, so fibers,
  `acquireRelease` resources, and child processes it started are cleaned up.
  Closing the spell closes all of them.
- **Errors show up.** A failed `run` shows the `Cause` in the bar. A failed
  action shows in the footer. A spell that fails to load is listed in a toast.
  Logs (`console.log`, `Effect.log`) go to `~/Library/Logs/Cantrip/host.log`.

### Ui API

| | |
|-|-|
| `Ui.render(view)` / `Ui.list(...)` / `Ui.detail(...)` | show or update this view |
| `Ui.push(viewOrProgram)` / `Ui.pop` | navigate |
| `Ui.query` | `Stream<string>` of this view's search text |
| `Ui.searchList({ search, debounce? })` | query → debounce → switchMap → render |
| `Ui.setLoading(bool)` | loading bar |
| `Ui.toast({ title, message?, style? })` | footer message |
| `Ui.hud(text)` / `Ui.hide` | hide the bar (and flash a message) |
| `Ui.copy(text)` / `Ui.open(urlOrPath)` | clipboard / default handler |

## Config

`~/.config/cantrip/config.json` (optional):

```json
{ "hotkey": "option+space", "bun": "/opt/homebrew/bin/bun" }
```

The hotkey takes `cmd`, `option`, `ctrl`, and `shift`, plus a key
(`space`, `a`–`z`, `0`–`9`, …). Without a `bun` entry, Cantrip uses the bun
that `scripts/bundle.sh` found at build time (the mise pin), then `~/.bun/bin`,
the mise shims, Homebrew, and your login shell. It needs bun ≥ 1.4.4; the host
log warns if it's older.

Environment variables, for development:

| Variable | |
|-|-|
| `CANTRIP_GRIMOIRE` | use a different grimoire |
| `CANTRIP_RUNTIME` | use a different `runtime/` |
| `CANTRIP_SHOW_ON_LAUNCH=1` | open the bar at launch |
| `CANTRIP_KEEP_OPEN=1` | don't hide when the bar loses focus |

## Layout

```text
app/        Swift package: panel, hotkey, host bridge, views (Sources/Cantrip)
runtime/    the `cantrip` package: SDK (src/sdk) and bun host (src/host)
grimoire/   example spells, copied to ~/.config/cantrip/grimoire on first run
scripts/    bundle.sh builds Cantrip.app
alchemy.run.ts  an empty stack, so projects.just-be.dev deploys succeed (nothing to deploy)
```

The wire protocol is documented in
[`runtime/src/host/Protocol.ts`](runtime/src/host/Protocol.ts) and mirrored in
[`app/Sources/Cantrip/Protocol.swift`](app/Sources/Cantrip/Protocol.swift). You
can drive the host by hand with `mise run host`:

```sh
echo '{"type":"run","session":"s1","command":"uuid"}' | mise run host
```
