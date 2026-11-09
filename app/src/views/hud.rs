//! A small message that pops in over the screen and goes away by itself: an edge overlay
//! (non-activating, click-through), so the app you were in keeps the keyboard.

use std::f32::consts::FRAC_PI_2;

use rune::{Color, Cx, El, Shadow, TextStyle, el, row, text};

use super::intro::Intro;
use super::{palette, play};
use crate::icons::{self, Icon};

pub const SIZE: (f32, f32) = (520.0, 96.0);

#[derive(Default)]
pub struct Hud {
    pub message: String,
    /// Counts messages; each one replays the pop-in.
    pub shown: u64,
}

pub fn view(h: &Hud, cx: &mut Cx<Hud>) -> El {
    let p = palette::arcane();
    let intro = Intro::new(h.shown);
    let spark = el()
        .size(22.0, 22.0)
        .radius(11.0)
        .center()
        .radial_gradient([0.35, 0.3], 20.0, [(0.0, p.violet.alpha(0.7)), (1.0, p.magenta.alpha(0.15))])
        .child(icons::view(&Icon::Symbol("wand.and.sparkles".into()), 13.0, p.text, cx.env().scale));
    let capsule = row()
        .key("capsule")
        .h(42.0)
        .pl(10.0)
        .pr(18.0)
        .gap(10.0)
        .center_y()
        .radius(21.0)
        .fill(p.raised)
        .linear_gradient(FRAC_PI_2, p.sweep(0.7))
        .border_gradient(1.0, Color::WHITE.alpha(0.24), Color::WHITE.alpha(0.05))
        .box_shadow(Shadow { x: 0.0, y: 8.0, blur: 28.0, spread: 0.0, color: Color::BLACK.alpha(0.45), inset: false })
        .box_shadow(Shadow { x: 0.0, y: 0.0, blur: 24.0, spread: 0.0, color: p.violet.alpha(0.35), inset: false })
        .child(play(spark, intro.map(Intro::orb)))
        .child(text(h.message.clone()).key("message").font(TextStyle::new(13.0, 600.0)).color(p.text).max_lines(2));
    row().key("hud").w_full().h_full().center().child(play(capsule, intro.map(Intro::hud)))
}
