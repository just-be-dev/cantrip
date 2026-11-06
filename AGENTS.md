# Cantrip

macOS command bar. Swift app (`app/`) + bun spell host (`runtime/`) talking JSON lines over stdio.

## Rules

- The protocol lives in two places: `runtime/src/host/Protocol.ts` and `app/Sources/Cantrip/Protocol.swift`. Change both together.
- The host's stdout is the protocol. Never write to it except through `Wire.send`; `console.*` is redirected to stderr (the host log).
- Every view in a session is a frame with its own Scope, query, and action handlers. A `Ui` is bound to one frame (depth). Keep it that way — `render` must never target "whatever is on top".
- Spells import `cantrip` and `effect/*`; the host symlinks both into the spells dir's `node_modules`. Don't make spells install packages.
- `Bun.resolveSync` before dynamic `import()` in the Registry works around a Bun 1.3.14 resolver bug with newly created files. Keep it.

- bun is pinned to 1.4.4 in `mise.toml`, `engines.bun` in `runtime/package.json`, and `MinimumBun` in `runtime/src/host/main.ts`. Bump all three together. Use `mise exec -- bun …` if your PATH has an older bun.

## Effect

Effect 4 (stable, `effect@^4.1`). Bare-module imports: `import * as Effect from "effect/Effect"`.
Unstable modules live at `effect/http/*`, `effect/process/*`. Services use `Context.Service` + `Layer.effect` + `.of`;
access a service in a pipe with `Service.use(...)`. Check `runtime/node_modules/effect/ai-docs` and the `.d.ts` files before guessing an API.

## Checks

```sh
mise run typecheck                 # runtime + example spells (tsgo, Effect language service)
cd app && swift build              # must stay warning-free
mise run host                      # drive the host by hand with JSON on stdin
CANTRIP_KEEP_OPEN=1 mise run dev    # run the app without hide-on-blur
```
