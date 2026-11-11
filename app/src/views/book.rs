//! The grimoire, bound as an ancient book (`Builtin::OpenGrimoire`): a two-page spread in the
//! bar's content. The left page is the index, every spell with its folio, filtered by the
//! field; the right page is the highlighted spell's entry, under an illuminated initial.
//!
//! Opening the book swings its cover over from the right; moving the highlight turns a leaf,
//! forward going down the index and back going up. Leaves are drawn on the frame clock
//! (`Leaves::frame`, from `AppSpec::poll`), so headless renders repeat; Reduce Motion turns
//! none and the entry only cross-fades.
//!
//! There is one book: leather, brass and parchment, painted once into pictures (`paper.rs`).
//! The theme only picks the inks it's written in (`Ink`).

mod paper;
mod warp;

pub use paper::prepare;
pub use warp::Warp;

use std::f32::consts::{FRAC_PI_2, PI};

use rune::{Color, El, Family, Shadow, TextStyle, Transition, Tx, col, el, row, spacer, text};

use super::palette::srgb;
use super::theme::Theme;
use super::{Look, RESULTS, play};
use crate::icons::{self, Icon};
use crate::protocol::{CommandMode, WireCommand, WireEntry};
use crate::store::{Cantrip, Row, ScrollWatch, row_key};

/// Headings and illuminated initials: a calligraphic hand.
const HAND: Family = Family::Named("Luminari, Apple Chancery, Iowan Old Style, Georgia");
/// The text: an old-style book face.
const BOOK: Family = Family::Named("Baskerville, Iowan Old Style, Palatino, Georgia");

/// The binding, centred in the content: two boards meeting at the spine.
const BOOK_W: f32 = 740.0;
const BOOK_H: f32 = 352.0;
/// The open pages, inside the boards and the edges of the pages beneath them.
const PAGE_INSET_X: f32 = 20.0;
const PAGE_INSET_Y: f32 = 9.0;
const PAGE_W: f32 = BOOK_W / 2.0 - PAGE_INSET_X;
const PAGE_H: f32 = BOOK_H - PAGE_INSET_Y - 15.0;
const SPINE: f32 = BOOK_W / 2.0;

/// One line of the index.
const INDEX_ROW_H: f32 = 27.0;
/// Space above the first line and below the last, inside the index's scroll.
const INDEX_PAD: f32 = crate::views::LIST_PAD;

/// How long a leaf takes to turn, and the cover to swing open.
const TURN: f64 = 0.42;
const COVER: f64 = 0.62;
/// At most this many leaves in the air: holding an arrow riffles through the pages.
const MAX_LEAVES: usize = 3;

/// Where line `index` of the index sits in its scroll content, top and bottom.
pub fn index_span(index: usize) -> (f32, f32) {
    let top = INDEX_PAD + index as f32 * INDEX_ROW_H;
    (top, top + INDEX_ROW_H)
}

// MARK: State

/// The book while it's open: its own query and highlight, and the leaves in the air.
#[derive(Clone, Debug, Default)]
pub struct Book {
    pub query: String,
    pub selection: usize,
    pub leaves: Leaves,
}

impl Book {
    /// Opened: the cover swings over.
    pub fn new() -> Self {
        let mut book = Book::default();
        book.leaves.push(Kind::Cover);
        book
    }

    /// The highlight moved to `index`: turn a leaf toward it.
    pub fn select(&mut self, index: usize) {
        if index != self.selection {
            self.leaves.push(Kind::Leaf {
                forward: index > self.selection,
                from: self.selection,
            });
        }
        self.selection = index;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// The front cover, swinging open from the right.
    Cover,
    /// A page, forward (right to left) or back, as the highlight moved from line `from`.
    Leaf { forward: bool, from: usize },
}

#[derive(Clone, Copy, Debug)]
struct Turn {
    kind: Kind,
    /// Frame-clock seconds it began; `None` until the next frame stamps it.
    began: Option<f64>,
    /// It has been drawn landed: lying exactly as what it lands on, so it can go.
    landed: bool,
}

/// The leaves turning, on the frame clock.
#[derive(Clone, Debug, Default)]
pub struct Leaves {
    turns: Vec<Turn>,
    now: f64,
    /// Drawing at all last frame, so the frame after the last leaf lands rebuilds too.
    drawing: bool,
    /// Reduce Motion is on: leaves don't turn.
    pub still: bool,
}

impl Leaves {
    fn push(&mut self, kind: Kind) {
        if self.turns.len() >= MAX_LEAVES {
            self.turns.remove(0);
        }
        self.turns.push(Turn {
            kind,
            began: None,
            landed: false,
        });
    }

    /// Advances to `t`. `still` (Reduce Motion) is asked only when a turn is waiting to begin.
    /// True when the view must rebuild.
    pub fn frame(&mut self, t: f64, still: impl FnOnce() -> bool, tx: &mut Tx) -> bool {
        self.now = t;
        if self.turns.iter().any(|turn| turn.began.is_none()) {
            self.still = still();
            if self.still {
                self.turns.clear();
            }
        }
        for turn in &mut self.turns {
            turn.began.get_or_insert(t);
        }
        // A turn that has run its course is drawn once landed, then goes: frames rarely fall
        // on its last instant, and one caught short of it would jump to what it lands on.
        self.turns.retain(|turn| !turn.landed);
        for turn in &mut self.turns {
            turn.landed = t - turn.began.unwrap_or(t) >= turn.kind.duration();
        }
        let drawing = !self.turns.is_empty();
        if drawing {
            tx.wake_after(std::time::Duration::ZERO);
        }
        let dirty = drawing || self.drawing;
        self.drawing = drawing;
        dirty
    }

    /// Each turn and how far it's gone, 0 to 1 (eased).
    fn progress(&self) -> impl Iterator<Item = (Kind, f32)> + '_ {
        self.turns.iter().map(|turn| {
            let elapsed = turn.began.map_or(0.0, |b| self.now - b);
            let u = (elapsed / turn.kind.duration()).clamp(0.0, 1.0) as f32;
            (turn.kind, ease(u))
        })
    }

    /// The sheet in the air whose face is up on the right, writing and all: the newest one
    /// there. Which turn it is (its place in `progress`), what it is, and how far it's gone.
    fn recto(&self) -> Option<(usize, Kind, f32)> {
        self.progress()
            .enumerate()
            .filter(|(_, (kind, u))| pose(*kind, *u).on_right)
            .last()
            .map(|(i, (kind, u))| (i, kind, u))
    }

    /// The newest leaf in the air with its back up on the left: the shader draws it from the
    /// left page itself, writing and all.
    fn verso(&self) -> Option<(usize, Kind, f32)> {
        self.progress()
            .enumerate()
            .filter(|(_, (kind, u))| *kind != Kind::Cover && !pose(*kind, *u).on_right)
            .last()
            .map(|(i, (kind, u))| (i, kind, u))
    }

    /// The line whose entry lies on the right under the leaves in the air: while a leaf turns
    /// back, the one it was turned from, until it lands on it.
    fn beneath(&self) -> Option<usize> {
        self.turns.iter().find_map(|turn| match turn.kind {
            Kind::Leaf { forward: false, from } => Some(from),
            _ => None,
        })
    }

    /// How far the cover has swung, while it's opening.
    fn cover(&self) -> Option<f32> {
        self.progress()
            .find(|(kind, _)| *kind == Kind::Cover)
            .map(|(_, u)| u)
    }
}

impl Kind {
    /// Where the sheet lies before it turns, face up on the right: x, y, width and height.
    fn flat(self) -> [f32; 4] {
        match self {
            Kind::Cover => [SPINE, 0.0, SPINE, BOOK_H],
            Kind::Leaf { .. } => paper::page_rect(false),
        }
    }

    fn duration(self) -> f64 {
        match self {
            Kind::Cover => COVER,
            Kind::Leaf { .. } => TURN,
        }
    }
}

/// A sheet `u` of the way over. It turns about the spine: its width is the cosine of its angle,
/// and it lifts a little as it stands up.
#[derive(Clone, Copy, Debug)]
struct Pose {
    /// Where it shows: x, y, width and height.
    rect: [f32; 4],
    /// How upright it stands: 0 flat, 1 on end.
    stand: f32,
    /// Which side of the spine it's over, and so which face is up.
    on_right: bool,
    /// The angle it's turned through.
    theta: f32,
}

fn pose(kind: Kind, u: f32) -> Pose {
    let forward = match kind {
        Kind::Cover => true,
        Kind::Leaf { forward, .. } => forward,
    };
    let theta = u * PI;
    let reach = theta.cos();
    // Forward starts on the right.
    let on_right = (reach > 0.0) == forward;
    let [_, top, w_max, h] = kind.flat();
    let w = (reach.abs() * w_max).max(0.5);
    let lift = theta.sin() * 5.0;
    let x = if on_right { SPINE } else { SPINE - w };
    Pose {
        rect: [x, top - lift, w, h + 2.0 * lift],
        stand: 1.0 - reach.abs(),
        on_right,
        theta,
    }
}

/// A sheet in the air for the compositor to draw: where it lies flat (in the capture, or on
/// the left page), where it shows now, how upright it stands, how far its shadow falls, and
/// whether it's the cover (which lies over the pages' curve, not on it).
#[derive(Clone, Copy, Debug)]
pub(super) struct Sheet {
    pub flat: [f32; 4],
    pub now: [f32; 4],
    pub stand: f32,
    pub fall: f32,
    pub cover: bool,
}

/// The sheets the compositor draws: the newest face up on the right (from its capture), and
/// the newest leaf back up on the left (from the left page).
pub(super) fn sheets(s: &Cantrip) -> (Option<Sheet>, Option<Sheet>) {
    let Some(book) = s.book.as_ref().filter(|_| s.session.is_none() && s.picker.is_none())
    else {
        return (None, None);
    };
    let sheet = |(_, kind, u): (usize, Kind, f32), flat: [f32; 4]| {
        let pose = pose(kind, u);
        Sheet {
            flat,
            now: pose.rect,
            stand: pose.stand,
            fall: 34.0 * pose.theta.sin(),
            cover: kind == Kind::Cover,
        }
    };
    (
        book.leaves.recto().map(|r| sheet(r, r.1.flat())),
        book.leaves.verso().map(|v| sheet(v, paper::page_rect(true))),
    )
}

/// Which of the right pages' pictures a line's entry is written on: its folio's.
fn variant(r: &Row) -> usize {
    let folio: usize = r.accessories.first().and_then(|n| n.parse().ok()).unwrap_or(1);
    (folio.max(1) - 1) % paper::VARIANTS
}

fn ease(u: f32) -> f32 {
    // Lifts slowly, falls flat.
    let s = u * u * (3.0 - 2.0 * u);
    s * 0.85 + u * 0.15
}

// MARK: Rows

/// The index: every spell in the grimoire, alphabetical; filtered by `query`, best match
/// first. Each row's only accessory is its folio, its place in the whole book.
///
/// An enabled entry is written as its spells (a daemon-only one as itself); a disabled one,
/// which isn't loaded, as one dormant line (`row_state`) that ↵ enables.
pub fn rows(commands: &[WireCommand], entries: &[WireEntry], query: &str) -> Vec<Row> {
    use crate::store::{ActionItem, ActionKindItem};
    let entry_of = |id: &str| id.split(':').next().unwrap_or(id).to_string();
    let reveal = |id: &str| {
        ActionItem::new(
            "reveal",
            "Show in Finder",
            "folder",
            ActionKindItem::RevealInFinder(
                crate::config::Config::spell_file(id)
                    .to_string_lossy()
                    .into_owned(),
            ),
        )
    };
    let copy_id = |id: &str| {
        ActionItem::new(
            "copy-id",
            "Copy Spell ID",
            "doc.on.doc",
            ActionKindItem::CopyText(id.to_string()),
        )
    };
    let toggle = |e: &WireEntry, enabled: bool| {
        ActionItem::new(
            if enabled { "enable" } else { "disable" },
            if enabled {
                "Enable Spell"
            } else {
                "Disable Spell"
            },
            if enabled { "sparkles" } else { "moon.zzz" },
            ActionKindItem::SetEnabled {
                entry: e.id.clone(),
                title: e.title.clone(),
                enabled,
            },
        )
    };
    // (row, the words a query matches)
    let mut lines: Vec<(Row, Vec<String>)> = Vec::new();
    for c in commands {
        let mut actions = vec![
            ActionItem::new(
                "run",
                "Cast Spell",
                "sparkles",
                ActionKindItem::RunSpell(c.clone()),
            ),
            reveal(&c.id),
            copy_id(&c.id),
        ];
        if let Some(e) = entries.iter().find(|e| e.id == entry_of(&c.id)) {
            actions.push(toggle(e, false));
        }
        let mut words = vec![c.title.clone(), c.subtitle.clone().unwrap_or_default()];
        words.extend(c.keywords.iter().cloned());
        lines.push((
            Row {
                id: format!("spell:{}", c.id),
                title: c.title.clone(),
                subtitle: c.subtitle.clone(),
                icon: Icon::parse(Some(c.icon.as_deref().unwrap_or("sparkles"))),
                accessories: Vec::new(),
                actions,
            },
            words,
        ));
    }
    for e in entries {
        if e.enabled && commands.iter().any(|c| entry_of(&c.id) == e.id) {
            continue;
        }
        let actions = if e.enabled {
            vec![reveal(&e.id), copy_id(&e.id), toggle(e, false)]
        } else {
            vec![toggle(e, true), reveal(&e.id)]
        };
        let mut words = vec![e.title.clone(), e.id.clone(), e.summary.clone().unwrap_or_default()];
        words.push(if e.enabled { "enabled" } else { "disabled" }.into());
        lines.push((
            Row {
                id: format!("entry:{}", e.id),
                title: e.title.clone(),
                subtitle: e.summary.clone(),
                icon: Icon::parse(Some(if e.enabled {
                    "menubar.rectangle"
                } else {
                    "moon.zzz"
                })),
                accessories: Vec::new(),
                actions,
            },
            words,
        ));
    }
    lines.sort_by_key(|(r, _)| r.title.to_lowercase());
    for (i, (r, _)) in lines.iter_mut().enumerate() {
        r.accessories = vec![(i + 1).to_string()];
    }
    if query.is_empty() {
        return lines.into_iter().map(|(r, _)| r).collect();
    }
    let mut scored: Vec<(Row, i64)> = lines
        .into_iter()
        .filter_map(|(r, words)| {
            let fields: Vec<&str> = words.iter().map(String::as_str).collect();
            crate::fuzzy::score_fields(query, &fields).map(|s| (r, s))
        })
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.1));
    scored.into_iter().map(|(r, _)| r).collect()
}

/// What a line of the index is: a spell to cast, a daemon keeping watch, or a dormant entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    Spell,
    Daemon,
    Dormant,
}

pub fn row_state(r: &Row) -> RowState {
    use crate::store::ActionKindItem;
    match r.actions.first().map(|a| &a.kind) {
        Some(ActionKindItem::SetEnabled { enabled: true, .. }) => RowState::Dormant,
        Some(ActionKindItem::RunSpell(_)) => RowState::Spell,
        _ => RowState::Daemon,
    }
}

// MARK: Inks

/// What the book is written in: the one thing a theme changes.
#[derive(Clone, Copy)]
struct Ink {
    ink: Color,
    /// Rubrics: initials, the highlight, the manicule.
    rubric: Color,
    /// Gold leaf.
    gilt: Color,
    /// The letter of an illuminated initial.
    letter: Color,
}

fn ink(theme: Theme) -> Ink {
    match theme {
        // Iron-gall brown, vermilion, and gold leaf.
        Theme::Sigil => Ink {
            ink: srgb(0.20, 0.12, 0.07, 1.0),
            rubric: srgb(0.58, 0.14, 0.09, 1.0),
            gilt: srgb(0.80, 0.62, 0.28, 1.0),
            letter: srgb(0.96, 0.82, 0.50, 1.0),
        },
        // Oak-gall black with a violet cast, murex purple, and gold leaf.
        Theme::Arcane => Ink {
            ink: srgb(0.16, 0.09, 0.24, 1.0),
            rubric: srgb(0.45, 0.14, 0.52, 1.0),
            gilt: srgb(0.80, 0.62, 0.28, 1.0),
            letter: srgb(0.97, 0.84, 0.56, 1.0),
        },
    }
}

/// Tooled gold on the leather: the cover's title.
const TOOLED: Color = Color {
    r: 0.58,
    g: 0.36,
    b: 0.10,
    a: 1.0,
};

fn hand(size: f32) -> TextStyle {
    TextStyle::new(size, 400.0).family(HAND)
}

fn book_face(size: f32) -> TextStyle {
    TextStyle::new(size, 400.0).family(BOOK)
}

fn italic(size: f32) -> TextStyle {
    book_face(size).italic()
}

/// A folio number, as the bottom of a page carries it.
fn folio(n: &str, v: Ink) -> El {
    row().w_full().justify_center().child(
        text(format!("— {n} —"))
            .font(italic(12.5).features("'onum' 1"))
            .color(v.ink.alpha(0.5)),
    )
}

/// A fleuron between two hairlines that fade outward.
fn flourish(v: Ink, width: f32) -> El {
    let line = |angle: f32| {
        el().w(width)
            .h(1.0)
            .linear_gradient(angle, [(0.0, v.ink.alpha(0.0)), (1.0, v.ink.alpha(0.35))])
    };
    row()
        .w_full()
        .justify_center()
        .center_y()
        .gap(8.0)
        .child(line(FRAC_PI_2))
        .child(text("❧").font(book_face(15.0)).color(v.rubric.alpha(0.85)))
        .child(line(-FRAC_PI_2))
}

// MARK: The spread

/// The spell a line of the index is.
fn command_of<'a>(s: &'a Cantrip, r: &Row) -> Option<&'a WireCommand> {
    s.commands
        .iter()
        .find(|c| r.id.strip_prefix("spell:") == Some(c.id.as_str()))
}

pub(super) fn view(s: &Cantrip, look: Look) -> El {
    let Some(book) = &s.book else {
        return el();
    };
    let look = Look {
        still: book.leaves.still,
        ..look
    };
    let v = ink(look.p.theme);
    let tex = paper::textures(look.scale);
    let rows = s.rows();
    // The entry on the right: the highlighted one, unless a leaf turning back is still on its
    // way to it.
    let beneath = book
        .leaves
        .beneath()
        .filter(|line| *line < rows.len())
        .unwrap_or(s.selection());
    let shown = rows.get(beneath).cloned();
    let command = shown.as_ref().and_then(|r| command_of(s, r));
    let page_picture = |r: Option<&Row>| &tex.pages[r.map_or(0, variant)];
    // While the cover swings over, the left side is bare desk; the cover lands as it.
    let opening = book.leaves.cover().is_some();

    let mut spread = el()
        .key("book")
        .role("group")
        .label("Grimoire")
        .size(BOOK_W, BOOK_H)
        .child(binding(false, &tex, "binding-right"));
    // Both boards before either page: their shadows on the desk lie under the paper.
    if !opening {
        spread = spread.child(binding(true, &tex, "binding-left"));
    }
    spread = spread.child(page_picture_at(false, page_picture(shown.as_ref()), "page-right"));
    if !opening {
        spread = spread.child(page_picture_at(true, &tex.page_left, "page-left"));
    }
    spread = spread
        .child(leaves(&book.leaves, true, &tex))
        // The index's ink seeps into the page once the cover has landed.
        .child(
            page(true)
                .opacity(if opening { 0.0 } else { 1.0 })
                .opacity_motion(rune::Spring::new(0.7, 0.0))
                .child(index_page(s, &rows, v, look.scale)),
        );
    if let Some((_, kind, _)) = book.leaves.recto() {
        // The sheet turning, lying flat with its writing, for the compositor to capture and
        // draw foreshortened as it turns (`warp`); then the right side again, over it. It
        // comes and goes at once: a fade would leave a ghost of it lying flat.
        let (face, picture) = match kind {
            Kind::Cover => (None, &tex.cover),
            Kind::Leaf { forward, from } => {
                let line = if forward { from } else { s.selection() };
                let r = rows.get(line);
                (r, page_picture(r))
            }
        };
        spread = spread.child(
            el().key("turning")
                .a11y_hidden()
                .inert()
                .pin(Some(0.0), Some(0.0), None, None)
                .size(BOOK_W, BOOK_H)
                .transition(Transition::none())
                .instant_changes()
                .child(flat_recto(s, kind, face, picture, v, look))
                .child(warp::capture())
                .child(binding(false, &tex, "binding-right-again"))
                .child(page_picture_at(false, page_picture(shown.as_ref()), "page-right-again")),
        );
    }
    let spread = spread
        .child(page(false).child(entry_page("entry", shown.as_ref(), command, v, look)))
        .child(leaves(&book.leaves, false, &tex))
        // Last: everything above, redrawn as it lies on curved pages, with the sheets in the
        // air drawn turning over them.
        .child(warp::slot(1.0));
    let spread = play(spread, look.intro.map(|intro| intro.row(0)));
    // Candlelight pooled on the desk round the book.
    col()
        .key("grimoire")
        .w_full()
        .h_full()
        .center()
        .radial_gradient(
            [0.5, 0.3],
            [520.0, 300.0],
            [
                (0.0, srgb(1.0, 0.72, 0.36, 0.16)),
                (0.6, srgb(1.0, 0.62, 0.30, 0.05)),
                (1.0, srgb(1.0, 0.6, 0.3, 0.0)),
            ],
        )
        .child(spread)
}

/// Its shadow on the desk, under a board.
fn desk_shadows(el: El) -> El {
    el.box_shadow(Shadow {
        x: 0.0,
        y: 12.0,
        blur: 24.0,
        spread: -2.0,
        color: Color::BLACK.alpha(0.7),
        inset: false,
    })
    .box_shadow(Shadow {
        x: 0.0,
        y: 3.0,
        blur: 4.0,
        spread: 0.0,
        color: Color::BLACK.alpha(0.5),
        inset: false,
    })
}

/// The binding either side of the spine, lying on the desk. It comes and goes at once (the
/// cover landing is it).
fn binding(left: bool, tex: &paper::Textures, key: &'static str) -> El {
    let half = el()
        .key(key)
        .a11y_hidden()
        .pin(Some(if left { 0.0 } else { SPINE }), Some(0.0), None, None)
        .size(SPINE, BOOK_H)
        .transition(Transition::none())
        .corners(if left {
            [7.0, 0.0, 0.0, 7.0]
        } else {
            [0.0, 7.0, 7.0, 0.0]
        })
        .pixels(if left { &tex.left } else { &tex.right });
    // Drawn again over a captured sheet, it already has its shadow.
    if key.ends_with("-again") {
        half
    } else {
        desk_shadows(half)
    }
}

/// An open page's picture, where it lies.
fn page_picture_at(left: bool, picture: &rune::Pixels, key: &'static str) -> El {
    let [x, y, w, h] = paper::page_rect(left);
    el().key(key)
        .a11y_hidden()
        .inert()
        .pin(Some(x), Some(y), None, None)
        .size(w, h)
        .transition(Transition::none())
        .pixels(picture)
}

/// Where a page's writing goes: over the open page painted in the binding.
fn page(left: bool) -> El {
    col()
        .key(if left { "page-index" } else { "page-entry" })
        .pin(
            Some(if left { PAGE_INSET_X } else { SPINE }),
            Some(PAGE_INSET_Y),
            None,
            None,
        )
        .size(PAGE_W, PAGE_H)
}

// MARK: Index

fn index_page(s: &Cantrip, rows: &[Row], v: Ink, scale: f32) -> El {
    let selection = s.selection();
    let heading = col()
        .w_full()
        .pt(20.0)
        .gap(4.0)
        .child(
            row()
                .w_full()
                .justify_center()
                .child(text("Index").font(hand(22.0)).color(v.ink)),
        )
        .child(flourish(v, 64.0));
    let mut list = col().key("index").w_full().instant_changes();
    for (i, r) in rows.iter().enumerate() {
        list = list.child(index_line(r, i, i == selection, v, scale));
    }
    let body: El = if rows.is_empty() {
        let message = if s.query().is_empty() {
            "No spells are written here yet.".to_string()
        } else {
            format!("No spell answers to “{}”.", s.query())
        };
        col()
            .key("index-empty")
            .w_full()
            .grow(1.0)
            .center()
            .child(text(message).font(italic(14.0)).color(v.ink.alpha(0.6)))
    } else {
        // The highlight: a wash of rubric that glides from line to line.
        let top = index_span(selection).0;
        let wash = el()
            .key("selection")
            .a11y_hidden()
            .pin(Some(0.0), Some(top), Some(0.0), None)
            .h(INDEX_ROW_H)
            .radius(3.0)
            .linear_gradient(
                FRAC_PI_2,
                [
                    (0.0, v.rubric.alpha(0.0)),
                    (0.12, v.rubric.alpha(0.13)),
                    (0.88, v.rubric.alpha(0.13)),
                    (1.0, v.rubric.alpha(0.0)),
                ],
            )
            .motion(crate::views::GLIDE);
        let watch = s.scroll.clone();
        col()
            .key(RESULTS)
            .on_scroll_offset(move |at: rune::ScrollPos| {
                watch.set(ScrollWatch {
                    offset: at.offset[1],
                    viewport: at.viewport[1],
                    max: at.max[1],
                })
            })
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .clip_fade(0.0, 6.0, 0.0, 10.0)
            .scroll(rune::ScrollSpec {
                stick_bottom: false,
            })
            .child(col().w_full().py(INDEX_PAD).child(wash).child(list))
    };
    col()
        .w_full()
        .h_full()
        .px(30.0)
        .pb(12.0)
        .gap(8.0)
        .child(heading)
        .child(body)
        .child(folio("i", v))
}

fn index_line(r: &Row, index: usize, selected: bool, v: Ink, scale: f32) -> El {
    let dormant = row_state(r) == RowState::Dormant;
    // A disabled spell is written faintly until it's enabled.
    let ink = match (selected, dormant) {
        (true, _) => v.rubric,
        (false, true) => v.ink.alpha(0.45),
        (false, false) => v.ink,
    };
    let n = r.accessories.first().cloned().unwrap_or_default();
    row()
        .key(row_key(index))
        .role("option")
        .label(if dormant {
            format!("{} (disabled)", r.title)
        } else {
            r.title.clone()
        })
        .checked(selected)
        .w_full()
        .h(INDEX_ROW_H)
        .px(6.0)
        .gap(6.0)
        .center_y()
        .on_press_tx(move |s: &mut Cantrip, tx: &mut Tx| {
            if s.selection() == index {
                if let Some(first) = s.rows().get(index).and_then(|r| r.actions.first().cloned()) {
                    s.perform(first, tx);
                }
            } else {
                s.set_selection(index);
            }
        })
        // A manicule points at the open entry.
        .child({
            let hand = el().w(16.0).center_y();
            if selected {
                hand.child(icons::view(
                    &Icon::Symbol("hand.point.right.fill".into()),
                    13.0,
                    v.rubric,
                    scale,
                ))
            } else {
                hand
            }
        })
        .child(
            text(r.title.clone())
                .font(if selected || dormant {
                    italic(15.0)
                } else {
                    book_face(15.0)
                })
                .color(ink)
                .truncate()
                .shrink()
                .min_w(0.0),
        )
        // Leaders to the folio.
        .child(
            text(" .".repeat(80))
                .font(book_face(13.0))
                .color(v.ink.alpha(0.32))
                .truncate_fade()
                .grow(1.0)
                .min_w(12.0),
        )
        .child(
            text(n)
                .font(book_face(14.0).features("'onum' 1"))
                .color(ink.alpha(0.8)),
        )
}

// MARK: Entry

fn entry_page(key: &str, r: Option<&Row>, command: Option<&WireCommand>, v: Ink, look: Look) -> El {
    let page = col().w_full().h_full().px(34.0).pt(22.0).pb(12.0).gap(10.0);
    let Some(r) = r else {
        return page
            .child(spacer())
            .child(
                row().w_full().justify_center().child(
                    text("The page is blank.")
                        .font(italic(15.0))
                        .color(v.ink.alpha(0.5)),
                ),
            )
            .child(spacer())
            .child(folio("ii", v));
    };
    let mut chars = r.title.chars();
    let initial: String = chars.next().map(String::from).unwrap_or_default();
    let rest: String = chars.collect();
    // An illuminated initial: a gilt frame round a rubric field, the letter in leaf.
    let illumination = el()
        .size(62.0, 62.0)
        .center()
        .radius(3.0)
        .fill(v.rubric)
        .radial_gradient(
            [0.3, 0.25],
            70.0,
            [
                (0.0, Color::WHITE.alpha(0.18)),
                (1.0, Color::BLACK.alpha(0.25)),
            ],
        )
        .border(2.0, v.gilt)
        .box_shadow(Shadow {
            x: 1.0,
            y: 2.0,
            blur: 3.0,
            spread: 0.0,
            color: srgb(0.25, 0.12, 0.04, 0.45),
            inset: false,
        })
        .child(
            el().pin(Some(4.0), Some(4.0), Some(4.0), Some(4.0))
                .radius(1.5)
                .border(1.0, v.gilt.alpha(0.55)),
        )
        .child(text(initial).font(hand(44.0)).color(v.letter));
    let state = row_state(r);
    let mode = match (state, command.map(|c| c.mode)) {
        (RowState::Dormant, _) => "disabled",
        (RowState::Daemon, _) => "keeps watch in the menu bar",
        (_, Some(CommandMode::NoView)) => "cast in a single breath",
        _ => "opens its own pages",
    };
    let title = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(4.0)
        .child(
            text(rest)
                .font(hand(27.0).lh(34.0))
                .color(v.ink)
                .truncate()
                .min_w(0.0),
        )
        .child(
            row()
                .gap(6.0)
                .center_y()
                .child(icons::view(&r.icon, 13.0, v.rubric.alpha(0.85), look.scale))
                .child(text(mode).font(italic(13.0)).color(v.ink.alpha(0.6))),
        );
    let mut entry = col()
        .key(format!("{key}-{}", r.id))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(12.0)
        // Under Reduce Motion no leaf turns, and entries cross-fade; otherwise the leaf has
        // already laid the new one down when it lands, and it swaps in at once.
        .transition(if look.still {
            Transition::opacity()
        } else {
            Transition::none()
        })
        // Its writing is all there the frame it is: no line fades in on its own.
        .instant_changes()
        .child(
            row()
                .w_full()
                .gap(12.0)
                .items_start()
                .child(illumination)
                .child(title.pt(14.0)),
        )
        .child(flourish(v, 90.0))
        .child(
            text(
                r.subtitle
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Its purpose is not written down.".into()),
            )
            .font(book_face(15.5).lh(22.0))
            .color(v.ink.alpha(0.9))
            .wrap()
            .max_lines(4)
            .selectable(true),
        );
    if let Some(c) = command.filter(|c| !c.keywords.is_empty()) {
        entry = entry.child(
            text(format!("Also called {}.", c.keywords.join(", ")))
                .font(italic(13.5).lh(19.0))
                .color(v.ink.alpha(0.6))
                .wrap()
                .max_lines(2),
        );
    }
    if state == RowState::Dormant {
        entry = entry.child(spacer()).child(
            row()
                .gap(6.0)
                .center_y()
                .child(icons::view(
                    &Icon::Symbol("sparkles".into()),
                    12.0,
                    v.rubric.alpha(0.8),
                    look.scale,
                ))
                .child(
                    text("Press ↵ to enable it.")
                        .font(italic(13.5))
                        .color(v.rubric.alpha(0.85)),
                ),
        );
    }
    if let Some(c) = command {
        entry = entry.child(spacer()).child(
            row()
                .gap(6.0)
                .center_y()
                .child(
                    text("Incantation")
                        .font(italic(12.5))
                        .color(v.rubric.alpha(0.8)),
                )
                .child(
                    text(c.id.clone())
                        .font(
                            TextStyle::new(11.5, 400.0)
                                .family(Family::SystemMono)
                                .tracking(0.2),
                        )
                        .color(v.ink.alpha(0.55))
                        .truncate()
                        .selectable(true),
                ),
        );
    }
    let n = r.accessories.first().cloned().unwrap_or_default();
    page.child(entry).child(folio(&n, v))
}

// MARK: Leaves

/// The sheets in the air the compositor doesn't draw. Rebuilt every frame while they turn;
/// their changes land at once (the frame clock moves them, not springs). The cover, once past
/// the spine, goes `under` the pages, which settle onto it; its inside is the left side of the
/// book itself, so it lands as it. The newest leaf either side is the compositor's (`sheets`),
/// writing and shadow too; any others riffling past are blank.
fn leaves(leaves: &Leaves, under: bool, tex: &paper::Textures) -> El {
    let mut layer = el()
        .key(if under { "leaves-under" } else { "leaves" })
        .a11y_hidden()
        .inert()
        .pin(Some(0.0), Some(0.0), None, None)
        .size(BOOK_W, BOOK_H)
        // The frame clock moves them: their places, shading and shadows land each frame as
        // drawn, or the shading lags behind and a sheet lands still shaded.
        .instant_changes()
        .motion(rune::Spring::NONE);
    let drawn = [leaves.recto(), leaves.verso()].map(|t| t.map(|(i, _, _)| i));
    for (i, (kind, u)) in leaves.progress().enumerate() {
        if under != (kind == Kind::Cover && u >= 0.5) || drawn.contains(&Some(i)) {
            continue;
        }
        layer = layer.child(leaf(i, kind, u, tex));
    }
    layer
}

/// The face-up sheet lying flat on the right, with what's written on it: the entry a leaf
/// turns away (forward) or toward (back), or the cover's tooled title.
fn flat_recto(
    s: &Cantrip,
    kind: Kind,
    face: Option<&Row>,
    picture: &rune::Pixels,
    v: Ink,
    look: Look,
) -> El {
    let [x, y, w, h] = kind.flat();
    let sheet = el()
        .key("recto")
        .a11y_hidden()
        .inert()
        .pin(Some(x), Some(y), None, None)
        .size(w, h)
        .pixels(picture);
    match kind {
        Kind::Cover => sheet.child(
            col()
                .pin(Some(0.0), Some(BOOK_H * 0.71), None, None)
                .size(w, 44.0)
                .center()
                // Pressed in: the dark of the impression above, the gold in it.
                .child(
                    el().child(
                        text("Grimoire")
                            .font(hand(32.0).tracking(1.5))
                            .color(Color::BLACK.alpha(0.7)),
                    )
                    .child(
                        el().pin(Some(0.0), Some(1.0), None, None).child(
                            text("Grimoire")
                                .font(hand(32.0).tracking(1.5))
                                .color(TOOLED.mix(Color::WHITE, 0.3)),
                        ),
                    ),
                ),
        ),
        Kind::Leaf { .. } => match face {
            // Written where the entry page writes it: its picture reaches past the page by the
            // tattered margin above.
            Some(r) => sheet.child(
                el().pin(Some(0.0), Some(paper::PAGE_MARGIN), None, None)
                    .size(PAGE_W, PAGE_H)
                    .child(entry_page("leaf-entry", Some(r), command_of(s, r), v, look)),
            ),
            None => sheet,
        },
    }
}

/// A sheet `u` of the way over (`pose`) the compositor doesn't draw: the cover's inside (the
/// left side of the book, binding and page), or a blank leaf riffling past.
fn leaf(i: usize, kind: Kind, u: f32, tex: &paper::Textures) -> El {
    let pose = pose(kind, u);
    let [x, y, w, h] = pose.rect;
    let (on_right, stand) = (pose.on_right, pose.stand);
    // Shaded as it stands, darkest at the spine; as it was once it lies flat.
    let (spine, free) = (
        Color::BLACK.alpha(0.5 * stand),
        Color::BLACK.alpha(0.12 * stand),
    );
    let light = el()
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .linear_gradient(
            FRAC_PI_2,
            if on_right {
                [(0.0, spine), (1.0, free)]
            } else {
                [(0.0, free), (1.0, spine)]
            },
        );
    let lifted = |el: El| {
        el.box_shadow(Shadow {
            x: if on_right { 6.0 } else { -6.0 } * stand,
            y: 4.0 + 4.0 * stand,
            blur: 14.0 * stand + 2.0,
            spread: 0.0,
            color: Color::BLACK.alpha(0.4 * stand),
            inset: false,
        })
    };
    let sheet = match (kind, on_right) {
        (Kind::Cover, false) => {
            // The left side of the book, squeezed to the width it shows at.
            let (kx, ky) = (w / SPINE, h / BOOK_H);
            let [px, py, pw, ph] = paper::page_rect(true);
            desk_shadows(lifted(
                el().pin(Some(x), Some(y), None, None)
                    .size(w, h)
                    .corners([7.0, 0.0, 0.0, 7.0])
                    .pixels(&tex.left),
            ))
            .child(
                el().pin(Some(px * kx), Some(py * ky), None, None)
                    .size(pw * kx, ph * ky)
                    .pixels(&tex.page_left),
            )
        }
        (Kind::Cover, true) => lifted(
            el().pin(Some(x), Some(y), None, None)
                .size(w, h)
                .pixels(&tex.cover),
        ),
        (Kind::Leaf { .. }, true) => lifted(
            el().pin(Some(x), Some(y), None, None)
                .size(w, h)
                .pixels(&tex.pages[0]),
        ),
        (Kind::Leaf { .. }, false) => lifted(
            el().pin(Some(x), Some(y), None, None)
                .size(w, h)
                .pixels(&tex.page_left),
        ),
    };
    el().key(format!("leaf-{i}"))
        .pin(Some(0.0), Some(0.0), None, None)
        .size(BOOK_W, BOOK_H)
        .child(sheet.child(light))
}
