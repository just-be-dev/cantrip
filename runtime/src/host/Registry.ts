/**
 * Finds, loads, and hot-reloads spells from the grimoire: the folder of spells.
 *
 * Layout of `~/.config/cantrip/grimoire` (or `$CANTRIP_GRIMOIRE`):
 *
 *   uuid.ts            → every exported Spell becomes a command, every Daemon a menu bar item
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
import * as Option from "effect/Option"
import * as Ref from "effect/Ref"
import * as Semaphore from "effect/Semaphore"
import * as Stream from "effect/Stream"
import * as SubscriptionRef from "effect/SubscriptionRef"
import * as Daemon from "../sdk/Daemon.ts"
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

export interface LoadedDaemon {
  readonly id: string
  readonly daemon: Daemon.Daemon
}

const fileStamp = (info: FileSystem.File.Info) =>
  `${Option.match(info.mtime, { onNone: () => 0, onSome: (d) => d.getTime() })}:${info.size}`

export class Registry extends Context.Service<Registry, {
  readonly get: (id: string) => Effect.Effect<LoadedSpell | undefined>
  /**
   * The loaded daemons. After a reload, a daemon whose file didn't change
   * is the same object as before, so the host can leave it running.
   */
  readonly daemons: SubscriptionRef.SubscriptionRef<ReadonlyMap<string, LoadedDaemon>>
  /** Re-import changed spells and send the new command list to the app. */
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
      const daemons = yield* SubscriptionRef.make<ReadonlyMap<string, LoadedDaemon>>(new Map())
      const lock = yield* Semaphore.make(1)
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

      interface Entry {
        readonly id: string
        /** The module to import. */
        readonly file: string
        /** The file, or the directory of a directory spell: everything that reloads with it. */
        readonly root: string
        /** Changes whenever any file under `root` does. */
        readonly stamp: string
      }

      /** mtime and size of every file under `root`, so edits to a directory spell's modules count too. */
      const stampOf = Effect.fn("Registry.stamp")(function*(root: string) {
        const info = yield* fs.stat(root)
        if (info.type !== "Directory") return fileStamp(info)
        const parts: Array<string> = []
        for (const name of (yield* fs.readDirectory(root, { recursive: true })).sort()) {
          if (name.split("/").some((part) => part === "node_modules" || part.startsWith("."))) continue
          const child = yield* fs.stat(path.join(root, name)).pipe(Effect.orElseSucceed(() => undefined))
          if (child && child.type === "File") parts.push(`${name}:${fileStamp(child)}`)
        }
        return parts.join("|")
      })

      const entries = Effect.fn("Registry.entries")(function*() {
        const found: Array<Entry> = []
        /** Shared `_` helpers: if they change, every entry reloads, since any of them may import one. */
        const helpers: Array<string> = []
        for (const name of (yield* fs.readDirectory(dir)).sort()) {
          if (name === "node_modules" || name.startsWith(".")) continue
          const full = path.join(dir, name)
          if (name.startsWith("_")) {
            helpers.push(`${name}:${yield* stampOf(full)}`)
            continue
          }
          const info = yield* fs.stat(full)
          if (info.type === "Directory") {
            for (const index of ["index.ts", "index.tsx", "index.js"]) {
              const file = path.join(full, index)
              if (yield* fs.exists(file)) {
                found.push({ id: name, file, root: full, stamp: yield* stampOf(full) })
                break
              }
            }
          } else if (/\.(ts|tsx|js|mjs)$/.test(name) && !name.endsWith(".d.ts") && name !== "tsconfig.json") {
            found.push({ id: name.replace(/\.[^.]+$/, ""), file: full, root: full, stamp: fileStamp(info) })
          }
        }
        return { found, helpers: helpers.join("|") }
      })

      /** The module objects of the last load, by entry id, so unchanged entries aren't imported again. */
      const modules = new Map<string, { readonly stamp: string; readonly exports: Record<string, unknown> }>()
      let helpersStamp: string | undefined
      const realDir = yield* fs.realPath(dir).pipe(Effect.orElseSucceed(() => dir))

      /**
       * Drop cached modules under `root` (in both its given and real path) so the
       * next import evaluates them again, including the files they import.
       * A `?v=` query alone only re-evaluates the entry file itself.
       */
      const evict = (root: string) => {
        const roots = [root, root.replace(dir, realDir)]
        for (const key of Object.keys(require.cache)) {
          const file = key.replace(/\?.*$/, "")
          if (roots.some((r) => file === r || file.startsWith(`${r}/`))) delete require.cache[key]
        }
      }

      const load = (entry: Entry) =>
        Effect.tryPromise(() => {
          evict(entry.root)
          // Resolve first: Bun 1.3.14 can't dynamically import a file created after
          // the process started unless the resolver has looked it up. The `?v=`
          // query busts the module cache so edits are picked up.
          return import(`${Bun.resolveSync(entry.file, dir)}?v=${generation}`) as Promise<Record<string, unknown>>
        })

      const reload = Effect.gen(function*() {
        generation += 1
        const nextSpells = new Map<string, LoadedSpell>()
        const nextDaemons = new Map<string, LoadedDaemon>()
        const errors: Array<string> = []
        const { found, helpers } = yield* entries()
        const helpersChanged = helpers !== helpersStamp
        if (helpersChanged && helpersStamp !== undefined) {
          for (const name of yield* fs.readDirectory(dir)) {
            if (name.startsWith("_")) evict(path.join(dir, name))
          }
        }
        helpersStamp = helpers
        const present = new Set(found.map((entry) => entry.id))
        for (const id of modules.keys()) if (!present.has(id)) modules.delete(id)

        for (const entry of found) {
          const previous = modules.get(entry.id)
          let exports = previous?.exports
          if (!previous || previous.stamp !== entry.stamp || helpersChanged) {
            const loaded = yield* load(entry).pipe(
              Effect.catch((error) => {
                errors.push(`${entry.id}: ${String(error.cause)}`)
                return Effect.as(Effect.logError(`Failed to load ${entry.file}`, error.cause), undefined)
              })
            )
            // A file that fails to load keeps its previous version, so a typo
            // mid-edit doesn't stop a running daemon.
            // Keeping the old stamp makes the next reload retry it (and report the error again).
            if (loaded) exports = loaded
            if (exports) modules.set(entry.id, { stamp: loaded ? entry.stamp : previous?.stamp ?? "", exports })
          }
          if (!exports) continue
          for (const [name, value] of Object.entries(exports)) {
            const id = name === "default" ? entry.id : `${entry.id}:${name}`
            if (Spell.isSpell(value)) nextSpells.set(id, { id, spell: value })
            else if (Daemon.isDaemon(value)) nextDaemons.set(id, { id, daemon: value })
          }
        }
        yield* Ref.set(spells, nextSpells)
        yield* SubscriptionRef.set(daemons, nextDaemons)
        const commands: Array<WireCommand> = [...nextSpells.values()].map(({ id, spell }) => ({
          id,
          title: spell.title,
          subtitle: spell.subtitle,
          icon: spell.icon,
          keywords: spell.keywords ?? [],
          mode: spell.mode ?? "view"
        }))
        yield* wire.send({ type: "commands", commands, errors })
        yield* Effect.logInfo(
          `Loaded ${commands.length} spell(s) and ${nextDaemons.size} daemon(s) from ${dir}${
            errors.length ? `, ${errors.length} failed` : ""
          }`
        )
      }).pipe(
        Effect.catchCause((cause) => Effect.logError("Reload failed", cause)),
        lock.withPermit,
        Effect.withSpan("Registry.reload")
      )

      yield* seed().pipe(Effect.catchCause((cause) => Effect.logError("Could not create the grimoire", cause)))
      yield* link().pipe(Effect.catchCause((cause) => Effect.logError("Could not link spell modules", cause)))
      yield* reload

      // Hot reload: any change outside node_modules re-imports what changed.
      yield* fs.watch(dir, { recursive: true }).pipe(
        Stream.filter((event) => !event.path.includes("node_modules")),
        Stream.debounce(Duration.millis(200)),
        Stream.runForEach(() => reload),
        Effect.catchCause((cause) => Effect.logWarning("Spell watcher stopped", cause)),
        Effect.forkScoped
      )

      return Registry.of({
        get: (id) => Effect.map(Ref.get(spells), (map) => map.get(id)),
        daemons,
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
