//! The wand in the search bar's orb: a small 3D model, rendered on the CPU into `Pixels` on
//! the frame clock. Each opening it twirls in and throws star sparkles from its tip as the
//! orb would pop (`intro.rs`); then it idles, rocking slowly with its tip star breathing and a
//! few sparkles drifting off it, while three stars orbit it. Every frame is a pure function of time, so headless renders are
//! repeatable. Reduce Motion gets one still frame. Nothing draws while the wand is off screen.

mod raster;

use std::cell::Cell;
use std::f32::consts::{FRAC_PI_4, TAU};
use std::sync::LazyLock;
use std::time::Duration;

use rune::{Color, El, Pixels, Tx, el};

use super::palette::{self, Palette, srgb};
use raster::{Canvas, M3, V3, Vert, add, dot, mix, mul, norm, sub};

/// Its square, in points: larger than the orb, so sparkles can fly past the orb's edge.
pub const CANVAS: f32 = 60.0;
/// Each pixel is the mean of this many samples a side.
const SUPERSAMPLE: usize = 2;
/// The twirl starts with the orb's pop (`intro::ORB`).
const DELAY: f64 = 0.05;
/// Long enough for the burst to burn out. After it, the wand idles at `IDLE_FPS`.
const FLOURISH: f64 = 1.4;
const IDLE_FPS: f64 = 30.0;

// The camera looks down -z from `DISTANCE`; the wand is two units long, centred on the origin.
const DISTANCE: f32 = 5.0;
const PT_PER_UNIT: f32 = 13.5;
/// Leans the tip toward the viewer.
const TILT: f32 = 0.3;
/// Down and left, so the tip's sparkles have room.
const OFFSET: V3 = [-0.05, -0.08, 0.0];
const TIP: V3 = [0.0, 1.06, 0.0];
const CAMERA: V3 = [0.0, 0.0, DISTANCE];
/// The idle rock about the vertical, in radians, and its period in seconds.
const ROCK: f32 = 0.35;
const ROCK_PERIOD: f32 = 5.0;

/// The wand's image and when it was last drawn. Lives in the store; `Cantrip::animate_wand`
/// calls `frame` at the start of every frame.
pub struct Wand {
    pixels: Pixels,
    /// The device scale the view last laid it out at.
    scale: Cell<f32>,
    /// The opening the current flourish belongs to, and the frame time it began.
    intro: u64,
    began: Option<f64>,
    still: bool,
    /// Frame time and scale of the image showing.
    drawn: Option<(f64, f32)>,
}

impl Wand {
    pub fn new() -> Wand {
        Wand {
            pixels: Pixels::new(),
            scale: Cell::new(2.0),
            intro: 0,
            began: None,
            still: false,
            drawn: None,
        }
    }

    /// The wand, centred on the orb it sits in (`orb` points across).
    pub fn view(&self, orb: f32, scale: f32) -> El {
        self.scale.set(scale);
        let inset = (orb - CANVAS) / 2.0;
        el().absolute(inset, inset)
            .size(CANVAS, CANVAS)
            .pixels(&self.pixels)
            .inert()
            .a11y_hidden()
    }

    /// Draws the frame at `t` (frame-clock seconds) if one is due. `shown`: the wand is on
    /// screen; `intro` counts openings, and a new one starts a flourish; `still` asks whether
    /// to hold still (Reduce Motion), once per opening.
    pub fn frame(
        &mut self,
        t: f64,
        shown: bool,
        intro: u64,
        still: impl FnOnce() -> bool,
        tx: &mut Tx,
    ) {
        if intro != self.intro {
            self.intro = intro;
            self.began = (intro > 0).then_some(t + DELAY);
            self.still = still();
            self.drawn = None;
        }
        if !shown {
            // Draw afresh when it's back.
            self.drawn = None;
            return;
        }
        let scale = self.scale.get();
        let due = match self.drawn {
            None => true,
            Some((_, drawn_at)) if drawn_at != scale => true,
            Some(_) if self.still => false,
            Some((last, _)) => {
                let flourishing = self.began.is_some_and(|b| t - b < FLOURISH);
                let gap = if flourishing { 0.0 } else { 1.0 / IDLE_FPS };
                // Setting pixels wakes the next frame; idle, wait out the gap instead.
                let wait = gap - (t - last);
                if wait > 0.002 {
                    tx.wake_after(Duration::from_secs_f64(wait));
                }
                wait <= 0.002
            }
        };
        if due {
            let clock = match self.began {
                _ if self.still => Clock::Still,
                Some(began) => Clock::Opened((t - began) as f32),
                None => Clock::Rest(t as f32),
            };
            let (side, rgba) = render(clock, scale, &palette::arcane());
            self.pixels.set(side, side, rgba);
            self.drawn = Some((t, scale));
        }
    }
}

/// Where the animation is.
#[derive(Clone, Copy, Debug)]
pub enum Clock {
    /// Never opened (launch, headless scenes): idling at this frame time.
    Rest(f32),
    /// Seconds since this opening's flourish began (negative while it waits).
    Opened(f32),
    /// Reduce Motion: the resting pose, no sparkles.
    Still,
}

// MARK: Rendering

/// The image at `clock`: its side in device pixels, and straight-alpha sRGB bytes.
pub fn render(clock: Clock, scale: f32, p: &Palette) -> (u32, Vec<u8>) {
    let side = ((CANVAS * scale).round() as usize).max(1);
    let pose = Pose::at(clock);

    // The model, supersampled.
    let ss = side * SUPERSAMPLE;
    let focal = PT_PER_UNIT * DISTANCE * scale * SUPERSAMPLE as f32;
    let project = |w: V3, f: f32, side: usize| {
        let z = DISTANCE - w[2];
        (
            side as f32 / 2.0 + f * w[0] / z,
            side as f32 / 2.0 - f * w[1] / z,
            z,
        )
    };
    let mesh = &*MESH;
    let verts: Vec<Vert<10>> = mesh
        .verts
        .iter()
        .map(|v| {
            let w = pose.place(v.pos);
            let n = pose.rot.apply(v.normal);
            let (x, y, z) = project(w, focal, ss);
            Vert {
                x,
                y,
                z,
                attr: [
                    n[0],
                    n[1],
                    n[2],
                    w[0],
                    w[1],
                    w[2],
                    v.pos[1],
                    v.cos,
                    v.sin,
                    v.material as u8 as f32,
                ],
            }
        })
        .collect();
    let mut canvas = Canvas::new(ss, ss);
    let looks = Looks::new(p);
    let shade = |a: &[f32; 10]| looks.shade(a);
    for t in &mesh.tris {
        canvas.triangle(
            [
                &verts[t[0] as usize],
                &verts[t[1] as usize],
                &verts[t[2] as usize],
            ],
            &shade,
        );
    }
    let mut out = canvas.downsample(SUPERSAMPLE);

    // Stars over it, far ones first.
    let focal = focal / SUPERSAMPLE as f32;
    let mut stars = sparkles(clock, p);
    if !matches!(clock, Clock::Opened(s) if s < 0.0) {
        // The tip's own star sits just in front of the tip.
        let tip = pose.place(TIP);
        stars.push(Star {
            at: add(tip, [0.0, 0.0, 0.25]),
            radius: 0.2 * pose.star,
            angle: pose.star_angle,
            color: rgb(p.violet.mix(Color::WHITE, 0.6)),
            alpha: 1.0,
        });
    }
    stars.sort_by(|a, b| a.at[2].total_cmp(&b.at[2]));
    for s in &stars {
        let (x, y, z) = project(s.at, focal, side);
        out.star([x, y, z], s.radius * focal / z, s.angle, s.color, s.alpha);
    }
    (side as u32, out.encode())
}

/// The model's placement on one frame.
struct Pose {
    rot: M3,
    scale: f32,
    /// The tip star's size (1 at rest) and spin.
    star: f32,
    star_angle: f32,
}

impl Pose {
    fn new(turn: f32, swing: f32, scale: f32, star: f32, star_angle: f32) -> Pose {
        // Diagonal like the old symbol, tip to the upper right and leaning out, then turned
        // about the vertical.
        let rot = M3::z(-FRAC_PI_4 + swing)
            .then(M3::x(TILT))
            .then(M3::y(turn));
        Pose {
            rot,
            scale,
            star,
            star_angle,
        }
    }

    fn at(clock: Clock) -> Pose {
        match clock {
            Clock::Still => Pose::new(0.0, 0.0, 1.0, 1.0, 0.0),
            Clock::Rest(t) => Pose::new(
                ROCK * (TAU * t / ROCK_PERIOD).sin(),
                0.0,
                1.0,
                breathe(t),
                t * 0.6,
            ),
            Clock::Opened(s) => {
                let s = s.max(0.0);
                // A full twirl, swinging back past upright and settling; the rock eases in after.
                let spin = TAU * ease_out(s / 0.62);
                let rock = ROCK * (TAU * (s - 0.62) / ROCK_PERIOD).sin() * smoothstep(0.62, 1.6, s);
                let (swing, scale) = if s < 0.32 {
                    let k = ease_out(s / 0.32);
                    (lerp(0.9, -0.22, k), lerp(0.6, 1.04, k))
                } else {
                    let k = ease_in_out((s - 0.32) / 0.3);
                    (lerp(-0.22, 0.0, k), lerp(1.04, 1.0, k))
                };
                // The tip star blooms as the burst leaves it, then breathes.
                let star = if s < BURST_AT {
                    0.35
                } else if s < 0.4 {
                    lerp(0.35, 1.35, ease_out((s - BURST_AT) / (0.4 - BURST_AT)))
                } else if s < 0.75 {
                    lerp(1.35, 1.0, ease_in_out((s - 0.4) / 0.35))
                } else {
                    1.0 + (breathe(s) - 1.0) * smoothstep(0.75, 1.3, s)
                };
                Pose::new(spin + rock, swing, scale, star, s * 0.8)
            }
        }
    }

    /// A point on the model, in the world.
    fn place(&self, p: V3) -> V3 {
        add(mul(self.rot.apply(p), self.scale), OFFSET)
    }
}

fn breathe(t: f32) -> f32 {
    1.0 + 0.12 * (TAU * t / 2.2).sin()
}

// MARK: Sparkles

/// The tip star blooms this far into the flourish.
const BURST_AT: f32 = 0.18;
/// Sparkles shed from the tip along the twirl, between these times.
const TRAIL: (f32, f32) = (0.06, 0.5);
const BURST: u32 = 22;
/// One twinkle drifts off the tip this often, each living up to `TWINKLE_LIFE`.
const TWINKLE_EVERY: f32 = 0.17;
const TWINKLE_LIFE: f32 = 1.4;
/// Stars circling the wand's axis, as (height on the wand, distance from it, size): a loose
/// spiral from the grip up to the tip. They go round once every `ORBIT` seconds, a third of a
/// turn apart, and spiral out from the shaft as the wand twirls in.
const ORBITERS: [(f32, f32, f32); 3] =
    [(-0.45, 0.62, 0.17), (0.12, 0.74, 0.14), (0.62, 0.56, 0.13)];
const ORBIT: f32 = 3.6;

struct Star {
    at: V3,
    /// To its points, in world units.
    radius: f32,
    angle: f32,
    color: V3,
    alpha: f32,
}

fn sparkles(clock: Clock, p: &Palette) -> Vec<Star> {
    let colors = [
        [1.0; 3],
        rgb(p.violet.mix(Color::WHITE, 0.45)),
        rgb(p.magenta.mix(Color::WHITE, 0.25)),
        rgb(p.amber),
    ];
    let color =
        |i: u32, salt: u32| colors[(hash(i, salt) * colors.len() as f32) as usize % colors.len()];
    let mut stars = orbiters(clock, [colors[3], colors[0], colors[1]]);
    let (now, opened) = match clock {
        Clock::Still => return stars,
        Clock::Rest(t) => (t, false),
        Clock::Opened(s) => (s, true),
    };
    // Where the tip was at time `at` on the same clock.
    let tip = |at: f32| {
        let pose = Pose::at(if opened {
            Clock::Opened(at)
        } else {
            Clock::Rest(at)
        });
        pose.place(TIP)
    };

    if opened {
        for i in 0..BURST {
            // Shed from wherever the tip is as it twirls, so they trace its arc, then flung
            // outward from the middle of the wand.
            let born = lerp(TRAIL.0, TRAIL.1, (i as f32 + hash(i, 9)) / BURST as f32);
            let (age, life) = (now - born, 0.55 + 0.5 * hash(i, 0));
            let u = age / life;
            if !(0.0..1.0).contains(&u) {
                continue;
            }
            let from = tip(born);
            let out = norm(sub(from, OFFSET));
            let dir = norm(add(
                out,
                mul([hash(i, 1) - 0.5, hash(i, 2) - 0.3, hash(i, 3) - 0.5], 1.4),
            ));
            let (speed, damping) = (0.7 + 1.0 * hash(i, 4), 3.0);
            let travel = speed * (1.0 - (-damping * age).exp()) / damping;
            stars.push(Star {
                at: add(add(from, mul(dir, travel)), [0.0, -0.2 * age * age, 0.0]),
                radius: (0.08 + 0.08 * hash(i, 5)) * life_size(u),
                angle: hash(i, 6) * TAU + (hash(i, 7) - 0.5) * 6.0 * age,
                color: color(i, 8),
                alpha: 1.0,
            });
        }
    }

    let first = ((now - TWINKLE_LIFE) / TWINKLE_EVERY).floor().max(0.0) as u32;
    let last = (now / TWINKLE_EVERY).floor().max(0.0) as u32;
    for i in first..=last {
        let id = i.wrapping_add(1000);
        let born = i as f32 * TWINKLE_EVERY + hash(id, 9) * 0.1;
        let (age, life) = (now - born, 0.9 + 0.5 * hash(id, 0));
        let u = age / life;
        if !(0.0..1.0).contains(&u) {
            continue;
        }
        let jitter = mul(
            [hash(id, 1) - 0.5, hash(id, 2) - 0.5, hash(id, 3) - 0.5],
            0.14,
        );
        let drift = mul(
            norm([hash(id, 4) - 0.3, hash(id, 5) + 0.3, hash(id, 6) - 0.5]),
            0.18 + 0.25 * hash(id, 7),
        );
        stars.push(Star {
            at: add(
                add(tip(born), jitter),
                add(mul(drift, age), [0.0, 0.06 * age * age, 0.0]),
            ),
            radius: (0.05 + 0.05 * hash(id, 10)) * life_size(u),
            angle: hash(id, 11) * TAU + age * 2.0,
            color: color(id, 8),
            alpha: 0.9,
        });
    }
    stars
}

/// The stars in orbit. They turn with the wand, pass behind it (hidden where it covers them,
/// dimmer while they're at the back), and hold still for Reduce Motion.
fn orbiters(clock: Clock, colors: [V3; 3]) -> Vec<Star> {
    let pose = Pose::at(clock);
    let (now, grow) = match clock {
        Clock::Still => (0.0, 1.0),
        Clock::Rest(t) => (t, 1.0),
        Clock::Opened(s) => (s, ease_out((s - 0.25) / 0.55)),
    };
    if grow <= 0.0 {
        return Vec::new();
    }
    let middle = pose.place([0.0; 3])[2];
    ORBITERS
        .iter()
        .zip(colors)
        .enumerate()
        .map(|(i, (&(height, reach, size), color))| {
            let phase = i as f32 / ORBITERS.len() as f32;
            let turn = TAU * (now / ORBIT + phase);
            let at = pose.place([reach * grow * turn.cos(), height, reach * grow * turn.sin()]);
            // 0 at the front of the orbit, 1 at the back.
            let back = ((middle - at[2]) / (reach * pose.scale) * 0.5 + 0.5).clamp(0.0, 1.0);
            let twinkle = 1.0 + 0.15 * (TAU * now / 1.3 + i as f32 * 2.1).sin();
            Star {
                at,
                radius: size * grow * twinkle,
                angle: now * 0.9 + phase * TAU,
                color,
                alpha: 1.0 - 0.5 * back,
            }
        })
        .collect()
}

/// A sparkle's size over its life (0…1): it swells, then winks out.
fn life_size(u: f32) -> f32 {
    const KEYS: [(f32, f32); 4] = [(0.0, 0.2), (0.25, 1.0), (0.6, 0.7), (1.0, 0.0)];
    let u = u.clamp(0.0, 1.0);
    KEYS.windows(2).find(|w| u <= w[1].0).map_or(0.0, |w| {
        lerp(w[0].1, w[1].1, (u - w[0].0) / (w[1].0 - w[0].0))
    })
}

/// A repeatable random number in [0, 1) for particle `i`'s property `salt`.
fn hash(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B).rotate_left(13);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

// MARK: Model

#[derive(Clone, Copy, PartialEq)]
enum Material {
    Gold = 0,
    Grip = 1,
    Shaft = 2,
    Tip = 3,
}

/// The wand's outline, bottom to top, as (height, radius) runs. A run is one material with
/// smooth normals; neighbouring runs share their end point and crease there. Turned about
/// the vertical, it's the whole model: a gold pommel and collar, a wrapped grip, a gold band,
/// a tapering lacquered shaft, a gold ferrule and a glowing tip.
const PROFILE: &[(Material, &[(f32, f32)])] = &[
    (
        Material::Gold,
        &[
            (-1.05, 0.0),
            (-1.04, 0.035),
            (-1.015, 0.068),
            (-0.98, 0.086),
            (-0.945, 0.088),
            (-0.915, 0.076),
            (-0.897, 0.062),
            (-0.887, 0.083),
            (-0.866, 0.09),
            (-0.846, 0.083),
            (-0.835, 0.07),
        ],
    ),
    (
        Material::Grip,
        &[
            (-0.835, 0.07),
            (-0.74, 0.078),
            (-0.6, 0.082),
            (-0.47, 0.076),
            (-0.39, 0.068),
        ],
    ),
    (
        Material::Gold,
        &[
            (-0.39, 0.068),
            (-0.385, 0.082),
            (-0.37, 0.09),
            (-0.35, 0.09),
            (-0.335, 0.082),
            (-0.33, 0.06),
        ],
    ),
    (
        Material::Shaft,
        &[
            (-0.33, 0.06),
            (-0.1, 0.054),
            (0.2, 0.047),
            (0.5, 0.041),
            (0.84, 0.035),
        ],
    ),
    (
        Material::Gold,
        &[
            (0.84, 0.035),
            (0.845, 0.046),
            (0.865, 0.05),
            (0.885, 0.046),
            (0.89, 0.036),
        ],
    ),
    (
        Material::Tip,
        &[
            (0.89, 0.036),
            (0.94, 0.034),
            (0.98, 0.026),
            (1.005, 0.014),
            (1.015, 0.0),
        ],
    ),
];
const SEGMENTS: usize = 24;
/// Widens the outline: thin enough to read as a wand, thick enough to read at 30 points.
const GIRTH: f32 = 1.75;

struct Vertex {
    pos: V3,
    normal: V3,
    /// Its angle about the axis, for the grip's wrap.
    cos: f32,
    sin: f32,
    material: Material,
}

struct Mesh {
    verts: Vec<Vertex>,
    tris: Vec<[u32; 3]>,
}

static MESH: LazyLock<Mesh> = LazyLock::new(|| {
    let mut mesh = Mesh {
        verts: Vec::new(),
        tris: Vec::new(),
    };
    for &(material, run) in PROFILE {
        let base = mesh.verts.len() as u32;
        for (i, &(y, r)) in run.iter().enumerate() {
            let r = r * GIRTH;
            // Outward in the (radius, height) plane, from the run's local tangent.
            let (a, b) = (run[i.saturating_sub(1)], run[(i + 1).min(run.len() - 1)]);
            let (nr, ny) = if r == 0.0 {
                (0.0, if i == 0 { -1.0 } else { 1.0 })
            } else {
                let (dr, dy) = ((b.1 - a.1) * GIRTH, b.0 - a.0);
                let len = (dr * dr + dy * dy).sqrt();
                (dy / len, -dr / len)
            };
            for j in 0..SEGMENTS {
                let (sin, cos) = (j as f32 / SEGMENTS as f32 * TAU).sin_cos();
                mesh.verts.push(Vertex {
                    pos: [r * cos, y, r * sin],
                    normal: [nr * cos, ny, nr * sin],
                    cos,
                    sin,
                    material,
                });
            }
        }
        let at = |i: usize, j: usize| base + (i * SEGMENTS + j % SEGMENTS) as u32;
        for i in 0..run.len() - 1 {
            for j in 0..SEGMENTS {
                let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j), at(i + 1, j + 1));
                mesh.tris.push([a, b, d]);
                mesh.tris.push([a, d, c]);
            }
        }
    }
    mesh
});

/// Lights and materials, in the Arcane palette's linear colours.
struct Looks {
    gold: V3,
    grip: V3,
    wrap: V3,
    shaft: V3,
    violet: V3,
    magenta: V3,
}

impl Looks {
    fn new(p: &Palette) -> Looks {
        Looks {
            gold: rgb(p.amber),
            grip: rgb(srgb(0.36, 0.22, 0.56, 1.0)),
            wrap: rgb(srgb(0.13, 0.08, 0.24, 1.0)),
            shaft: rgb(srgb(0.17, 0.12, 0.30, 1.0)),
            violet: rgb(p.violet),
            magenta: rgb(p.magenta),
        }
    }

    /// Attributes: world normal, world position, height on the model, angle (cos, sin),
    /// material. Blinn-Phong under a warm key and a violet fill, with a magenta-violet rim.
    fn shade(&self, a: &[f32; 10]) -> [f32; 4] {
        let n = norm([a[0], a[1], a[2]]);
        let v = norm(sub(CAMERA, [a[3], a[4], a[5]]));
        let y = a[6];
        let key = norm([-0.45, 0.75, 0.6]);
        let fill = norm([0.7, -0.35, 0.45]);
        let diffuse = dot(n, key).max(0.0);
        let fill = dot(n, fill).max(0.0);
        let shine = |power: i32| dot(n, norm(add(key, v))).max(0.0).powi(power);
        let rim = (1.0 - dot(n, v).max(0.0)).powi(3);
        let white = [1.0; 3];
        let c = match a[9].round() as u8 {
            0 => {
                // Polished metal: a fake sky in the reflection does most of the work.
                let up = sub(mul(n, 2.0 * dot(n, v)), v)[1];
                let lit = 0.12 + 0.6 * diffuse + 0.5 * smoothstep(-0.2, 0.8, up);
                add(
                    add(
                        mul(self.gold, lit),
                        mul(mix(self.gold, white, 0.6), 1.2 * shine(40)),
                    ),
                    mul(self.magenta, 0.3 * rim),
                )
            }
            1 => {
                // A ribbon wound twice around the grip.
                let turn = a[8].atan2(a[7]) / TAU * 2.0 + y * 7.0;
                let s = turn - turn.floor();
                let ribbon = smoothstep(0.0, 0.06, s) * (1.0 - smoothstep(0.42, 0.48, s));
                let base = mix(self.grip, self.wrap, ribbon);
                let lit = add(
                    mul(base, 0.18 + 0.85 * diffuse),
                    mul(self.violet, 0.12 * fill),
                );
                add(
                    add(lit, mul(white, 0.18 * shine(18))),
                    mul(self.violet, 0.7 * rim),
                )
            }
            2 => {
                // Lacquer that glows warmer toward the tip, as if charged.
                let lit = add(
                    mul(self.shaft, 0.2 + 0.9 * diffuse),
                    mul(self.violet, 0.15 * fill),
                );
                let edge = mul(
                    mix(self.violet, self.magenta, smoothstep(-0.3, 0.8, y)),
                    0.9 * rim,
                );
                let charge = mul(self.magenta, 0.22 * smoothstep(0.45, 0.9, y));
                add(add(add(lit, mul(white, 0.65 * shine(60))), edge), charge)
            }
            _ => {
                let glow = mix(
                    white,
                    mul(mix(self.violet, self.magenta, 0.5), 1.1),
                    0.25 + 0.6 * rim,
                );
                mul(glow, 1.15)
            }
        };
        [c[0], c[1], c[2], 1.0]
    }
}

fn rgb(c: Color) -> V3 {
    [c.r, c.g, c.b]
}

// MARK: Easing

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}
fn ease_in_out(t: f32) -> f32 {
    smoothstep(0.0, 1.0, t)
}

#[cfg(test)]
mod tests {
    use super::{CANVAS, Clock, render};

    fn opaque(rgba: &[u8]) -> usize {
        rgba.chunks(4).filter(|p| p[3] > 200).count()
    }

    #[test]
    fn frames_are_a_function_of_time() {
        let p = super::palette::arcane();
        let (side, a) = render(Clock::Opened(0.4), 2.0, &p);
        assert_eq!(side, (CANVAS * 2.0) as u32);
        assert_eq!(a.len(), (side * side * 4) as usize);
        assert_eq!(a, render(Clock::Opened(0.4), 2.0, &p).1);
        assert_ne!(a, render(Clock::Opened(0.5), 2.0, &p).1);
    }

    #[test]
    fn the_wand_fills_part_of_its_square() {
        let p = super::palette::arcane();
        for clock in [
            Clock::Still,
            Clock::Rest(3.0),
            Clock::Opened(0.3),
            Clock::Opened(2.0),
        ] {
            let (side, rgba) = render(clock, 2.0, &p);
            let covered = opaque(&rgba) as f32 / (side * side) as f32;
            assert!((0.01..0.1).contains(&covered), "{clock:?}: {covered}");
        }
    }

    #[test]
    fn the_burst_sparkles_then_settles() {
        let p = super::palette::arcane();
        let lit = |clock| {
            render(clock, 2.0, &p)
                .1
                .chunks(4)
                .filter(|px| px[3] > 0)
                .count()
        };
        // Mid-flourish there's far more sparkle than once it has burnt out.
        assert!(lit(Clock::Opened(0.45)) > lit(Clock::Opened(3.0)) + 100);
        // Reduce Motion: no sparkles at all, just the wand and its tip star.
        assert!(lit(Clock::Still) < lit(Clock::Rest(3.0)));
    }
}
