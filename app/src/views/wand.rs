//! The wand in the search bar: a small 3D model, rendered on the CPU into `Pixels` on the frame
//! clock, with a mind of its own (`motion.rs`). Each opening it twirls in and throws sparkles
//! as the orb would pop (`intro.rs`). Then it reacts to the bar: it flicks with the selection
//! and leans with the scroll, aims at the field and writes each typed character with a spark
//! (`spells.rs`), takes deleted ones back as dust, droops when nothing matches, perks up under
//! the pointer, twirls when clicked, thrusts when something is cast, and spins away when the
//! bar closes. Three stars orbit it throughout.
//!
//! A frame depends only on the frame clock and the cues the wand was given, so headless
//! renders repeat. Reduce Motion gets one still frame and no spells. Nothing draws while the
//! wand is off screen; settled, it draws at `IDLE_FPS`.

mod model;
mod motion;
mod raster;
mod sparkle;
pub mod spells;

use std::cell::Cell;
use std::f32::consts::{FRAC_PI_4, TAU};
use std::time::Duration;

use rune::{Color, El, Pixels, Span, Tx, el};

use super::palette::{self, Palette};
use model::{Looks, MESH, rgb};
pub use motion::Gesture;
use motion::{BLEND, Mood, Puff, Stance, ease_in_out, ease_out, lerp, smoothstep};
use raster::{Canvas, M3, V3, Vert, add, mul, norm, sub};
use sparkle::{Mote, ORBIT, ORBITERS, Spawner, Star, orbiter};
use spells::{Spells, Stamped};

/// Its square, in points: larger than the orb, so sparkles can fly past the orb's edge.
pub const CANVAS: f32 = 60.0;
/// Each pixel is the mean of this many samples a side.
const SUPERSAMPLE: usize = 2;
/// The twirl starts with the orb's pop (`intro::ORB`).
const DELAY: f64 = 0.05;
/// Long enough for the opening burst to burn out.
const FLOURISH: f64 = 1.4;
const IDLE_FPS: f64 = 30.0;
/// The wand stays in the search bar this long into a cast, even as a spell opens over it.
const CAST_HOLD: f32 = 0.5;
/// Typing keeps it attentive this long after the last keystroke.
const ATTENTION: f32 = 1.1;

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

/// The opening's sparkles, shed from the tip along the twirl between these times.
const TRAIL: (f32, f32) = (0.06, 0.5);
const TRAIL_COUNT: u32 = 22;

/// What the bar tells the wand each frame.
pub struct Signals<'a> {
    /// It's on screen.
    pub shown: bool,
    /// Counts openings; a new one starts a flourish.
    pub intro: u64,
    pub query: &'a str,
    /// The query matches nothing.
    pub fizzled: bool,
    /// How far the results are scrolled, in points.
    pub scroll: f32,
}

struct Playing {
    gesture: Gesture,
    at: f64,
    /// Which of its puffs have been thrown, one bit each.
    fired: u32,
}

/// The wand's state machine, its sparkles, and the image it last drew. Lives in the store;
/// `Cantrip::animate_wand` calls `frame` at the start of every frame.
pub struct Wand {
    pixels: Pixels,
    /// The device scale the view last laid it out at.
    scale: Cell<f32>,
    /// Frame time and scale of the image showing.
    drawn: Option<(f64, f32)>,

    /// The opening the current flourish belongs to, and the frame time it began.
    intro: u64,
    began: Option<f64>,
    still: bool,
    now: f64,
    last: Option<f64>,
    shown: bool,

    mood: Mood,
    mood_since: f64,
    mood_from: Stance,
    typed_at: f64,
    hovered: bool,
    /// Cued since the last frame; stamped with its time.
    cues: Vec<Gesture>,
    playing: Vec<Playing>,
    /// Smoothed scroll velocity, as a lean, and the offset it was measured from.
    lean: f32,
    scrolled: Option<f32>,

    motes: Vec<Mote>,
    spawner: Spawner,
    next_twinkle: f64,
    trail: u32,
    /// The orbiting stars' phase, in turns.
    orbit: f32,

    spells: Spells,
    spelling: bool,
    casting: bool,
    /// The tip, in points from the canvas's centre, as last posed.
    tip: [f32; 2],
    pose: Pose,
    stance: Stance,
}

impl Default for Wand {
    fn default() -> Self {
        Self::new()
    }
}

impl Wand {
    pub fn new() -> Wand {
        Wand {
            pixels: Pixels::new(),
            scale: Cell::new(2.0),
            drawn: None,
            intro: 0,
            began: None,
            still: false,
            now: 0.0,
            last: None,
            shown: false,
            mood: Mood::Idle,
            mood_since: f64::NEG_INFINITY,
            mood_from: Stance::default(),
            typed_at: f64::NEG_INFINITY,
            hovered: false,
            cues: Vec::new(),
            playing: Vec::new(),
            lean: 0.0,
            scrolled: None,
            motes: Vec::new(),
            spawner: Spawner::default(),
            next_twinkle: 0.0,
            trail: TRAIL_COUNT,
            orbit: 0.0,
            spells: Spells::default(),
            spelling: false,
            casting: false,
            tip: [0.0; 2],
            pose: Pose::new(Stance::default(), 0.0),
            stance: Stance::default(),
        }
    }

    // MARK: Cues

    /// Plays `gesture` from the next frame.
    pub fn cue(&mut self, gesture: Gesture) {
        if !self.still {
            self.cues.push(gesture);
        }
    }

    pub fn hover(&mut self, inside: bool) {
        self.hovered = inside;
    }

    /// The query was edited from `old` to `new`: the wand writes what was typed and takes back
    /// what was deleted.
    pub fn edited(&mut self, old: &str, new: &str) {
        if !self.still {
            self.spells.edited(old, new);
        }
    }

    /// It's mid-cast: the view keeps it in the search bar even if a spell opened.
    pub fn casting(&self) -> bool {
        self.cues.contains(&Gesture::Cast)
            || self
                .playing
                .iter()
                .any(|p| p.gesture == Gesture::Cast && ((self.now - p.at) as f32) < CAST_HOLD)
    }

    /// Gestures cued since the last frame.
    #[cfg(test)]
    pub fn cued(&self) -> &[Gesture] {
        &self.cues
    }

    // MARK: View

    /// The wand, centred on the `size`-point slot it sits in.
    pub fn view(&self, size: f32, scale: f32) -> El {
        self.scale.set(scale);
        let inset = (size - CANVAS) / 2.0;
        el().absolute(inset, inset)
            .size(CANVAS, CANVAS)
            .pixels(&self.pixels)
            .inert()
            .a11y_hidden()
    }

    /// Colours for the query's characters: hidden while their spark flies, then glowing.
    pub fn spans(&self, query: &str, p: &Palette) -> Vec<Span> {
        self.spells.spans(self.now, query, p)
    }

    /// The sparks flying between the wand and the field, over the window.
    pub fn spells_view(&self, query: &str, p: &Palette) -> Option<El> {
        self.spells.view(self.now, query, p)
    }

    // MARK: Frames

    /// Advances to `t` (frame-clock seconds) and draws a frame if one is due. `still` asks
    /// whether to hold still (Reduce Motion), once per opening. True when the view must
    /// rebuild: spells are flying, or the cast that held the wand in place ended.
    pub fn frame(
        &mut self,
        t: f64,
        signals: Signals,
        still: impl FnOnce() -> bool,
        tx: &mut Tx,
    ) -> bool {
        let busy = self.advance(t, &signals, still);
        let dirty = self.spells.active() || self.spelling || self.casting != self.casting();
        self.spelling = self.spells.active();
        self.casting = self.casting();
        if !self.shown {
            // Draw afresh when it's back.
            self.drawn = None;
            return dirty;
        }
        let scale = self.scale.get();
        // A window draws only when asked: every frame asks for the next one, at once while
        // anything moves (`Pixels::set` here doesn't: this frame's own draw takes it), else at
        // the idle rate. Reduce Motion asks for nothing once it has drawn.
        let due = match self.drawn {
            None => true,
            Some((_, drawn_at)) if drawn_at != scale => true,
            Some(_) if self.still => false,
            Some((last, _)) => busy || t - last >= 1.0 / IDLE_FPS - 0.002,
        };
        if !self.still {
            let next = if busy { 0.0 } else { 1.0 / IDLE_FPS };
            tx.wake_after(Duration::from_secs_f64(next));
        }
        if due {
            let (side, rgba) = self.render(scale, &palette::arcane());
            self.pixels.set(side, side, rgba);
            self.drawn = Some((t, scale));
        }
        dirty
    }

    /// Runs the state machine to `t`. True while anything is moving faster than the idle rock.
    fn advance(&mut self, t: f64, signals: &Signals, still: impl FnOnce() -> bool) -> bool {
        if signals.intro != self.intro {
            self.intro = signals.intro;
            self.began = (signals.intro > 0).then_some(t + DELAY);
            self.still = still();
            self.drawn = None;
            self.playing.clear();
            self.cues.clear();
            self.motes.clear();
            self.spells.clear();
            self.trail = 0;
            self.next_twinkle = t + DELAY;
            self.mood = Mood::Idle;
            self.mood_from = Stance::default();
            self.mood_since = f64::NEG_INFINITY;
        }
        let dt = self.last.map_or(0.0, |last| (t - last).clamp(0.0, 0.1)) as f32;
        self.last = Some(t);
        self.now = t;

        if !signals.shown {
            self.shown = false;
            self.cues.clear();
            self.spells.clear();
            self.scrolled = None;
            return false;
        }
        if !self.shown {
            self.shown = true;
            // Back from a spell (not an opening, which has its own flourish).
            if self.began.is_some_and(|b| t - b > 1.0) && !self.still {
                self.cues.push(Gesture::Return);
            }
        }
        if self.still {
            self.cues.clear();
            self.spells.clear();
            self.pose = Pose::new(Stance::default(), 0.0);
            self.stance = Stance::default();
            return false;
        }

        // Cues and spells become gestures, stamped now.
        for gesture in self.cues.drain(..) {
            self.playing.push(Playing {
                gesture,
                at: t,
                fired: 0,
            });
        }
        let tip = [
            super::WAND_CENTER[0] + self.tip[0],
            super::WAND_CENTER[1] + self.tip[1],
        ];
        for stamped in self.spells.stamp(t, tip, signals.query) {
            self.typed_at = t;
            let gesture = match stamped {
                Stamped::Typed => Gesture::Tap,
                Stamped::Erased { delay, all: true } => Gesture::Sweep { delay },
                Stamped::Erased { delay, all: false } => Gesture::Erase { delay },
            };
            self.playing.push(Playing {
                gesture,
                at: t,
                fired: 0,
            });
        }

        // The mood: the most insistent that holds.
        let mood = if ((t - self.typed_at) as f32) < ATTENTION {
            Mood::Attentive
        } else if signals.fizzled {
            Mood::Fizzled
        } else if self.hovered {
            Mood::Curious
        } else {
            Mood::Idle
        };
        if mood != self.mood {
            self.mood_from = self.mood_stance(t);
            self.mood = mood;
            self.mood_since = t;
        }

        // Scrolling leans it: down tips it down.
        let offset = signals.scroll;
        let velocity = match self.scrolled {
            Some(before) if dt > 0.0 => (offset - before) / dt,
            _ => 0.0,
        };
        self.scrolled = Some(offset);
        let target = -(velocity / 1800.0).clamp(-1.0, 1.0) * 0.3;
        self.lean += (target - self.lean) * (1.0 - (-dt * 12.0).exp());

        // The stance: base animation, mood, gestures, lean.
        let mut gestures = Stance::default();
        for p in &self.playing {
            gestures = gestures.add(p.gesture.stance((t - p.at) as f32));
        }
        let stance = self
            .base(t)
            .add(self.mood_stance(t))
            .add(gestures.tame())
            .add(Stance {
                swing: self.lean,
                ..Stance::default()
            });
        self.orbit = (self.orbit + dt / ORBIT * (1.0 + stance.orbit).max(0.1)).fract();
        self.stance = stance;
        self.pose = Pose::new(stance, t as f32);
        let tip_world = self.pose.place(TIP);
        let focal = PT_PER_UNIT * DISTANCE;
        let z = DISTANCE - tip_world[2];
        self.tip = [focal * tip_world[0] / z, -focal * tip_world[1] / z];

        // Sparkles: gestures' puffs, the opening's trail, idle twinkles.
        let colors = sparkle_colors(&palette::arcane());
        for p in &mut self.playing {
            let age = (t - p.at) as f32;
            for (i, puff) in p.gesture.puffs().iter().enumerate() {
                if p.fired & (1 << i) == 0 && age >= puff.at {
                    p.fired |= 1 << i;
                    self.motes
                        .extend(self.spawner.puff(t, tip_world, puff, &colors));
                }
            }
        }
        if let Some(began) = self.began {
            let s = (t - began) as f32;
            while self.trail < TRAIL_COUNT
                && s >= lerp(TRAIL.0, TRAIL.1, self.trail as f32 / TRAIL_COUNT as f32)
            {
                self.trail += 1;
                // Shed along the twirl, flung outward from the middle of the wand.
                let out = Puff {
                    at: 0.0,
                    count: 1,
                    aim: norm(sub(tip_world, OFFSET)),
                    spread: 0.6,
                    speed: 1.2,
                    size: 0.12,
                    life: 0.8,
                };
                self.motes
                    .extend(self.spawner.puff(t, tip_world, &out, &colors));
            }
        }
        if t >= self.next_twinkle && self.began.is_none_or(|b| t >= b) {
            let lively = self.mood != Mood::Fizzled;
            self.motes
                .push(self.spawner.twinkle(t, tip_world, lively, &colors));
            self.next_twinkle = t + match self.mood {
                Mood::Idle => 0.17,
                Mood::Curious => 0.11,
                Mood::Attentive => 0.08,
                Mood::Fizzled => 0.32,
            };
        }

        self.motes.retain(|m| m.alive(t));
        self.playing
            .retain(|p| ((t - p.at) as f32) < p.gesture.length());

        let flourishing = self.began.is_some_and(|b| t - b < FLOURISH);
        let blending = ((t - self.mood_since) as f32) < BLEND;
        flourishing
            || blending
            || !self.playing.is_empty()
            || self.spells.active()
            || self.lean.abs() > 0.002
            || self.mood == Mood::Attentive
    }

    /// The mood's stance at `t`, blended in from where the last one left off.
    fn mood_stance(&self, t: f64) -> Stance {
        let k = ease_in_out(((t - self.mood_since) as f32) / BLEND);
        self.mood_from.lerp(self.mood.stance(t as f32), k)
    }

    /// The opening flourish (or, before any opening, a plain idle rock).
    fn base(&self, t: f64) -> Stance {
        let Some(began) = self.began else {
            let t = t as f32;
            return Stance {
                turn: ROCK * (TAU * t / ROCK_PERIOD).sin(),
                star: breathe(t) - 1.0,
                ..Stance::default()
            };
        };
        let raw = (t - began) as f32;
        let s = raw.max(0.0);
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
        // The tip star blooms as the burst leaves it, then breathes; it's out until the start.
        const BLOOM: f32 = 0.18;
        let star = if raw < 0.0 {
            0.0
        } else if s < BLOOM {
            0.35
        } else if s < 0.4 {
            lerp(0.35, 1.35, ease_out((s - BLOOM) / (0.4 - BLOOM)))
        } else if s < 0.75 {
            lerp(1.35, 1.0, ease_in_out((s - 0.4) / 0.35))
        } else {
            1.0 + (breathe(s) - 1.0) * smoothstep(0.75, 1.3, s)
        };
        Stance {
            turn: spin + rock,
            swing,
            grow: scale - 1.0,
            star: star - 1.0,
            // The orbiting stars spiral out from the shaft as it twirls in.
            reach: ease_out((s - 0.25) / 0.55) - 1.0,
            ..Stance::default()
        }
    }

    // MARK: Rendering

    /// Everything to draw over the model: motes, the orbiting stars, the tip's own star.
    fn stars(&self, p: &Palette) -> Vec<Star> {
        let pose = &self.pose;
        let st = self.stance;
        let t = self.now;
        let mut stars: Vec<Star> = self.motes.iter().filter_map(|m| m.star(t)).collect();

        let reach = (1.0 + st.reach).max(0.0);
        let glow = (1.0 + st.glow).clamp(0.0, 1.5);
        if reach > 0.01 {
            let colors = [
                rgb(p.amber),
                [1.0; 3],
                rgb(p.violet.mix(Color::WHITE, 0.45)),
            ];
            let middle = pose.place([0.0; 3])[2];
            for (i, &(_, distance, size)) in ORBITERS.iter().enumerate() {
                let at = pose.place(orbiter(i, self.orbit, reach));
                // 0 at the front of the orbit, 1 at the back: dimmer there.
                let back = ((middle - at[2]) / (distance * reach * pose.scale).max(1e-3) * 0.5
                    + 0.5)
                    .clamp(0.0, 1.0);
                let twinkle = 1.0 + 0.15 * (TAU * t as f32 / 1.3 + i as f32 * 2.1).sin();
                stars.push(Star {
                    at,
                    radius: size * reach.min(1.0) * twinkle * (1.0 + 0.3 * st.glow.max(0.0)),
                    angle: t as f32 * 0.9 + i as f32 * 2.1,
                    color: colors[i],
                    alpha: ((1.0 - 0.5 * back) * glow).min(1.0),
                });
            }
        }

        let star = (1.0 + st.star).max(0.0);
        if star > 0.01 {
            // The tip's own star sits just in front of the tip.
            stars.push(Star {
                at: add(pose.place(TIP), [0.0, 0.0, 0.25]),
                radius: 0.2 * star * pose.scale.min(1.0),
                angle: pose.star_angle,
                color: rgb(p.violet.mix(Color::WHITE, 0.6)),
                alpha: 1.0,
            });
        }
        stars
    }

    /// The image as posed: its side in device pixels, and straight-alpha sRGB bytes.
    fn render(&self, scale: f32, p: &Palette) -> (u32, Vec<u8>) {
        render(&self.pose, self.stars(p), scale, p)
    }
}

fn sparkle_colors(p: &Palette) -> [V3; 4] {
    [
        [1.0; 3],
        rgb(p.violet.mix(Color::WHITE, 0.45)),
        rgb(p.magenta.mix(Color::WHITE, 0.25)),
        rgb(p.amber),
    ]
}

/// The model in `pose` with `stars` over it.
fn render(pose: &Pose, mut stars: Vec<Star>, scale: f32, p: &Palette) -> (u32, Vec<u8>) {
    let side = ((CANVAS * scale).round() as usize).max(1);

    // The model, supersampled.
    let ss = side * SUPERSAMPLE;
    let focal = PT_PER_UNIT * DISTANCE * scale * SUPERSAMPLE as f32;
    let project = |w: V3, f: f32, side: usize| {
        let z = DISTANCE - w[2];
        [
            side as f32 / 2.0 + f * w[0] / z,
            side as f32 / 2.0 - f * w[1] / z,
            z,
        ]
    };
    let mut canvas = Canvas::new(ss, ss);
    if pose.scale > 0.02 {
        let mesh = &*MESH;
        let verts: Vec<Vert<10>> = mesh
            .verts
            .iter()
            .map(|v| {
                let w = pose.place(v.pos);
                let n = pose.rot.apply(v.normal);
                let [x, y, z] = project(w, focal, ss);
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
    }
    let mut out = canvas.downsample(SUPERSAMPLE);

    // Stars over it, far ones first.
    let focal = focal / SUPERSAMPLE as f32;
    stars.sort_by(|a, b| a.at[2].total_cmp(&b.at[2]));
    for s in &stars {
        let at = project(s.at, focal, side);
        out.star(at, s.radius * focal / at[2], s.angle, s.color, s.alpha);
    }
    (side as u32, out.encode())
}

/// The model's placement on one frame.
struct Pose {
    rot: M3,
    scale: f32,
    lift: f32,
    star_angle: f32,
}

impl Pose {
    fn new(stance: Stance, t: f32) -> Pose {
        // Diagonal like the old symbol, tip to the upper right and leaning out, then turned
        // about the vertical.
        let rot = M3::z(-FRAC_PI_4 + stance.swing)
            .then(M3::x(TILT + stance.pitch))
            .then(M3::y(stance.turn));
        Pose {
            rot,
            scale: (1.0 + stance.grow).max(0.0),
            lift: stance.lift,
            star_angle: t * 0.7,
        }
    }

    /// A point on the model, in the world.
    fn place(&self, p: V3) -> V3 {
        add(
            add(mul(self.rot.apply(p), self.scale), OFFSET),
            [0.0, self.lift, 0.0],
        )
    }
}

fn breathe(t: f32) -> f32 {
    1.0 + 0.12 * (TAU * t / 2.2).sin()
}

#[cfg(test)]
mod tests {
    use super::{CANVAS, Gesture, Signals, Wand};

    fn signals(query: &str) -> Signals<'_> {
        Signals {
            shown: true,
            intro: 1,
            query,
            fizzled: false,
            scroll: 0.0,
        }
    }

    /// A wand opened at 0 and run at 60 fps to `until`.
    fn run(wand: &mut Wand, from: f64, until: f64, query: &str) {
        let mut t = from;
        while t <= until {
            wand.advance(t, &signals(query), || false);
            t += 1.0 / 60.0;
        }
    }

    fn image(wand: &Wand) -> (u32, Vec<u8>) {
        wand.render(2.0, &super::palette::arcane())
    }

    fn lit(wand: &Wand) -> usize {
        image(wand).1.chunks(4).filter(|px| px[3] > 0).count()
    }

    #[test]
    fn frames_repeat_for_the_same_cues() {
        let (mut a, mut b) = (Wand::new(), Wand::new());
        for w in [&mut a, &mut b] {
            run(w, 0.0, 0.4, "");
            w.cue(Gesture::Poke);
            run(w, 0.41, 0.8, "");
        }
        let (side, rgba) = image(&a);
        assert_eq!(side, (CANVAS * 2.0) as u32);
        assert_eq!(rgba.len(), (side * side * 4) as usize);
        assert_eq!(rgba, image(&b).1);
    }

    #[test]
    fn the_wand_fills_part_of_its_square() {
        let mut wand = Wand::new();
        for until in [0.35, 2.0, 4.0] {
            run(&mut wand, 0.0, until, "");
            let (side, rgba) = image(&wand);
            let covered =
                rgba.chunks(4).filter(|p| p[3] > 200).count() as f32 / (side * side) as f32;
            assert!((0.01..0.1).contains(&covered), "{until}: {covered}");
        }
    }

    #[test]
    fn the_opening_sparkles_then_settles() {
        let mut wand = Wand::new();
        run(&mut wand, 0.0, 0.5, "");
        let bursting = lit(&wand);
        run(&mut wand, 0.5, 3.0, "");
        assert!(bursting > lit(&wand) + 100, "{bursting} vs {}", lit(&wand));
    }

    #[test]
    fn gestures_move_the_wand_and_end() {
        let mut wand = Wand::new();
        run(&mut wand, 0.0, 2.0, "");
        let rest = wand.stance;
        wand.cue(Gesture::Flick { up: true });
        run(&mut wand, 2.0, 2.06, "");
        assert!(
            wand.stance.swing > rest.swing + 0.15,
            "flicked up: {:?}",
            wand.stance
        );
        wand.cue(Gesture::Flick { up: false });
        run(&mut wand, 2.7, 2.76, "");
        assert!(wand.stance.swing < -0.15, "flicked down: {:?}", wand.stance);
        run(&mut wand, 2.76, 4.0, "");
        assert!(wand.playing.is_empty(), "and they end");
    }

    #[test]
    fn a_cast_holds_the_wand_in_place_then_lets_go() {
        let mut wand = Wand::new();
        run(&mut wand, 0.0, 2.0, "");
        wand.cue(Gesture::Cast);
        assert!(wand.casting(), "from the moment it's cued");
        run(&mut wand, 2.0, 2.3, "");
        assert!(wand.casting());
        run(&mut wand, 2.3, 2.6, "");
        assert!(!wand.casting());
    }

    #[test]
    fn typing_writes_with_sparks_and_makes_it_attentive() {
        let p = super::palette::arcane();
        let mut wand = Wand::new();
        run(&mut wand, 0.0, 2.0, "");
        wand.edited("", "c");
        run(&mut wand, 2.0, 2.0, "c");
        assert!(wand.spells.active());
        // In flight: the character is hidden.
        let spans = wand.spans("c", &p);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].color.unwrap().a, 0.0);
        assert!(wand.spells_view("c", &p).is_some());
        run(&mut wand, 2.0, 2.4, "c");
        assert_eq!(wand.mood, super::Mood::Attentive);
        assert!(wand.spans("c", &p)[0].color.unwrap().a > 0.9, "landed");
        run(&mut wand, 2.4, 4.0, "c");
        assert!(!wand.spells.active(), "and done");
        assert_eq!(wand.mood, super::Mood::Idle);
    }

    #[test]
    fn nothing_matching_fizzles_it() {
        let mut wand = Wand::new();
        let mut t = 0.0;
        while t < 3.0 {
            let s = Signals {
                fizzled: t > 1.5,
                ..signals("zzz")
            };
            wand.advance(t, &s, || false);
            t += 1.0 / 60.0;
        }
        assert_eq!(wand.mood, super::Mood::Fizzled);
        assert!(wand.stance.swing < -0.5, "it droops: {:?}", wand.stance);
    }

    #[test]
    fn reduce_motion_holds_still() {
        let mut wand = Wand::new();
        wand.advance(0.0, &signals(""), || true);
        wand.cue(Gesture::Poke);
        wand.edited("", "c");
        run(&mut wand, 0.0, 1.0, "c");
        assert!(wand.playing.is_empty() && !wand.spells.active() && wand.motes.is_empty());
    }
}
