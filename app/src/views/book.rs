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
use crate::protocol::{CommandMode, WireCommand};
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
}

/// The leaves turning, on the frame clock.
#[derive(Clone, Debug, Default)]
pub struct Leaves {
    turns: Vec<Turn>,
    now: f64,
    /// Drawing at all last frame, so the frame after the last leaf lands rebuilds too.
    drawing: bool,
}

impl Leaves {
    fn push(&mut self, kind: Kind) {
        if self.turns.len() >= MAX_LEAVES {
            self.turns.remove(0);
        }
        self.turns.push(Turn { kind, began: None });
    }

    /// Advances to `t`. `still` (Reduce Motion) is asked only when a turn is waiting to begin.
    /// True when the view must rebuild.
    pub fn frame(&mut self, t: f64, still: impl FnOnce() -> bool, tx: &mut Tx) -> bool {
        self.now = t;
        if self.turns.iter().any(|turn| turn.began.is_none()) && still() {
            self.turns.clear();
        }
        for turn in &mut self.turns {
            turn.began.get_or_insert(t);
        }
        self.turns
            .retain(|turn| t - turn.began.unwrap_or(t) < turn.kind.duration());
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

    /// How far the cover has swung, while it's opening.
    fn cover(&self) -> Option<f32> {
        self.progress()
            .find(|(kind, _)| *kind == Kind::Cover)
            .map(|(_, u)| u)
    }
}

impl Kind {
    fn duration(self) -> f64 {
        match self {
            Kind::Cover => COVER,
            Kind::Leaf { .. } => TURN,
        }
    }
}

fn ease(u: f32) -> f32 {
    // Lifts slowly, falls flat.
    let s = u * u * (3.0 - 2.0 * u);
    s * 0.85 + u * 0.15
}

// MARK: Rows

/// The index: every spell, alphabetical; filtered by `query`, best match first. Each row's
/// only accessory is its folio, its place in the whole book.
pub fn rows(commands: &[WireCommand], query: &str) -> Vec<Row> {
    use crate::store::{ActionItem, ActionKindItem};
    let mut sorted: Vec<&WireCommand> = commands.iter().collect();
    sorted.sort_by_key(|c| c.title.to_lowercase());
    let row = |(i, c): (usize, &WireCommand)| Row {
        id: format!("spell:{}", c.id),
        title: c.title.clone(),
        subtitle: c.subtitle.clone(),
        icon: Icon::parse(Some(c.icon.as_deref().unwrap_or("sparkles"))),
        accessories: vec![(i + 1).to_string()],
        actions: vec![
            ActionItem::new(
                "run",
                "Cast Spell",
                "sparkles",
                ActionKindItem::RunSpell(c.clone()),
            ),
            ActionItem::new(
                "reveal",
                "Show in Finder",
                "folder",
                ActionKindItem::RevealInFinder(
                    crate::config::Config::spell_file(&c.id)
                        .to_string_lossy()
                        .into_owned(),
                ),
            ),
            ActionItem::new(
                "copy-id",
                "Copy Spell ID",
                "doc.on.doc",
                ActionKindItem::CopyText(c.id.clone()),
            ),
        ],
    };
    let all = sorted.into_iter().enumerate();
    if query.is_empty() {
        return all.map(row).collect();
    }
    let mut scored: Vec<(Row, i64)> = all
        .filter_map(|(i, c)| {
            let mut fields = vec![c.title.as_str(), c.subtitle.as_deref().unwrap_or("")];
            fields.extend(c.keywords.iter().map(String::as_str));
            crate::fuzzy::score_fields(query, &fields).map(|s| (row((i, c)), s))
        })
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.1));
    scored.into_iter().map(|(r, _)| r).collect()
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
    let v = ink(look.p.theme);
    let tex = paper::textures(look.scale);
    let rows = s.rows();
    let selected = rows.get(s.selection()).cloned();
    let command = selected.as_ref().and_then(|r| command_of(s, r));
    // While the cover swings over, the left half waits for it.
    let left_shown = book
        .leaves
        .cover()
        .map_or(1.0, |u| ((u - 0.55) * 2.6).clamp(0.0, 1.0));

    // The open book, either side of the spine, and its shadow on the desk.
    let half = |left: bool| {
        el().key(if left {
            "binding-left"
        } else {
            "binding-right"
        })
        .a11y_hidden()
        .pin(Some(if left { 0.0 } else { SPINE }), Some(0.0), None, None)
        .size(SPINE, BOOK_H)
        .corners(if left {
            [7.0, 0.0, 0.0, 7.0]
        } else {
            [0.0, 7.0, 7.0, 0.0]
        })
        .pixels(if left { &tex.left } else { &tex.right })
        .box_shadow(Shadow {
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
    };
    let spread = el()
        .key("book")
        .role("group")
        .label("Grimoire")
        .size(BOOK_W, BOOK_H)
        .child(half(false))
        .child(half(true).opacity(left_shown))
        .child(leaves(s, &rows, &book.leaves, true, v, &tex, look))
        .child(
            page(true)
                .opacity(left_shown)
                .child(index_page(s, &rows, v, look.scale)),
        )
        .child(page(false).child(entry_page("entry", selected.as_ref(), command, v, look)))
        .child(leaves(s, &rows, &book.leaves, false, v, &tex, look))
        // Last: everything above, redrawn as it lies on curved pages. The pages settle into
        // their curve as the cover lands.
        .child(warp::slot(
            book.leaves
                .cover()
                .map_or(1.0, |u| ((u - 0.55) / 0.45).clamp(0.0, 1.0)),
        ));
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
    let ink = if selected { v.rubric } else { v.ink };
    let n = r.accessories.first().cloned().unwrap_or_default();
    row()
        .key(row_key(index))
        .role("option")
        .label(r.title.clone())
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
                .font(if selected {
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
    let mode = match command.map(|c| c.mode) {
        Some(CommandMode::NoView) => "cast in a single breath",
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
        .transition(Transition::opacity())
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

/// The leaves in the air, over the spread. Rebuilt every frame while they turn; their changes
/// land at once (the frame clock moves them, not springs). The cover, once past the spine,
/// goes `under` the pages, which settle onto it.
fn leaves(
    s: &Cantrip,
    rows: &[Row],
    leaves: &Leaves,
    under: bool,
    v: Ink,
    tex: &paper::Textures,
    look: Look,
) -> El {
    let mut layer = el()
        .key(if under { "leaves-under" } else { "leaves" })
        .a11y_hidden()
        .inert()
        .pin(Some(0.0), Some(0.0), None, None)
        .size(BOOK_W, BOOK_H)
        .instant_changes();
    for (i, (kind, u)) in leaves.progress().enumerate() {
        if under != (kind == Kind::Cover && u >= 0.5) {
            continue;
        }
        // Its face on the right is the entry it turns away (forward) or toward (back); the
        // other side, over the index, is the back of the page, blank.
        let face = match kind {
            Kind::Cover => None,
            Kind::Leaf { forward, from } => {
                let line = if forward { from } else { s.selection() };
                rows.get(line).map(|r| {
                    let key = format!("leaf-entry-{i}");
                    entry_page(&key, Some(r), command_of(s, r), v, look)
                })
            }
        };
        layer = layer.child(leaf(i, kind, u, face, tex));
    }
    layer
}

/// One leaf, `u` of the way over. It turns about the spine: its width is the cosine of its
/// angle, it lifts a little as it stands up, and it casts a shadow on the page it's leaving.
fn leaf(i: usize, kind: Kind, u: f32, face: Option<El>, tex: &paper::Textures) -> El {
    let forward = match kind {
        Kind::Cover => true,
        Kind::Leaf { forward, .. } => forward,
    };
    let theta = u * PI;
    let reach = theta.cos();
    // Which side of the spine it's over: forward starts on the right.
    let on_right = (reach > 0.0) == forward;
    let (w_max, h, top) = match kind {
        Kind::Cover => (SPINE, BOOK_H, 0.0),
        Kind::Leaf { .. } => (PAGE_W, PAGE_H, PAGE_INSET_Y),
    };
    let w = (reach.abs() * w_max).max(0.5);
    let lift = theta.sin() * 5.0;
    let x = if on_right { SPINE } else { SPINE - w };
    // The side that faces up: brighter lying flat, shaded as it stands.
    let stand = 1.0 - reach.abs();
    let picture = match (kind, on_right) {
        (Kind::Cover, true) => &tex.cover,
        (Kind::Cover, false) => &tex.inside,
        (Kind::Leaf { .. }, true) => &tex.leaf_right,
        (Kind::Leaf { .. }, false) => &tex.leaf_left,
    };
    // Shaded as it stands, darkest at the spine.
    let shade = Color::BLACK.alpha(0.1 + 0.45 * stand);
    let (from, to) = if on_right {
        (shade, Color::BLACK.alpha(0.12 * stand))
    } else {
        (Color::BLACK.alpha(0.12 * stand), shade)
    };
    let mut sheet = el()
        .pin(Some(x), Some(top - lift), None, None)
        .size(w, h + 2.0 * lift)
        .pixels(picture)
        .box_shadow(Shadow {
            x: if on_right { 6.0 } else { -6.0 } * stand,
            y: 4.0 + 4.0 * stand,
            blur: 14.0 * stand + 2.0,
            spread: 0.0,
            color: Color::BLACK.alpha(0.4 * stand),
            inset: false,
        });
    let light = el()
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .linear_gradient(FRAC_PI_2, [(0.0, from), (1.0, to)]);
    sheet = match kind {
        // Its title, tooled in gold below the sigil, while it still faces up.
        Kind::Cover => sheet.child(
            col()
                .pin(Some(0.0), Some(BOOK_H * 0.71), None, None)
                .size(w, 44.0)
                .center()
                .opacity(if on_right {
                    (reach.abs() - 0.3).max(0.0) / 0.7
                } else {
                    0.0
                })
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
        Kind::Leaf { .. } => match face.filter(|_| on_right) {
            // The writing, foreshortened away as the page stands up.
            Some(face) => sheet.child(
                el().pin(Some(0.0), Some(lift), None, None)
                    .size(PAGE_W, PAGE_H)
                    .opacity(reach.abs().powf(0.6))
                    .child(face),
            ),
            None => sheet,
        },
    };
    let sheet = sheet.child(light);
    // Its shadow falls on the page beside its free edge.
    let fall = 34.0 * theta.sin();
    let shadow = el()
        .pin(
            Some(if on_right { x + w } else { x - fall }),
            Some(PAGE_INSET_Y),
            None,
            None,
        )
        .size(fall.max(0.5), PAGE_H)
        .linear_gradient(
            FRAC_PI_2,
            if on_right {
                [
                    (0.0, Color::BLACK.alpha(0.22 * stand)),
                    (1.0, Color::BLACK.alpha(0.0)),
                ]
            } else {
                [
                    (0.0, Color::BLACK.alpha(0.0)),
                    (1.0, Color::BLACK.alpha(0.22 * stand)),
                ]
            },
        );
    el().key(format!("leaf-{i}"))
        .pin(Some(0.0), Some(0.0), None, None)
        .size(BOOK_W, BOOK_H)
        .child(shadow)
        .child(sheet)
}
