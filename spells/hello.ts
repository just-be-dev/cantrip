// A static list with every kind of action. Swift filters the list as you type.
import * as Effect from "effect/Effect"
import { Action, Spell, Ui, View } from "incant"

const spells = [
  { name: "Lumos", effect: "Lights the tip of the wand", icon: "lightbulb" },
  { name: "Accio", effect: "Summons an object", icon: "hand.raised" },
  { name: "Alohomora", effect: "Unlocks doors", icon: "lock.open" },
  { name: "Expelliarmus", effect: "Disarms an opponent", icon: "wand.and.sparkles" },
  { name: "Wingardium Leviosa", effect: "Makes objects fly", icon: "feather" }
]

export default Spell.make({
  title: "Spellbook",
  subtitle: "Example: a static list with actions",
  icon: "book.closed",
  run: Ui.list({
    placeholder: "Search the spellbook…",
    items: spells.map((spell) => ({
      title: spell.name,
      subtitle: spell.effect,
      icon: spell.icon,
      accessories: [`${spell.name.length} letters`],
      actions: [
        Action.push(
          "Show Details",
          View.detail({
            markdown: `# ${spell.name}\n\n${spell.effect}.\n\n- Say it **clearly**\n- Mind the \`wand\` movement`,
            metadata: [
              { label: "Incantation", value: spell.name },
              { label: "Letters", value: String(spell.name.length) }
            ],
            actions: [Action.copy("Copy Incantation", spell.name)]
          })
        ),
        Action.copy("Copy Incantation", spell.name),
        Action.run(
          "Cast",
          Effect.gen(function*() {
            yield* Ui.toast({ title: `${spell.name}!`, message: spell.effect, style: "success" })
          }),
          { icon: "sparkles" }
        ),
        Action.open("Look It Up", `https://www.google.com/search?q=${encodeURIComponent(spell.name)}`)
      ]
    }))
  })
})
