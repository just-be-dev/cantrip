//! The bar's entrance, about 0.6 s. The wand pops in first, alone (`wand.rs`); at `GROW` it
//! casts the bar, which grows out of its tip with a shockwave while the wand flies home to the
//! search row. The aurora, the glowing rule and the rows follow quickly behind. All of it is `El::sequence` on Rune's render clock: no state changes while
//! it plays, and it sleeps once it lands. Reduce Motion lands every final pose at once.

use rune::{Beat, Color, Easing, El, Sequence, Shadow, el};

use super::palette::Palette;

/// Holds the panel invisible while it's hidden, so it never flashes its last frame.
pub const CLOSED: &[Beat] = &[Beat::new(0.0).opacity(0.0), Beat::new(0.001).opacity(0.0)];

/// When the wand casts the bar: seconds into the opening.
pub const GROW: f64 = 0.14;

/// The bar swells out of the wand's tip (the shell's origin). Until then it holds at a speck
/// under the tip: a native blur ignores opacity, so it hides by size.
const PANEL: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.001),
    Beat::new(GROW).opacity(0.0).scale(0.001),
    Beat::new(GROW + 0.05)
        .opacity(1.0)
        .scale(0.25)
        .easing(Easing::EASE_IN),
    Beat::new(GROW + 0.22).easing(Easing::ENTER),
];
const BLOOM: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.6),
    Beat::new(0.3)
        .opacity(1.0)
        .scale(1.06)
        .easing(Easing::ENTER),
    Beat::new(0.55).scale(1.0),
];
/// The HUD's spark pops in.
const ORB: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.4),
    Beat::new(0.24)
        .opacity(1.0)
        .scale(1.18)
        .easing(Easing::OVERSHOOT),
    Beat::new(0.5).scale(1.0),
];
const RING: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.4),
    Beat::new(0.04).opacity(0.9).scale(0.6),
    Beat::new(0.32)
        .opacity(0.0)
        .scale(4.0)
        .easing(Easing::EASE_OUT),
];
const DRAW: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.0),
    Beat::new(0.35).easing(Easing::DRAW_ON),
];
const RISE: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.97),
    Beat::new(0.22).easing(Easing::ENTER),
];
const POP: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.82),
    Beat::new(0.24)
        .opacity(1.0)
        .scale(1.05)
        .easing(Easing::OVERSHOOT),
    Beat::new(0.4).scale(1.0),
];
const FADE: &[Beat] = &[Beat::new(0.0).opacity(0.0), Beat::new(0.25)];

/// Rows past this many are below the fold anyway; they just appear.
pub const STAGGERED_ROWS: usize = 9;

/// The intro's steps. Each opening replays them: a sequence replays when its spec changes, so
/// alternate openings nudge every delay by a millisecond.
#[derive(Clone, Copy)]
pub struct Intro {
    n: u64,
}

impl Intro {
    /// `None` until the first opening: launch, headless scenes and tests start at rest.
    pub fn new(intro: u64) -> Option<Intro> {
        (intro > 0).then_some(Intro { n: intro })
    }

    fn at(self, beats: &'static [Beat], delay: f64) -> Sequence {
        Sequence::new(beats).delay(delay + (self.n % 2) as f64 * 0.001)
    }

    pub fn panel(self) -> Sequence {
        self.at(PANEL, 0.0)
    }
    pub fn bloom(self, i: usize) -> Sequence {
        self.at(BLOOM, GROW + i as f64 * 0.04)
    }
    pub fn rule(self) -> Sequence {
        self.at(DRAW, GROW + 0.06)
    }
    pub fn row(self, i: usize) -> Sequence {
        self.at(RISE, GROW + 0.08 + i as f64 * 0.02)
    }
    /// The HUD's spark pops in with its capsule.
    pub fn orb(self) -> Sequence {
        self.at(ORB, 0.05)
    }
    /// The HUD's capsule pops in.
    pub fn hud(self) -> Sequence {
        self.at(POP, 0.0)
    }
    /// The selection highlight rises in with the first rows. Its spec must not depend on which
    /// row is selected: a changed spec replays the sequence, so the highlight would vanish and
    /// fade back in on every move instead of gliding.
    pub fn highlight(self) -> Sequence {
        self.row(0)
    }
    pub fn footer(self) -> Sequence {
        self.at(FADE, GROW + 0.12)
    }

    /// A shockwave from the wand's tip as it casts the bar, centred on (`cx`, `cy`). Keyed by
    /// the opening, so each one is fresh.
    pub fn burst(self, cx: f32, cy: f32, p: Palette) -> El {
        const SIZE: f32 = 150.0;
        let ring = el()
            .absolute(SIZE / 2.0 - 30.0, SIZE / 2.0 - 30.0)
            .size(60.0, 60.0)
            .radius(30.0)
            .border(1.5, Color::WHITE.alpha(0.7))
            .box_shadow(Shadow {
                x: 0.0,
                y: 0.0,
                blur: 18.0,
                spread: 0.0,
                color: p.magenta.alpha(0.7),
                inset: false,
            })
            .sequence(self.at(RING, GROW - 0.02));
        el().key(format!("burst-{}", self.n))
            .a11y_hidden()
            .inert()
            .pin(Some(cx - SIZE / 2.0), Some(cy - SIZE / 2.0), None, None)
            .size(SIZE, SIZE)
            .raise()
            .child(ring)
    }
}
