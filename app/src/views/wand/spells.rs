//! The wand writing the query. Each typed character stays hidden while a spark flies from the
//! wand's tip to where it goes; it appears as the spark lands, glowing, and cools to the
//! text's colour. A deleted character's ghost fades where it was while its dust streams back
//! into the tip. All of it is a function of the frame clock, drawn by the view: the field's
//! per-frame [`Span`]s hide and colour the characters, and one [`Shape`] holds the sparks.

use rune::{Color, El, Shape, Span, TextStyle, Transition, canvas, el, text};

use super::super::palette::Palette;
use super::super::{FIELD_X, ROW_MID, WAND_CENTER, WINDOW_SIZE};
use super::motion::{ease_in_out, ease_out, lerp, smoothstep};
use super::sparkle::hash;

/// The search field's type, which the spells measure to find characters.
pub fn query_style() -> TextStyle {
    TextStyle::new(21.0, 400.0).lh(28.0).tracking(-0.2)
}

/// A character glows this long after it lands, cooling to the text's colour.
const HEAT: f32 = 0.5;
/// A deleted character's ghost fades over this long.
const FADE: f32 = 0.4;
/// When the query is emptied, the placeholder fades in over this window (seconds after the
/// deletion), as the deleted text's ghost fades out of the way.
const PLACEHOLDER_IN: (f32, f32) = (0.3, 0.55);
/// Seconds between the characters of one edit appearing (a paste writes itself out).
const STAGGER: f32 = 0.03;
const TRAIL: usize = 6;

/// Characters on their way: bytes `start..end` of the query.
#[derive(Clone, Debug)]
struct Glyphs {
    start: usize,
    end: usize,
    /// Frame time and wand tip (window points) when the spark left; `None` until the frame
    /// after the keystroke stamps it.
    born: Option<(f64, [f32; 2])>,
    flight: f32,
    seed: u32,
}

/// Characters deleted, fading where they were.
#[derive(Clone, Debug)]
struct Ghost {
    text: String,
    /// Where they started and how wide they were, in window points.
    x: f32,
    width: f32,
    born: Option<(f64, [f32; 2])>,
    seed: u32,
}

/// The wand's spells on the query, in flight.
#[derive(Default)]
pub struct Spells {
    glyphs: Vec<Glyphs>,
    ghosts: Vec<Ghost>,
    seed: u32,
    /// When a deletion emptied the query: the placeholder waits for the ghost.
    cleared: Option<f64>,
}

/// What the wand should do about an edit, once it's stamped.
pub enum Stamped {
    Typed,
    /// Deleted; the dust reaches the tip `delay` seconds later. `all`: the query is empty now.
    Erased {
        delay: f32,
        all: bool,
    },
}

impl Spells {
    /// The query went from `old` to `new`: what was typed flies in from the wand; what was
    /// deleted fades and flies back to it.
    pub fn edited(&mut self, old: &str, new: &str) {
        let (at, removed, inserted) = diff(old, new);
        if removed.is_empty() && inserted == 0 {
            return;
        }
        // Earlier spells after the edit move with the text; any it touched are done.
        let shift = |i: usize| (i as isize + inserted as isize - removed.len() as isize) as usize;
        self.glyphs.retain_mut(|g| {
            if g.end <= at {
                true
            } else if g.start >= at + removed.len() {
                (g.start, g.end) = (shift(g.start), shift(g.end));
                true
            } else {
                false
            }
        });
        self.seed = self.seed.wrapping_add(1);
        if !removed.is_empty() {
            let style = query_style();
            self.ghosts.push(Ghost {
                x: FIELD_X + width(&old[..at], style),
                width: width(removed, style),
                text: removed.to_string(),
                born: None,
                seed: self.seed,
            });
        }
        if inserted > 0 {
            self.glyphs.push(Glyphs {
                start: at,
                end: at + inserted,
                born: None,
                flight: 0.0,
                seed: self.seed,
            });
        }
    }

    /// Starts the spells cued since the last frame, from the wand's tip at `tip` (window
    /// points), and drops finished ones. Returns what each new one asks of the wand.
    pub fn stamp(&mut self, t: f64, tip: [f32; 2], query: &str) -> Vec<Stamped> {
        let mut cues = Vec::new();
        let style = query_style();
        for g in &mut self.glyphs {
            if g.born.is_none() {
                let to = glyph_center(query, g.start, g.end, style);
                g.flight = flight(tip, to);
                g.born = Some((t, tip));
                cues.push(Stamped::Typed);
            }
        }
        for g in &mut self.ghosts {
            if g.born.is_none() {
                let from = [g.x + g.width / 2.0, ROW_MID];
                g.born = Some((t, tip));
                if query.is_empty() {
                    self.cleared = Some(t);
                }
                cues.push(Stamped::Erased {
                    delay: flight(from, tip) + 0.04,
                    all: query.is_empty(),
                });
            }
        }
        self.glyphs.retain(|g| {
            g.born.is_none_or(|(b, _)| {
                ((t - b) as f32) < g.flight + stagger(g) * g.chars() as f32 + HEAT
            })
        });
        self.ghosts.retain(|g| {
            g.born
                .is_none_or(|(b, _)| ((t - b) as f32) < FADE.max(dust_end(g)))
        });
        if !query.is_empty()
            || self
                .cleared
                .is_some_and(|c| ((t - c) as f32) >= PLACEHOLDER_IN.1)
        {
            self.cleared = None;
        }
        cues
    }

    pub fn active(&self) -> bool {
        !self.glyphs.is_empty() || !self.ghosts.is_empty() || self.cleared.is_some()
    }

    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.ghosts.clear();
        self.cleared = None;
    }

    /// How much of the placeholder shows at `t` (0…1): none while deleted text is still
    /// fading where it would be, then fading in.
    pub fn placeholder(&self, t: f64) -> f32 {
        if self.ghosts.iter().any(|g| g.born.is_none()) {
            // Deleted since this frame's poll: its ghost hasn't even started.
            return 0.0;
        }
        match self.cleared {
            Some(c) => smoothstep(PLACEHOLDER_IN.0, PLACEHOLDER_IN.1, (t - c) as f32),
            None => 1.0,
        }
    }

    /// Colours for the field's characters at `t`: hidden until their spark lands, then hot,
    /// cooling to `p.text`.
    pub fn spans(&self, t: f64, query: &str, p: &Palette) -> Vec<Span> {
        let hot = Color::WHITE.mix(p.magenta, 0.4);
        let mut spans = Vec::new();
        for g in &self.glyphs {
            let Some(chars) = query.get(g.start..g.end) else {
                continue;
            };
            let age = g.born.map_or(0.0, |(b, _)| (t - b) as f32);
            for (k, (i, c)) in chars.char_indices().enumerate() {
                let landed = age - g.flight - k as f32 * stagger(g);
                // Not stamped yet (typed after this frame's poll): its spark hasn't left.
                let color = if g.born.is_none() || landed < 0.0 {
                    Color::TRANSPARENT
                } else {
                    let heat = 1.0 - (landed / HEAT).min(1.0);
                    p.text.mix(hot, heat * heat)
                };
                let at = g.start + i;
                spans.push(Span::new(at, at + c.len_utf8()).color(color));
            }
        }
        spans
    }

    /// The sparks, dust and ghosts at `t`, as a layer over the window.
    pub fn view(&self, t: f64, query: &str, p: &Palette) -> Option<El> {
        if !self.active() {
            return None;
        }
        let style = query_style();
        let mut shape = Shape::new();
        let mut layer = el()
            .key("spells")
            .a11y_hidden()
            .inert()
            .pin(Some(0.0), Some(0.0), None, None)
            .size(WINDOW_SIZE.0, WINDOW_SIZE.1)
            .raise()
            // Everything here moves every frame on its own clock: no entrances, no springs.
            .transition(Transition::none())
            .instant_changes();

        for g in &self.ghosts {
            // Unstamped (the edit came after this frame's poll), it shows as just deleted.
            let (born, tip) = g.born.unwrap_or((t, WAND_CENTER));
            let age = (t - born) as f32;
            // The ghost: the deleted text, paling, rising and fading from where it stood.
            let fade = 1.0 - (age / FADE).min(1.0);
            if fade > 0.0 {
                let color = p.text.mix(p.violet, 1.0 - fade).alpha(0.85 * fade * fade);
                let rise = 8.0 * ease_out(1.0 - fade);
                layer = layer.child(
                    el().absolute(g.x, ROW_MID - 14.0 - rise)
                        .child(text(g.text.clone()).font(style).color(color)),
                );
            }
            // Its dust, streaming back into the tip.
            for k in 0..dust_count(g) {
                let start = k as f32 * 0.025;
                let from = [
                    g.x + g.width * hash(g.seed, k * 3),
                    ROW_MID + (hash(g.seed, k * 3 + 1) - 0.5) * 14.0,
                ];
                let fly = flight(from, tip);
                let u = (age - start) / fly;
                if !(0.0..1.0).contains(&u) {
                    continue;
                }
                let lift = -(8.0 + 18.0 * hash(g.seed, k * 3 + 2));
                let at = arc(from, tip, lift, ease_in_out(u));
                let color = p.violet.mix(Color::WHITE, 0.35);
                shape = spark(shape, at, 1.4 + 0.8 * (1.0 - u), color, 0.9);
            }
        }

        for g in &self.glyphs {
            let Some((born, from)) = g.born else { continue };
            let age = (t - born) as f32;
            let to = glyph_center(query, g.start, g.end, style);
            let lift = -(16.0 + 0.06 * (to[0] - from[0]).abs()) * (0.8 + 0.4 * hash(g.seed, 9));
            let u = age / g.flight;
            if u < 1.0 {
                // The spark and its tail.
                for k in (0..TRAIL).rev() {
                    let back = u - k as f32 * 0.06;
                    if back < 0.0 {
                        continue;
                    }
                    let fade = 1.0 - k as f32 / TRAIL as f32;
                    let color = if k == 0 {
                        Color::WHITE
                    } else {
                        p.magenta.mix(p.violet, k as f32 / TRAIL as f32)
                    };
                    let at = arc(from, to, lift, smoothstep(0.0, 1.0, back));
                    shape = spark(shape, at, 0.6 + 1.6 * fade, color, fade);
                }
            } else {
                // It lands: a star pops just over the characters as they appear, and a ring
                // of motes flies out from them.
                let v = (age - g.flight) / 0.28;
                if v < 1.0 {
                    let color = Color::WHITE.mix(p.magenta, 0.3 * v);
                    shape = star(
                        shape,
                        [to[0] + 3.0, to[1] - 12.0],
                        lerp(2.5, 6.5, ease_out(v)),
                        v * 1.2,
                        color.alpha(1.0 - v),
                    );
                    for k in 0..4 {
                        let a =
                            std::f32::consts::FRAC_PI_4 + k as f32 * std::f32::consts::FRAC_PI_2;
                        let r = lerp(5.0, 17.0, ease_out(v));
                        let at = [to[0] + r * a.cos(), to[1] + r * a.sin()];
                        shape = spark(
                            shape,
                            at,
                            1.4 * (1.0 - v),
                            p.violet.mix(Color::WHITE, 0.5),
                            1.0 - v,
                        );
                    }
                }
            }
        }
        Some(
            layer.child(
                canvas(shape)
                    .absolute(0.0, 0.0)
                    .size(WINDOW_SIZE.0, WINDOW_SIZE.1),
            ),
        )
    }
}

impl Glyphs {
    fn chars(&self) -> usize {
        // Bytes bound it; good enough for a stagger's length.
        self.end - self.start
    }
}

fn stagger(g: &Glyphs) -> f32 {
    STAGGER.min(0.35 / g.chars().max(1) as f32)
}

fn dust_count(g: &Ghost) -> u32 {
    (3 + g.text.chars().count()).min(12) as u32
}

/// When the last of a ghost's dust reaches the tip, from its birth.
fn dust_end(g: &Ghost) -> f32 {
    let Some((_, tip)) = g.born else { return 0.0 };
    dust_count(g) as f32 * 0.025 + flight([g.x, ROW_MID], tip)
}

/// The middle of bytes `start..end` of `query`, in window points.
fn glyph_center(query: &str, start: usize, end: usize, style: TextStyle) -> [f32; 2] {
    let x = |i: usize| query.get(..i).map_or(0.0, |s| width(s, style));
    [FIELD_X + (x(start) + x(end)) / 2.0, ROW_MID]
}

/// How wide `text` sets in `style`, in points (0 for nothing: `advance` gives an empty line
/// some width).
fn width(text: &str, style: TextStyle) -> f32 {
    if text.is_empty() {
        0.0
    } else {
        rune::text::advance(text, style)
    }
}

/// How long a spark takes between two points: quick, a little longer for far ones.
fn flight(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    0.12 + d / 2600.0
}

/// A point `u` of the way along a quadratic arc from `a` to `b`, bowed by `lift` points.
fn arc(a: [f32; 2], b: [f32; 2], lift: f32, u: f32) -> [f32; 2] {
    let c = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0 + lift];
    let m = 1.0 - u;
    [
        m * m * a[0] + 2.0 * m * u * c[0] + u * u * b[0],
        m * m * a[1] + 2.0 * m * u * c[1] + u * u * b[1],
    ]
}

/// The bytes where `new` differs from `old`: the offset, what was removed from `old`, and
/// how many bytes of `new` replace it.
pub fn diff<'a>(old: &'a str, new: &str) -> (usize, &'a str, usize) {
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((i, _), _)| i);
    let (old_rest, new_rest) = (&old[prefix..], &new[prefix..]);
    let suffix = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    (
        prefix,
        &old_rest[..old_rest.len() - suffix],
        new_rest.len() - suffix,
    )
}

/// A glowing dot: a hot core in soft rings.
pub(super) fn spark(shape: Shape, at: [f32; 2], r: f32, color: Color, alpha: f32) -> Shape {
    if alpha <= 0.01 || r <= 0.05 {
        return shape;
    }
    shape
        .fill(circle(at, r * 2.6), color.alpha(0.10 * alpha))
        .fill(circle(at, r * 1.6), color.alpha(0.25 * alpha))
        .fill(circle(at, r), color.mix(Color::WHITE, 0.5).alpha(alpha))
}

/// A four-point star, `r` to its points, turned by `angle`.
fn star(shape: Shape, at: [f32; 2], r: f32, angle: f32, color: Color) -> Shape {
    let points: Vec<[f32; 2]> = (0..32)
        .map(|i| {
            // An astroid: x = cos³, y = sin³.
            let a = i as f32 / 32.0 * std::f32::consts::TAU;
            let (x, y) = (a.cos().powi(3), a.sin().powi(3));
            let (s, c) = angle.sin_cos();
            [at[0] + r * (x * c - y * s), at[1] + r * (x * s + y * c)]
        })
        .collect();
    shape
        .fill(circle(at, r * 0.55), color.alpha(color.a * 0.3))
        .fill(points, color)
}

fn circle(at: [f32; 2], r: f32) -> Vec<[f32; 2]> {
    (0..16)
        .map(|i| {
            let a = i as f32 / 16.0 * std::f32::consts::TAU;
            [at[0] + r * a.cos(), at[1] + r * a.sin()]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::diff;

    #[test]
    fn diff_finds_the_edit() {
        assert_eq!(diff("cal", "calc"), (3, "", 1));
        assert_eq!(diff("calc", "cal"), (3, "c", 0));
        assert_eq!(diff("cat", "cart"), (2, "", 1));
        assert_eq!(diff("hello", ""), (0, "hello", 0));
        assert_eq!(diff("aaa", "aaaa"), (3, "", 1));
        assert_eq!(diff("héllo", "hallo"), (1, "é", 1));
    }
}
