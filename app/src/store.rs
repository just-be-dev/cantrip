//! App state: the root search, the running spell session, and what each key and action does.
//! Everything here runs on the UI thread, in Rune handlers and `AppHandle::update`s.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rune::{Key, KeyEvent, TextEdit, Tx};

use crate::catalog::AppEntry;
use crate::fuzzy;
use crate::icons::Icon;
use crate::protocol::*;
use crate::services::Services;
use crate::views::theme::Theme;
use crate::views::wand::{Gesture, Signals, Wand};

/// Set while a shown panel waits for its first drawn frame to start the intro.
pub static AWAITING_FRAME: AtomicBool = AtomicBool::new(false);
/// Play the intro anyway if no frame or keyboard arrives by then (another app kept focus).
pub const REVEAL_FALLBACK: Duration = Duration::from_millis(600);

/// The results list's scroll position: offset, visible height and furthest offset, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollWatch {
    pub offset: f32,
    pub viewport: f32,
    pub max: f32,
}

/// A glide asked for this recently may still be under way.
const GLIDE_RECENT: Duration = Duration::from_millis(600);

/// Reopening the bar after this long starts fresh at the root search (like Raycast).
pub const FRESH_AFTER: Duration = Duration::from_secs(90);

/// One row in a list, from the root search or from a spell.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Icon,
    pub accessories: Vec<String>,
    pub actions: Vec<ActionItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionItem {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub kind: ActionKindItem,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActionKindItem {
    Wire(WireAction),
    RunSpell(WireCommand),
    LaunchApp(AppEntry),
    RevealInFinder(String),
    CopyText(String),
    Builtin(Builtin),
    DaemonMenu { daemon: String, item: WireMenuItem },
}

impl ActionItem {
    fn wire(action: &WireAction) -> Self {
        ActionItem {
            id: action.id.clone(),
            title: action.title.clone(),
            icon: action.icon.clone(),
            kind: ActionKindItem::Wire(action.clone()),
        }
    }
    fn new(id: &str, title: &str, icon: &str, kind: ActionKindItem) -> Self {
        ActionItem {
            id: id.into(),
            title: title.into(),
            icon: Some(icon.into()),
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    ReloadGrimoire,
    RestartHost,
    OpenGrimoire,
    OpenJournal,
    Quit,
    /// Open the theme picker (`ThemePicker`).
    ChooseTheme,
    /// Switch the bar's magic to a theme, as the picker does when one is chosen.
    UseTheme(Theme),
}

impl Builtin {
    /// The ones the root search offers. (`UseTheme` is the picker's.)
    pub const ALL: [Builtin; 6] = [
        Builtin::ReloadGrimoire,
        Builtin::RestartHost,
        Builtin::OpenGrimoire,
        Builtin::OpenJournal,
        Builtin::ChooseTheme,
        Builtin::Quit,
    ];

    pub fn raw(self) -> &'static str {
        match self {
            Builtin::ReloadGrimoire => "reloadGrimoire",
            Builtin::RestartHost => "restartHost",
            Builtin::OpenGrimoire => "openGrimoire",
            Builtin::OpenJournal => "openJournal",
            Builtin::Quit => "quit",
            Builtin::ChooseTheme => "theme",
            Builtin::UseTheme(Theme::Sigil) => "theme:sigil",
            Builtin::UseTheme(Theme::Arcane) => "theme:arcane",
        }
    }

    /// What it actually does, for tooltips and the bar's subtitle.
    pub fn detail(self) -> &'static str {
        match self {
            Builtin::ReloadGrimoire => "Load new and changed spells (saving a file does this too)",
            Builtin::RestartHost => {
                "Restart the background process that runs spells and daemons; stops any running spell"
            }
            Builtin::OpenGrimoire => "Your spells folder",
            Builtin::OpenJournal => "The host's log",
            Builtin::Quit => "Close Cantrip",
            Builtin::ChooseTheme => "Preview the bar's magical themes and pick one",
            Builtin::UseTheme(t) => t.describe(),
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Builtin::ReloadGrimoire => "Reload Grimoire",
            Builtin::RestartHost => "Restart Host",
            Builtin::OpenGrimoire => "Open Grimoire",
            Builtin::OpenJournal => "Open Journal",
            Builtin::Quit => "Quit Cantrip",
            Builtin::ChooseTheme => "Change Theme",
            Builtin::UseTheme(Theme::Sigil) => "Sigil Theme",
            Builtin::UseTheme(Theme::Arcane) => "Arcane Theme",
        }
    }

    /// An SF Symbol name.
    pub fn icon(self) -> &'static str {
        match self {
            Builtin::ReloadGrimoire => "arrow.clockwise",
            Builtin::RestartHost => "arrow.triangle.2.circlepath",
            Builtin::OpenGrimoire => "books.vertical",
            Builtin::OpenJournal => "scroll",
            Builtin::Quit => "power",
            Builtin::ChooseTheme => "paintpalette",
            Builtin::UseTheme(Theme::Sigil) => "seal",
            Builtin::UseTheme(Theme::Arcane) => "sparkles",
        }
    }
}

/// The theme picker, over the root search: one row per theme, the highlighted one worn live
/// (palette, wand, spells and all) so scrolling previews it. ↵ keeps it; Esc, or the bar
/// closing, puts back the one it opened on.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemePicker {
    pub query: String,
    pub selection: usize,
    /// The theme in use when it opened.
    pub before: Theme,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub style: ToastStyle,
    pub title: String,
    pub message: Option<String>,
    pub until: Instant,
}

/// A view in a spell's stack. `view == None` means the spell hasn't rendered it yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub view: Option<WireView>,
    pub query: String,
    pub selection: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub id: String,
    pub command: WireCommand,
    pub frames: Vec<Frame>,
    pub host_loading: bool,
    pub awaiting_push: bool,
    pub error: Option<(String, String)>,
}

impl Session {
    pub fn new(id: String, command: WireCommand) -> Self {
        Session {
            id,
            command,
            frames: vec![Frame::default()],
            host_loading: false,
            awaiting_push: false,
            error: None,
        }
    }

    pub fn loading(&self) -> bool {
        self.host_loading
            || self.awaiting_push
            || (self.frames.last().is_some_and(|f| f.view.is_none()) && self.error.is_none())
    }
}

pub struct Cantrip {
    pub commands: Vec<WireCommand>,
    pub load_errors: Vec<String>,
    pub apps: Vec<AppEntry>,
    /// Each running daemon's menu bar item, by daemon id. Its menu actions are searchable too.
    pub daemons: BTreeMap<String, WireStatusItem>,
    pub session: Option<Session>,
    /// Open over the root search when no spell is (`Builtin::ChooseTheme`).
    pub picker: Option<ThemePicker>,
    pub actions_open: bool,
    pub actions_selection: usize,
    pub toast: Option<Toast>,
    /// The search field. It shows the query of whatever is on top (the root, or the session's
    /// top frame); `sync_field` puts that query back when the top changes.
    pub field: TextEdit,
    /// Whether the panel is on screen and has the keyboard.
    pub visible: bool,
    pub focused: bool,
    pub hidden_at: Option<Instant>,
    /// Set by Reload Grimoire / Restart Host; the next spell list reports back and clears it.
    pub reload_requested: bool,
    pub usage: HashMap<String, i64>,
    pub services: Services,
    /// When installed apps were last scanned.
    pub last_scan: Option<Instant>,
    /// Whether the bar shows its open pose. Hiding drops it, so the panel waits invisible until
    /// it's on screen with the keyboard, and the intro then plays from the start.
    pub revealed: bool,
    /// Counts openings; each one replays the intro. 0 means it has never played (launch,
    /// headless scenes, tests), so nothing animates in.
    pub intro: u64,
    /// The rows rise in after an opening, until the first keystroke.
    pub intro_rows: bool,
    /// The exit is playing; the window hides when it ends.
    pub closing: bool,
    /// Counts closings; each one replays the exit.
    pub outro: u64,
    shown_at: Option<Instant>,
    /// The results list's scroll position, as Rune last reported it.
    pub scroll: Rc<Cell<ScrollWatch>>,
    /// The 3D wand in the search bar, drawn on the frame clock (`animate_wand`).
    pub wand: Wand,
    /// How the bar's magic looks (`views::theme`).
    pub theme: Theme,
    /// Where the last glide is headed and when it was asked for: key repeat asks again before
    /// the list gets there, so the next step starts from where it's going.
    scroll_target: Option<(f32, Instant)>,

    root_query: String,
    root_selection: usize,
    next_session: u64,
    rows_cache: RefCell<Option<Rc<Vec<Row>>>>,
}

impl Cantrip {
    pub fn new(services: Services) -> Self {
        let mut field = TextEdit::new("");
        field.focused = true;
        let usage = services.load_usage();
        let theme = services.load_theme().unwrap_or_default();
        Cantrip {
            commands: Vec::new(),
            load_errors: Vec::new(),
            apps: Vec::new(),
            daemons: BTreeMap::new(),
            session: None,
            picker: None,
            actions_open: false,
            actions_selection: 0,
            toast: None,
            field,
            visible: false,
            focused: false,
            hidden_at: None,
            reload_requested: false,
            usage,
            services,
            last_scan: None,
            revealed: true,
            intro: 0,
            intro_rows: false,
            closing: false,
            outro: 0,
            shown_at: None,
            scroll: Rc::default(),
            wand: Wand::new(),
            theme,
            scroll_target: None,
            root_query: String::new(),
            root_selection: 0,
            next_session: 0,
            rows_cache: RefCell::new(None),
        }
    }

    /// Something the rows depend on changed.
    pub fn changed(&self) {
        self.rows_cache.borrow_mut().take();
    }

    // MARK: Query & selection

    pub fn query(&self) -> &str {
        match &self.session {
            Some(s) => s.frames.last().map_or("", |f| f.query.as_str()),
            None => match &self.picker {
                Some(p) => &p.query,
                None => &self.root_query,
            },
        }
    }

    /// The field's text changed: it becomes the query of whatever is on top.
    pub fn set_query(&mut self, text: String) {
        if text == self.query() {
            return;
        }
        self.intro_rows = false;
        self.actions_open = false;
        if let Some(session) = &mut self.session {
            let depth = session.frames.len() - 1;
            let frame = &mut session.frames[depth];
            frame.query = text.clone();
            frame.selection = 0;
            if let Some(WireView::List {
                filtering: false, ..
            }) = frame.view
            {
                let id = session.id.clone();
                self.services.send(AppMessage::Query {
                    session: id,
                    depth,
                    text,
                });
            }
        } else if let Some(picker) = &mut self.picker {
            self.wand.edited(&picker.query, &text, self.theme);
            picker.query = text;
            picker.selection = 0;
            self.changed();
            self.preview_theme();
        } else {
            self.wand.edited(&self.root_query, &text, self.theme);
            self.root_query = text;
            self.root_selection = 0;
        }
        self.changed();
    }

    /// Type into the field as a person would: the field and the query change together.
    pub fn type_query(&mut self, text: &str) {
        self.field.replace_text(text);
        self.field.set_caret(text.len());
        self.set_query(text.to_string());
    }

    /// Put the top's query in the field, for when the top changes (push, pop, a new session).
    fn sync_field(&mut self) {
        let query = self.query().to_string();
        if self.field.text != query {
            self.field.replace_text(query.clone());
            self.field.set_caret(query.len());
        }
        self.field.focused = true;
    }

    pub fn selection(&self) -> usize {
        match &self.session {
            Some(s) => s.frames.last().map_or(0, |f| f.selection),
            None => self.picker.as_ref().map_or(self.root_selection, |p| p.selection),
        }
    }

    pub fn set_selection(&mut self, index: usize) {
        match &mut self.session {
            Some(s) => {
                if let Some(f) = s.frames.last_mut() {
                    f.selection = index
                }
            }
            None => match &mut self.picker {
                Some(p) => {
                    p.selection = index;
                    self.preview_theme();
                }
                None => self.root_selection = index,
            },
        }
    }

    /// The label over the list, when it has one: the root's results, the picker's themes.
    pub fn section(&self) -> Option<&'static str> {
        if self.session.is_some() || !self.query().is_empty() {
            None
        } else if self.picker.is_some() {
            Some("Themes")
        } else {
            Some("Results")
        }
    }

    pub fn placeholder(&self) -> String {
        let Some(session) = &self.session else {
            if self.picker.is_some() {
                return "Choose a theme…".into();
            }
            return "Cast a spell or open an app…".into();
        };
        if let Some(Some(WireView::List {
            placeholder: Some(p),
            ..
        })) = session.frames.last().map(|f| &f.view)
        {
            return p.clone();
        }
        format!("Search {}…", session.command.title)
    }

    // MARK: What's on screen

    pub fn current_view(&self) -> Option<&WireView> {
        self.session.as_ref()?.frames.last()?.view.as_ref()
    }

    pub fn rows(&self) -> Rc<Vec<Row>> {
        if let Some(rows) = self.rows_cache.borrow().as_ref() {
            return rows.clone();
        }
        let rows = Rc::new(self.compute_rows());
        *self.rows_cache.borrow_mut() = Some(rows.clone());
        rows
    }

    fn compute_rows(&self) -> Vec<Row> {
        let Some(session) = &self.session else {
            if let Some(picker) = &self.picker {
                return theme_rows(&picker.query, picker.before);
            }
            return self.root_rows();
        };
        let Some(Some(WireView::List {
            filtering, items, ..
        })) = session.frames.last().map(|f| &f.view)
        else {
            return Vec::new();
        };
        let row = |item: &WireListItem| Row {
            id: item.id.clone(),
            title: item.title.clone(),
            subtitle: item.subtitle.clone(),
            icon: Icon::parse(item.icon.as_deref()),
            accessories: item.accessories.clone(),
            actions: item.actions.iter().map(ActionItem::wire).collect(),
        };
        let query = self.query();
        if !*filtering || query.is_empty() {
            return items.iter().map(row).collect();
        }
        let mut scored: Vec<(Row, i64)> = items
            .iter()
            .filter_map(|item| {
                let mut fields = vec![item.title.as_str(), item.subtitle.as_deref().unwrap_or("")];
                fields.extend(item.keywords.iter().map(String::as_str));
                fuzzy::score_fields(query, &fields).map(|s| (row(item), s))
            })
            .collect();
        scored.sort_by_key(|s| std::cmp::Reverse(s.1));
        scored.into_iter().map(|(r, _)| r).collect()
    }

    fn root_rows(&self) -> Vec<Row> {
        let mut candidates: Vec<(Row, Vec<String>, i64)> = Vec::new();
        let used = |id: &str| self.usage.get(id).copied().unwrap_or(0);
        for command in &self.commands {
            let id = format!("spell:{}", command.id);
            let mut fields = vec![
                command.title.clone(),
                command.subtitle.clone().unwrap_or_default(),
            ];
            fields.extend(command.keywords.iter().cloned());
            let row = Row {
                title: command.title.clone(),
                subtitle: command.subtitle.clone(),
                icon: Icon::parse(Some(command.icon.as_deref().unwrap_or("sparkles"))),
                accessories: vec!["Spell".into()],
                actions: vec![
                    ActionItem::new(
                        "run",
                        "Cast Spell",
                        "sparkles",
                        ActionKindItem::RunSpell(command.clone()),
                    ),
                    ActionItem::new(
                        "copy-id",
                        "Copy Spell ID",
                        "doc.on.doc",
                        ActionKindItem::CopyText(command.id.clone()),
                    ),
                ],
                id,
            };
            let u = used(&row.id);
            candidates.push((row, fields, u));
        }
        for app in &self.apps {
            let row = Row {
                id: format!("app:{}", app.path),
                title: app.name.clone(),
                subtitle: None,
                icon: Icon::File(app.path.clone()),
                accessories: vec!["Application".into()],
                actions: vec![
                    ActionItem::new(
                        "open",
                        "Open Application",
                        "arrow.up.forward.app",
                        ActionKindItem::LaunchApp(app.clone()),
                    ),
                    ActionItem::new(
                        "reveal",
                        "Show in Finder",
                        "folder",
                        ActionKindItem::RevealInFinder(app.path.clone()),
                    ),
                    ActionItem::new(
                        "copy-path",
                        "Copy Path",
                        "doc.on.doc",
                        ActionKindItem::CopyText(app.path.clone()),
                    ),
                ],
            };
            let u = used(&row.id);
            candidates.push((row, vec![app.name.clone()], u));
        }
        for (daemon, status) in &self.daemons {
            for (path, item) in menu_actions(&status.menu, &[]) {
                let title = path.join(" › ");
                let icon = item
                    .icon
                    .clone()
                    .or_else(|| status.daemon_icon.clone())
                    .or_else(|| status.icon.image_name().map(str::to_string));
                let mut accessories = if item.checked {
                    vec!["✓".to_string()]
                } else {
                    Vec::new()
                };
                accessories.push(status.title.clone());
                let row = Row {
                    id: format!("daemon:{daemon}:{title}"),
                    title: title.clone(),
                    subtitle: item.subtitle.clone(),
                    icon: Icon::parse(Some(icon.as_deref().unwrap_or("sparkle"))),
                    accessories,
                    actions: vec![ActionItem {
                        id: "choose".into(),
                        title: item.title.clone(),
                        icon: item.icon.clone(),
                        kind: ActionKindItem::DaemonMenu {
                            daemon: daemon.clone(),
                            item: item.clone(),
                        },
                    }],
                };
                let u = used(&row.id);
                candidates.push((
                    row,
                    vec![
                        title,
                        status.title.clone(),
                        item.subtitle.clone().unwrap_or_default(),
                    ],
                    u,
                ));
            }
        }
        for builtin in Builtin::ALL {
            let row = Row {
                id: format!("builtin:{}", builtin.raw()),
                title: builtin.title().into(),
                subtitle: Some(builtin.detail().into()),
                icon: Icon::parse(Some(builtin.icon())),
                accessories: vec!["Cantrip".into()],
                actions: vec![ActionItem::new(
                    "run",
                    builtin.title(),
                    builtin.icon(),
                    ActionKindItem::Builtin(builtin),
                )],
            };
            let u = used(&row.id);
            candidates.push((
                row,
                vec![builtin.title().into(), builtin.detail().into()],
                u,
            ));
        }

        if self.root_query.is_empty() {
            // Most used first; spells before apps when tied (a stable sort keeps their order).
            candidates.sort_by_key(|c| std::cmp::Reverse(c.2));
            return candidates.into_iter().map(|c| c.0).collect();
        }
        let mut scored: Vec<(Row, i64)> = candidates
            .into_iter()
            .filter_map(|(row, fields, u)| {
                fuzzy::score_fields(&self.root_query, &fields).map(|s| (row, s + (u * 15).min(300)))
            })
            .collect();
        scored.sort_by_key(|s| std::cmp::Reverse(s.1));
        scored.into_iter().map(|(r, _)| r).collect()
    }

    pub fn selected_row(&self) -> Option<Row> {
        self.rows().get(self.selection()).cloned()
    }

    pub fn current_actions(&self) -> Vec<ActionItem> {
        if let Some(WireView::Detail { actions, .. }) = self.current_view() {
            return actions.iter().map(ActionItem::wire).collect();
        }
        self.selected_row().map(|r| r.actions).unwrap_or_default()
    }

    // MARK: Actions

    pub fn perform(&mut self, action: ActionItem, tx: &mut Tx) {
        self.actions_open = false;
        if self.session.is_none() {
            self.wand.cue(Gesture::Cast);
        }
        match action.kind {
            ActionKindItem::RunSpell(command) => {
                self.bump_usage(&format!("spell:{}", command.id));
                self.run(command);
            }
            ActionKindItem::LaunchApp(app) => {
                self.bump_usage(&format!("app:{}", app.path));
                self.services.open(&app.path);
                self.hide_panel();
            }
            ActionKindItem::RevealInFinder(path) => {
                self.services.reveal(&path);
                self.hide_panel();
            }
            ActionKindItem::CopyText(text) => self.copy(text, tx),
            ActionKindItem::Builtin(builtin) => self.perform_builtin(builtin, tx),
            ActionKindItem::Wire(wire) => self.perform_wire(wire, tx),
            ActionKindItem::DaemonMenu { daemon, item } => {
                if let Some(row) = self.selected_row() {
                    self.bump_usage(&row.id);
                }
                self.choose_menu_item(&daemon, &item, tx);
            }
        }
    }

    /// A daemon's menu item chosen from the bar's search or from its menu bar item.
    pub fn choose_menu_item(&mut self, daemon: &str, item: &WireMenuItem, tx: &mut Tx) {
        match item.action {
            MenuAction::Callback => {
                self.services.send(AppMessage::Menu {
                    daemon: daemon.into(),
                    item: item.id.clone(),
                });
                self.hide_panel();
            }
            MenuAction::Copy => self.copy(item.value.clone().unwrap_or_default(), tx),
            MenuAction::Open => {
                self.services.open(item.value.as_deref().unwrap_or(""));
                self.hide_panel();
            }
            MenuAction::None => {}
        }
    }

    fn perform_wire(&mut self, action: WireAction, tx: &mut Tx) {
        let Some(session) = &mut self.session else {
            return;
        };
        let depth = session.frames.len() - 1;
        let id = session.id.clone();
        match action.kind {
            ActionKind::Copy => self.copy(action.value.unwrap_or_default(), tx),
            ActionKind::Open => {
                self.services.open(action.value.as_deref().unwrap_or(""));
                self.hide_panel();
            }
            ActionKind::Callback => self.services.send(AppMessage::Action {
                session: id,
                depth,
                action: action.id,
            }),
            ActionKind::Push => {
                session.awaiting_push = true;
                self.services.send(AppMessage::Action {
                    session: id,
                    depth,
                    action: action.id,
                });
            }
        }
    }

    pub fn perform_builtin(&mut self, builtin: Builtin, tx: &mut Tx) {
        match builtin {
            Builtin::ReloadGrimoire => {
                self.reload_requested = true;
                self.services.send(AppMessage::Reload);
                self.show_toast(ToastStyle::Info, "Reloading grimoire…", None, tx);
            }
            Builtin::RestartHost => {
                self.end_session();
                self.reload_requested = true;
                self.services.restart_host();
                self.show_toast(ToastStyle::Info, "Restarting host…", None, tx);
            }
            Builtin::OpenGrimoire => {
                self.services
                    .open(&crate::config::Config::grimoire_directory().to_string_lossy());
                self.hide_panel();
            }
            Builtin::OpenJournal => {
                self.services
                    .open(&crate::host::log_path().to_string_lossy());
                self.hide_panel();
            }
            Builtin::Quit => tx.quit(),
            Builtin::ChooseTheme => self.open_theme_picker(),
            Builtin::UseTheme(theme) => {
                if let Some(picker) = self.picker.take() {
                    // Back to a fresh root search, wearing it.
                    self.wand.edited(&picker.query, "", theme);
                    self.root_query.clear();
                    self.root_selection = 0;
                    self.sync_field();
                }
                self.set_theme(theme);
                self.services.save_theme(theme);
                let title = format!("{} theme", theme.title());
                self.show_toast(ToastStyle::Success, &title, None, tx);
            }
        }
    }

    /// The bar's magic. The wand draws afresh in it on its next frame.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.changed();
    }

    /// Opens the theme picker on the theme in use. The root query goes back into the wand.
    pub fn open_theme_picker(&mut self) {
        self.end_session();
        let before = self.theme;
        self.wand.edited(&self.root_query, "", before);
        self.picker = Some(ThemePicker {
            query: String::new(),
            selection: Theme::ALL.iter().position(|t| *t == before).unwrap_or(0),
            before,
        });
        self.actions_open = false;
        self.changed();
        self.sync_field();
    }

    /// Closes the theme picker without choosing: the theme it opened on comes back, and so
    /// does the root query.
    pub fn cancel_theme_picker(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        self.set_theme(picker.before);
        self.wand.edited(&picker.query, &self.root_query, picker.before);
        self.actions_open = false;
        self.sync_field();
    }

    /// Wear the picker's highlighted theme.
    fn preview_theme(&mut self) {
        if self.picker.is_none() {
            return;
        }
        let theme = self.selected_row().and_then(|r| {
            r.actions.iter().find_map(|a| match a.kind {
                ActionKindItem::Builtin(Builtin::UseTheme(t)) => Some(t),
                _ => None,
            })
        });
        if let Some(theme) = theme.filter(|t| *t != self.theme) {
            self.set_theme(theme);
        }
    }

    fn run(&mut self, command: WireCommand) {
        self.next_session += 1;
        let id = format!("s{}", self.next_session);
        if command.mode == CommandMode::NoView {
            self.hide_panel();
            self.services.send(AppMessage::Run {
                session: id,
                command: command.id,
            });
            return;
        }
        self.end_session();
        let command_id = command.id.clone();
        self.session = Some(Session::new(id.clone(), command));
        self.services.send(AppMessage::Run {
            session: id,
            command: command_id,
        });
        self.changed();
        self.sync_field();
    }

    pub fn pop_frame(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        let depth = session.frames.len() - 1;
        if depth == 0 {
            self.end_session();
            return;
        }
        session.frames.pop();
        session.awaiting_push = false;
        let id = session.id.clone();
        self.services.send(AppMessage::Pop { session: id, depth });
        self.changed();
        self.sync_field();
    }

    pub fn end_session(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        self.services.send(AppMessage::Close {
            session: session.id,
        });
        self.actions_open = false;
        self.changed();
        self.sync_field();
    }

    /// Back to the root search with an empty query.
    pub fn reset(&mut self) {
        self.end_session();
        self.root_query.clear();
        self.root_selection = 0;
        self.changed();
        self.sync_field();
    }

    pub fn copy(&mut self, text: String, tx: &mut Tx) {
        tx.copy(text);
        self.hide_panel();
        self.services.hud("Copied to clipboard", self.theme);
    }

    fn bump_usage(&mut self, key: &str) {
        *self.usage.entry(key.to_string()).or_insert(0) += 1;
        self.services.save_usage(&self.usage);
        self.changed();
    }

    pub fn show_toast(
        &mut self,
        style: ToastStyle,
        title: &str,
        message: Option<String>,
        tx: &mut Tx,
    ) {
        let life = Duration::from_secs(if style == ToastStyle::Failure { 6 } else { 3 });
        self.toast = Some(Toast {
            style,
            title: title.into(),
            message,
            until: Instant::now() + life,
        });
        tx.wake_after(life);
    }

    // MARK: The panel

    pub fn hide_panel(&mut self) {
        self.actions_open = false;
        if !self.visible {
            return;
        }
        // Leaving without choosing is choosing nothing.
        self.cancel_theme_picker();
        self.visible = false;
        self.focused = false;
        self.revealed = false;
        self.hidden_at = Some(Instant::now());
        AWAITING_FRAME.store(false, Ordering::Relaxed);
        if self.services.live {
            // Disperse first; the window hides when that's done (`finish_close`). The wand
            // spins away with it, unless it's in the middle of casting what closed the bar.
            if !self.wand.casting() {
                self.wand.cue(Gesture::Vanish);
            }
            self.closing = true;
            self.outro += 1;
            self.services
                .hide_panel_after(crate::views::outro::OUTRO, self.outro);
        } else {
            self.services.hide_panel();
        }
    }

    /// The exit `outro` has played: hide the window, unless the bar was reopened meanwhile.
    pub fn finish_close(&mut self, outro: u64) {
        if self.closing && self.outro == outro {
            self.closing = false;
            self.services.hide_panel();
        }
    }

    pub fn show_panel(&mut self) {
        if self.hidden_at.is_some_and(|at| at.elapsed() > FRESH_AFTER) {
            self.reset();
        }
        if self.closing {
            // Reopened mid-exit: the window is still up, so the intro plays straight away.
            self.closing = false;
            self.visible = true;
            self.sync_field();
            self.reveal();
            self.services.show_panel();
            return;
        }
        if !(self.visible && self.revealed) {
            self.revealed = false;
            self.shown_at = Some(Instant::now());
        }
        self.visible = true;
        self.sync_field();
        self.opening();
        self.services.show_panel();
    }

    /// The panel gained or lost the keyboard.
    pub fn focus_changed(&mut self, focused: bool) {
        self.focused = focused;
        if focused && self.visible && !self.revealed {
            if self.services.live {
                // The window's first visible frame comes a few hundred ms after the keyboard
                // does; `main` reveals on it, so the intro isn't spent while nobody can see it.
                AWAITING_FRAME.store(true, Ordering::Relaxed);
            } else {
                self.reveal();
            }
        } else if !focused && self.visible && !self.services.keep_open {
            self.hide_panel();
        }
    }

    /// Play the intro.
    pub fn reveal(&mut self) {
        AWAITING_FRAME.store(false, Ordering::Relaxed);
        self.revealed = true;
        self.intro += 1;
        self.intro_rows = true;
        self.shown_at = None;
    }

    /// The hotkey: show the bar, or hide it if it's up and has the keyboard.
    pub fn toggle_panel(&mut self) {
        if self.visible && self.focused {
            self.hide_panel()
        } else {
            self.show_panel()
        }
    }

    /// Draws the wand's next frame, `t` seconds into the frame clock. It's in the search bar
    /// while no spell is open (or one is opening as it casts), and up whenever the panel is
    /// open or dispersing. True when the view must rebuild for it.
    pub fn animate_wand(&mut self, t: f64, tx: &mut Tx) -> bool {
        let at_root = self.session.is_none() || self.wand.casting();
        let shown = at_root && (self.visible && self.revealed || self.closing);
        let fizzled =
            self.session.is_none() && !self.query().is_empty() && self.rows().is_empty();
        let query = match &self.picker {
            Some(p) => &p.query,
            None => &self.root_query,
        };
        let signals = Signals {
            theme: self.theme,
            shown,
            intro: self.intro,
            query,
            fizzled,
            scroll: self.scroll.get().offset,
        };
        let live = self.services.live;
        self.wand
            .frame(t, signals, || live && crate::platform::reduce_motion(), tx)
    }

    /// Timers: toasts and the HUD go away by themselves. True when something visible changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        // Shown but the keyboard never arrived (another app kept it): play the intro anyway.
        if self.visible
            && !self.revealed
            && self
                .shown_at
                .is_some_and(|at| now.duration_since(at) > REVEAL_FALLBACK)
        {
            self.reveal();
            return true;
        }
        if self.toast.as_ref().is_some_and(|t| t.until <= now) {
            self.toast = None;
            return true;
        }
        false
    }

    // MARK: Keys

    /// Returns true when the key was handled.
    pub fn handle_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        let m = ev.mods;
        let ctrl_letter = |c: char| {
            m.ctrl
                && !m.cmd
                && matches!(&ev.key, Key::Text(t) if t.eq_ignore_ascii_case(&c.to_string()))
        };
        match &ev.key {
            Key::Down => self.move_selection(1, tx),
            Key::Up => self.move_selection(-1, tx),
            _ if ctrl_letter('n') => self.move_selection(1, tx),
            _ if ctrl_letter('p') => self.move_selection(-1, tx),
            Key::Enter => self.submit(m.cmd, tx),
            Key::Command('k') if !m.shift => {
                self.actions_open = !self.actions_open;
                self.actions_selection = 0;
            }
            Key::Command('r') if !m.shift && self.session.is_none() => {
                self.perform_builtin(Builtin::ReloadGrimoire, tx)
            }
            Key::Escape => self.escape(),
            _ => return false,
        }
        true
    }

    /// ↵ runs the first action, ⌘↵ the second; with ⌘K open, the highlighted one.
    pub fn submit(&mut self, secondary: bool, tx: &mut Tx) {
        let actions = self.current_actions();
        let index = if self.actions_open {
            self.actions_selection
        } else if secondary {
            1
        } else {
            0
        };
        if let Some(action) = actions.into_iter().nth(index) {
            self.perform(action, tx);
        }
    }

    fn move_selection(&mut self, delta: i64, tx: &mut Tx) {
        if self.actions_open {
            let count = self.current_actions().len() as i64;
            if count > 0 {
                self.actions_selection =
                    ((self.actions_selection as i64 + delta).rem_euclid(count)) as usize;
            }
        } else {
            let count = self.rows().len() as i64;
            if count > 0 {
                let next = (self.selection() as i64 + delta).clamp(0, count - 1) as usize;
                // The wand flicks with the selection, or shakes its head at the list's end.
                let up = delta < 0;
                self.wand.cue(if next == self.selection() {
                    Gesture::Bump { up }
                } else {
                    Gesture::Flick { up }
                });
                self.set_selection(next);
                self.reveal_selection(tx);
            }
        }
    }

    /// Keep the selected row in view, gliding the list just far enough on the spring the
    /// highlight moves on, so the two travel together and the highlight holds still at the
    /// list's edge instead of jumping a row and springing back.
    pub fn reveal_selection(&mut self, tx: &mut Tx) {
        let at = self.scroll.get();
        if at.viewport <= 0.0 {
            // Not laid out yet: no glide to match.
            tx.scroll_into_view(row_key(self.selection()));
            return;
        }
        let header = self.section().is_some();
        let (top, bottom) = crate::views::row_span(self.selection(), header);
        let from = match self.scroll_target {
            Some((target, asked)) if asked.elapsed() < GLIDE_RECENT => target,
            _ => at.offset,
        };
        let pad = crate::views::LIST_PAD;
        let to = if self.selection() == 0 {
            0.0
        } else if top - pad < from {
            top - pad
        } else if bottom + pad > from + at.viewport {
            bottom + pad - at.viewport
        } else {
            return;
        };
        let to = to.clamp(0.0, at.max.max(0.0));
        if (to - from).abs() < 0.5 && (to - at.offset).abs() < 0.5 {
            return;
        }
        self.scroll_target = Some((to, Instant::now()));
        tx.scroll_to_with(crate::views::RESULTS, 0.0, to, crate::views::GLIDE);
    }

    fn escape(&mut self) {
        if self.actions_open {
            self.actions_open = false;
        } else if self.session.is_some() {
            self.pop_frame();
        } else if !self.query().is_empty() {
            self.set_query(String::new());
            self.sync_field();
        } else if self.picker.is_some() {
            self.cancel_theme_picker();
        } else {
            self.hide_panel();
        }
    }

    // MARK: Host messages

    pub fn receive(&mut self, message: HostMessage, tx: &mut Tx) {
        match message {
            HostMessage::Commands { commands, errors } => {
                let count = commands.len();
                self.commands = commands;
                self.load_errors = errors.clone();
                self.changed();
                if !errors.is_empty() {
                    let title = format!("{} spell(s) failed to load", errors.len());
                    self.show_toast(ToastStyle::Failure, &title, Some(errors.join("\n")), tx);
                } else if self.reload_requested {
                    self.wand.cue(Gesture::Recharge);
                    self.show_toast(
                        ToastStyle::Success,
                        &format!("{count} spells loaded"),
                        None,
                        tx,
                    );
                }
                self.reload_requested = false;
            }
            HostMessage::Render {
                session,
                depth,
                view,
            } => {
                let Some(s) = self.session.as_mut().filter(|s| s.id == session) else {
                    return;
                };
                if depth < s.frames.len() {
                    s.frames[depth].view = Some(view);
                } else if depth == s.frames.len() {
                    s.frames.push(Frame {
                        view: Some(view),
                        ..Frame::default()
                    });
                    s.awaiting_push = false;
                    self.actions_open = false;
                    self.changed();
                    self.sync_field();
                }
                self.changed();
                let count = self.rows().len();
                if self.selection() >= count {
                    self.set_selection(count.saturating_sub(1));
                }
            }
            HostMessage::Pop { session, depth } => {
                let Some(s) = self.session.as_mut().filter(|s| s.id == session) else {
                    return;
                };
                s.frames.truncate(depth);
                if s.frames.is_empty() {
                    self.end_session();
                } else {
                    self.changed();
                    self.sync_field();
                }
            }
            HostMessage::Closed { session } => {
                if self.session.as_ref().is_some_and(|s| s.id == session) {
                    self.session = None;
                    self.changed();
                    self.sync_field();
                }
            }
            HostMessage::Loading { session, loading } => {
                if let Some(s) = self.session.as_mut().filter(|s| s.id == session) {
                    s.host_loading = loading;
                }
            }
            HostMessage::Toast {
                style,
                title,
                message,
                ..
            } => self.show_toast(style, &title, message, tx),
            HostMessage::Hud { message } => {
                self.hide_panel();
                self.services.hud(&message, self.theme);
            }
            HostMessage::Hide => self.hide_panel(),
            HostMessage::Clipboard { text } => tx.copy(text),
            HostMessage::Open { target } => self.services.open(&target),
            HostMessage::Done { session, .. } => {
                if self.session.as_ref().is_some_and(|s| s.id == session) {
                    self.session = None;
                    self.changed();
                    self.sync_field();
                    self.hide_panel();
                }
            }
            HostMessage::Status { daemon, item } => {
                self.services.status(&daemon, &item);
                self.daemons.insert(daemon, item);
                self.changed();
            }
            HostMessage::StatusRemoved { daemon } => {
                self.services.status_removed(&daemon);
                self.daemons.remove(&daemon);
                self.changed();
            }
            HostMessage::Error {
                session,
                title,
                message,
            } => {
                if let Some(s) = self.session.as_mut().filter(|s| s.id == session) {
                    s.error = Some((title, message));
                    s.awaiting_push = false;
                } else {
                    eprintln!("Cantrip: {title}\n{message}");
                    self.services.hud(&format!("⚠︎ {title}"), self.theme);
                }
            }
        }
    }

    /// The host restarted: every session it knew about is gone.
    pub fn host_restarted(&mut self, tx: &mut Tx) {
        self.daemons.clear();
        self.services.remove_all_status();
        self.changed();
        if self.session.is_some() && !self.reload_requested {
            self.session = None;
            self.sync_field();
            self.show_toast(
                ToastStyle::Failure,
                "Host restarted",
                Some("The running spell stopped.".into()),
                tx,
            );
        }
    }
}

/// The picker's rows: every theme matching `query`, the one in use (`current`) ticked.
fn theme_rows(query: &str, current: Theme) -> Vec<Row> {
    Theme::ALL
        .into_iter()
        .filter(|t| query.is_empty() || fuzzy::score_fields(query, &[t.title(), t.describe()]).is_some())
        .map(|t| Row {
            id: format!("theme:{}", t.id()),
            title: t.title().into(),
            subtitle: Some(t.describe().into()),
            icon: Icon::parse(Some(Builtin::UseTheme(t).icon())),
            accessories: if t == current { vec!["✓".into()] } else { Vec::new() },
            actions: vec![ActionItem::new(
                "use",
                "Use Theme",
                "checkmark",
                ActionKindItem::Builtin(Builtin::UseTheme(t)),
            )],
        })
        .collect()
}

/// The element key of the row at `index` in the results list.
pub fn row_key(index: usize) -> String {
    format!("row-{index}")
}

/// Every enabled, choosable item in a daemon's menu, with its path of titles.
pub fn menu_actions(
    entries: &[WireMenuEntry],
    path: &[String],
) -> Vec<(Vec<String>, WireMenuItem)> {
    let mut out = Vec::new();
    for entry in entries {
        let WireMenuEntry::Item(item) = entry else {
            continue;
        };
        if !item.enabled {
            continue;
        }
        let mut here = path.to_vec();
        here.push(item.title.clone());
        if item.action != MenuAction::None {
            out.push((here.clone(), item.clone()));
        }
        out.extend(menu_actions(&item.submenu, &here));
    }
    out
}
