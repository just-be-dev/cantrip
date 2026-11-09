# Cantrip

macOS command bar. Rust app drawn with [Rune](https://github.com/anomalyco/rune) (`app/`) + bun spell host (`runtime/`) talking JSON lines over stdio.

## Rules

- The protocol lives in two places: `runtime/src/host/Protocol.ts` and `app/src/protocol.rs`. Change both together.
- The host's stdout is the protocol. Never write to it except through `Wire.send`; `console.*` is redirected to stderr (the host log).
- Every view in a session is a frame with its own Scope, query, and action handlers. A `Ui` is bound to one frame (depth). Keep it that way — `render` must never target "whatever is on top".
- Daemons (`runtime/src/host/Daemons.ts`) are host-owned, not session-owned: one Scope each, forked from the host's. Every way the host ends (stdin EOF, SIGTERM) must close that scope so finalizers run. Insomnia's finalizer is what turns sleep back on. Never `process.exit` before the layer scope has closed.
- A daemon's finalizers learn why it stops via `Daemon.stopReason`. Only Restart Host (the app sends `handoff` before SIGTERM), `cantrip handoff` (run by `scripts/bundle.sh --install`), and reloads count as hand-overs; stdin EOF or a bare SIGTERM is always `"shutdown"`, so a quit or crash still fails safe.
- Reload only re-imports entries whose files changed, and keeps the previous module if the new one fails to load. A daemon whose exported object is unchanged keeps running. Don't make a reload restart daemons wholesale.
- CLI commands (`runtime/src/host/Commands.ts`, `runtime/src/cli/client.ts`) run in the host, never in a separate process, so a daemon's command shares its live state. The socket frame format is documented at the top of Commands.ts; change the client with it. Link management must only ever touch symlinks that point at Cantrip's shim.
- Spells import `cantrip` and `effect/*`; the host symlinks both into the grimoire's `node_modules`. Don't make spells install packages.
- `Bun.resolveSync` before dynamic `import()` in the Registry works around a Bun 1.3.14 resolver bug with newly created files. Keep it.

- The app (`app/`) is a Rune app. Rune draws the panel and the HUD; `app/src/platform/` is the AppKit Rune doesn't do (status items and their menus, SF Symbols rasterized to pixels, NSWorkspace, user defaults, the login item). State lives in `store.rs`; it reaches the outside only through `services.rs`, which headless runs and tests leave empty so effects land in `Services::log`.
- Check UI work headlessly: add or adjust a scene in `app/src/fixtures.rs`, render it with `target/release/Cantrip --scene NAME --snap out.png` (or `--tree`, `--script`), and cover behaviour in `app/src/tests.rs`. Headless runs never start the host, the menu bar or NSWorkspace; `app/scripts/search.json` is a sample `--script`. The opening and closing animations are `El::sequence` choreography in `app/src/views/intro.rs` and `outro.rs` (the `closing` scene shows the exit; render it with `--at`), replayed per opening; see the opening frame by frame with `--scene opening --script scripts/open.json --strip strip.png --start 0.08 --frames 30 --every 3 --cols 5`.
- The wand in the search bar is a 3D model rendered on the CPU (`app/src/views/wand.rs`, rasterizer in `wand/raster.rs`) into a `Pixels` image from `AppSpec::poll`, on the frame clock. Its behaviour is a state machine (`wand/motion.rs`): moods held while something is true (typing, no results, hovered) and one-shot gestures the store cues (`wand.cue`) for events (selection moves, casts, closing). Typing goes through `wand.edited`: `wand/spells.rs` hides each new character until a spark from the tip lands on it, drawn as field spans and one `Shape` layer that the view rebuilds every frame while spells fly. A frame depends only on the frame clock and the cues given, so headless renders repeat; it draws only while on screen (full rate while anything moves, 30 fps idle), so a hidden bar costs nothing, and Reduce Motion gets a still wand and no spells. Keep it that way.
- `scripts/bundle.sh --install` builds a self-contained Cantrip.app (bun inside, in `Contents/Resources/bun/`) and installs it to `/Applications`. The icon is code: `app/examples/icon.rs`; `scripts/icon.sh` re-renders `app/Cantrip.icns` (committed).
- Rune is pinned to a `dev` rev in `app/Cargo.toml` (both `rune` and `rune-kit`), with its toolchain in `app/rust-toolchain.toml` and `rust` in `mise.toml`. Bump them together. Rune colours are linear; use `views::palette::srgb` for sRGB values.

- bun is pinned to 1.4.4 in `mise.toml`, `engines.bun` in `runtime/package.json`, and `MinimumBun` in `runtime/src/host/main.ts`. Bump all three together. Use `mise exec -- bun …` if your PATH has an older bun.

- The repo lives on projects.just-be.dev (`origin`), mirrored to github.com/just-be-dev/cantrip. Every push to `main` runs a deploy; the root `package.json` + `alchemy.run.ts` are an empty Alchemy stack so those succeed. Don't add resources to it unless Cantrip grows something to host.

## Effect

Effect 4 (stable, `effect@^4.1`). Bare-module imports: `import * as Effect from "effect/Effect"`.
Unstable modules live at `effect/http/*`, `effect/process/*`. Services use `Context.Service` + `Layer.effect` + `.of`;
access a service in a pipe with `Service.use(...)`. Check `runtime/node_modules/effect/ai-docs` and the `.d.ts` files before guessing an API.

## Checks

```sh
mise run typecheck                 # runtime + example spells (tsgo, Effect language service)
mise run check                     # app: clippy -D warnings + tests (Rune Runtime drives keys/clicks headlessly)
mise run snap                      # app: render every headless scene to app/target/snaps/*.png
mise run host                      # drive the host by hand with JSON on stdin
CANTRIP_KEEP_OPEN=1 mise run dev    # run the app without hide-on-blur
```
