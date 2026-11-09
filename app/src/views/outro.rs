//! The bar's exit: the glass blurs, swells a little and fades, while stardust lifts off it and
//! drifts up and outward past its edges. The window hides once it's gone (`OUTRO`).
//!
//! A sequence can only scale and fade, so each mote rides its own panel-sized layer scaled about
//! a point below the panel's centre: scaling that layer carries the mote up and away.

use std::time::Duration;

use rune::{Beat, Color, Easing, El, Sequence, Shadow, el};

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
            // How far it travels: its layer's end scale, about a point under the panel.
            let drift = [NEAR, MID, FAR][(noise(i, 4) * 2.99) as usize];
            let mote = el()
                .absolute(mx - dot / 2.0, my - dot / 2.0)
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
                });
            layer = layer.child(
                el().absolute(x, y)
                    .size(w, h)
                    .origin(0.5, 0.5 + noise(i, 5) * 0.6)
                    .sequence(self.at(drift, noise(i, 6) as f64 * 0.12))
                    .child(mote),
            );
        }
        layer
    }
}

/// Sigil's seal: a ring closes in over the wand as the glass fades, turning to a point.
const SEAL: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(2.6),
    Beat::new(0.24)
        .opacity(0.9)
        .scale(1.0)
        .easing(Easing::EASE_OUT),
    Beat::new(0.46)
        .opacity(0.0)
        .scale(0.08)
        .easing(Easing::EASE_IN),
];

impl Outro {
    /// Sigil's exit, instead of stardust: two fine gold rings close over the wand at (`x`,
    /// `y`) as the glass goes.
    pub fn seal(self, x: f32, y: f32, p: Palette) -> El {
        let ring = |r: f32, alpha: f32| {
            el().absolute(30.0 - r, 30.0 - r)
                .size(2.0 * r, 2.0 * r)
                .radius(r)
                .border(1.0, p.glow.alpha(alpha))
        };
        el().key(format!("seal-{}", self.n))
            .a11y_hidden()
            .inert()
            .pin(Some(x - 30.0), Some(y - 30.0), None, None)
            .size(60.0, 60.0)
            .raise()
            .sequence(self.at(SEAL, 0.06))
            .child(ring(26.0, 0.85))
            .child(ring(20.0, 0.45))
    }
}

/// Three reaches, near to far (a sequence's beats are static, so the spread comes from a few).
const NEAR: &[Beat] = &drift(1.2);
const MID: &[Beat] = &drift(1.42);
const FAR: &[Beat] = &drift(1.7);
