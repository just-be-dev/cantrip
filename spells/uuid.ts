// A no-view spell: the bar hides right away and the spell runs in the background.
import * as Effect from "effect/Effect"
import { Spell, Ui } from "cantrip"

export default Spell.make({
  title: "Generate UUID",
  subtitle: "Copy a fresh v4 UUID",
  icon: "number",
  keywords: ["guid", "id", "random"],
  mode: "no-view",
  run: Effect.gen(function*() {
    const id = crypto.randomUUID()
    yield* Ui.copy(id)
    yield* Ui.hud(`Copied ${id}`)
  })
})
