/**
 * Finds, loads, and hot-reloads spells from the grimoire: the folder of spells.
 *
 * Layout of `~/.config/cantrip/grimoire` (or `$CANTRIP_GRIMOIRE`):
 *
 *   uuid.ts            → every exported Spell becomes a command
 *   github/index.ts    → a directory spell, for spells with their own files
 *   _helpers.ts        → names starting with `_` or `.` are skipped
 *   node_modules/      → managed by Cantrip: links `cantrip` and `effect` to the host
 */
import * as NodePath from "node:path"
import * as Config from "effect/Config"
import * as Context from "effect/Context"
import * as Duration from "effect/Duration"
import * as Effect from "effect/Effect"
import * as FileSystem from "effect/FileSystem"
import * as Layer from "effect/Layer"
import * as Path from "effect/Path"
import * as Ref from "effect/Ref"
import * as Stream from "effect/Stream"
import * as Spell from "../sdk/Spell.ts"
import type { WireCommand } from "./Protocol.ts"
import { Wire } from "./Wire.ts"

const runtimeDir = NodePath.resolve(import.meta.dir, "../..")

export const GrimoireDir = Config.String("CANTRIP_GRIMOIRE").pipe(
  Config.withDefault(`${process.env.HOME}/.config/cantrip/grimoire`),
  Config.map((dir) => NodePath.resolve(dir))
)
/** Copied into the grimoire the first time it is created. */
export const ExamplesDir = Config.String("CANTRIP_EXAMPLES_DIR").pipe(
  Config.withDefault(`${runtimeDir}/../grimoire`),
  Config.map((dir) => NodePath.resolve(dir))
)

export interface LoadedSpell {
  readonly id: string
  readonly spell: Spell.Spell
}

export class Registry extends Context.Service<Registry, {
  readonly get: (id: string) => Effect.Effect<LoadedSpell | undefined>
  /** Re-import every spell and send the new command list to the app. */
  readonly reload: Effect.Effect<void>
}>()("cantrip/host/Registry") {
  static readonly layer = Layer.effect(
    Registry,
    Effect.gen(function*() {
      const fs = yield* FileSystem.FileSystem
      const path = yield* Path.Path
      const wire = yield* Wire
      const dir = yield* GrimoireDir
      const examples = yield* ExamplesDir

      const spells = yield* Ref.make(new Map<string, LoadedSpell>())
      let generation = 0

      const seed = Effect.fn("Registry.seed")(function*() {
        if (yield* fs.exists(dir)) return
        // Before it was called the grimoire, this folder was `spells/` next to it.
        const legacy = path.join(path.dirname(dir), "spells")
        if (path.basename(dir) === "grimoire" && (yield* fs.exists(legacy))) {
          yield* fs.rename(legacy, dir)
          return yield* Effect.logInfo(`Moved ${legacy} to ${dir}`)
        }
        yield* fs.makeDirectory(dir, { recursive: true })
        if (yield* fs.exists(examples)) {
          for (const name of yield* fs.readDirectory(examples)) {
            if (name === "node_modules" || name === "tsconfig.json" || name.startsWith(".")) continue
            yield* fs.copy(path.join(examples, name), path.join(dir, name))
          }
        }
        yield* Effect.logInfo(`Created ${dir}`)
      })

      /** Point `node_modules/{cantrip,effect}` at this host so spells need no install. */
      const link = Effect.fn("Registry.link")(function*() {
        const modules = path.join(dir, "node_modules")
        yield* fs.makeDirectory(modules, { recursive: true })
        const links: ReadonlyArray<readonly [string, string]> = [
          ["cantrip", runtimeDir],
          ["effect", path.join(runtimeDir, "node_modules", "effect")],
          ["@types/bun", path.join(runtimeDir, "node_modules", "@types", "bun")],
          ["bun-types", path.join(runtimeDir, "node_modules", "bun-types")]
        ]
        for (const [name, target] of links) {
          const at = path.join(modules, name)
          const current = yield* fs.readLink(at).pipe(Effect.orElseSucceed(() => undefined))
          if (current === target) continue
          yield* fs.remove(at, { recursive: true }).pipe(Effect.ignore)
          yield* fs.makeDirectory(path.dirname(at), { recursive: true })
          yield* fs.symlink(target, at)
        }
        const tsconfig = path.join(dir, "tsconfig.json")
        if (!(yield* fs.exists(tsconfig))) {
          yield* fs.writeFileString(tsconfig, `${JSON.stringify(spellsTsconfig, null, 2)}\n`)
        }
      })

      const entries = Effect.fn("Registry.entries")(function*() {
        const found: Array<{ readonly id: string; readonly file: string }> = []
        for (const name of (yield* fs.readDirectory(dir)).sort()) {
          if (name === "node_modules" || name.startsWith(".") || name.startsWith("_")) continue
          const full = path.join(dir, name)
          const info = yield* fs.stat(full)
          if (info.type === "Directory") {
            for (const index of ["index.ts", "index.tsx", "index.js"]) {
              const file = path.join(full, index)
              if (yield* fs.exists(file)) {
                found.push({ id: name, file })
                break
              }
            }
          } else if (/\.(ts|tsx|js|mjs)$/.test(name) && !name.endsWith(".d.ts") && name !== "tsconfig.json") {
            found.push({ id: name.replace(/\.[^.]+$/, ""), file: full })
          }
        }
        return found
      })

      const reload = Effect.gen(function*() {
        generation += 1
        const next = new Map<string, LoadedSpell>()
        const errors: Array<string> = []
        for (const entry of yield* entries()) {
          // Resolve first: Bun 1.3.14 can't dynamically import a file created after
          // the process started unless the resolver has looked it up. The `?v=`
          // query busts the module cache so edits are picked up.
          const loaded = yield* Effect.tryPromise(() =>
            import(`${Bun.resolveSync(entry.file, dir)}?v=${generation}`)
          ).pipe(
            Effect.map((mod: Record<string, unknown>) => mod),
            Effect.catch((error) => {
              errors.push(`${entry.id}: ${String(error.cause)}`)
              return Effect.as(Effect.logError(`Failed to load ${entry.file}`, error.cause), undefined)
            })
          )
          if (!loaded) continue
          for (const [name, value] of Object.entries(loaded)) {
            if (!Spell.isSpell(value)) continue
            const id = name === "default" ? entry.id : `${entry.id}:${name}`
            next.set(id, { id, spell: value })
          }
        }
        yield* Ref.set(spells, next)
        const commands: Array<WireCommand> = [...next.values()].map(({ id, spell }) => ({
          id,
          title: spell.title,
          subtitle: spell.subtitle,
          icon: spell.icon,
          keywords: spell.keywords ?? [],
          mode: spell.mode ?? "view"
        }))
        yield* wire.send({ type: "commands", commands, errors })
        yield* Effect.logInfo(`Loaded ${commands.length} spell(s) from ${dir}${errors.length ? `, ${errors.length} failed` : ""}`)
      }).pipe(
        Effect.catchCause((cause) => Effect.logError("Reload failed", cause)),
        Effect.withSpan("Registry.reload")
      )

      yield* seed().pipe(Effect.catchCause((cause) => Effect.logError("Could not create the grimoire", cause)))
      yield* link().pipe(Effect.catchCause((cause) => Effect.logError("Could not link spell modules", cause)))
      yield* reload

      // Hot reload: any change outside node_modules re-imports everything.
      yield* fs.watch(dir, { recursive: true }).pipe(
        Stream.filter((event) => !event.path.includes("node_modules")),
        Stream.debounce(Duration.millis(200)),
        Stream.runForEach(() => reload),
        Effect.catchCause((cause) => Effect.logWarning("Spell watcher stopped", cause)),
        Effect.forkScoped
      )

      return Registry.of({
        get: (id) => Effect.map(Ref.get(spells), (map) => map.get(id)),
        reload
      })
    })
  )
}

const spellsTsconfig = {
  compilerOptions: {
    lib: ["ESNext"],
    target: "ESNext",
    module: "Preserve",
    moduleDetection: "force",
    moduleResolution: "bundler",
    allowImportingTsExtensions: true,
    verbatimModuleSyntax: true,
    noEmit: true,
    strict: true,
    skipLibCheck: true,
    noUncheckedIndexedAccess: true,
    types: ["bun"]
  }
}
