# Incant

A language for spells written as rituals. Status: **draft spec, nothing built.**

A `.spell.md` file in the grimoire is Markdown that reads as a ritual and runs as a
program. It sits beside the `.ts` spells, is enabled the same way (`spells.json`), and
its body is what the Grimoire book shows on the right-hand page.

## 1. The four forms

| Form | What it is | Title begins | Ends with | Compiles to |
|---|---|---|---|---|
| **Spell** | cast from the bar | anything else | `_So mote it be._` | `Spell.make` |
| **Cantrip** | a spell of a single sentence | anything else | nothing | `Spell.make`, no-view |
| **Ritual** | runs on a schedule, keeps nothing | `Ritual of` | `_So mote it be._` | a host-owned daemon with no menu bar item |
| **Summoning** | a demon: lives on, remembers, answers | `Conjure` | `_So it is bound._` | `Daemon.summon` |

Forms that act on their own (rituals, summonings) say so in the title. Forms that you
cast yourself don't need to: their shape tells them apart.

## 2. The file

### 2.1 Name and place

- A spell is `<true-name>.spell.md` at the top of the grimoire.
- The **true name** is the filename's stem: lowercase, `[a-z][a-z0-9-]*`. It's what the
  CLI (`cantrip insomnia on`), `_Invoke_` and `_Command_` use, and what the book prints
  under the title. Renaming the file renames the spell.
- Nothing in the file names itself as code: the plain name lives in front matter and
  the poetic name is the title.

### 2.2 Layout

```
---
front matter        metadata (YAML)
---

# The Title         required; the first thing after the front matter

body                verse, workings, sections, in any order the form allows

closing words       _So mote it be._ or _So it is bound._ (none for a cantrip)
```

### 2.3 Front matter

| Key | Forms | Required | Meaning |
|---|---|---|---|
| `name` | all | yes | The plain name: the bar, the left page, toasts, the menu bar tooltip. |
| `icon` | all | no (`sparkles`) | SF Symbol for the bar and the left page. |
| `aliases` | all | no | Extra words the bar's search matches. |
| `description` | all | no | Subtitle in the bar; the description on the left page. |
| `schedule` | ritual | yes | When the ritual runs (§3.3). |
| `sudo` | all | no | Exact commands this spell may run as root, without a password (§4.3). |

Front matter is plain configuration: its keys say literally what they are. The
ritual's voice belongs to the title and the body.

Unknown keys are an error, so a misspelled key can't be silently ignored.

### 2.4 The title

- Required. Exactly one `#` heading, the first thing after the front matter, plain
  text only (no rubrics or values).
- Its opening words decide the form: `Conjure …` is a summoning and `Ritual of …` is
  a ritual. Anything else is a spell or a cantrip, depending on its shape:
  *Conjure Vigil, the Sleepless One*, *Ritual of Cleansing*.
- These openings are reserved: no spell's title may begin with `Conjure` or
  `Ritual of`. *Conjure a Password* would be a summoning, so call that spell
  *Forge a Password*. *Conjurer's Guide* and *Ritual Timer* are fine: the match is on
  whole words, and only the whole phrase `Ritual of` is reserved.
- The title can't lie: a `Conjure` file without a demon's sections, or a spell
  that tries to keep a vessel, is an error.
- The title sits on the right page in the display face, where "Incantation" is now.

### 2.5 Blocks

| Markdown | Is | Runs |
|---|---|---|
| `> lines` | **verse**: the words of the ritual, shown on the page | never |
| a line with `_rubrics_` | a **working** (§2.6) | yes |
| `- list` | the body of the working above it that ends in `:` | yes |
| `## Heading` | a **section** of a summoning (§3.4); fixed names | — |
| table | data, after `_Gather into the_ ☐:` | — |
| ` ```ts ` fence | the **old tongue** (§2.10) | yes |
| ` ```md ` fence | a page to unveil, after `_Unveil:_` | — |
| `---` | a separator in a throne's menu; a fleuron anywhere else | — |
| `<!-- … -->` | a note for the scribe | not shown, never runs |

Anything else (a plain paragraph with no rubric, images, links, raw HTML) is an error:
*"This line is neither working nor verse. Set verse apart with `>`."* That's why verse
is quoted rather than "any line without a verb": a mistyped rubric can't quietly
become poetry.

### 2.6 Rubrics and gaps

- `_underscores_` are **rubrics**: the fixed words of the language, drawn in red ink, as
  in a medieval rite. `*asterisks*` are plain emphasis, allowed only in verse.
- A working alternates rubrics and **gaps**, and must match one of the patterns in §4.
  This spec writes patterns with ☐ for each gap: `_Lay_ ☐ _upon the glass_`.
- Rubrics are matched ignoring case, extra whitespace, and punctuation at their edges,
  so `_Draw a name unknown, and call it_ id` and `_Draw a name unknown_, _and call it_ id`
  are the same working.
- An unknown rubric is an error, with the nearest known one as a suggestion.

### 2.7 Sentences

- A sentence ends with `.`. Line breaks inside a sentence are wrapping: the page keeps
  them, the parser ignores them.
- `,`, `;` and `_then_` chain clauses in one sentence; they run in order.
- A **governing** clause (`_Should_ ☐`, `_For each_ ☐ _in_ ☐`, `_When commanded_ ☐`,
  `_Offer_ ☐`, `_Each time the seeker speaks …_`, `_Every_ ☐`) governs:
  - the rest of its sentence, when followed by `,`;
  - the list, table or fence below it, when followed by `:`.
- `_Else_` starts the sentence or list item right after a `_Should_`.
- `…, _and call it_ ☐` (or `_call them_`) names what the clause before it produced.

### 2.8 Values

What goes in a gap:

| Gap | Example | Meaning |
|---|---|---|
| `"text"` | `"Copied {id}"` | words for people. `{…}` interpolates; `{{` is a brace. |
| `` `text` `` | `` `pmset -a disablesleep 1` `` | words for machines: commands, paths, URLs. Interpolates too. |
| number | `3`, `0.5`, `thrice` | `once`, `twice` and `thrice` work in count slots. |
| duration | `10 minutes`, `an hour`, `a day` | also `a breath` (1 s), `a fortnight`, `a moon's turn` (29.5 days) |
| truth | `true`, `false` | |
| nothing | `nothing`, `empty` | no value; an empty list |
| name | `id`, `last omen`, `omen.package.name` | bound earlier; may be several words; `.` reaches into a field |
| sigil | `cup.and.saucer.fill` | in slots that take a sigil |
| true name | `insomnia` | in slots that take a spell or a demon |
| expression | `"Let Sleep" _if_ awake _else_ "Keep Awake"` | §4.7 |

**Each slot has a type**, and the gap is read as that type. That's how
`cup.and.saucer` is a sigil in `_Wear_ ☐` but `omen.package.name` is a field in
`_Name it_ ☐`. A name you've bound always wins over a sigil.

**The slot decides the escaping.** Values interpolated into a cauldron's command are
shell-quoted; values interpolated into a raven's or a door's URL are percent-encoded.
`` `bun add {name}` `` can't be turned into a second command by a strange package name.

### 2.9 Names

- `_Let_ ☐ _be_ ☐.` or `…, _and call it_ ☐` bind a name. A name lives until the end of
  the block it was bound in. Naming the same thing twice is an error; the vessel is the
  only thing that changes (`_Fill_`).
- Names given by the language: `the reason` (inside `_Should the working sour_` and
  `_Should it slip its bonds_`), plus whatever a governing clause binds
  (`_For each_ word …`, `_call their words_ query`).

### 2.10 The old tongue

When the rubrics run out, drop into TypeScript:

````markdown
_Speak in the old tongue, and call the answer_ count:
```ts
text.split(/\s+/).filter(Boolean).length
```
````

- The fence is the body of an `Effect.gen`: names in scope are variables (`last omen`
  becomes `lastOmen`), `yield*` works, and `import` lines at the top may import
  `cantrip` and `effect/*`.
- The last expression, or `return`, is the answer. Throwing or failing sours the working.

## 3. The forms in full

### 3.1 Spell

- Cast from the bar.
- If it lays an altar (`_Upon the altar …_`) or unveils a page, it opens in the bar.
  Otherwise the bar vanishes at once and it works in the background (no-view).
- May end with one `_Should the working sour:_` block, just before the closing words.
- No `##` sections, no vessel.
- Ends with `_So mote it be._`

````markdown
---
name: Spellbook
icon: book.closed
description: An example list with every kind of offer.
---

# A Book of Shadows

> Four old words of power, carved in stone,
> upon the altar, each laid out alone.

_Gather into the_ tome:

| name | effect | sigil |
|---|---|---|
| Lumos | lights the tip of the wand | lightbulb |
| Accio | summons a thing unto thee | hand.raised |
| Alohomora | unlocks what is barred | lock.open |
| Wingardium Leviosa | lifts what is heavy | feather |

_Upon the altar lay each_ word _of_ tome, _asking_ "Which word of power?":
- _Name it_ word.name.
- _Tell of_ word.effect.
- _Mark it_ word.sigil.
- _Scratch beside it_ "{the count of word.name} letters".
- _Offer_ "Cast", _marked_ sparkles,
  _toll the bell of triumph_ "{word.name}!", _telling_ word.effect.
- _Offer_ "Show Details", _unveil:_
  ```md
  # {word.name}

  {word.effect}.
  ```
- _Offer_ "Copy Incantation", _lay_ word.name _upon the glass_.
- _Offer_ "Look It Up",
  _throw wide the door to_ `https://www.google.com/search?q={word.name}`.

_So mote it be._
````

A no-view spell:

````markdown
---
name: Word Count
icon: textformat.123
aliases: [count, words]
description: Counts the words on the clipboard.
---

# The Tally of Many Words

_Draw from the glass what lies there, and call it_ text.
_Speak in the old tongue, and call the answer_ count:
```ts
text.split(/\s+/).filter(Boolean).length
```
_Breathe_ "{count} words" _upon the wind_.

_So mote it be._
````

### 3.2 Cantrip

- A spell whose body is **one sentence** of workings: no lists, tables, fences or
  sections. Verse is allowed above it.
- Always no-view. No closing words: the one sentence is the whole ritual.

```markdown
---
name: Shrug
icon: face.smiling
---

# The Charm of Indifference

> Neither yea nor nay I say.

_Lay_ "¯\_(ツ)_/¯" _upon the glass_.
```

```markdown
---
name: Generate UUID
icon: number
aliases: [uuid, guid, id, random]
description: Copies a fresh v4 UUID.
---

# The Hex of the Nameless Number

> From the hollow, draw a name
> no soul has spoken, none can claim.

_Draw from the void a name unknown, and call it_ id;
_lay_ id _upon the glass_,
_then breathe_ "Copied {id}" _upon the wind_.
```

### 3.3 Ritual

- Title begins `Ritual of`; front matter has `schedule`.
- The body is like a no-view spell's, but nobody is watching: no altar, no unveiling.
  `_Breathe_` shows the HUD; `_Toll the bell_` posts a notification.
- **A ritual keeps nothing.** No vessel, no throne, no pact. It may invoke spells and
  command demons. A ritual that needs memory should be a summoning.
- If the Mac slept through a performance, the ritual is performed once on waking, not
  once per missed time. If the last performance is still running, the next is skipped.
- Casting a ritual from the bar or the book performs it now (handy for testing).
- Ends with `_So mote it be._`

`schedule` takes:

```
every 15 minutes          every hour            every 2 hours
every day at 09:00        every weekday at 09:00
on mondays and thursdays at 08:30
every day at midnight     every day at noon
when the moon is full     when the moon is new     (at midnight, or "… at 21:00")
```

Times are local.

```markdown
---
name: Sweep Downloads
icon: trash
description: Bins anything in Downloads older than a month.
schedule: when the moon is full
---

# Ritual of Cleansing

> What gathers, rots; what lingers, stains.
> Sweep the hearth of what remains.

_Sweep into the bin all within_ `~/Downloads` _older than_ a moon's turn.

_So mote it be._
```

```markdown
---
name: Morning
icon: sunrise
schedule: every weekday at 09:00
---

# Ritual of Waking

_Invoke_ open-calendar,
_then send the raven to_ `https://wttr.in/?format=3`, _and call what it carries_ sky,
_then toll the bell_ "The day is begun", _telling_ sky.

_So mote it be._
```

### 3.4 Summoning

- Title begins `Conjure`. Ends with `_So it is bound._`
- The body is verse and **sections**, in this order. All are optional, but a demon must
  have at least one of the Throne, the Pact or the Hunt.

| Section | Holds | Becomes |
|---|---|---|
| `## The Vessel` | what the demon remembers | state (`SubscriptionRef`) |
| `## The Throne` | its place in the menu bar | `MenuBar.render`, redrawn when the vessel changes |
| `## The Pact` | the commands it answers | the daemon's `command`; the CLI |
| `## The Hunt` | what it does on its own | schedules and listeners forked in its scope |
| `## The Beseeching` | a view of it in the bar | *new*: a spell the daemon provides |
| `## The Bonds` | what happens when it fails | restart policy |
| `## The Banishing` | what to undo when it stops for good | finalizer |
| `## The Passing` | what to do when handed to a new host | finalizer, on hand-off only |

#### The Vessel

```markdown
_Pour in_ awake, _at first_ false.
_Pour in_ hoard, _at first_ empty, _to outlast the grave_.
```

- A plain vessel survives hand-offs (reloads and Restart Host) and is lost when the
  demon is banished.
- `_to outlast the grave_` survives everything: it's saved to disk under the true name.
- Only the demon changes its vessel (`_Fill_`, `_Turn_ ☐ _upon its head_`, `_Cast_ ☐ _into the_ ☐` …;
  §4.6), and only in the Pact and the Hunt.

#### The Throne

Describes the menu bar item as it should look given the vessel. It's read again
whenever the vessel changes.

```markdown
_Wear_ cup.and.saucer.fill _if_ awake, _else_ cup.and.saucer.
_Whisper_ "Awake" _if_ awake, _else_ "Free to sleep" _to those who linger_.

- _Carve_ "Sleep".
- _Offer_ "Keep Awake", _checked if_ awake, _and when touched, command_ "toggle".
---
- _Offer_ "Usage", _bearing_ "{used}%", _leading to:_
  - …
```

- `_Wear_ ☐` takes a sigil; `_Wear a ring filled to_ ☐` takes a fraction (0–1) for a
  progress ring, with `_burning warning_` or `_burning critical_` for its tone.
- **The pact is the only door.** Touching a menu entry may lay something on the glass,
  open a door, or command the demon. Anything that changes the vessel goes through the
  Pact, so the menu and the CLI can't drift apart.

#### The Pact

```markdown
- _When commanded_ "on", …
- _When commanded_ "for" _with_ span, …
- _When asked of its state_, _answer_ "awake" _if_ awake, _else_ "asleep".
```

- `cantrip <true-name> <command> [words…]` from a shell; `_with_ ☐` binds the words
  after the command.
- `_When asked of its state_` answers a bare `cantrip <true-name>`. `_Answer_ ☐` replies
  to whoever asked.
- From other spells: `_Command_ insomnia "on".` and
  `_Ask_ insomnia _of its state, and call it_ mood.` Inside its own pact, a demon
  commands itself with `_command_ ☐` and no true name.
- Commands are handled one at a time, in the order they arrive.

#### The Hunt

```markdown
_Upon summoning:_            once, when it's summoned
_Every_ 10 minutes:          on a schedule (any `schedule` phrase without "every"
                             also works: _At_ 09:00 _each weekday:_)
_Whenever the throne is opened:_
_Whenever_ awake _changes:_
```

*Later:* `_Whenever aught is laid upon the glass:_` needs the host to watch the clipboard.

#### The Beseeching

```markdown
_It may be beseeched, asking_ "What was lost?":
- _Upon the altar lay each_ thing _of_ hoard …
```

Adds an entry to the bar (named after `name`) that shows the demon's live vessel.
Changes made from it still go through the Pact. *Needs a new SDK capability:
daemon-provided views.*

#### The Bonds

```markdown
_Should it slip its bonds, conjure it again no more than_ thrice, _each time waiting longer_.
_Should it slip them once more:_
- _Toll the bell of woe_ "Malphas has fled the circle", _telling_ the reason.
```

Without this section a failed demon stays down, and the ledger shows "Fled the circle"
with the reason.

#### The Banishing and the Passing

| The host stops the demon because… | `StopReason` | Runs |
|---|---|---|
| quit, crash, stdin EOF, bare SIGTERM | `shutdown` | the Banishing |
| disabled or deleted | `removed` | the Banishing |
| it failed | `failed` | the Banishing |
| reload, Restart Host, `cantrip handoff` | `reload`, `restart` | the Passing if there is one, **otherwise the Banishing** |

Running the Banishing when there's no Passing is the safe default: a demon that
cleans up still cleans up on a hand-off, unless it says otherwise. `_Leave all as it is._`
is the working for a Passing that does nothing.

#### Example

```markdown
---
name: Insomnia
icon: cup.and.saucer
aliases: [caffeinate, awake, sleep]
description: Keeps the Mac awake, lid open or shut.
sudo:
  - pmset -a disablesleep 0
  - pmset -a disablesleep 1
---

# Conjure Vigil, the Sleepless One

> By salt and ash I draw the ring,
> by blood and wax I name the thing;
> that keeps the lamp when all lamps die,
> and holds the lid of every eye.

## The Vessel

_Pour in_ awake, _at first_ false.

## The Throne

_Wear_ cup.and.saucer.fill _if_ awake, _else_ cup.and.saucer.

- _Offer_ "Keep Awake", _checked if_ awake, _and when touched, command_ "toggle".

## The Pact

- _When commanded_ "on",
  _mutter into the cauldron_ `pmset -a disablesleep 1` _sealed in blood_,
  _and fill_ awake _with_ true.
- _When commanded_ "off",
  _mutter into the cauldron_ `pmset -a disablesleep 0` _sealed in blood_,
  _and fill_ awake _with_ false.
- _When commanded_ "toggle",
  _command_ "off" _if_ awake, _else_ "on".
- _When asked of its state_,
  _answer_ "awake" _if_ awake, _else_ "asleep".

## The Banishing

> Let no machine be cursed to wake forever.

_Mutter into the cauldron_ `pmset -a disablesleep 0` _sealed in blood_.

## The Passing

_Leave all as it is._

_So it is bound._
```

```markdown
---
name: Status Watch
icon: bird
description: Tells you when the status page changes.
---

# Conjure Malphas, Who Watches the Wire

> Crow that circles, crow that waits,
> cry out when the world abates.

## The Vessel

_Pour in_ last omen, _at first_ nothing, _to outlast the grave_.

## The Hunt

_Every_ 10 minutes:
- _Send the raven to_ `https://status.example.com/api/v2/status.json`,
  _and call what it carries_ omen.
- _Should_ omen.status.description _differ from_ last omen,
  _toll the bell_ "The wind has turned", _telling_ omen.status.description,
  _and fill_ last omen _with_ omen.status.description.

## The Bonds

_Should it slip its bonds, conjure it again no more than_ thrice, _each time waiting longer_.
_Should it slip them once more:_
- _Toll the bell of woe_ "Malphas has fled the circle", _telling_ the reason.

_So it is bound._
```

## 4. The workings

☐ is a gap. *(new)* marks things the host or SDK can't do yet.

### 4.1 Drawing values

| Working | Does | Maps to |
|---|---|---|
| `_Let_ ☐ _be_ ☐` | names a value | `const` |
| `…, _and call it_ ☐` | names what the clause before produced | `const` |
| `_Draw from the void a name unknown_` | a fresh UUID | `crypto.randomUUID()` |
| `_Draw from the glass what lies there_` | the clipboard's text | *(new)* |
| `_Read the hour_` | the current time | `Clock` |
| `_Gather into the_ ☐:` + table | static data; columns become fields | array literal |
| `_Speak in the old tongue, and call the answer_ ☐:` + ` ```ts ` | TypeScript | `Effect.gen` |

### 4.2 The bar

| Working | Does | Maps to |
|---|---|---|
| `_Upon the altar lay each_ ☐ _of_ ☐, _asking_ ☐:` | a filtered list | `Ui.list` |
| … `- _Name it_ ☐` | item title | `ListItem.title` |
| … `- _Tell of_ ☐` | item subtitle | `subtitle` |
| … `- _Mark it_ ☐` | item sigil | `icon` |
| … `- _Scratch beside it_ ☐` | an accessory | `accessories` |
| … `- _Known also as_ ☐` | item keywords | `keywords` |
| … `- _Offer_ ☐` (`_marked_ ☐`) | an action, governing its body | `Action.*` (below) |
| `_Should the altar stand empty, say_ ☐` | empty text | `emptyText` |
| `_Each time the seeker speaks, call their words_ ☐:` | live search; the body ends in an altar | `Ui.searchList` |
| `_Unveil_ ☐` / `_Unveil:_` + ` ```md ` (`_noting_ ☐ _as_ ☐`) | a detail page | `View.detail` |
| `_Lay_ ☐ _upon the glass_` | copy | `Ui.copy` |
| `_Breathe_ ☐ _upon the wind_` | HUD | `Ui.hud` |
| `_Toll the bell_ ☐` (`_telling_ ☐`) | toast | `Ui.toast` info |
| `_Toll the bell of triumph_ ☐` / `_of woe_ ☐` | success / failure toast | `Ui.toast` |
| `_Vanish_` | hide the bar | `Ui.hide` |
| `_Turn back_` | go back a view | `Ui.pop` |

An offer whose body is a single `_Lay … upon the glass_` compiles to `Action.copy`;
a single `_Throw wide the door to …_` to `Action.open`; an altar or an unveiling to
`Action.push`; anything else to `Action.run`.

### 4.3 The world

| Working | Does | Maps to |
|---|---|---|
| `_Throw wide the door to_ ☐` | open a URL, path or app | `Ui.open` |
| `_Send the raven to_ ☐` (`_bearing_ ☐`), `_and call what it carries_ ☐` | HTTP GET (POST when bearing); JSON is parsed | `HttpClient` |
| `_Mutter into the cauldron_ ☐` (`_sealed in blood_`), `_and call what rises_ ☐` | run a shell command; its trimmed stdout; a non-zero exit sours | `effect/process` |
| `_Read the scroll_ ☐` | read a file | `FileSystem` |
| `_Inscribe_ ☐ _upon the scroll_ ☐` | write a file | `FileSystem` |
| `_Sweep into the bin_ ☐` | move to the Trash | *(new)* |
| `_Sweep into the bin all within_ ☐ _older than_ ☐` | Trash a folder's old files | *(new)* |
| `_Wait_ ☐` | sleep | `Effect.sleep` |

`_sealed in blood_` runs a command as root, through `sudo`. It's only allowed for a
command listed in `sudo:`, matched exactly:

- The host turns the list into a sudoers rule for those exact commands, at
  `/etc/sudoers.d/cantrip-<true-name>`, so nothing else the program can do becomes
  passwordless. Programs are resolved to absolute paths (`pmset` is `/usr/bin/pmset`).
- The first time the spell needs it, the host shows the commands and asks for an
  admin password once to install the rule. Changing the list asks again. Disabling
  or deleting the spell offers to remove the rule.
- A sealed command can't contain `{…}`: a command that changes with its values
  can't match a fixed rule. Choose between fixed commands with `_Should_` instead.

*"The cauldron asks blood for `pmset -a disablesleep 1`, but none was promised. Add it
to `sudo:`."*

### 4.4 Other spells and demons

| Working | Does |
|---|---|
| `_Invoke_ ☐` | casts a spell or performs a ritual, by true name |
| `_Command_ ☐ ☐` (`_with_ ☐`) | commands a demon |
| `_Ask_ ☐ _of its state_` | a demon's answer to `_When asked of its state_` |

### 4.5 Flow

| Working | Does |
|---|---|
| `_Should_ ☐` … `_Else_` … | if / else |
| `_For each_ ☐ _in_ ☐` | loop |
| `_then_` | sequence (same as `,`) |
| `_Should the working sour:_` | catches failure anywhere above; binds `the reason` |
| suffix `_trying no more than_ ☐` | retry (`thrice`, `5 times`), with backoff |
| suffix `_waiting no longer than_ ☐` | timeout |

### 4.6 The vessel (summonings only)

| Working | Does |
|---|---|
| `_Pour in_ ☐, _at first_ ☐` (`_to outlast the grave_`) | declares a vessel |
| `_Fill_ ☐ _with_ ☐` | sets it |
| `_Turn_ ☐ _upon its head_` | flips a truth |
| `_Add_ ☐ _to_ ☐` / `_Take_ ☐ _from_ ☐` | arithmetic |
| `_Cast_ ☐ _into the_ ☐` (`_keeping no more than_ ☐`) | appends to a list, dropping the oldest |
| `_Cast_ ☐ _out of the_ ☐` | removes from a list |
| `_Empty the_ ☐` | clears it |

### 4.7 Expressions

Conditions are subjunctive, as in old rites:

| Expression | Means |
|---|---|
| `☐ _if_ ☐ _else_ ☐` | choose |
| a bare name | true if set and not false or empty |
| `☐ _be_ ☐` / `☐ _not be_ ☐` | equals / doesn't |
| `☐ _be more than_ ☐` / `_be less than_ ☐` | compare |
| `☐ _differ from_ ☐` | not equal (reads better after `_Should_`) |
| `☐ _hold_ ☐` | contains |
| `_not_ ☐`, `☐ _and_ ☐`, `☐ _or_ ☐` | logic (only inside a condition's gap) |
| `_the count of_ ☐` | length |

Anything more involved belongs in the old tongue.

## 5. On the page

The right page renders the body, not the code:

- The **title** in the display face, where "Incantation" is now, wrapping to two lines
  if it must. The **true name** below it, small, like a scribe's note.
- **Rubrics** in the theme's accent ink; gap values in the body ink; `"strings"` with
  curly quotes; `` `machine words` `` and fences in mono.
- **Verse** indented, in italic.
- `##` sections in small caps in the display face; lists with ❧; tables ruled;
  `---` as a fleuron; comments hidden; the closing words centered.

The left page keeps what it has: the illuminated `name`, the form's line ("keeps watch
in the menu bar", "performed every weekday at 09:00"), `description`, and the ledger, which
gains **Schedule** for rituals and **Sudo**, listing the commands, for anything that declares them.

## 6. Loading and reloading (sketch)

- The Registry picks up `*.spell.md` beside `*.ts`, parses it, and compiles it to a
  `Spell` or `Daemon` value: an interpreter running on Effect, no TypeScript written to
  disk. A file that doesn't parse shows in the book as failing to load, with the error.
- Enablement is unchanged: `spells.json`'s `enabled` list, by true name.
- **Reload keeps a demon running if its program didn't change.** The program is the
  parsed workings; verse, comments, the title, `description` and `aliases` aren't part of it.
  Re-wording a poem never restarts a demon. Changing a working hands the demon to a new
  master: the Passing runs, the new one is summoned, and its vessel carries over.
- The protocol (`Protocol.ts` and `protocol.rs` together) gains the entry's form and
  title, and the book's source for a `.spell.md` is its body.

## 7. Errors

Errors speak in the book's voice, followed by the plain reason and the line:

| | |
|---|---|
| *"This page has no name."* | missing `#` title |
| *"No working begins _Lay … upon the glas_. Did you mean _upon the glass_?"* | unknown rubric |
| *"This line is neither working nor verse. Set verse apart with `>`."* | stray paragraph |
| *"The working is unfinished. It should end: _So mote it be._"* | missing closing words |
| *"Blood seals only words that never change."* | `{…}` in a sealed command |
| *"A ritual keeps nothing. Conjure a demon to remember for you."* | vessel in a ritual |
| *"A demon with no throne, no pact and no hunt is only a shadow."* | empty summoning |
| *"Only the pact may change what the vessel holds."* | throne entry changing state |
| *"id already has a name."* | rebinding |
| *"No spell answers to open-calender."* | unknown true name |
| *"Only demons are conjured. Call this spell something else, or give it a pact."* | a `Conjure` title on a spell's body |
| *"A ritual needs its hour. Give it a time to be performed, or call it something else."* | a `Ritual of` title without `schedule` |
| *"A cantrip is a single breath. This one takes three."* | multi-sentence cantrip |

## 8. Open questions

1. **Long incantations.** A summoning can outgrow one page. Continue onto the next leaf
   (as a real grimoire continues a rite overleaf), or shrink to fit? The book is one
   spread per spell today.
2. **Many small cantrips.** One file each is heavy for one-liners. Allow a
   `cantrips.spell.md` with one `#` per cantrip, each its own spread?
3. **New host support:** reading and watching the clipboard, the Trash, daemon-provided
   views (the Beseeching), lunar schedules.
4. **The name.** *Incant* is a working title.
