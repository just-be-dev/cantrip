//! The settings' controls, at the end of their rows in the results list (`store::Settings`):
//! the hotkey as keycaps (or listening for a new one), the theme by name, and a switch.

use rune::{Color, Cycle, El, Shadow, el, row, text};

use super::{Look, font, keycap};
use crate::icons::{self, Icon};
use crate::store::{Cantrip, Row};

/// What row `r` of the settings ends in, and the colour of its subtitle when it isn't the
/// usual one (a hotkey that was refused, or couldn't be registered).
pub(super) fn control(s: &Cantrip, r: &Row, look: Look) -> (Option<El>, Option<Color>) {
    let p = look.p;
    match r.id.as_str() {
        "setting:hotkey" => match s.recording() {
            Some(recording) => (
                Some(listening(&recording.held, recording.error.is_some(), look)),
                recording.error.as_ref().map(|_| p.red),
            ),
            None => {
                let broken = s.hotkey_error.is_some();
                (
                    Some(keys(
                        &crate::config::hotkey_keycaps(&s.hotkey),
                        broken,
                        look,
                    )),
                    broken.then_some(p.red),
                )
            }
        },
        "setting:theme" => (
            Some(
                row()
                    .key("theme-value")
                    .gap(6.0)
                    .center_y()
                    .child(
                        text(s.theme.title())
                            .font(font(13.0, 600.0))
                            .color(p.glow.mix(p.text, 0.35)),
                    )
                    .child(icons::view(
                        &Icon::Symbol("chevron.right".into()),
                        11.0,
                        p.muted,
                        look.scale,
                    )),
            ),
            None,
        ),
        "setting:login" => (Some(switch(s.login, look)), None),
        _ => (None, None),
    }
}

/// The hotkey's keycaps. One that isn't registered is struck through in red.
fn keys(labels: &[String], broken: bool, look: Look) -> El {
    let mut caps = row().key("hotkey-keys").gap(4.0).center_y();
    for label in labels {
        caps = caps.child(keycap(label, look));
    }
    if broken {
        caps = caps.opacity(0.6).child(
            el().pin(Some(-2.0), Some(10.0), Some(-2.0), None)
                .h(1.0)
                .fill(look.p.red),
        );
    }
    caps
}

/// Listening for a new hotkey: a glowing well that shows the modifiers as they're held, with a
/// beating dot while it waits.
fn listening(held: &rune::Mods, refused: bool, look: Look) -> El {
    let p = look.p;
    let edge = if refused { p.red } else { p.glow };
    let mut well = row()
        .key("hotkey-recording")
        .role("status")
        .label("Recording a new hotkey")
        .h(26.0)
        .min_w(120.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(8.0)
        .continuous_corners()
        .fill(edge.alpha(0.10))
        .border(1.0, edge.alpha(0.55))
        .box_shadow(Shadow {
            x: 0.0,
            y: 0.0,
            blur: 14.0,
            spread: 0.0,
            color: edge.alpha(0.35 * p.radiance),
            inset: false,
        })
        .child(el().size(7.0, 7.0).radius(3.5).fill(edge).cycle(Cycle {
            period: 1.2,
            phase: 0.0,
            stops: &[(0.0, 1.0), (0.5, 0.25), (1.0, 1.0)],
            still: 1.0,
            fps: 30.0,
        }));
    let mods = rune::modifier_keycaps(*held);
    if mods.is_empty() {
        well = well.child(
            text("Listening…")
                .font(font(12.0, 600.0))
                .color(p.text.alpha(0.8)),
        );
    } else {
        for label in mods {
            well = well.child(keycap(&label, look));
        }
    }
    well
}

/// An on/off switch, its knob gliding across.
fn switch(on: bool, look: Look) -> El {
    let p = look.p;
    let (w, h, knob) = (34.0, 20.0, 16.0);
    let inset = (h - knob) / 2.0;
    el().key("switch")
        .role("switch")
        .checked(on)
        .size(w, h)
        .radius(h / 2.0)
        .fill(if on {
            p.glow.alpha(0.75)
        } else {
            Color::WHITE.alpha(0.10)
        })
        .border(1.0, Color::WHITE.alpha(if on { 0.18 } else { 0.10 }))
        .child(
            el().key("knob")
                .pin(
                    Some(if on { w - knob - inset } else { inset }),
                    Some(inset),
                    None,
                    None,
                )
                .size(knob, knob)
                .radius(knob / 2.0)
                .fill(p.text)
                .box_shadow(Shadow {
                    x: 0.0,
                    y: 1.0,
                    blur: 3.0,
                    spread: 0.0,
                    color: Color::BLACK.alpha(0.4),
                    inset: false,
                })
                .motion(super::GLIDE),
        )
}
