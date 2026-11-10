//! The bar's exit. Arcane: the glass blurs, swells a little and fades, while stardust lifts off
//! it and drifts up and outward past its edges. Sigil: a binding sigil (a ring of runes round a
//! seven-pointed star) closes in over the panel while the glass falls into its heart; then it
//! collapses to a point that flares and goes out. The window hides once it's gone (`OUTRO`).
//!
//! A sequence can only scale and fade, so each mote scales about a point below the panel's
//! centre, far outside itself (`origin` beyond 0..1): scaling carries it up and away. The mote is
//! a single childless element carrying its own sequence: a fading element with children is
//! drawn into an offscreen layer of its own, and 72 panel-sized ones a frame made the exit stutter.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::time::Duration;

use rune::{Beat, Color, Easing, El, LineCap, Sequence, Shadow, Shape, canvas, el};

use super::palette::Palette;

/// How long the exit plays before the window hides.
pub const OUTRO: Duration = Duration::from_millis(520);

const GLASS: &[Beat] = &[
    Beat::new(0.0),
    Beat::new(0.1).scale(1.012).easing(Easing::EASE_OUT),
    Beat::new(0.36)
        .opacity(0.0)
        .scale(1.045)
        .easing(Easing::EASE_IN),
];
/// A mote's layer: it flickers on as the glass goes, holds, then drifts out to `reach` and fades.
const fn drift(reach: f32) -> [Beat; 4] {
    [
        Beat::new(0.0).opacity(0.0),
        Beat::new(0.07)
            .opacity(1.0)
            .scale(1.0 + (reach - 1.0) * 0.15)
            .easing(Easing::EASE_OUT),
        Beat::new(0.24)
            .opacity(0.9)
            .scale(1.0 + (reach - 1.0) * 0.55),
        Beat::new(0.46)
            .opacity(0.0)
            .scale(reach)
            .easing(Easing::EASE_OUT),
    ]
}
const MOTES: usize = 72;

/// The exit's steps. Like the intro, alternate closings nudge every delay so it replays.
#[derive(Clone, Copy)]
pub struct Outro {
    n: u64,
}

/// A small deterministic hash, so the dust lands in the same scattered places every time.
fn noise(i: usize, salt: u32) -> f32 {
    let mut x = (i as u32).wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

impl Outro {
    pub fn new(n: u64) -> Outro {
        Outro { n }
    }

    fn at(self, beats: &'static [Beat], delay: f64) -> Sequence {
        Sequence::new(beats).delay(delay + (self.n % 2) as f64 * 0.001)
    }

    /// The glass itself.
    pub fn glass(self) -> Sequence {
        self.at(GLASS, 0.0)
    }

    /// Stardust over the panel, whose top-left is (`x`, `y`) in the window and which is
    /// `size` points.
    pub fn dust(self, x: f32, y: f32, size: (f32, f32), p: Palette) -> El {
        let (w, h) = size;
        let mut layer = el()
            .key(format!("dust-{}", self.n))
            .a11y_hidden()
            .inert()
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .raise();
        for i in 0..MOTES {
            let (mx, my) = (noise(i, 1) * w, noise(i, 2) * h);
            let dot = 2.2 + noise(i, 3) * 3.6;
            let color = [
                p.glow2.mix(Color::WHITE, 0.2),
                p.glow.mix(Color::WHITE, 0.45),
                Color::WHITE,
                p.cool.mix(Color::WHITE, 0.3),
            ][i % 4];
            // How far it travels: its end scale, about a point under the panel.
            let drift = [NEAR, MID, FAR][(noise(i, 4) * 2.99) as usize];
            // Where it scales from: a point on the panel's vertical centre line, below the
            // middle, in units of the mote's own size.
            let (left, top) = (x + mx - dot / 2.0, y + my - dot / 2.0);
            let pivot = (x + 0.5 * w, y + (0.5 + noise(i, 5) * 0.6) * h);
            layer = layer.child(
                el().absolute(left, top)
                    .size(dot, dot)
                    .radius(dot / 2.0)
                    .fill(color)
                    .box_shadow(Shadow {
                        x: 0.0,
                        y: 0.0,
                        blur: 6.0 + dot * 2.0,
                        spread: 0.5,
                        color: color.alpha(0.85),
                        inset: false,
                    })
                    .origin((pivot.0 - left) / dot, (pivot.1 - top) / dot)
                    .sequence(self.at(drift, noise(i, 6) as f64 * 0.12)),
            );
        }
        layer
    }
}

/// Three reaches, near to far (a sequence's beats are static, so the spread comes from a few).
const NEAR: &[Beat] = &drift(1.2);
const MID: &[Beat] = &drift(1.42);
const FAR: &[Beat] = &drift(1.7);

// MARK: Sigil's binding

/// Sigil's glass draws in a breath, then falls into its centre (the shell's origin), fading
/// as it goes.
const FALL: &[Beat] = &[
    Beat::new(0.0),
    Beat::new(0.07).scale(1.015).easing(Easing::EASE_OUT),
    Beat::new(0.30)
        .opacity(0.5)
        .scale(0.10)
        .easing(Easing::EASE_IN),
    Beat::new(0.34)
        .opacity(0.0)
        .scale(0.02)
        .easing(Easing::EASE_IN),
];
/// The binding sigil: it closes in over the falling glass, holds, then collapses to a point.
const BIND: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(2.2),
    Beat::new(0.26)
        .opacity(1.0)
        .scale(1.0)
        .easing(Easing::EASE_OUT),
    Beat::new(0.32).scale(1.06).easing(Easing::EASE_OUT),
    Beat::new(0.44)
        .opacity(0.0)
        .scale(0.02)
        .easing(Easing::EASE_IN),
];
/// Its inner figure turns up later and goes first.
const BIND_INNER: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.4),
    Beat::new(0.08).opacity(0.0).scale(0.4),
    Beat::new(0.26)
        .opacity(1.0)
        .scale(1.0)
        .easing(Easing::OVERSHOOT),
    Beat::new(0.40)
        .opacity(0.0)
        .scale(0.02)
        .easing(Easing::EASE_IN),
];
/// The point it leaves flares and goes out, as a ring widens from it.
const FLARE: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.2),
    Beat::new(0.38).opacity(0.0).scale(0.2),
    Beat::new(0.42)
        .opacity(1.0)
        .scale(1.0)
        .easing(Easing::EASE_OUT),
    Beat::new(0.52)
        .opacity(0.0)
        .scale(1.8)
        .easing(Easing::EASE_OUT),
];
/// A ring widening from the flare.
const FLARE_RING: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.1),
    Beat::new(0.40).opacity(0.0).scale(0.1),
    Beat::new(0.42).opacity(0.9).scale(0.2),
    Beat::new(0.52)
        .opacity(0.0)
        .scale(1.0)
        .easing(Easing::EASE_OUT),
];
/// The sigil's outer figure, `r` points in radius and centred in a `2r + 4` box: two circles
/// with ticks between them, and a ring of runes inside.
fn outer(r: f32, color: Color) -> Shape {
    let c = r + 2.0;
    let at = |r: f32, a: f32| [c + r * a.cos(), c + r * a.sin()];
    let circle =
        |r: f32| -> Vec<[f32; 2]> { (0..=96).map(|j| at(r, TAU * j as f32 / 96.0)).collect() };
    let mut shape = Shape::new()
        .stroke(circle(r), 4.0, color.alpha(color.a * 0.15))
        .stroke(circle(r), 1.2, color)
        .stroke(circle(r * 0.88), 1.0, color.alpha(color.a * 0.55));
    for j in 0..24 {
        let a = j as f32 / 24.0 * TAU;
        let long = if j % 2 == 0 { 0.88 } else { 0.93 };
        shape = shape.line(
            at(r * long, a),
            at(r, a),
            1.0,
            LineCap::Butt,
            color.alpha(color.a * 0.7),
        );
    }
    // Runes round the band between the circles.
    for j in 0..12 {
        let a = (j as f32 + 0.5) / 12.0 * TAU;
        for line in super::runes::strokes(j * 5 + 3, at(r * 0.76, a), r * 0.1, a + FRAC_PI_2) {
            shape = shape.stroke(line, 1.0, color.alpha(color.a * 0.8));
        }
    }
    shape
}

/// The inner figure: a seven-pointed star (every third corner joined) in a small circle.
fn inner(r: f32, color: Color) -> Shape {
    let c = r + 2.0;
    let at = |r: f32, a: f32| [c + r * a.cos(), c + r * a.sin()];
    let star: Vec<[f32; 2]> = (0..=7)
        .map(|j| at(r, -FRAC_PI_2 + (j * 3 % 7) as f32 * TAU / 7.0))
        .collect();
    let small: Vec<[f32; 2]> = (0..=64)
        .map(|j| at(r * 0.3, TAU * j as f32 / 64.0))
        .collect();
    let ring: Vec<[f32; 2]> = (0..=96).map(|j| at(r, TAU * j as f32 / 96.0)).collect();
    Shape::new()
        .stroke(star.clone(), 3.5, color.alpha(color.a * 0.18))
        .stroke(star, 1.0, color)
        .stroke(ring, 1.0, color.alpha(color.a * 0.4))
        .stroke(small, 1.0, color.alpha(color.a * 0.8))
}

impl Outro {
    /// Sigil's glass, falling into the binding sigil's heart.
    pub fn fall(self) -> Sequence {
        self.at(FALL, 0.0)
    }

    /// Sigil's exit, instead of stardust: a sigil centred on (`cx`, `cy`) closes in over the
    /// glass as it falls (`fall`), then collapses to a point that flares and goes out. Each part
    /// is a childless element with its own sequence, so nothing fades through an offscreen layer.
    pub fn binding(self, cx: f32, cy: f32, p: Palette) -> El {
        let gold = p.glow.mix(Color::WHITE, 0.15);
        let (ro, ri) = (150.0, 98.0);
        let flare = 30.0;
        el().key(format!("binding-{}", self.n))
            .a11y_hidden()
            .inert()
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .raise()
            .child(
                canvas(outer(ro, gold.alpha(0.9)))
                    .absolute(cx - ro - 2.0, cy - ro - 2.0)
                    .size(2.0 * ro + 4.0, 2.0 * ro + 4.0)
                    .sequence(self.at(BIND, 0.0)),
            )
            .child(
                canvas(inner(ri, gold.alpha(0.75)))
                    .absolute(cx - ri - 2.0, cy - ri - 2.0)
                    .size(2.0 * ri + 4.0, 2.0 * ri + 4.0)
                    .sequence(self.at(BIND_INNER, 0.0)),
            )
            .child(
                el().absolute(cx - 40.0, cy - 40.0)
                    .size(80.0, 80.0)
                    .radius(40.0)
                    .border(1.0, gold.alpha(0.9))
                    .sequence(self.at(FLARE_RING, 0.0)),
            )
            .child(
                el().absolute(cx - flare / 2.0, cy - flare / 2.0)
                    .size(flare, flare)
                    .radius(flare / 2.0)
                    .radial_gradient(
                        [0.5, 0.5],
                        [0.5, 0.5],
                        [(0.0, Color::WHITE), (0.25, gold), (1.0, gold.alpha(0.0))],
                    )
                    .box_shadow(Shadow {
                        x: 0.0,
                        y: 0.0,
                        blur: 30.0,
                        spread: 2.0,
                        color: gold.alpha(0.7),
                        inset: false,
                    })
                    .sequence(self.at(FLARE, 0.0)),
            )
    }
}
