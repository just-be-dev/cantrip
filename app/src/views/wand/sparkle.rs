//! The wand's sparkles: motes thrown from its tip that fly and fade on their own, and the
//! stars that orbit it. A mote remembers where and when it was born, so its path depends only
//! on that and the clock, wherever the wand has gone since.

use std::f32::consts::TAU;

use super::motion::{Puff, lerp};
use super::raster::{V3, add, mul, norm};

/// A star to draw: a four-point sparkle at a point in the world.
pub struct Star {
    pub at: V3,
    /// To its points, in world units.
    pub radius: f32,
    pub angle: f32,
    pub color: V3,
    pub alpha: f32,
}

/// A sparkle in flight.
#[derive(Clone, Debug)]
pub struct Mote {
    pub born: f64,
    pub from: V3,
    pub dir: V3,
    pub speed: f32,
    /// How fast it loses its speed.
    pub damping: f32,
    /// Upward acceleration; negative falls.
    pub rise: f32,
    pub life: f32,
    pub size: f32,
    pub angle: f32,
    pub spin: f32,
    pub color: V3,
    pub alpha: f32,
}

impl Mote {
    pub fn alive(&self, t: f64) -> bool {
        ((t - self.born) as f32) < self.life
    }

    pub fn star(&self, t: f64) -> Option<Star> {
        let age = (t - self.born) as f32;
        let u = age / self.life;
        if !(0.0..1.0).contains(&u) {
            return None;
        }
        let travel = self.speed * (1.0 - (-self.damping * age).exp()) / self.damping;
        Some(Star {
            at: add(
                add(self.from, mul(self.dir, travel)),
                [0.0, 0.5 * self.rise * age * age, 0.0],
            ),
            radius: self.size * life_size(u),
            angle: self.angle + self.spin * age,
            color: self.color,
            alpha: self.alpha,
        })
    }
}

/// Makes motes from a running seed, so the same cues give the same sparkles.
#[derive(Default)]
pub struct Spawner {
    seed: u32,
}

impl Spawner {
    /// A repeatable random number in [0, 1) for the next mote's property `salt`.
    fn roll(&self, salt: u32) -> f32 {
        hash(self.seed, salt)
    }

    /// The motes of `puff`, thrown from `from` at `t`, in `colors` at random.
    pub fn puff(&mut self, t: f64, from: V3, puff: &Puff, colors: &[V3]) -> Vec<Mote> {
        let aim = norm(puff.aim);
        (0..puff.count)
            .map(|_| {
                self.seed = self.seed.wrapping_add(1);
                let r = |salt| self.roll(salt);
                let jitter = [r(1) - 0.5, r(2) - 0.5, r(3) - 0.5];
                Mote {
                    born: t,
                    from,
                    dir: norm(add(aim, mul(jitter, 2.4 * puff.spread))),
                    speed: puff.speed * (0.55 + 0.7 * r(4)),
                    damping: 3.2,
                    rise: -0.4,
                    life: puff.life * (0.7 + 0.6 * r(5)),
                    size: puff.size * (0.7 + 0.6 * r(6)),
                    angle: r(7) * TAU,
                    spin: (r(8) - 0.5) * 6.0,
                    color: colors[(r(9) * colors.len() as f32) as usize % colors.len()],
                    alpha: 1.0,
                }
            })
            .collect()
    }

    /// One idle twinkle drifting off the tip: gently up when `lively`, falling and dim when
    /// it's a fizzle.
    pub fn twinkle(&mut self, t: f64, from: V3, lively: bool, colors: &[V3]) -> Mote {
        self.seed = self.seed.wrapping_add(1);
        let r = |salt| self.roll(salt);
        let jitter = mul([r(1) - 0.5, r(2) - 0.5, r(3) - 0.5], 0.14);
        let color = colors[(r(9) * colors.len() as f32) as usize % colors.len()];
        if lively {
            Mote {
                born: t,
                from: add(from, jitter),
                dir: norm([r(4) - 0.3, r(5) + 0.3, r(6) - 0.5]),
                speed: 0.18 + 0.25 * r(7),
                damping: 0.01,
                rise: 0.12,
                life: 0.9 + 0.5 * r(0),
                size: 0.05 + 0.05 * r(10),
                angle: r(11) * TAU,
                spin: 2.0,
                color,
                alpha: 0.9,
            }
        } else {
            Mote {
                born: t,
                from: add(from, jitter),
                dir: norm([r(4) - 0.5, -0.2, r(6) - 0.5]),
                speed: 0.08,
                damping: 0.01,
                rise: -0.7,
                life: 0.8 + 0.4 * r(0),
                size: 0.035 + 0.03 * r(10),
                angle: r(11) * TAU,
                spin: 0.5,
                color: mul(color, 0.55),
                alpha: 0.6,
            }
        }
    }
}

/// Stars circling the wand's axis, as (height on the wand, distance from it, size): a loose
/// spiral from the grip up to the tip, a third of a turn apart.
pub const ORBITERS: [(f32, f32, f32); 3] =
    [(-0.45, 0.62, 0.17), (0.12, 0.74, 0.14), (0.62, 0.56, 0.13)];
/// Seconds for one turn at rest.
pub const ORBIT: f32 = 3.6;

/// Where orbiter `i` sits about the wand's own axis at orbit phase `phase` (turns), scaled by
/// `reach`, before the wand's pose places it.
pub fn orbiter(i: usize, phase: f32, reach: f32) -> V3 {
    let (height, distance, _) = ORBITERS[i];
    let turn = TAU * (phase + i as f32 / ORBITERS.len() as f32);
    [
        distance * reach * turn.cos(),
        height,
        distance * reach * turn.sin(),
    ]
}

/// A sparkle's size over its life (0…1): it swells, then winks out.
pub fn life_size(u: f32) -> f32 {
    const KEYS: [(f32, f32); 4] = [(0.0, 0.2), (0.25, 1.0), (0.6, 0.7), (1.0, 0.0)];
    let u = u.clamp(0.0, 1.0);
    KEYS.windows(2).find(|w| u <= w[1].0).map_or(0.0, |w| {
        lerp(w[0].1, w[1].1, (u - w[0].0) / (w[1].0 - w[0].0))
    })
}

/// A repeatable random number in [0, 1) for particle `i`'s property `salt`.
pub fn hash(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B).rotate_left(13);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}
