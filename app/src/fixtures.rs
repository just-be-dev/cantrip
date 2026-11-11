//! Headless scenes: states to render with `--snap`/`--tree --scene NAME`, and to start tests from.
//! They stand in for the host, which a headless run doesn't start.

use std::time::{Duration, Instant};

use crate::catalog::AppEntry;
use crate::protocol::*;
use crate::store::{Cantrip, Frame, Session, Toast};

pub type Scene = (&'static str, fn(&mut Cantrip));

pub const SCENES: &[Scene] = &[
    ("root", root),
    ("opening", opening),
    ("root-arcane", root_arcane),
    ("opening-arcane", opening_arcane),
    ("themes", themes),
    ("grimoire", grimoire),
    ("grimoire-arcane", grimoire_arcane),
    ("grimoire-opening", grimoire_opening),
    ("grimoire-turning", grimoire_turning),
    ("grimoire-turning-back", grimoire_turning_back),
    ("closing", closing),
    ("closing-arcane", closing_arcane),
    ("query", query),
    ("empty", empty),
    ("actions", actions),
    ("toast", toast),
    ("list", list),
    ("detail", detail),
    ("loading", loading),
    ("error", error),
];

fn command(
    id: &str,
    title: &str,
    subtitle: Option<&str>,
    icon: &str,
    mode: CommandMode,
) -> WireCommand {
    WireCommand {
        id: id.into(),
        title: title.into(),
        subtitle: subtitle.map(Into::into),
        icon: Some(icon.into()),
        keywords: Vec::new(),
        mode,
    }
}

pub fn commands() -> Vec<WireCommand> {
    vec![
        command(
            "hello.ts:hello",
            "Hello",
            Some("A list with every kind of action"),
            "hand.wave",
            CommandMode::View,
        ),
        command(
            "uuid.ts:uuid",
            "Generate UUID",
            Some("Copy a fresh UUID"),
            "number",
            CommandMode::NoView,
        ),
        command(
            "git.ts:branches",
            "Git Branches",
            None,
            "arrow.triangle.branch",
            CommandMode::View,
        ),
        command(
            "emoji.ts:party",
            "Party Mode",
            Some("Emoji icons work too"),
            "\u{1F389}",
            CommandMode::NoView,
        ),
    ]
}

fn apps() -> Vec<AppEntry> {
    [
        ("Calculator", "/System/Applications/Calculator.app"),
        ("Calendar", "/System/Applications/Calendar.app"),
        ("Finder", "/System/Library/CoreServices/Finder.app"),
        ("Safari", "/Applications/Safari.app"),
        (
            "System Settings",
            "/System/Applications/System Settings.app",
        ),
    ]
    .iter()
    .map(|(name, path)| AppEntry {
        name: (*name).into(),
        path: (*path).into(),
    })
    .collect()
}

fn daemon() -> WireStatusItem {
    WireStatusItem {
        title: "Insomnia".into(),
        daemon_icon: Some("cup.and.saucer".into()),
        icon: WireBarIcon::Ring {
            value: Some(0.4),
            tone: WireTone::Normal,
            busy: false,
            pulse: false,
            badge: false,
        },
        label: None,
        tooltip: None,
        menu: vec![WireMenuEntry::Item(WireMenuItem {
            id: "stay-awake".into(),
            title: "Keep Awake".into(),
            subtitle: Some("Until you turn it off".into()),
            icon: Some("bolt".into()),
            accessory: None,
            progress: None,
            tone: WireTone::Normal,
            checked: true,
            enabled: true,
            key: None,
            action: MenuAction::Callback,
            value: None,
            submenu: Vec::new(),
        })],
    }
}

pub fn root(s: &mut Cantrip) {
    s.commands = commands();
    s.apps = apps();
    s.daemons.insert("insomnia".into(), daemon());
    s.usage.insert("spell:hello.ts:hello".into(), 3);
    s.visible = true;
    s.changed();
}

/// Shown and waiting for the keyboard: `scripts/open.json` focuses it and the intro plays.
pub fn opening(s: &mut Cantrip) {
    root(s);
    s.revealed = false;
}

/// The root search in the Arcane theme (the others are in Sigil, the default).
fn root_arcane(s: &mut Cantrip) {
    root(s);
    s.set_theme(crate::views::theme::Theme::Arcane);
}

/// The opening in the Arcane theme.
fn opening_arcane(s: &mut Cantrip) {
    opening(s);
    s.set_theme(crate::views::theme::Theme::Arcane);
}

/// The theme picker, scrolled to Arcane: the bar wears it while it's highlighted.
fn themes(s: &mut Cantrip) {
    root(s);
    s.type_query("them");
    s.open_theme_picker();
    s.set_selection(1);
}

/// A grimoire with more in it than the root's: the book's scenes leaf through these.
fn tome() -> Vec<WireCommand> {
    let mut all = commands();
    let mut more = |id: &str, title: &str, subtitle: &str, icon: &str, keywords: &[&str]| {
        let mut c = command(id, title, Some(subtitle), icon, CommandMode::View);
        c.keywords = keywords.iter().map(|k| (*k).into()).collect();
        all.push(c);
    };
    more(
        "clipboard:history",
        "Clipboard History",
        "Everything you copied today, searchable, and pasted back with a word",
        "doc.on.clipboard",
        &["paste", "copy"],
    );
    more(
        "windows:arrange",
        "Arrange Windows",
        "Halves, thirds and quarters of the screen for the frontmost window",
        "rectangle.split.2x1",
        &["tile", "snap"],
    );
    more(
        "timer:brew",
        "Brew Timer",
        "Counts down while the tea steeps, and rings a bell when it's done",
        "timer",
        &["tea", "countdown"],
    );
    more(
        "weather:sky",
        "Read the Sky",
        "Tomorrow's weather, as the clouds would tell it",
        "cloud.sun",
        &["weather", "forecast"],
    );
    more(
        "notes:scribe",
        "Scribe a Note",
        "Write a line into today's journal",
        "pencil.and.scribble",
        &["journal"],
    );
    all
}

/// Spells in the grimoire that are disabled until enabled in the book.
pub fn dormant() -> Vec<WireEntry> {
    let entry = |id: &str, title: &str, summary: &str| WireEntry {
        id: id.into(),
        title: title.into(),
        enabled: false,
        summary: Some(summary.into()),
    };
    vec![
        entry(
            "insomnia",
            "Insomnia",
            "Keeps the Mac awake, lid closed too, with `pmset -a disablesleep`.",
        ),
        entry(
            "claude-usage",
            "Claude Usage",
            "A daemon: how much of your Claude plan you have left, as a ring in the menu bar.",
        ),
    ]
}

/// What a spell's file might say, for the book's incantation pages (a headless run reads no
/// files): the repository's own examples where they exist.
fn incantation(c: &WireCommand) -> String {
    match c.id.as_str() {
        "hello.ts:hello" => include_str!("../../grimoire/hello.ts").replace("export default", "export const hello ="),
        "uuid.ts:uuid" => include_str!("../../grimoire/uuid.ts").replace("export default", "export const uuid ="),
        _ => {
            let name = c.id.split(':').next_back().unwrap_or("spell");
            let subtitle = c.subtitle.clone().unwrap_or_default();
            let view = if c.mode == CommandMode::View {
                "  run: Ui.list({\n    placeholder: \"Search…\",\n    items: []\n  })"
            } else {
                "  run: Effect.gen(function*() {\n    yield* Ui.hud(\"Done\")\n  })"
            };
            format!(
                "// {subtitle}\nimport * as Effect from \"effect/Effect\"\nimport {{ Spell, Ui }} from \"cantrip\"\n\n// {}.\nexport const {name} = Spell.make({{\n  title: \"{}\",\n  subtitle: \"{subtitle}\",\n  icon: \"{}\",\n  keywords: [{}],\n{view}\n}})\n",
                c.title,
                c.title,
                c.icon.clone().unwrap_or_default(),
                c.keywords.iter().map(|k| format!("\"{k}\"")).collect::<Vec<_>>().join(", "),
            )
        }
    }
}

/// The book's incantations, as its fixtures' files would have them.
fn write_incantations(s: &mut Cantrip) {
    let Some(book) = &s.book else { return };
    for c in &s.commands {
        book.set_source(&c.id, &incantation(c));
    }
    book.set_source("insomnia", include_str!("../../grimoire/insomnia/index.ts"));
    book.set_source("claude-usage", include_str!("../../grimoire/claude-usage.ts"));
}

/// The grimoire as a book, open on its third spell.
fn grimoire(s: &mut Cantrip) {
    root(s);
    s.commands = tome();
    s.entries = dormant();
    s.open_book();
    if let Some(book) = &mut s.book {
        // Opened long ago: no cover in the air.
        *book = crate::views::book::Book::default();
        book.selection = 2;
    }
    write_incantations(s);
    s.changed();
}

/// The book in the Arcane theme: night vellum and star-ink.
fn grimoire_arcane(s: &mut Cantrip) {
    grimoire(s);
    s.set_theme(crate::views::theme::Theme::Arcane);
}

/// The book just opened: its cover swings over (render it over time with `--at`/`--strip`).
fn grimoire_opening(s: &mut Cantrip) {
    root(s);
    s.commands = tome();
    s.entries = dormant();
    s.open_book();
    write_incantations(s);
}

/// A leaf turning forward, from the third entry to the fourth (render with `--at`/`--strip`).
fn grimoire_turning(s: &mut Cantrip) {
    grimoire(s);
    s.set_selection(3);
}

/// A leaf turning back, from the third entry to the second (render with `--at`/`--strip`).
fn grimoire_turning_back(s: &mut Cantrip) {
    grimoire(s);
    s.set_selection(1);
}

/// Mid-exit: in Sigil the glass falls into a binding sigil (render it over time with
/// `--at`/`--strip`).
fn closing(s: &mut Cantrip) {
    root(s);
    s.visible = false;
    s.revealed = false;
    s.closing = true;
    s.outro = 1;
}

/// The exit in the Arcane theme: the glass disperses into stardust.
fn closing_arcane(s: &mut Cantrip) {
    closing(s);
    s.set_theme(crate::views::theme::Theme::Arcane);
}

fn query(s: &mut Cantrip) {
    root(s);
    s.type_query("cal");
}

fn empty(s: &mut Cantrip) {
    root(s);
    s.type_query("zzzz");
}

fn actions(s: &mut Cantrip) {
    root(s);
    s.actions_open = true;
    s.actions_selection = 1;
}

fn toast(s: &mut Cantrip) {
    root(s);
    s.toast = Some(Toast {
        style: ToastStyle::Failure,
        title: "1 spell(s) failed to load".into(),
        message: Some("broken.ts: Cannot find module 'nope'".into()),
        until: Instant::now() + Duration::from_secs(3600),
    });
}

fn action(id: &str, title: &str, icon: &str, kind: ActionKind, value: Option<&str>) -> WireAction {
    WireAction {
        id: id.into(),
        title: title.into(),
        icon: Some(icon.into()),
        kind,
        value: value.map(Into::into),
    }
}

pub fn hello_list() -> WireView {
    let item = |id: &str, title: &str, subtitle: &str, icon: &str, accessory: &str| WireListItem {
        id: id.into(),
        title: title.into(),
        subtitle: Some(subtitle.into()),
        icon: Some(icon.into()),
        accessories: vec![accessory.into()],
        keywords: Vec::new(),
        actions: vec![
            action(
                "copy",
                "Copy Title",
                "doc.on.doc",
                ActionKind::Copy,
                Some(title),
            ),
            action(
                "details",
                "Show Details",
                "info.circle",
                ActionKind::Push,
                None,
            ),
            action(
                "open",
                "Open Website",
                "safari",
                ActionKind::Open,
                Some("https://example.com"),
            ),
        ],
    };
    WireView::List {
        placeholder: Some("Filter greetings…".into()),
        filtering: true,
        empty_text: None,
        items: vec![
            item("en", "Hello", "English", "globe", "en"),
            item("fr", "Bonjour", "French", "globe.europe.africa", "fr"),
            item("ja", "こんにちは", "Japanese", "globe.asia.australia", "ja"),
            item("es", "Hola", "Spanish", "globe.americas", "es"),
        ],
    }
}

fn session(s: &mut Cantrip, view: Option<WireView>) {
    root(s);
    let mut session = Session::new("s1".into(), commands()[0].clone());
    session.frames = vec![Frame {
        view,
        ..Frame::default()
    }];
    s.session = Some(session);
    s.changed();
    s.type_query("");
}

pub fn list(s: &mut Cantrip) {
    session(s, Some(hello_list()));
}

fn detail(s: &mut Cantrip) {
    session(s, Some(hello_list()));
    let view = WireView::Detail {
        markdown: "# Bonjour\n\nA greeting in **French**, said `bɔ̃.ʒuʁ`.\n\n- Formal enough for strangers\n- Casual enough for friends\n\n> Bonjour, le monde.\n\n```ts\nyield* Ui.copy(\"Bonjour\")\n```".into(),
        metadata: vec![
            WireMetadata { label: "Language".into(), value: "French".into() },
            WireMetadata { label: "Speakers".into(), value: "321 million".into() },
        ],
        actions: vec![action("copy", "Copy Greeting", "doc.on.doc", ActionKind::Copy, Some("Bonjour"))],
    };
    if let Some(session) = &mut s.session {
        session.frames.push(Frame {
            view: Some(view),
            ..Frame::default()
        });
    }
    s.changed();
    s.type_query("");
}

fn loading(s: &mut Cantrip) {
    session(s, None);
}

fn error(s: &mut Cantrip) {
    session(s, None);
    if let Some(session) = &mut s.session {
        session.error = Some((
            "Spell failed".into(),
            "TypeError: undefined is not a function\n    at hello.ts:12:5".into(),
        ));
    }
}
