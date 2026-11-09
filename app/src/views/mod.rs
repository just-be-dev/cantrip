//! The bar, drawn with Rune in the Arcane look (`palette.rs`): search field over an aurora,
//! results or a spell's view, footer, and ⌘K actions.

pub mod hud;
pub mod intro;
pub mod outro;
pub mod palette;

use std::f32::consts::{FRAC_PI_2, PI};

use rune::{
    Sequence,
    Color, Cx, Cycle, El, ScrollSpec, Shadow, Spring, SystemMaterial, TextStyle, Transition, Tx,
    Vec2, col, el, row, spacer, text, text_input,
};
use rune_kit::markdown::{Markdown, MarkdownStyle};

use crate::icons::{self, Icon};
use crate::protocol::{ToastStyle, WireMetadata, WireView};
use crate::store::{Cantrip, Row, row_key};
use intro::Intro;
use outro::Outro;
use palette::Palette;

pub const PANEL_SIZE: (f32, f32) = (760.0, 480.0);
pub const RADIUS: f32 = 16.0;
/// Clear space around the panel inside its window, for its shadow and glow and for sparks and
/// stardust to fly past its edges.
pub const MARGIN: f32 = 44.0;
/// The window: the panel and its margin.
pub const WINDOW_SIZE: (f32, f32) = (PANEL_SIZE.0 + 2.0 * MARGIN, PANEL_SIZE.1 + 2.0 * MARGIN);

const ROW_H: f32 = 44.0;
const ROW_GAP: f32 = 2.0;
const LIST_PAD: f32 = 8.0;
const SECTION_H: f32 = 26.0;
/// The selection glides between rows with a touch of overshoot.
const GLIDE: Spring = Spring::new(0.34, 0.16);

/// Everything a piece of the view needs to draw itself.
#[derive(Clone, Copy)]
struct Look {
    p: Palette,
    scale: f32,
    /// The entrance to play, after an opening.
    intro: Option<Intro>,
}

fn font(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight)
}

/// Small caps-ish labels: section headers, metadata labels.
fn eyebrow(label: &str, look: Look) -> El {
    text(label.to_uppercase())
        .font(font(10.5, 650.0).tracking(1.1))
        .color(look.p.faint)
}

pub fn view(s: &Cantrip, cx: &mut Cx<Cantrip>) -> El {
    let look = Look {
        p: palette::arcane(),
        scale: cx.env().scale,
        intro: Intro::new(s.intro).filter(|_| s.revealed),
    };
    let p = look.p;
    let mut root = el().key("root").w_full().h_full().padding(MARGIN).child(panel(s, look));
    if let Some(intro) = look.intro {
        // Thrown from the wand's orb: 16 pt in, centred in the 64 pt search row.
        root = root.child(intro.burst(MARGIN + 16.0 + 17.0, MARGIN + 32.0, p));
    }
    if s.closing {
        root = root.child(Outro::new(s.outro).dust(MARGIN, MARGIN, PANEL_SIZE, p));
    }
    root
}

fn panel(s: &Cantrip, look: Look) -> El {
    let p = look.p;
    let loading = s.session.as_ref().is_some_and(|s| s.loading());
    let mut panel = col()
        .key("panel")
        .w_full()
        .h_full()
        .radius(RADIUS)
        .continuous_corners()
        .clip_rounded(RADIUS)
        .fill(p.bg)
        .border_gradient(1.0, Color::WHITE.alpha(0.20), Color::WHITE.alpha(0.05))
        .box_shadow(Shadow {
            x: 0.0,
            y: 0.0,
            blur: 60.0,
            spread: -10.0,
            color: p.violet.alpha(0.16),
            inset: true,
        })

        // The search field keeps the keyboard whatever is clicked (rows, actions, back).
        .preserve_focus_on_press(true)
        .child(aurora(look))
        .child(search_bar(s, look))
        .child(loading_bar(loading, look))
        .child(glow_rule(look))
        .child(content(s, look))
        .child(play(footer(s, look), look.intro.map(Intro::footer)));
    if s.actions_open {
        panel = panel.child(action_panel(s, look));
    }
    if let Some(intro) = look.intro {
        panel = panel.child(intro.stars(PANEL_SIZE.0, p));
    }
    // Live and at rest, the desktop behind is blurred (the material replaces the fill) and
    // tinted with ink by the aurora; headless renders draw the fill. The native blur sits under
    // everything Rune draws and doesn't fade with it, so it's only there while the bar is open:
    // closing, and while hidden, the ink glass fades on its own.
    if s.revealed && !s.closing {
        panel = panel.system_material(SystemMaterial::Popover);
    }
    // The shell carries what the panel's rounded clip would cut off (its shadow and glow) and
    // the whole entrance and exit.
    let shell = el()
        .key("shell")
        .w_full()
        .h_full()
        .radius(RADIUS)
        .continuous_corners()
        // The window draws no shadow of its own (it's larger than the panel): this is it.
        .box_shadow(Shadow { x: 0.0, y: 18.0, blur: 48.0, spread: -6.0, color: Color::BLACK.alpha(0.55), inset: false })
        .box_shadow(Shadow { x: 0.0, y: 0.0, blur: 40.0, spread: -8.0, color: p.violet.alpha(0.3), inset: false })
        // Dispersing, the glass goes soft as it fades.
        .blur(if s.closing { 9.0 } else { 0.0 })
        .child(panel);
    match (s.revealed, look.intro) {
        _ if s.closing => shell.sequence(Outro::new(s.outro).glass()),
        (false, _) => shell.sequence(Sequence::new(intro::CLOSED)),
        (true, Some(intro)) => shell.sequence(intro.panel()),
        (true, None) => shell,
    }
}

/// `el` with its part of the entrance, if one is playing.
pub(crate) fn play(el: El, sequence: Option<Sequence>) -> El {
    match sequence {
        Some(sequence) => el.sequence(sequence),
        None => el,
    }
}

/// Light behind the glass: violet and magenta pooled at the top, a cool wash at the bottom.
fn aurora(look: Look) -> El {
    let p = look.p;
    let pool = |i: usize, center: [f32; 2], radius: [f32; 2], color: Color, strength: f32| {
        // Each pool blooms out from its own centre.
        let pool = el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).origin(center[0], center[1]);
        play(pool, look.intro.map(|intro| intro.bloom(i)))
            .a11y_hidden()
            .radial_gradient(
                center,
                radius,
                [
                    (0.0, color.alpha(strength)),
                    (0.45, color.alpha(strength * 0.35)),
                    (1.0, color.alpha(0.0)),
                ],
            )
    };
    el().key("aurora")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .a11y_hidden()
        // Ink over the blur, so what's behind reads as light, not as text.
        .fill(p.bg.alpha(0.84))
        .child(pool(0, [0.12, -0.05], [460.0, 210.0], p.violet, 0.34))
        .child(pool(1, [0.78, -0.10], [420.0, 170.0], p.magenta, 0.20))
        .child(pool(2, [0.55, 1.08], [520.0, 160.0], p.cyan, 0.09))
}

/// A hairline that glows in the middle.
fn glow_rule(look: Look) -> El {
    // It draws itself across from the leading edge.
    let rule = el().key("rule").origin(0.0, 0.5);
    play(rule, look.intro.map(Intro::rule))
        .h(1.0)
        .w_full()
        .linear_gradient(FRAC_PI_2, palette::arcane().rule())
}

// MARK: Search bar

/// The wand (or back button) in a glowing orb.
fn orb(size: f32, look: Look) -> El {
    let p = look.p;
    el().size(size, size)
        .radius(size / 2.0)
        .center()
        .radial_gradient(
            [0.35, 0.3],
            size * 0.9,
            [
                (0.0, p.violet.alpha(0.55)),
                (0.7, p.magenta.alpha(0.18)),
                (1.0, p.magenta.alpha(0.06)),
            ],
        )
        .border_gradient(1.0, Color::WHITE.alpha(0.28), Color::WHITE.alpha(0.04))
        .box_shadow(Shadow {
            x: 0.0,
            y: 0.0,
            blur: 22.0,
            spread: 0.0,
            color: p.violet.alpha(0.45),
            inset: false,
        })
}

fn search_bar(s: &Cantrip, look: Look) -> El {
    let p = look.p;
    let leading = if s.session.is_some() {
        orb(34.0, look)
            .key("back")
            .role("button")
            .label("Back")
            .hover(Color::WHITE.alpha(0.08))
            .on_press(|s: &mut Cantrip| s.pop_frame())
            .child(icons::view(
                &Icon::Symbol("chevron.left".into()),
                16.0,
                p.text,
                look.scale,
            ))
    } else {
        play(orb(34.0, look).key("wand"), look.intro.map(Intro::orb)).child(icons::view(
            &Icon::Symbol("wand.and.sparkles".into()),
            18.0,
            p.text,
            look.scale,
        ))
    };
    let field = text_input(&s.field, font(21.0, 400.0).lh(28.0).tracking(-0.2))
        .key("query")
        .single_line()
        .grow(1.0)
        .color(p.text)
        .caret(p.magenta, p.violet.alpha(0.45))
        .placeholder(s.placeholder(), p.faint)
        .on_edit(|s: &mut Cantrip| &mut s.field)
        .on_input_change(|s: &mut Cantrip, _| {
            let text = s.field.text.clone();
            s.set_query(text);
        })
        .on_input_key(|s: &mut Cantrip, ev, tx| s.handle_key(ev, tx))
        // Real ↵ is taken by the key handler; this covers accessibility (AXConfirm).
        .on_submit(|s: &mut Cantrip, tx: &mut Tx| s.submit(false, tx));
    row()
        .key("search")
        .h(64.0)
        .px(16.0)
        .gap(14.0)
        .center_y()
        .child(leading)
        .child(field)
}

/// A comet that runs under the search field while a spell is working, from per-segment
/// opacity loops on Rune's clock (no view rebuilds).
fn loading_bar(active: bool, look: Look) -> El {
    const SEGMENTS: usize = 32;
    const STOPS: &[(f32, f32)] = &[(0.0, 0.0), (0.1, 1.0), (0.32, 0.0), (1.0, 0.0)];
    let mut bar = row().key("loading").h(2.0).w_full();
    if active {
        for i in 0..SEGMENTS {
            let t = i as f32 / (SEGMENTS - 1) as f32;
            let phase = -t * 0.75;
            bar = bar.child(
                el().grow(1.0)
                    .h_full()
                    .fill(look.p.violet.mix(look.p.magenta, t))
                    .cycle(Cycle {
                        period: 1.1,
                        phase,
                        stops: STOPS,
                        still: 0.4,
                        fps: 60.0,
                    }),
            );
        }
    }
    bar
}

// MARK: Content

fn content(s: &Cantrip, look: Look) -> El {
    let body = if let Some((title, message)) = s.session.as_ref().and_then(|s| s.error.as_ref()) {
        error_view(title, message, look)
    } else if let Some(WireView::Detail {
        markdown, metadata, ..
    }) = s.current_view()
    {
        detail_view(markdown, metadata, look)
    } else if s.session.is_some() && s.current_view().is_none() {
        el()
    } else {
        results(s, look)
    };
    col()
        .key("content")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(body.grow(1.0).min_h(0.0))
}

fn results(s: &Cantrip, look: Look) -> El {
    let p = look.p;
    let rows = s.rows();
    if rows.is_empty() {
        let message = match s.current_view() {
            Some(WireView::List {
                empty_text: Some(t),
                ..
            }) if s.query().is_empty() => t.clone(),
            _ if s.query().is_empty() => "Nothing here".into(),
            _ => format!("No spell answers to “{}”", s.query()),
        };
        return col()
            .key("empty")
            .w_full()
            .center()
            .gap(16.0)
            .child(orb(64.0, look).child(icons::view(
                &Icon::Symbol("sparkle.magnifyingglass".into()),
                30.0,
                p.text.alpha(0.85),
                look.scale,
            )))
            .child(text(message).font(font(13.5, 500.0)).color(p.muted));
    }
    let selection = s.selection();
    let header = s.session.is_none() && s.query().is_empty();
    let mut list = col().key("rows").w_full().gap(ROW_GAP).instant_changes();
    if header {
        list = list.child(
            row()
                .key("section")
                .h(SECTION_H)
                .px(12.0)
                .center_y()
                .child(eyebrow("Results", look))
                .child(spacer())
                .child(
                    text(rows.len().to_string())
                        .font(font(10.5, 600.0).features("'tnum'"))
                        .color(p.faint),
                ),
        );
    }
    for (i, r) in rows.iter().enumerate() {
        let rise = look.intro.filter(|_| s.intro_rows && i < intro::STAGGERED_ROWS).map(|intro| intro.row(i));
        list = list.child(play(row_view(r, i, i == selection, look), rise));
    }
    // The selection is one element that glides from row to row, under them and outside the
    // list's instant changes, so filtering snaps while moving the selection springs.
    let top = LIST_PAD
        + if header { SECTION_H + ROW_GAP } else { 0.0 }
        + selection as f32 * (ROW_H + ROW_GAP);
    let highlight = el()
        .key("selection")
        .a11y_hidden()
        .pin(Some(LIST_PAD), Some(top), Some(LIST_PAD), None)
        .h(ROW_H)
        .radius(11.0)
        .continuous_corners()
        .linear_gradient(FRAC_PI_2, p.sweep(1.0))
        .border_gradient(1.0, Color::WHITE.alpha(0.16), Color::WHITE.alpha(0.03))
        .box_shadow(Shadow {
            x: 0.0,
            y: 4.0,
            blur: 22.0,
            spread: -4.0,
            color: p.violet.alpha(0.35),
            inset: false,
        })
        .motion(GLIDE)
        .child(
            el().pin(Some(0.0), Some(12.0), None, None)
                .size(3.0, ROW_H - 24.0)
                .radius(1.5)
                .linear_gradient(PI, [(0.0, p.violet), (1.0, p.magenta)])
                .box_shadow(Shadow {
                    x: 0.0,
                    y: 0.0,
                    blur: 10.0,
                    spread: 0.0,
                    color: p.magenta.alpha(0.8),
                    inset: false,
                }),
        );
    col()
        .key("results")
        .w_full()
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .w_full()
                .padding(LIST_PAD)
                .child(play(highlight, look.intro.filter(|_| s.intro_rows).map(|intro| intro.row(selection.min(intro::STAGGERED_ROWS)))))
                .child(list),
        )
}

/// A coloured tag at the end of a row.
fn chip(label: &str, look: Look) -> El {
    let c = look.p.kind(label);
    row()
        .h(20.0)
        .px(8.0)
        .center()
        .radius(6.0)
        .fill(c.alpha(0.12))
        .border(1.0, c.alpha(0.22))
        .child(text(label.to_string()).font(font(11.0, 600.0)).color(c))
}

fn row_view(r: &Row, index: usize, selected: bool, look: Look) -> El {
    let p = look.p;
    let icon = match &r.icon {
        // App icons bring their own tile.
        Icon::File(_) => icons::view(&r.icon, 28.0, p.text, look.scale),
        other => el()
            .size(28.0, 28.0)
            .radius(8.0)
            .center()
            .fill(Color::WHITE.alpha(if selected { 0.10 } else { 0.05 }))
            .border(1.0, Color::WHITE.alpha(0.06))
            .child(icons::view(
                other,
                18.0,
                if selected { p.text } else { p.muted.alpha(0.9) },
                look.scale,
            )),
    };
    let mut line = row()
        .key(row_key(index))
        .role("option")
        .label(r.title.clone())
        .checked(selected)
        .w_full()
        .h(ROW_H)
        .pl(12.0)
        .pr(10.0)
        .gap(12.0)
        .center_y()
        .radius(11.0)
        .continuous_corners()
        .hover(Color::WHITE.alpha(0.035))
        .on_press_tx(move |s: &mut Cantrip, tx: &mut Tx| {
            s.set_selection(index);
            if let Some(first) = s.rows().get(index).and_then(|r| r.actions.first().cloned()) {
                s.perform(first, tx);
            }
        })
        .child(icon)
        .child(
            text(r.title.clone())
                .font(font(14.5, if selected { 600.0 } else { 500.0 }))
                .color(if selected { p.text } else { p.text.alpha(0.86) })
                .truncate()
                .max_w(440.0),
        );
    // The subtitle takes what the title leaves, and gives way first.
    line = match r.subtitle.as_ref().filter(|s| !s.is_empty()) {
        Some(subtitle) => line.child(
            text(subtitle.clone())
                .font(font(13.0, 400.0))
                .color(p.muted)
                .truncate()
                .grow(1.0)
                .min_w(0.0)
                .mr(8.0),
        ),
        None => line.child(spacer().min_w(8.0)),
    };
    for accessory in &r.accessories {
        line = line.child(chip(accessory, look));
    }
    line
}

fn detail_view(markdown: &str, metadata: &[WireMetadata], look: Look) -> El {
    let p = look.p;
    let style = MarkdownStyle::neutral(true)
        .inline_code_bg(p.violet.alpha(0.16))
        .code_block(Color::BLACK.alpha(0.28), Color::WHITE.alpha(0.07));
    let body = col()
        .key("detail-scroll")
        .grow(1.0)
        .h_full()
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .w_full()
                .px(24.0)
                .py(20.0)
                .child(Markdown::new(markdown).selectable(true).render(&style)),
        );
    let mut detail = row().key("detail").w_full().h_full().child(body);
    if !metadata.is_empty() {
        let mut items = col().w_full().padding(18.0).gap(16.0);
        for item in metadata {
            items = items.child(
                col().gap(4.0).child(eyebrow(&item.label, look)).child(
                    text(item.value.clone())
                        .font(font(13.5, 500.0))
                        .color(p.text)
                        .wrap()
                        .selectable(true),
                ),
            );
        }
        detail = detail
            .child(el().w(1.0).h_full().linear_gradient(
                PI,
                [
                    (0.0, Color::TRANSPARENT),
                    (0.3, p.violet.alpha(0.35)),
                    (1.0, Color::TRANSPARENT),
                ],
            ))
            .child(
                col()
                    .key("metadata")
                    .w(230.0)
                    .h_full()
                    .fill(Color::WHITE.alpha(0.02))
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .child(items),
            );
    }
    detail
}

fn error_view(title: &str, message: &str, look: Look) -> El {
    let p = look.p;
    col()
        .key("error")
        .w_full()
        .h_full()
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col().w_full().padding(18.0).child(
                col()
                    .w_full()
                    .padding(18.0)
                    .gap(12.0)
                    .radius(12.0)
                    .continuous_corners()
                    .linear_gradient(PI, [(0.0, p.red.alpha(0.14)), (1.0, p.red.alpha(0.03))])
                    .border_gradient(1.0, p.red.alpha(0.4), p.red.alpha(0.08))
                    .child(
                        row()
                            .gap(10.0)
                            .center_y()
                            .child(icons::view(
                                &Icon::Symbol("exclamationmark.triangle.fill".into()),
                                18.0,
                                p.red,
                                look.scale,
                            ))
                            .child(text(title.to_string()).font(font(15.0, 650.0)).color(p.red)),
                    )
                    .child(
                        text(message.to_string())
                            .font(
                                font(12.0, 400.0)
                                    .family(rune::tok::Family::SystemMono)
                                    .lh(18.0),
                            )
                            .color(p.text.alpha(0.72))
                            .wrap()
                            .selectable(true),
                    ),
            ),
        )
}

// MARK: Footer and actions

fn footer(s: &Cantrip, look: Look) -> El {
    let p = look.p;
    let leading = if let Some(toast) = &s.toast {
        let (symbol, color) = match toast.style {
            ToastStyle::Success => ("checkmark.circle.fill", p.green),
            ToastStyle::Failure => ("xmark.octagon.fill", p.red),
            ToastStyle::Info => ("sparkles", p.violet.mix(Color::WHITE, 0.3)),
        };
        let mut line = row()
            .key("toast")
            .h(24.0)
            .pl(6.0)
            .pr(10.0)
            .gap(6.0)
            .center_y()
            .radius(12.0)
            .shrink()
            .min_w(0.0)
            .fill(color.alpha(0.12))
            .border(1.0, color.alpha(0.25))
            .help(toast.message.clone().unwrap_or_else(|| toast.title.clone()))
            .transition(Transition::opacity().combined(Transition::offset(-8.0, 0.0)))
            .child(icons::view(
                &Icon::Symbol(symbol.into()),
                15.0,
                color,
                look.scale,
            ))
            .child(
                text(toast.title.clone())
                    .font(font(12.0, 600.0))
                    .color(p.text),
            );
        if let Some(message) = &toast.message {
            line = line.child(
                text(message.replace('\n', " · "))
                    .font(font(12.0, 400.0))
                    .color(p.muted)
                    .truncate()
                    .shrink(),
            );
        }
        line
    } else if let Some(session) = &s.session {
        row()
            .key("spell")
            .gap(7.0)
            .center_y()
            .child(icons::view(
                &Icon::parse(session.command.icon.as_deref()),
                15.0,
                p.muted,
                look.scale,
            ))
            .child(
                text(session.command.title.clone())
                    .font(font(12.0, 600.0))
                    .color(p.muted),
            )
    } else {
        row()
            .key("brand")
            .gap(6.0)
            .center_y()
            .child(text("✦").font(font(12.0, 600.0)).color(p.magenta))
            .child(
                text("cantrip")
                    .font(font(12.0, 650.0).tracking(0.6))
                    .color(p.muted),
            )
    };
    let mut bar = row()
        .key("footer")
        .h(42.0)
        .px(14.0)
        .gap(12.0)
        .center_y()
        .fill(Color::BLACK.alpha(0.22))
        .child(leading)
        .child(spacer().min_w(12.0));
    if let Some(primary) = s.current_actions().first() {
        bar = bar
            .child(key_hint("primary-hint", &primary.title, &["↵"], look))
            .child(el().w(1.0).h(16.0).fill(Color::WHITE.alpha(0.08)));
    }
    col()
        .w_full()
        .child(el().h(1.0).w_full().fill(Color::WHITE.alpha(0.06)))
        .child(bar.child(key_hint("actions-hint", "Actions", &["⌘", "K"], look)))
}

fn keycap(label: &str, look: Look) -> El {
    el().h(20.0)
        .min_w(20.0)
        .px(5.0)
        .center()
        .radius(5.0)
        .fill(Color::WHITE.alpha(0.07))
        .border_gradient(1.0, Color::WHITE.alpha(0.16), Color::WHITE.alpha(0.03))
        .box_shadow(Shadow {
            x: 0.0,
            y: 1.0,
            blur: 0.0,
            spread: 0.0,
            color: Color::BLACK.alpha(0.4),
            inset: false,
        })
        .child(
            text(label.to_string())
                .font(font(11.0, 600.0))
                .color(look.p.text.alpha(0.8)),
        )
}

fn key_hint(key: &str, title: &str, keys: &[&str], look: Look) -> El {
    let mut hint = row().key(key.to_string()).gap(4.0).center_y().child(
        text(title.to_string())
            .font(font(12.0, 600.0))
            .color(look.p.text.alpha(0.88))
            .mr(4.0),
    );
    for k in keys {
        hint = hint.child(keycap(k, look));
    }
    hint
}

fn action_panel(s: &Cantrip, look: Look) -> El {
    let p = look.p;
    let actions = s.current_actions();
    let mut list = col().gap(2.0).child(
        row()
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(eyebrow("Actions", look)),
    );
    if actions.is_empty() {
        list = list.child(
            text("No actions")
                .font(font(13.0, 400.0))
                .color(p.muted)
                .padding(10.0),
        );
    }
    for (i, action) in actions.iter().enumerate() {
        let selected = i == s.actions_selection;
        let keys: &[&str] = match i {
            0 => &["↵"],
            1 => &["⌘", "↵"],
            _ => &[],
        };
        let chosen = action.clone();
        let mut item = row()
            .key(format!("action-{i}"))
            .role("menuitem")
            .label(action.title.clone())
            .h(34.0)
            .px(8.0)
            .gap(10.0)
            .center_y()
            .radius(8.0)
            .continuous_corners()
            .hover(Color::WHITE.alpha(0.04))
            .on_press_tx(move |s: &mut Cantrip, tx: &mut Tx| s.perform(chosen.clone(), tx))
            .child(icons::view(
                &Icon::parse(Some(action.icon.as_deref().unwrap_or("circle"))),
                16.0,
                if selected { p.text } else { p.muted },
                look.scale,
            ))
            .child(
                text(action.title.clone())
                    .font(font(13.0, if selected { 600.0 } else { 500.0 }))
                    .color(p.text),
            )
            .child(spacer().min_w(16.0));
        if selected {
            item = item
                .linear_gradient(FRAC_PI_2, p.sweep(1.0))
                .border_gradient(1.0, Color::WHITE.alpha(0.14), Color::WHITE.alpha(0.03));
        }
        for k in keys {
            item = item.child(keycap(k, look));
        }
        list = list.child(item);
    }
    col()
        .key("actions")
        .pin(None, None, Some(12.0), Some(50.0))
        .w(290.0)
        .padding(6.0)
        .radius(14.0)
        .continuous_corners()
        .fill(p.raised)
        .radial_gradient(
            [0.1, 0.0],
            [260.0, 120.0],
            [(0.0, p.violet.alpha(0.22)), (1.0, p.violet.alpha(0.0))],
        )
        .border_gradient(1.0, Color::WHITE.alpha(0.22), Color::WHITE.alpha(0.05))
        .box_shadow(Shadow {
            x: 0.0,
            y: 14.0,
            blur: 40.0,
            spread: 0.0,
            color: Color::BLACK.alpha(0.55),
            inset: false,
        })
        .box_shadow(Shadow {
            x: 0.0,
            y: 0.0,
            blur: 30.0,
            spread: 0.0,
            color: p.violet.alpha(0.22),
            inset: false,
        })
        .transition(
            Transition::opacity().combined(Transition::scale_anchor(0.94, Vec2 { x: 1.0, y: 1.0 })),
        )
        .raise()
        .child(list)
}
