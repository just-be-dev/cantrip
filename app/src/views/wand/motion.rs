//! What the wand does, as a state machine. A **mood** is held while something about the bar is
//! true (the pointer is on it, the user is typing, nothing matches); the wand blends into a new
//! mood's stance over `BLEND` seconds. A **gesture** is a one-shot move cued by an event (a
//! keystroke, a selection move, a cast); gestures layer over the mood and over each other, and
//! each is a pure function of its age. A [`Stance`] is how far a frame is from rest.

use std::f32::consts::TAU;

/// Seconds to blend from one mood's stance into the next.
pub const BLEND: f32 = 0.45;

/// Offsets from the resting pose. Every field is 0 at rest and adds up across the base
/// animation, the mood and the gestures playing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stance {
    /// About the world's vertical, radians: the twirl.
    pub turn: f32,
    /// In the screen's plane, radians; positive raises the tip.
    pub swing: f32,
    /// Toward the viewer, radians; positive brings the tip out.
    pub pitch: f32,
    /// Added to the model's scale of 1.
    pub grow: f32,
    /// Up, in world units.
    pub lift: f32,
    /// Added to the tip star's size of 1.
    pub star: f32,
    /// Added to the orbiting stars' speed, radius and brightness (each 1 at rest).
    pub orbit: f32,
    pub reach: f32,
    pub glow: f32,
}

impl Stance {
    pub fn add(self, o: Stance) -> Stance {
        Stance {
            turn: self.turn + o.turn,
            swing: self.swing + o.swing,
            pitch: self.pitch + o.pitch,
            grow: self.grow + o.grow,
            lift: self.lift + o.lift,
            star: self.star + o.star,
            orbit: self.orbit + o.orbit,
            reach: self.reach + o.reach,
            glow: self.glow + o.glow,
        }
    }

    pub fn lerp(self, o: Stance, t: f32) -> Stance {
        let m = |a: f32, b: f32| lerp(a, b, t);
        Stance {
            turn: m(self.turn, o.turn),
            swing: m(self.swing, o.swing),
            pitch: m(self.pitch, o.pitch),
            grow: m(self.grow, o.grow),
            lift: m(self.lift, o.lift),
            star: m(self.star, o.star),
            orbit: m(self.orbit, o.orbit),
            reach: m(self.reach, o.reach),
            glow: m(self.glow, o.glow),
        }
    }

    /// Keeps a pile-up of gestures (key repeat, fast typing) from flinging the wand around.
    pub fn tame(mut self) -> Stance {
        self.swing = self.swing.clamp(-0.7, 0.7);
        self.pitch = self.pitch.clamp(-0.6, 0.8);
        self.lift = self.lift.clamp(-0.15, 0.15);
        self.star = self.star.min(2.2);
        self
    }
}

/// What the wand holds while it lasts, most insistent first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    /// Rocking gently, stars in their orbits.
    Idle,
    /// The pointer is on it: it perks up, the tip brightens and the stars quicken.
    Curious,
    /// Typing: it aims at the field and the stars hurry round.
    Attentive,
    /// The query matches nothing: it droops, its stars slow and dim, and it fizzles.
    Fizzled,
}

impl Mood {
    pub fn stance(self, t: f32) -> Stance {
        match self {
            Mood::Idle => Stance::default(),
            Mood::Curious => Stance {
                swing: 0.08,
                pitch: 0.14,
                star: 0.3,
                orbit: 0.9,
                glow: 0.2,
                ..Stance::default()
            },
            Mood::Attentive => Stance {
                swing: -0.3,
                pitch: 0.2,
                star: 0.3,
                orbit: 1.4,
                reach: -0.1,
                glow: 0.15,
                ..Stance::default()
            },
            Mood::Fizzled => Stance {
                // A slow, tired sag.
                swing: -1.0 + 0.05 * (t * 1.7).sin(),
                lift: -0.12,
                grow: -0.05,
                star: -0.55,
                orbit: -0.7,
                reach: -0.3,
                glow: -0.55,
                ..Stance::default()
            },
        }
    }
}

/// One-shot moves, cued by what happens in the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// The selection moved: the tip flicks up or down with it.
    Flick { up: bool },
    /// The selection hit the end of the list: a little shake of the head.
    Bump { up: bool },
    /// A character typed: a jab toward the field (the spark flies from the view).
    Tap,
    /// Characters deleted: a recoil, and the tip flares as their dust arrives `delay` later.
    Erase { delay: f32 },
    /// The whole query cleared: a sweep, and the tip flares as the dust arrives.
    Sweep { delay: f32 },
    /// Something run: wind up, thrust, and a burst of stars.
    Cast,
    /// Back at the root search from a spell: a quick twirl back into place.
    Return,
    /// Clicked: a double twirl, a burst, and the stars race round.
    Poke,
    /// The grimoire reloaded: the stars spiral into the tip and burst back out.
    Recharge,
    /// The bar closing: it spins down to a point and pops.
    Vanish,
}

/// Sparkles a gesture throws from the tip, `at` seconds in.
#[derive(Clone, Copy, Debug)]
pub struct Puff {
    pub at: f32,
    pub count: u32,
    /// The direction they favour, in the world; `spread` 0 is a beam and 1 all around.
    pub aim: [f32; 3],
    pub spread: f32,
    pub speed: f32,
    pub size: f32,
    pub life: f32,
}

impl Gesture {
    /// How long it lasts; after this it contributes nothing.
    pub fn length(self) -> f32 {
        match self {
            Gesture::Flick { .. } => 0.7,
            Gesture::Bump { .. } => 0.6,
            Gesture::Tap => 0.45,
            Gesture::Erase { delay } => delay + 0.6,
            Gesture::Sweep { delay } => delay.max(0.6) + 0.6,
            Gesture::Cast => 0.9,
            Gesture::Return => 0.8,
            Gesture::Poke => 1.3,
            Gesture::Recharge => 1.3,
            Gesture::Vanish => 0.55,
        }
    }

    /// Its offset from the stance under it, `a` seconds in.
    pub fn stance(self, a: f32) -> Stance {
        let zero = Stance::default();
        match self {
            Gesture::Flick { up } => {
                let k = kick(a, 20.0, 8.0) * sign(up);
                Stance {
                    swing: 0.38 * k,
                    lift: 0.07 * k,
                    star: 0.25 * (-a * 7.0).exp(),
                    ..zero
                }
            }
            Gesture::Bump { up } => Stance {
                turn: 0.3 * kick(a, 34.0, 7.0),
                swing: 0.08 * kick(a, 30.0, 9.0) * sign(up),
                ..zero
            },
            Gesture::Tap => {
                let k = kick(a, 24.0, 11.0);
                Stance {
                    swing: -0.16 * k,
                    pitch: 0.14 * k,
                    star: 0.45 * (-a * 9.0).exp(),
                    ..zero
                }
            }
            Gesture::Erase { delay } => Stance {
                swing: 0.12 * kick(a, 22.0, 10.0),
                star: 0.6 * flare(a - delay),
                ..zero
            },
            Gesture::Sweep { delay } => {
                let k = kick(a, 9.0, 4.0);
                Stance {
                    swing: 0.45 * k,
                    turn: -0.7 * k,
                    star: 0.9 * flare(a - delay),
                    orbit: 1.5 * flare(a - delay),
                    ..zero
                }
            }
            Gesture::Cast => {
                // Wind up (tip back and up), thrust (tip out and forward), then settle.
                let (pitch, swing, grow) = if a < 0.13 {
                    let k = ease_in_out(a / 0.13);
                    (-0.35 * k, 0.3 * k, -0.05 * k)
                } else if a < 0.24 {
                    let k = ease_out((a - 0.13) / 0.11);
                    (
                        lerp(-0.35, 0.6, k),
                        lerp(0.3, -0.35, k),
                        lerp(-0.05, 0.18, k),
                    )
                } else {
                    let d = a - 0.24;
                    let settle = (-d * 6.0).exp() * (d * 14.0).cos();
                    (0.6 * settle, -0.35 * settle, 0.18 * settle)
                };
                Stance {
                    pitch,
                    swing,
                    grow,
                    star: 1.6 * flare(a - 0.16),
                    orbit: 3.0 * flare(a - 0.1),
                    ..zero
                }
            }
            Gesture::Return => {
                let k = ease_out(a / 0.55);
                Stance {
                    turn: -TAU * (1.0 - k),
                    grow: -0.45 * (1.0 - k),
                    reach: -(1.0 - k),
                    star: 0.8 * flare(a - 0.2),
                    ..zero
                }
            }
            Gesture::Poke => Stance {
                turn: 2.0 * TAU * ease_out(a / 0.9),
                grow: 0.12 * kick(a, 10.0, 5.0),
                star: flare(a - 0.1),
                orbit: 5.0 * (-a * 2.5).exp(),
                glow: 0.4 * (-a * 2.5).exp(),
                ..zero
            },
            Gesture::Recharge => {
                let gather = smoothstep(0.0, 0.45, a) * (1.0 - smoothstep(0.5, 1.0, a));
                Stance {
                    turn: TAU * ease_in_out(a / 1.0),
                    reach: -0.95 * gather,
                    orbit: 4.0 * gather,
                    star: 1.4 * flare(a - 0.5),
                    glow: 0.5 * gather,
                    ..zero
                }
            }
            Gesture::Vanish => {
                let k = ease_in(a / 0.5);
                Stance {
                    turn: 1.5 * TAU * k,
                    grow: -0.95 * k,
                    reach: -k,
                    star: 1.2 * flare(a - 0.3),
                    ..zero
                }
            }
        }
    }

    /// The sparkles it throws.
    pub fn puffs(self) -> &'static [Puff] {
        const fn puff(
            at: f32,
            count: u32,
            aim: [f32; 3],
            spread: f32,
            speed: f32,
            size: f32,
            life: f32,
        ) -> Puff {
            Puff {
                at,
                count,
                aim,
                spread,
                speed,
                size,
                life,
            }
        }
        const UP: &[Puff] = &[puff(0.03, 4, [0.4, 1.0, 0.2], 0.5, 1.3, 0.065, 0.5)];
        const DOWN: &[Puff] = &[puff(0.03, 4, [0.4, -1.0, 0.2], 0.5, 1.3, 0.065, 0.5)];
        const SWEEP: &[Puff] = &[puff(0.12, 7, [-0.6, 0.8, 0.3], 0.7, 1.4, 0.07, 0.6)];
        const CAST: &[Puff] = &[puff(0.16, 26, [1.0, 0.25, 0.6], 0.85, 2.6, 0.11, 0.8)];
        const RETURN: &[Puff] = &[puff(0.15, 10, [0.3, 1.0, 0.3], 1.0, 1.5, 0.08, 0.7)];
        const POKE: &[Puff] = &[puff(0.12, 16, [0.0, 1.0, 0.3], 1.2, 2.0, 0.09, 0.8)];
        const RECHARGE: &[Puff] = &[puff(0.5, 20, [0.0, 1.0, 0.3], 1.3, 1.8, 0.09, 0.9)];
        const VANISH: &[Puff] = &[puff(0.32, 12, [0.0, 1.0, 0.4], 1.3, 1.6, 0.08, 0.6)];
        match self {
            Gesture::Flick { up: true } => UP,
            Gesture::Flick { up: false } => DOWN,
            Gesture::Sweep { .. } => SWEEP,
            Gesture::Cast => CAST,
            Gesture::Return => RETURN,
            Gesture::Poke => POKE,
            Gesture::Recharge => RECHARGE,
            Gesture::Vanish => VANISH,
            _ => &[],
        }
    }
}

fn sign(up: bool) -> f32 {
    if up { 1.0 } else { -1.0 }
}

/// A damped spring's answer to a tap, peaking at 1: out, back past rest, and settling.
pub fn kick(a: f32, freq: f32, damp: f32) -> f32 {
    if a <= 0.0 {
        return 0.0;
    }
    let peak_at = (freq / damp).atan() / freq;
    let peak = (-damp * peak_at).exp() * (freq * peak_at).sin();
    (-damp * a).exp() * (freq * a).sin() / peak
}

/// A flash that snaps on at 0 and fades: 0 before, about 1 at its brightest.
pub fn flare(x: f32) -> f32 {
    if x <= 0.0 {
        0.0
    } else {
        1.15 * (-x * 6.0).exp() * (1.0 - (-x * 40.0).exp())
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}
pub fn ease_in(t: f32) -> f32 {
    t.clamp(0.0, 1.0).powi(3)
}
pub fn ease_in_out(t: f32) -> f32 {
    smoothstep(0.0, 1.0, t)
}
