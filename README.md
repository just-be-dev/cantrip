# Incant 🔮

A command bar for macOS: press **⌥ Space**, type, hit ↵. The UI is native
Swift (AppKit + SwiftUI). The commands, called **spells**, are
[Effect](https://effect.website) programs that run on [Bun](https://bun.sh).

```text
┌────────────── Incant.app (Swift) ──────────────┐        ┌──── bun host ─────┐
│ hotkey · panel · fuzzy search · apps · views   │ stdio  │ loads spells      │
│ draws List / Detail natively, copy/open/HUD    │◀──────▶│ runs Effect fibers│
└────────────────────────────────────────────────┘  JSON  └───────────────────┘
                                                   lines   ~/.config/incant/spells/*.ts
```

## Quick start

```sh
mise install            # bun 1.4
mise run install        # runtime deps
mise run dev            # run from source; the bar opens at launch
mise run install-app    # build Incant.app → ~/Applications and launch it
```

On first launch Incant creates `~/.config/incant/spells/` and copies the
examples from [`spells/`](spells) into it. Edit them or add files there, and
they reload as soon as you save.

> Raycast also uses ⌥ Space. Quit it or change one of the two shortcuts (see
> [Config](#config)).

## Keys

| Key | Does |
|-----|------|
| ⌥ Space | open or close the bar |
| ↑ ↓ / ⌃N ⌃P | move the selection |
| ↵ / ⌘↵ | first / second action |
| ⌘K | every action for the selection |
| Esc | close actions → go back one view → clear the search → hide |
| ⌘R | reload spells (from the root search) |

The bar hides when it loses focus. Reopen it within 90 seconds and you're
right where you left off; after that it starts fresh.

## Writing a spell

A spell is a `.ts` file in `~/.config/incant/spells/`. Every exported
`Spell.make(...)` becomes a command. A directory with an `index.ts` works too.
Names starting with `_` are skipped, which is handy for shared helpers.

You don't need to install anything. Incant links `incant` and `effect` into
`node_modules/` and writes a `tsconfig.json`, so your editor has types.

```ts
import * as Effect from "effect/Effect"
import { Action, Spell, Ui } from "incant"

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
  Logs (`console.log`, `Effect.log`) go to `~/Library/Logs/Incant/host.log`.

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

`~/.config/incant/config.json` (optional):

```json
{ "hotkey": "option+space", "bun": "/opt/homebrew/bin/bun" }
```

The hotkey takes `cmd`, `option`, `ctrl`, and `shift`, plus a key
(`space`, `a`–`z`, `0`–`9`, …). Without a `bun` entry, Incant looks in
`~/.bun/bin`, the mise shims, Homebrew, and then your login shell.

Environment variables, for development:

| Variable | |
|-|-|
| `INCANT_SPELLS_DIR` | use a different spells directory |
| `INCANT_RUNTIME` | use a different `runtime/` |
| `INCANT_SHOW_ON_LAUNCH=1` | open the bar at launch |
| `INCANT_KEEP_OPEN=1` | don't hide when the bar loses focus |

## Layout

```text
app/        Swift package: panel, hotkey, host bridge, views (Sources/Incant)
runtime/    the `incant` package: SDK (src/sdk) and bun host (src/host)
spells/     example spells, copied to ~/.config/incant/spells on first run
scripts/    bundle.sh builds Incant.app
```

The wire protocol is documented in
[`runtime/src/host/Protocol.ts`](runtime/src/host/Protocol.ts) and mirrored in
[`app/Sources/Incant/Protocol.swift`](app/Sources/Incant/Protocol.swift). You
can drive the host by hand with `mise run host`:

```sh
echo '{"type":"run","session":"s1","command":"uuid"}' | mise run host
```
