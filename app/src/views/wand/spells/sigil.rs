//! Sigil's spells. A typed letter is sent as an orb of energy from the wand's tip: it floats
//! over on an arch of its own (higher or flatter, peaking earlier or later, at its own pace,
//! wandering a little), trailing a short comet's tail with two strands spiralling round it,
//! and blooms open where the letter goes into the letter's rune, which dissolves into the
//! letter as it cools from gold. A deleted letter turns back into its rune and the wand draws
//! it in, nearest first, the tip flaring as each arrives. Window points; every frame is a
//! function of the clock and each orb's seed.

use rune::{Color, El, Shape, TextStyle, el, text};

use super::super::super::palette::Palette;
use super::super::super::runes;
use super::super::motion::{ease_out, lerp, smoothstep};
use super::super::sparkle::hash;

/// After it blooms, the rune holds this long, then dissolves into the letter over `DISSOLVE`;
/// the letter then cools from gold over `COOL`.
pub const LAND: f32 = 0.02;
pub const HOLD: f32 = 0.12;
pub const DISSOLVE: f32 = 0.16;
pub const COOL: f32 = 0.6;
/// The orb's bloom as it lands.
const BLOOM: f32 = 0.14;
/// Deleting: the letter turns back into its rune over `TURN_BACK`, which reaches the wand at
/// `ABSORB`; clearing takes the letters `ERASE_STAGGER` apart, nearest the wand first.
pub const TURN_BACK: f32 = 0.1;
pub const ABSORB: f32 = 0.42;
pub const ERASE_STAGGER: f32 = 0.035;
/// A rune's height: about a capital's.
const RUNE_H: f32 = 15.0;
/// A short comet's tail, not a line back to the wand: the last 0.07 s of the path.
const TAIL: usize = 18;
const TAIL_STEP: f32 = 0.004;

/// When the last of `chars` deleted letters reaches the wand.
pub fn absorb_end(chars: usize) -> f32 {
    chars.saturating_sub(1) as f32 * ERASE_STAGGER + ABSORB
}

/// One orb's path from `from` to `to`, by `seed`.
pub struct Orb {
    from: [f32; 2],
    to: [f32; 2],
    seed: u32,
    pub flight: f32,
}

impl Orb {
    pub fn new(from: [f32; 2], to: [f32; 2], seed: u32) -> Orb {
        let len = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt();
        let flight = (0.16 + len / 2400.0) * (0.85 + 0.35 * hash(seed, 1));
        Orb {
            from,
            to,
            seed,
            flight,
        }
    }

    /// Where it is `age` seconds after it left. The same arch up and over each time, but with
    /// its own height, peak, pace and wander, the wander dying out at both ends.
    pub fn at(&self, age: f32) -> [f32; 2] {
        let r = |k: u32| hash(self.seed, k + 10);
        let ([fx, fy], [tx, ty]) = (self.from, self.to);
        let (dx, dy) = (tx - fx, ty - fy);
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let lift = -(10.0 + dx.abs() * 0.05) * (0.6 + 0.8 * r(2));
        let mid = [fx + dx * (0.4 + 0.2 * r(3)), fy.min(ty) + lift];
        let normal = [-dy / len, dx / len];
        let sign = if r(5) > 0.5 { 1.0 } else { -1.0 };
        let amp = (1.2 + len * 0.035).min(5.0) * (0.5 + 0.5 * r(4)) * sign;
        let (f1, f2) = (0.6 + 0.6 * r(6), 1.6 + 0.9 * r(7));
        let (ph1, ph2) = (r(8) * std::f32::consts::TAU, r(9) * std::f32::consts::TAU);
        let settle = 2.0 + 1.2 * r(10);
        let a = (age / self.flight).clamp(0.0, 1.0);
        let u = 1.0 - (1.0 - a).powf(settle);
        let m = 1.0 - u;
        let bx = m * m * fx + 2.0 * m * u * mid[0] + u * u * tx;
        let by = m * m * fy + 2.0 * m * u * mid[1] + u * u * ty;
        let env = (std::f32::consts::PI * u).sin().max(0.0).powf(0.8);
        let tau = std::f32::consts::TAU;
        let n = env * amp * ((tau * f1 * u + ph1).sin() + 0.35 * (tau * f2 * u + ph2).sin());
        [bx + normal[0] * n, by + normal[1] * n]
    }

    /// Adds it, `age` seconds after it left, to `shape`: its tail, the strands round it, and
    /// its head, blooming as it lands. Nothing once the bloom is over.
    pub fn draw(&self, mut shape: Shape, age: f32, p: &Palette) -> Shape {
        let landed = age - self.flight;
        if age < 0.0 || landed >= BLOOM {
            return shape;
        }
        let fade = if landed > 0.0 {
            1.0 - landed / BLOOM
        } else {
            1.0
        };
        let pts: Vec<[f32; 2]> = (0..=TAIL)
            .map(|k| self.at((age - k as f32 * TAIL_STEP).clamp(0.0, self.flight)))
            .collect();
        let taper = |k: usize| 1.0 - k as f32 / TAIL as f32;
        let length: f32 = pts
            .windows(2)
            .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
            .sum();
        if length > 1.0 {
            // The tail: a wide faint ribbon, then a bright core, both tapering away.
            let gold = p.glow;
            for (width, color) in [
                (7.0, gold.alpha(0.10)),
                (3.2, gold.mix(Color::WHITE, 0.3).alpha(0.45)),
                (1.3, gold.mix(Color::WHITE, 0.8).alpha(0.8)),
            ] {
                if let Some(ribbon) = ribbon(&pts, |k| width * (0.35 + 0.65 * taper(k))) {
                    let a = color.a * fade;
                    shape = shape.fill(ribbon, color.alpha(a * 0.85));
                }
            }
            // Two strands spiralling round it, like current along a wire.
            let spin = hash(self.seed, 30) * 6.0;
            for phase in [0.0, std::f32::consts::PI] {
                let strand: Vec<[f32; 2]> = (0..=TAIL)
                    .map(|k| {
                        let [x, y] = pts[k];
                        let [nx, ny] = pts[(k + 1).min(TAIL)];
                        let (tx, ty) = (x - nx, y - ny);
                        let l = (tx * tx + ty * ty).sqrt().max(1e-3);
                        let off =
                            (age * 26.0 + k as f32 * 0.55 + phase + spin).sin() * 1.7 * taper(k);
                        [x - ty / l * off, y + tx / l * off]
                    })
                    .collect();
                shape = shape.stroke(strand, 0.6, gold.mix(Color::WHITE, 0.5).alpha(0.38 * fade));
            }
        }
        // The head, and its bloom.
        let bloom = if landed > 0.0 { landed / BLOOM } else { 0.0 };
        let swell = (age / 0.05).min(1.0);
        let r = (3.4 + 6.0 * bloom) * (0.6 + 0.4 * swell);
        let at = pts[0];
        let gold = p.glow;
        shape
            .fill(circle(at, r * 3.0), gold.alpha(0.10 * (1.0 - bloom)))
            .fill(
                circle(at, r * 1.8),
                gold.mix(Color::WHITE, 0.2).alpha(0.22 * (1.0 - bloom)),
            )
            .fill(
                circle(at, r),
                gold.mix(Color::WHITE, 0.55).alpha(0.9 * (1.0 - bloom)),
            )
            .fill(
                circle(at, r * 0.5),
                Color::WHITE.alpha(0.95 * (1.0 - bloom)),
            )
    }
}

/// A tapering band through `pts`, `width(k)` wide at point `k`, as one polygon.
fn ribbon(pts: &[[f32; 2]], width: impl Fn(usize) -> f32) -> Option<Vec<[f32; 2]>> {
    let n = pts.len();
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for k in 0..n {
        let a = pts[k.saturating_sub(1)];
        let b = pts[(k + 1).min(n - 1)];
        let (tx, ty) = (b[0] - a[0], b[1] - a[1]);
        let l = (tx * tx + ty * ty).sqrt();
        if l < 1e-3 {
            continue;
        }
        let (nx, ny) = (-ty / l, tx / l);
        let w = width(k) / 2.0;
        left.push([pts[k][0] + nx * w, pts[k][1] + ny * w]);
        right.push([pts[k][0] - nx * w, pts[k][1] - ny * w]);
    }
    if left.len() < 2 {
        return None;
    }
    right.reverse();
    left.extend(right);
    Some(left)
}

fn circle(at: [f32; 2], r: f32) -> Vec<[f32; 2]> {
    (0..20)
        .map(|i| {
            let a = i as f32 / 20.0 * std::f32::consts::TAU;
            [at[0] + r * a.cos(), at[1] + r * a.sin()]
        })
        .collect()
}

/// The rune for `c`, centred on `at`, `scale` times its size, blurred by `blur`.
pub fn rune(c: char, at: [f32; 2], scale: f32, color: Color, blur: f32) -> El {
    let h = RUNE_H * scale.max(0.05);
    let (w, hh) = runes::view_size(h);
    runes::view(runes::of(c), h, color)
        .absolute(at[0] - w / 2.0, at[1] - hh / 2.0)
        .blur(blur)
}

/// The letter `c` drawn over the field at `x` (its left edge), in `color`, blurred by `blur`.
pub fn letter(c: char, x: f32, top: f32, style: TextStyle, color: Color, blur: f32) -> El {
    el().absolute(x, top)
        .blur(blur)
        .child(text(c.to_string()).font(style).color(color))
}

/// Where a letter is drawn: the character, its middle, its box's left and top, and its type.
pub struct Spot {
    pub c: char,
    pub center: [f32; 2],
    pub left: f32,
    pub top: f32,
    pub style: TextStyle,
}

/// A just-written letter's rune and the letter dissolving out of it, `v` seconds after its orb
/// landed: the rune blooms open, holds, then blurs away as the letter sharpens in.
pub fn arriving(at: &Spot, v: f32, p: &Palette) -> Vec<El> {
    let mut out = Vec::new();
    if !(0.0..HOLD + DISSOLVE).contains(&v) {
        return out;
    }
    let gold = p.glow.mix(Color::WHITE, 0.15);
    let enter = smoothstep(0.0, 0.06, v);
    let d = ((v - HOLD) / DISSOLVE).clamp(0.0, 1.0);
    let scale = lerp(0.6, 1.0, ease_out(v / 0.06)) + 0.15 * d;
    let blur = 3.0 * (1.0 - enter) + 5.0 * d;
    out.push(rune(
        at.c,
        at.center,
        scale,
        gold.alpha(enter * (1.0 - d)),
        blur,
    ));
    if d > 0.0 {
        out.push(letter(
            at.c,
            at.left,
            at.top,
            at.style,
            gold.alpha(d),
            6.0 * (1.0 - d),
        ));
    }
    out
}

/// A deleted letter, `a` seconds into its leaving: still itself before 0; turning back into
/// its rune; then the rune drawn into the wand at `tip`, accelerating and shrinking.
pub fn leaving(at: &Spot, tip: [f32; 2], a: f32, p: &Palette) -> Vec<El> {
    let mut out = Vec::new();
    let gold = p.glow.mix(Color::WHITE, 0.15);
    let (c, center) = (at.c, at.center);
    if a < 0.0 {
        out.push(letter(c, at.left, at.top, at.style, p.text, 0.0));
    } else if a < TURN_BACK {
        let d = a / TURN_BACK;
        out.push(letter(
            c,
            at.left,
            at.top,
            at.style,
            p.text.mix(gold, d).alpha(1.0 - d),
            5.0 * d,
        ));
        out.push(rune(c, center, 1.0, gold.alpha(d), 4.0 * (1.0 - d)));
    } else if a < ABSORB {
        let u = (a - TURN_BACK) / (ABSORB - TURN_BACK);
        let k = u.powf(1.6);
        let pos = [lerp(center[0], tip[0], k), lerp(center[1], tip[1], k)];
        let color = gold
            .mix(p.glow2.mix(Color::WHITE, 0.3), k)
            .alpha(1.0 - 0.85 * k);
        out.push(rune(c, pos, 1.0 - 0.9 * k, color, k));
    }
    out
}
