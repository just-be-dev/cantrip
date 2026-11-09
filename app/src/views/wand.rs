//! The wand in the search bar: a small 3D model, rendered on the CPU into `Pixels` on the frame
//! clock, with a mind of its own (`motion.rs`). Each opening it pops in first, alone and large
//! in the middle of where the bar will be; it casts the bar, which grows out of its tip
//! (`intro.rs`), then shrinks as it flies home to the search row. While it flies it draws into
//! its own larger image over the window (`hero_view`), so it stays sharp at any size. Then it
//! reacts to the bar: it flicks with the selection
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
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};
use std::time::Duration;

use rune::{Color, El, Pixels, Shape, Span, Transition, Tx, canvas, el};

use super::palette::{Palette, srgb};
use super::runes;
use super::theme::Theme;
use model::{Looks, MESH, rgb};
pub use motion::Gesture;
use motion::{BLEND, Mood, Stance, ease_in_out, ease_out, flare, lerp, smoothstep};
use raster::{Canvas, M3, V3, Vert, add, mul};
use sparkle::{Mote, ORBIT, ORBITERS, Spawner, Star, orbiter};
use spells::{Spells, Stamped};

/// Its square, in points: larger than its slot, so sparkles can fly past the slot's edge.
pub const CANVAS: f32 = 60.0;
/// Each pixel is the mean of this many samples a side.
const SUPERSAMPLE: usize = 2;
/// The opening starts with the intro's sequences.
const DELAY: f64 = 0.0;
/// Long enough for the opening to land and the rock to ease in.
const FLOURISH: f64 = 2.0;

// The opening, in seconds from its start. The wand pops in at `HERO`, `HERO_ZOOM` times its
// size, until it casts the bar at `CAST` (`intro::GROW`); it flies home from `FLY` to `LAND`.
const CAST: f32 = super::intro::GROW as f32;
const FLY: f32 = 0.18;
const LAND: f32 = 0.5;
const HERO_ZOOM: f32 = 3.0;
/// The large image's square, in points: the wand at `HERO_ZOOM` with room for its stars.
const HERO_CANVAS: f32 = 150.0;
/// Where it appears, in window points: a little above the middle of the bar to come.
const HERO: [f32; 2] = [
    super::WINDOW_SIZE.0 / 2.0,
    super::MARGIN + super::PANEL_SIZE.1 * 0.42,
];
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

/// What the bar tells the wand each frame.
pub struct Signals<'a> {
    /// How the magic looks.
    pub theme: Theme,
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
    /// The large image it draws into while it flies in.
    hero: Pixels,
    /// The device scale the view last laid it out at.
    scale: Cell<f32>,
    /// Frame time, scale, and whether it was the large image, of the image showing.
    drawn: Option<(f64, f32, bool)>,
    /// Whether the last frame had the opening's layer up (`hero_view`).
    heroing: bool,
    /// The theme it's drawn in.
    theme: Theme,

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
    /// The orbiting stars' phase, in turns.
    orbit: f32,

    spells: Spells,
    spelling: bool,
    casting: bool,
    /// The tip, in points from the canvas's centre at its size in the slot, as last posed.
    tip: [f32; 2],
    /// Where its canvas is centred, in window points, and how large it is.
    center: [f32; 2],
    zoom: f32,
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
            hero: Pixels::new(),
            scale: Cell::new(2.0),
            drawn: None,
            heroing: false,
            theme: Theme::default(),
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
            orbit: 0.0,
            spells: Spells::default(),
            spelling: false,
            casting: false,
            tip: [0.0; 2],
            center: super::WAND_CENTER,
            zoom: 1.0,
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
    pub fn edited(&mut self, old: &str, new: &str, theme: Theme) {
        if !self.still {
            self.spells.edited(old, new, theme);
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
            // It takes over from the flying wand in place: no fade in.
            .transition(Transition::none())
            .inert()
            .a11y_hidden()
    }

    /// Seconds into the opening while its layer is up (not under Reduce Motion): until the wand
    /// lands, and in Sigil until the sigil has faded.
    fn opening(&self) -> Option<f32> {
        let s = (self.now - self.began?) as f32;
        let end = match self.theme {
            Theme::Sigil => SIGIL_END.max(LAND),
            Theme::Arcane => LAND,
        };
        (!self.still && s < end).then_some(s.max(0.0))
    }

    /// It's flying in: it draws over the window (`hero_view`), not in its slot.
    pub fn flying(&self) -> bool {
        self.opening().is_some_and(|s| s < LAND)
    }

    /// The opening over the window: the wand on its way in, and in Sigil the sigil it
    /// inscribes around its glow as it casts the bar.
    pub fn hero_view(&self, scale: f32) -> Option<El> {
        let s = self.opening()?;
        let mut layer = el()
            .key("hero")
            .a11y_hidden()
            .inert()
            .pin(Some(0.0), Some(0.0), None, None)
            .size(super::WINDOW_SIZE.0, super::WINDOW_SIZE.1)
            .raise()
            .transition(Transition::none())
            .instant_changes();
        if self.theme == Theme::Sigil
            && let Some(sigil) = sigil(s, &self.theme.palette())
        {
            layer = layer.child(sigil);
        }
        if s < LAND {
            self.scale.set(scale);
            let (center, _) = placement(s);
            layer = layer.child(
                el().absolute(center[0] - HERO_CANVAS / 2.0, center[1] - HERO_CANVAS / 2.0)
                    .size(HERO_CANVAS, HERO_CANVAS)
                    .pixels(&self.hero),
            );
        }
        Some(layer)
    }

    /// How much of the field's placeholder shows (0…1): it waits for deleted text to fade.
    pub fn placeholder_alpha(&self) -> f32 {
        self.spells.placeholder(self.now)
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
        // The opening's layer moves every frame, and goes once it lands.
        let heroing = self.opening().is_some();
        let dirty = self.spells.active()
            || self.spelling
            || self.casting != self.casting()
            || heroing
            || self.heroing;
        self.spelling = self.spells.active();
        self.casting = self.casting();
        self.heroing = heroing;
        if !self.shown {
            // Draw afresh when it's back.
            self.drawn = None;
            return dirty;
        }
        let scale = self.scale.get();
        // A window draws only when asked: every frame asks for the next one, at once while
        // anything moves (`Pixels::set` here doesn't: this frame's own draw takes it), else at
        // the idle rate. Reduce Motion asks for nothing once it has drawn.
        let flying = self.flying();
        let due = match self.drawn {
            None => true,
            Some((_, drawn_at, hero)) if drawn_at != scale || hero != flying => true,
            Some(_) if self.still => false,
            Some((last, _, _)) => busy || t - last >= 1.0 / IDLE_FPS - 0.002,
        };
        if !self.still {
            let next = if busy { 0.0 } else { 1.0 / IDLE_FPS };
            tx.wake_after(Duration::from_secs_f64(next));
        }
        if due {
            let (side, rgba) = self.render(scale, &self.theme.palette());
            let pixels = if flying { &self.hero } else { &self.pixels };
            pixels.set(side, side, rgba);
            self.drawn = Some((t, scale, flying));
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
            // Idle twinkles wait until it's home.
            self.next_twinkle = t + DELAY + LAND as f64;
            self.mood = Mood::Idle;
            self.mood_from = Stance::default();
            self.mood_since = f64::NEG_INFINITY;
        }
        if signals.theme != self.theme {
            // Draw afresh in the new theme; its sparkles would look wrong in it.
            self.theme = signals.theme;
            self.drawn = None;
            self.motes.clear();
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
            if self.began.is_some_and(|b| t - b > (LAND + 0.5) as f64) && !self.still {
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
            self.center[0] + self.tip[0] * self.zoom,
            self.center[1] + self.tip[1] * self.zoom,
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
        (self.center, self.zoom) = match self.opening() {
            Some(s) => placement(s),
            None => (super::WAND_CENTER, 1.0),
        };

        // Sparkles: gestures' puffs and idle twinkles.
        let colors = sparkle_colors(&self.theme.palette());
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
        if t >= self.next_twinkle && self.began.is_none_or(|b| t >= b) {
            let lively = self.mood != Mood::Fizzled;
            self.motes
                .push(self.spawner.twinkle(t, tip_world, lively, &colors));
            // Sigil's dust drifts off the tip more rarely than Arcane's twinkles.
            let rarer = if self.theme == Theme::Sigil { 2.2 } else { 1.0 };
            self.next_twinkle = t + rarer
                * match self.mood {
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
        let rock = ROCK * (TAU * (s - LAND) / ROCK_PERIOD).sin() * smoothstep(LAND, LAND + 0.8, s);
        // The tip star: a glow that blooms as the wand comes in pointing at you, which the bar
        // grows out of; it cools as the wand flies, flashes as it lands, then breathes.
        let star = if raw < 0.0 {
            0.0
        } else if s < CAST {
            lerp(0.0, 2.4, ease_out(s / CAST))
        } else if s < LAND {
            lerp(2.4, 1.0, ease_in_out((s - CAST) / (LAND - CAST)))
        } else {
            1.0 + 0.4 * flare(s - LAND) + (breathe(s) - 1.0) * smoothstep(LAND, LAND + 0.5, s)
        };
        Stance {
            star: star - 1.0,
            // The orbiting stars spiral out from the shaft as it comes in.
            reach: ease_out(s / 0.3) - 1.0,
            turn: rock,
            ..opening_stance(s)
        }
    }

    // MARK: Rendering

    /// Everything to draw over the model, in the theme's look.
    fn marks(&self, p: &Palette) -> Vec<Mark> {
        match p.theme {
            Theme::Arcane => self.stars(p).into_iter().map(Mark::Star).collect(),
            Theme::Sigil => self.sigil_marks(p),
        }
    }

    /// Sigil: dust where Arcane has sparkles, a slow ring of runes where it has orbiting stars,
    /// and at the tip a pale point of light in a thin halo.
    fn sigil_marks(&self, p: &Palette) -> Vec<Mark> {
        let pose = &self.pose;
        let st = self.stance;
        let t = self.now;
        let mut marks: Vec<Mark> = self
            .motes
            .iter()
            .filter_map(|m| m.star(t))
            .map(|s| Mark::Dot {
                at: s.at,
                radius: s.radius * 0.32,
                glow: 2.6,
                color: s.color,
                alpha: s.alpha * 0.85,
            })
            .collect();

        let reach = (1.0 + st.reach).max(0.0);
        let glow = (1.0 + st.glow).clamp(0.0, 1.5);
        if reach > 0.01 {
            let middle = pose.place([0.0; 3]);
            let r = 1.5 * reach * pose.scale;
            for i in 0..RING_RUNES {
                // A fifth of the stars' pace: the ring turns once every eighteen seconds.
                let a = self.orbit * TAU / 5.0 + i as f32 / RING_RUNES as f32 * TAU;
                let (sin, cos) = a.sin_cos();
                let front = sin > 0.0;
                marks.push(Mark::Rune {
                    at: add(middle, [r * cos, -0.42 * r * sin, 0.7 * r * sin]),
                    index: i * 3 + 1,
                    height: 0.42 * pose.scale.min(1.4),
                    angle: a + FRAC_PI_2,
                    color: rgb(p.glow),
                    alpha: (if front { 0.45 } else { 0.15 }) * glow * reach.min(1.0),
                });
            }
        }

        let star = (1.0 + st.star).max(0.0);
        if star > 0.01 {
            let at = add(pose.place(TIP), [0.0, 0.0, 0.25]);
            marks.push(Mark::Dot {
                at,
                radius: 0.065 * star.sqrt() * pose.scale.min(1.0),
                glow: 2.5 + star,
                color: rgb(srgb(0.97, 0.94, 0.86, 1.0)),
                alpha: 1.0,
            });
            marks.push(Mark::Ring {
                at,
                radius: 0.2 * (0.6 + 0.4 * star.min(3.0)) * pose.scale.min(1.0),
                width: 0.7,
                color: rgb(p.glow),
                alpha: (0.16 + 0.12 * star.min(2.5)).min(0.55),
            });
        }
        marks
    }

    /// Arcane: motes, the orbiting stars, the tip's own star.
    fn stars(&self, p: &Palette) -> Vec<Star> {
        let pose = &self.pose;
        let st = self.stance;
        let t = self.now;
        let mut stars: Vec<Star> = self.motes.iter().filter_map(|m| m.star(t)).collect();

        let reach = (1.0 + st.reach).max(0.0);
        let glow = (1.0 + st.glow).clamp(0.0, 1.5);
        if reach > 0.01 {
            let colors = [rgb(p.amber), [1.0; 3], rgb(p.glow.mix(Color::WHITE, 0.45))];
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
                color: rgb(p.glow.mix(Color::WHITE, 0.6)),
                alpha: 1.0,
            });
        }
        stars
    }

    /// The image as posed: its side in device pixels, and straight-alpha sRGB bytes. Flying,
    /// it's the large image, with the wand at its size in flight.
    fn render(&self, scale: f32, p: &Palette) -> (u32, Vec<u8>) {
        let size = if self.flying() { HERO_CANVAS } else { CANVAS };
        render(&self.pose, self.marks(p), size, scale, self.zoom, p)
    }
}

/// How the wand is turned `s` seconds into the opening. It comes in pointing at you (`AIM`),
/// so it's mostly its glowing tip, and holds that as it casts the bar. Then it turns to point
/// along its way home and flies tip first; slowing as it arrives, it swings into its resting
/// diagonal, a little past and back, like a spring.
fn opening_stance(s: f32) -> Stance {
    // Growing in as it comes.
    let grow = lerp(0.6, 1.0, ease_out(s / CAST)) - 1.0;
    let heading = heading();
    // Facing you → pointing along the way home.
    let k = ease_in_out((s - CAST) / (TURN_AT - CAST));
    let (mut swing, pitch) = (lerp(AIM.0, heading, k), lerp(AIM.1, 0.0, k));
    // Arriving: a damped swing into rest.
    let u = s - SETTLE_AT;
    if u > 0.0 {
        let ease = smoothstep(0.0, 0.06, u);
        let spring = heading * (-9.0 * u).exp() * (13.0 * u).cos();
        swing = lerp(heading, spring, ease);
    }
    // A little bank as it travels.
    let flight = ((s - FLY) / (LAND - FLY)).clamp(0.0, 1.0);
    Stance {
        swing,
        pitch,
        grow,
        turn: 0.35 * (std::f32::consts::PI * flight).sin(),
        ..Stance::default()
    }
}

/// Pointing at you: (swing, pitch) that turn the tip toward the viewer, the shaft
/// foreshortened behind it but not quite end-on, so it still reads as a wand.
const AIM: (f32, f32) = (FRAC_PI_4, 1.25 - TILT);
/// It has turned to its heading by here, and starts swinging into rest here.
const TURN_AT: f32 = CAST + 0.12;
const SETTLE_AT: f32 = LAND - 0.14;

/// The swing that points the tip from `HERO` toward its slot, as drawn (y up, the shaft leaning
/// out by `TILT`).
fn heading() -> f32 {
    let home = super::WAND_CENTER;
    let (dx, dy) = (home[0] - HERO[0], HERO[1] - home[1]);
    (-dx).atan2(dy / TILT.cos()) + FRAC_PI_4
}

/// Where the wand's canvas is centred (window points) and how large the wand is, `s` seconds
/// into the opening: at `HERO` until `FLY`, then straight along the vector to its slot,
/// shrinking as it goes.
fn placement(s: f32) -> ([f32; 2], f32) {
    let home = super::WAND_CENTER;
    let k = ease_in_out(((s - FLY) / (LAND - FLY)).clamp(0.0, 1.0));
    (
        [lerp(HERO[0], home[0], k), lerp(HERO[1], home[1], k)],
        lerp(HERO_ZOOM, 1.0, k),
    )
}

/// The tip as the wand casts the bar (pointing at you), in points from the canvas's centre at
/// the slot's size.
const CAST_TIP: [f32; 2] = [-0.85, -4.3];

/// Where the tip's glow is as the wand casts the bar, in window points: the bar grows from here.
pub fn hero_tip() -> [f32; 2] {
    [
        HERO[0] + CAST_TIP[0] * HERO_ZOOM,
        HERO[1] + CAST_TIP[1] * HERO_ZOOM,
    ]
}

impl Wand {
    /// The tip as it casts the bar, measured from the model (`CAST_TIP` keeps it as a
    /// constant).
    #[cfg(test)]
    fn cast_tip() -> [f32; 2] {
        let w = Pose::new(opening_stance(CAST), 0.0).place(TIP);
        let z = DISTANCE - w[2];
        let focal = PT_PER_UNIT * DISTANCE;
        [focal * w[0] / z, -focal * w[1] / z]
    }
}

fn sparkle_colors(p: &Palette) -> [V3; 4] {
    match p.theme {
        Theme::Arcane => [
            [1.0; 3],
            rgb(p.glow.mix(Color::WHITE, 0.45)),
            rgb(p.glow2.mix(Color::WHITE, 0.25)),
            rgb(p.amber),
        ],
        Theme::Sigil => [
            rgb(p.text),
            rgb(p.glow),
            rgb(p.glow.mix(Color::WHITE, 0.4)),
            rgb(p.glow2.mix(Color::WHITE, 0.3)),
        ],
    }
}

/// Runes in the ring Sigil's wand wears.
const RING_RUNES: usize = 6;

/// What's drawn over the model.
enum Mark {
    /// A four-point star (Arcane's sparkles, orbiters and tip).
    Star(Star),
    /// A soft point of light, `radius` world units across its core, with a halo `glow` times
    /// as wide.
    Dot {
        at: V3,
        radius: f32,
        glow: f32,
        color: V3,
        alpha: f32,
    },
    /// A thin ring round `at`, `radius` world units, `width` points wide.
    Ring {
        at: V3,
        radius: f32,
        width: f32,
        color: V3,
        alpha: f32,
    },
    /// A rune, `height` world units tall, turned by `angle` radians.
    Rune {
        at: V3,
        index: usize,
        height: f32,
        angle: f32,
        color: V3,
        alpha: f32,
    },
}

impl Mark {
    fn at(&self) -> V3 {
        match self {
            Mark::Star(s) => s.at,
            Mark::Dot { at, .. } | Mark::Ring { at, .. } | Mark::Rune { at, .. } => *at,
        }
    }
}

/// When Sigil's sigil has faded, in seconds into the opening.
const SIGIL_END: f32 = 0.75;

/// Sigil's opening: around the wand's glow, a sigil inscribes itself (two circles, two
/// triangles, a ring of ticks and an inner circle, each drawing on in turn) as the wand casts
/// the bar, then widens, turning, and fades as the bar swells out of it. Window points.
fn sigil(s: f32, p: &Palette) -> Option<El> {
    if s >= SIGIL_END {
        return None;
    }
    let [cx, cy] = hero_tip();
    let grow = 0.28;
    let (scale, turn, alpha) = if s < grow {
        let k = ease_out(s / grow);
        (lerp(0.7, 1.0, k), lerp(-0.35, 0.0, k), k)
    } else {
        let k = (s - grow) / (SIGIL_END - grow);
        let swell = if k < 0.3 {
            lerp(1.0, 1.3, ease_out(k / 0.3))
        } else {
            lerp(1.3, 3.2, ease_in_out((k - 0.3) / 0.7))
        };
        (swell, 0.32 * k, (1.0 - k).powi(2))
    };
    let at = |r: f32, a: f32| {
        let a = a + turn;
        [cx + scale * r * a.cos(), cy + scale * r * a.sin()]
    };
    let drawn = |i: usize| ease_in_out((s - i as f32 * 0.02) / 0.26);
    let gold = |a: f32| p.glow.alpha(0.85 * a * alpha);
    let circle = |r: f32, k: f32| -> Vec<[f32; 2]> {
        let n = (72.0 * k).ceil() as usize;
        (0..=n)
            .map(|j| at(r, -FRAC_PI_2 + TAU * k * j as f32 / n.max(1) as f32))
            .collect()
    };
    let polygon = |r: f32, start: f32, k: f32| -> Vec<[f32; 2]> {
        // Three corners, drawn on along its perimeter.
        let corners: Vec<[f32; 2]> = (0..=3)
            .map(|j| at(r, start + j as f32 * TAU / 3.0))
            .collect();
        let edges = 3.0 * k;
        let mut pts = vec![corners[0]];
        for j in 0..3 {
            let f = (edges - j as f32).clamp(0.0, 1.0);
            if f <= 0.0 {
                break;
            }
            let (a, b) = (corners[j], corners[j + 1]);
            pts.push([lerp(a[0], b[0], f), lerp(a[1], b[1], f)]);
        }
        pts
    };
    let mut shape = Shape::new();
    for (i, (r, a)) in [(100.0, 1.0), (88.0, 0.6), (30.0, 0.5)]
        .into_iter()
        .enumerate()
    {
        let k = drawn([0, 1, 5][i]);
        if k > 0.0 {
            shape = shape.stroke(circle(r, k), 1.0, gold(a));
        }
    }
    for (i, (start, a)) in [(-FRAC_PI_2, 0.7), (FRAC_PI_2, 0.45)]
        .into_iter()
        .enumerate()
    {
        let k = drawn(2 + i);
        if k > 0.0 {
            shape = shape.stroke(polygon(78.0, start, k), 1.0, gold(a));
        }
    }
    let ticks = drawn(4);
    for j in 0..12 {
        if (j as f32) < ticks * 12.0 {
            let a = j as f32 / 12.0 * TAU;
            shape = shape.line(
                at(92.0, a),
                at(100.0, a),
                1.0,
                rune::LineCap::Butt,
                gold(0.6),
            );
        }
    }
    Some(
        canvas(shape)
            .absolute(0.0, 0.0)
            .size(super::WINDOW_SIZE.0, super::WINDOW_SIZE.1),
    )
}

/// The model in `pose` with `marks` over it, `size` points square at `scale`, the wand `zoom`
/// times its size in the slot.
fn render(
    pose: &Pose,
    mut marks: Vec<Mark>,
    size: f32,
    scale: f32,
    zoom: f32,
    p: &Palette,
) -> (u32, Vec<u8>) {
    let side = ((size * scale).round() as usize).max(1);

    // The model, supersampled.
    let ss = side * SUPERSAMPLE;
    let focal = PT_PER_UNIT * DISTANCE * scale * zoom * SUPERSAMPLE as f32;
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

    // Marks over it, far ones first.
    let focal = focal / SUPERSAMPLE as f32;
    marks.sort_by(|a, b| a.at()[2].total_cmp(&b.at()[2]));
    for m in &marks {
        let at = project(m.at(), focal, side);
        let px = |world: f32| world * focal / at[2];
        match m {
            Mark::Star(s) => out.star(at, px(s.radius), s.angle, s.color, s.alpha),
            Mark::Dot {
                radius,
                glow,
                color,
                alpha,
                ..
            } => out.dot(at, px(*radius), *glow, *color, *alpha),
            Mark::Ring {
                radius,
                width,
                color,
                alpha,
                ..
            } => out.ring(at, px(*radius), width * scale, *color, *alpha),
            Mark::Rune {
                index,
                height,
                angle,
                color,
                alpha,
                ..
            } => {
                let lines = runes::strokes(*index, [at[0], at[1]], px(*height), *angle);
                out.strokes(at, &lines, 1.1 * scale, 1.6 * scale, *color, *alpha);
            }
        }
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
            // These cover the motion and Arcane's sparkles; Sigil's marks have their own test.
            theme: super::Theme::Arcane,
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
        wand.render(2.0, &super::Theme::Arcane.palette())
    }

    #[test]
    fn the_bar_grows_from_where_the_tip_is() {
        let tip = Wand::cast_tip();
        let kept = super::CAST_TIP;
        assert!(
            (tip[0] - kept[0]).abs() < 0.5 && (tip[1] - kept[1]).abs() < 0.5,
            "CAST_TIP is {kept:?}, the model's tip is {tip:?}"
        );
        // And that's where it is as it casts the bar.
        let mut wand = Wand::new();
        run(&mut wand, 0.0, super::CAST as f64, "");
        let t = wand.tip;
        assert!(
            (t[0] - tip[0]).abs() < 1.0 && (t[1] - tip[1]).abs() < 1.0,
            "{t:?} vs {tip:?}"
        );
    }

    #[test]
    fn it_comes_in_large_then_flies_home() {
        let mut wand = Wand::new();
        run(&mut wand, 0.0, 0.1, "");
        assert!(wand.flying());
        assert_eq!((wand.center, wand.zoom), (super::HERO, super::HERO_ZOOM));
        run(&mut wand, 0.1, 0.32, "");
        assert!(
            wand.flying() && wand.zoom < super::HERO_ZOOM && wand.zoom > 1.0,
            "shrinking"
        );
        run(&mut wand, 0.32, 0.6, "");
        assert!(!wand.flying());
        assert_eq!(
            (wand.center, wand.zoom),
            (super::super::WAND_CENTER, 1.0),
            "home, in about half a second"
        );
    }

    #[test]
    fn frames_repeat_for_the_same_cues() {
        let (mut a, mut b) = (Wand::new(), Wand::new());
        for w in [&mut a, &mut b] {
            run(w, 0.0, 1.4, "");
            w.cue(Gesture::Poke);
            run(w, 1.41, 1.8, "");
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
    fn it_starts_as_a_glow_pointing_at_you_then_aims_home() {
        let mut wand = Wand::new();
        run(&mut wand, 0.0, 0.0, "");
        let start = wand.stance.star;
        run(&mut wand, 0.0, super::CAST as f64, "");
        assert!(
            wand.stance.star > start + 1.5,
            "the glow blooms: {start} → {}",
            wand.stance.star
        );
        assert!(
            wand.stance.pitch > 0.8,
            "pointing at you: {:?}",
            wand.stance
        );
        run(
            &mut wand,
            super::CAST as f64,
            (super::TURN_AT + 0.02) as f64,
            "",
        );
        let heading = super::heading();
        assert!(
            (wand.stance.swing - heading).abs() < 0.05 && wand.stance.pitch.abs() < 0.05,
            "then along the way home: {:?}",
            wand.stance
        );
        // The tip leads: it points the way the wand is going.
        let (from, _) = super::placement(super::FLY);
        let (to, _) = super::placement(super::LAND);
        let tip = wand.tip;
        let way = [to[0] - from[0], to[1] - from[1]];
        let along = tip[0] * way[0] + tip[1] * way[1];
        assert!(along > 0.0, "tip {tip:?} leads along {way:?}");
        run(&mut wand, (super::TURN_AT + 0.02) as f64, 1.2, "");
        assert!(
            wand.stance.swing.abs() < 0.02,
            "and settles into rest: {:?}",
            wand.stance
        );
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
        let p = super::Theme::Arcane.palette();
        let mut wand = Wand::new();
        run(&mut wand, 0.0, 2.0, "");
        wand.edited("", "c", super::Theme::Arcane);
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
        wand.edited("", "c", super::Theme::Arcane);
        run(&mut wand, 0.0, 1.0, "c");
        assert!(wand.playing.is_empty() && !wand.spells.active() && wand.motes.is_empty());
    }
}
