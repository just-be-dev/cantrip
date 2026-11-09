//! The bar's entrance: it swells in with a little overshoot, the aurora blooms, the wand
//! flares and throws sparks, the glowing rule draws itself across, and the rows rise in one
//! after another. All of it is `El::sequence` on Rune's render clock: no state changes while
//! it plays, and it sleeps once it lands. Reduce Motion lands every final pose at once.

use rune::{Beat, Color, Easing, El, Sequence, Shadow, el};

use super::palette::Palette;

/// Holds the panel invisible while it's hidden, so it never flashes its last frame.
pub const CLOSED: &[Beat] = &[Beat::new(0.0).opacity(0.0), Beat::new(0.001).opacity(0.0)];

const PANEL: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.94),
    Beat::new(0.2).opacity(1.0).scale(1.012).easing(Easing::ENTER),
    Beat::new(0.42).scale(1.0),
];
const BLOOM: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.45),
    Beat::new(0.5).opacity(1.0).scale(1.12).easing(Easing::ENTER),
    Beat::new(1.1).scale(1.0),
];
const ORB: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.4),
    Beat::new(0.24).opacity(1.0).scale(1.18).easing(Easing::OVERSHOOT),
    Beat::new(0.5).scale(1.0),
];
const SPARKS: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.2),
    Beat::new(0.08).opacity(1.0).scale(0.6).easing(Easing::EASE_OUT),
    Beat::new(0.7).opacity(0.0).scale(1.45).easing(Easing::EASE_OUT),
];
const RING: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.4),
    Beat::new(0.06).opacity(0.9).scale(0.6),
    Beat::new(0.6).opacity(0.0).scale(2.4).easing(Easing::EASE_OUT),
];
const TWINKLE: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.0),
    Beat::new(0.14).opacity(1.0).scale(1.3).easing(Easing::EASE_OUT),
    Beat::new(0.5).opacity(0.0).scale(0.15).easing(Easing::EASE_IN),
];
/// Stars along the search row: (x, y, size in points, colour), left to right.
const STARS: &[(f32, f32, f32, usize)] = &[
    (78.0, 14.0, 11.0, 0), (112.0, 46.0, 8.0, 1), (158.0, 22.0, 13.0, 2), (205.0, 52.0, 7.0, 0),
    (248.0, 10.0, 9.0, 1), (296.0, 40.0, 12.0, 0), (344.0, 18.0, 7.0, 2), (392.0, 50.0, 10.0, 1),
    (438.0, 12.0, 8.0, 0), (486.0, 36.0, 12.0, 2), (532.0, 54.0, 7.0, 1), (580.0, 20.0, 10.0, 0),
    (628.0, 44.0, 8.0, 2), (676.0, 14.0, 11.0, 1), (720.0, 40.0, 7.0, 0),
];
const DRAW: &[Beat] = &[Beat::new(0.0).opacity(0.0).scale(0.0), Beat::new(0.55).easing(Easing::DRAW_ON)];
const RISE: &[Beat] = &[Beat::new(0.0).opacity(0.0).scale(0.97), Beat::new(0.3).easing(Easing::ENTER)];
const POP: &[Beat] = &[
    Beat::new(0.0).opacity(0.0).scale(0.82),
    Beat::new(0.24).opacity(1.0).scale(1.05).easing(Easing::OVERSHOOT),
    Beat::new(0.4).scale(1.0),
];
const FADE: &[Beat] = &[Beat::new(0.0).opacity(0.0), Beat::new(0.35)];

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
        self.at(BLOOM, 0.04 + i as f64 * 0.08)
    }
    pub fn orb(self) -> Sequence {
        self.at(ORB, 0.05)
    }
    pub fn rule(self) -> Sequence {
        self.at(DRAW, 0.12)
    }
    pub fn row(self, i: usize) -> Sequence {
        self.at(RISE, 0.14 + i as f64 * 0.032)
    }
    /// The HUD's capsule pops in.
    pub fn hud(self) -> Sequence {
        self.at(POP, 0.0)
    }
    pub fn footer(self) -> Sequence {
        self.at(FADE, 0.22)
    }

    /// Stars that twinkle across the search row from the wand outward, as if the bar were
    /// drawn by it. Keyed by the opening.
    pub fn stars(self, width: f32, p: Palette) -> El {
        let mut layer = el().key(format!("stars-{}", self.n)).a11y_hidden().inert()
            .pin(Some(0.0), Some(0.0), Some(0.0), None).h(64.0).raise();
        for &(x, y, size, c) in STARS {
            let color = [p.magenta.mix(Color::WHITE, 0.25), p.violet.mix(Color::WHITE, 0.45), Color::WHITE][c];
            let delay = 0.08 + (x / width) as f64 * 0.42;
            layer = layer.child(
                el().absolute(x - size / 2.0, y - size / 2.0).size(size, size).center()
                    .sequence(self.at(TWINKLE, delay))
                    .child(rune::text("✦").font(rune::TextStyle::new(size, 600.0).lh(size)).color(color)),
            );
        }
        layer
    }

    /// Sparks and a shockwave thrown from the wand, centred on (`cx`, `cy`). Keyed by the
    /// opening, so each one is a fresh burst.
    pub fn burst(self, cx: f32, cy: f32, p: Palette) -> El {
        const SIZE: f32 = 150.0;
        const SPARK_COUNT: usize = 14;
        let mut sparks = el().size(SIZE, SIZE).sequence(self.at(SPARKS, 0.07));
        for i in 0..SPARK_COUNT {
            let angle = i as f32 / SPARK_COUNT as f32 * std::f32::consts::TAU + 0.2;
            let (radius, size) = if i % 2 == 0 { (50.0, 5.0) } else { (36.0, 3.0) };
            let color = [p.magenta, p.violet.mix(Color::WHITE, 0.4), Color::WHITE][i % 3];
            let (x, y) = (SIZE / 2.0 + radius * angle.cos() - size / 2.0, SIZE / 2.0 + radius * angle.sin() - size / 2.0);
            sparks = sparks.child(
                el().absolute(x, y).size(size, size).radius(size / 2.0).fill(color)
                    .box_shadow(Shadow { x: 0.0, y: 0.0, blur: 8.0, spread: 1.0, color: color.alpha(0.9), inset: false }),
            );
        }
        let ring = el().absolute(SIZE / 2.0 - 30.0, SIZE / 2.0 - 30.0).size(60.0, 60.0).radius(30.0)
            .border(1.5, Color::WHITE.alpha(0.7))
            .box_shadow(Shadow { x: 0.0, y: 0.0, blur: 18.0, spread: 0.0, color: p.magenta.alpha(0.7), inset: false })
            .sequence(self.at(RING, 0.04));
        el().key(format!("burst-{}", self.n))
            .a11y_hidden()
            .inert()
            .pin(Some(cx - SIZE / 2.0), Some(cy - SIZE / 2.0), None, None)
            .size(SIZE, SIZE)
            .raise()
            .child(ring)
            .child(sparks)
    }
}
