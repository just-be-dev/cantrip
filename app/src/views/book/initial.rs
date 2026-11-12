//! Illuminated initials, after initialis (github.com/azlen/initialis, MIT, by Azlen Elza): a
//! letter, its frame and ground, an ornament grown out of it, and two to four inks printed into
//! the page. This is a subset of that engine in Rust (the growth in `ornament.rs`, the field it
//! grows through in `field.rs`, leaves and flowers in `shapes.rs`, and the faces' capitals in
//! `caps.rs`), drawn on the CPU like the pages are.
//!
//! Each spell has an initial of its own, seeded by its id (`Style::of`), so it's the same every
//! time; the styles are the grimoire's own family (`Recipe`), in the theme's inks (`Inks`). An
//! initial is built once (`Scene`) and painted at any moment of its growing in
//! (`Scene::paint`), from the letter in its frame alone (0) to the ornament all grown (1).

mod caps;
mod field;
mod geom;
mod ornament;
mod shapes;

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::sync::{Arc, Mutex};

use tiny_skia::{FillRule, LineCap, LineJoin, Mask, Path, PathBuilder, Stroke, Transform};

use caps::{Face, caps};
use field::Field;
use geom::{Box2, Noise, P, Rng, hash, offset_polygon};
use ornament::{Items, Opts, Seed, Terminal, grow, sizes};
use shapes::{Detail, Flower, Leaf};

/// The initial's own units: it's 1000 across.
const W: f32 = 1000.0;

/// The inks an initial is printed in, as straight sRGB.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Inks {
    pub ink: [f32; 3],
    pub rubric: [f32; 3],
    pub gilt: [f32; 3],
    pub azure: [f32; 3],
}

impl Inks {
    fn of(&self, slot: Slot) -> Option<[f32; 3]> {
        match slot {
            Slot::Ink => Some(self.ink),
            Slot::Rubric => Some(self.rubric),
            Slot::Gilt => Some(self.gilt),
            Slot::Azure => Some(self.azure),
            Slot::Paper | Slot::None => None,
        }
    }

    fn key(&self) -> u64 {
        let mut h: u64 = 1469598103934665603;
        for v in [self.ink, self.rubric, self.gilt, self.azure].iter().flatten() {
            h ^= v.to_bits() as u64;
            h = h.wrapping_mul(1099511628211);
        }
        h
    }
}

/// What something is painted in. Paper knocks out what's under it, so the page shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    Ink,
    Rubric,
    Gilt,
    Azure,
    Paper,
    None,
}

impl Slot {
    fn channel(self) -> Option<usize> {
        match self {
            Slot::Ink => Some(0),
            Slot::Rubric => Some(1),
            Slot::Gilt => Some(2),
            Slot::Azure => Some(3),
            Slot::Paper | Slot::None => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Frame {
    None,
    Rect,
    Rounded,
    Arch,
    Quatrefoil,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pattern {
    None,
    Hatch,
    Diaper,
    Dots,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Seeds {
    /// Into the biggest openings.
    Gaps,
    /// From the field's corners, towards the middle.
    Corners(usize),
    /// Up from the foot, mirrored.
    Root,
}

/// The grimoire's family of initials. Each is a kind of book hand, after the old ones initialis
/// gathers, varied by each spell's seed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Recipe {
    /// A pale letter and vines cut out of a ground of solid ink: a woodcut.
    Woodcut,
    /// A gold letter on a patterned coloured ground, with gilt sprigs.
    Gilt,
    /// A coloured letter with no frame, and fine penwork in another colour round it.
    Penwork,
    /// A letter outlined in ink on the bare page, with scrolls mirrored either side.
    Engraved,
}

/// Every setting of an initial that the grimoire's recipes vary.
#[derive(Clone, Copy, Debug)]
struct Style {
    face: Face,
    size: f32,
    dy: f32,
    letter: Slot,
    embolden: f32,
    gap: f32,
    outline: f32,
    outline_slot: Slot,
    inline: f32,
    inline_off: f32,
    inline_slot: Slot,
    echo: usize,
    echo_gap: f32,
    echo_w: f32,
    echo_slot: Slot,
    frame: Frame,
    margin: f32,
    rules: usize,
    w1: f32,
    rule_gap: f32,
    w2: f32,
    rule_slot: Slot,
    ground: Slot,
    pattern: Pattern,
    pat_scale: f32,
    pat_angle: f32,
    pat_w: f32,
    pat_slot: Slot,
    orn: Opts,
    halo: Option<f32>,
    seeds: Seeds,
    mirror: bool,
    orn_fill: Slot,
    orn_stroke: Slot,
    orn_stroke_w: f32,
    detail_w: f32,
    detail_slot: Slot,
    edge: f32,
    edge_scale: f32,
    mottle: f32,
    speckle: f32,
}

impl Style {
    /// The initial of the spell `id`: always the same one.
    fn of(id: &str) -> Style {
        let mut r = Rng::new(hash(&["cantrip", id]));
        let recipe = r.pick(&[
            Recipe::Woodcut,
            Recipe::Woodcut,
            Recipe::Gilt,
            Recipe::Penwork,
            Recipe::Engraved,
        ]);
        Style::recipe(recipe, &mut r)
    }

    fn recipe(recipe: Recipe, r: &mut Rng) -> Style {
        let orn = Opts {
            margin: 10.0,
            stem_w: 16.0,
            taper: 0.4,
            curl: 0.75,
            turns: 1.1,
            undulate: 0.0,
            wander: 0.25,
            thin: 0.3,
            branch: 0.5,
            max_depth: 2,
            branch_angle: 0.7,
            length: 1.0,
            leaf: Leaf::Acanthus,
            leaf_size: 0.9,
            leaf_every: 0.55,
            leaf_w: 0.6,
            lobes: 4.0,
            lobe_depth: 0.42,
            leaf_curl: 0.5,
            leaf_angle: 0.8,
            leaf_outer: true,
            flower: Flower::Rosette,
            flower_chance: 0.2,
            flower_size: 1.0,
            petals: 6,
            berries: 0.4,
            berry_size: 1.4,
            dots: 0.2,
            beads: 0.0,
            terminal: Terminal::Leaf,
            density: 0.8,
            sprout: 0.85,
            mains: 3,
            fill_leaves: true,
            detail: Detail::Veins,
        };
        let base = Style {
            face: Face::Roman,
            size: 0.66,
            dy: 0.0,
            letter: Slot::Paper,
            embolden: 0.0,
            gap: 16.0,
            outline: 0.0,
            outline_slot: Slot::Ink,
            inline: 0.0,
            inline_off: 14.0,
            inline_slot: Slot::Ink,
            echo: 0,
            echo_gap: 18.0,
            echo_w: 7.0,
            echo_slot: Slot::Rubric,
            frame: Frame::Rect,
            margin: 22.0,
            rules: 1,
            w1: 16.0,
            rule_gap: 12.0,
            w2: 7.0,
            rule_slot: Slot::Ink,
            ground: Slot::Ink,
            pattern: Pattern::None,
            pat_scale: 34.0,
            pat_angle: 0.0,
            pat_w: 6.0,
            pat_slot: Slot::Paper,
            orn,
            halo: None,
            seeds: Seeds::Gaps,
            mirror: false,
            orn_fill: Slot::Paper,
            orn_stroke: Slot::None,
            orn_stroke_w: 6.0,
            detail_w: 5.0,
            detail_slot: Slot::Ink,
            edge: 0.25,
            edge_scale: 7.0,
            mottle: 0.12,
            speckle: 0.0,
        };
        let leaf = r.pick(&[Leaf::Acanthus, Leaf::Acanthus, Leaf::Trefoil, Leaf::Ivy, Leaf::Ovate]);
        let flower = r.pick(&[Flower::Rosette, Flower::Quatre, Flower::Bud, Flower::Daisy]);
        match recipe {
            Recipe::Woodcut => {
                let ground = r.pick(&[Slot::Ink, Slot::Rubric, Slot::Rubric, Slot::Azure]);
                Style {
                    face: r.pick(&[Face::Roman, Face::Garamond, Face::Uncial, Face::Fraktur]),
                    size: r.range(0.6, 0.7),
                    ground,
                    rule_slot: ground,
                    rules: r.pick(&[1, 2]),
                    frame: r.pick(&[Frame::Rect, Frame::Rect, Frame::Rounded]),
                    inline: if r.chance(0.5) { 6.0 } else { 0.0 },
                    inline_slot: ground,
                    seeds: if r.chance(0.4) { Seeds::Corners(2) } else { Seeds::Gaps },
                    detail_slot: ground,
                    speckle: r.range(0.0, 0.25),
                    orn: Opts {
                        leaf,
                        flower,
                        undulate: if r.chance(0.3) { 0.7 } else { 0.0 },
                        density: r.range(0.7, 0.85),
                        curl: r.range(0.65, 0.85),
                        stem_w: r.range(18.0, 24.0),
                        margin: 12.0,
                        leaf_size: 1.25,
                        flower_size: 1.2,
                        berry_size: 1.8,
                        dots: 0.1,
                        terminal: r.pick(&[Terminal::Leaf, Terminal::Flower, Terminal::Curl]),
                        ..base.orn
                    },
                    ..base
                }
            }
            Recipe::Gilt => {
                let ground = r.pick(&[Slot::Azure, Slot::Azure, Slot::Rubric]);
                Style {
                    face: r.pick(&[Face::Fraktur, Face::Fraktur, Face::Roman, Face::Lombardic]),
                    size: r.range(0.66, 0.74),
                    letter: Slot::Gilt,
                    outline: 5.0,
                    outline_slot: Slot::Ink,
                    gap: 14.0,
                    ground,
                    rules: 2,
                    w1: 16.0,
                    w2: 6.0,
                    rule_slot: Slot::Gilt,
                    frame: r.pick(&[Frame::Rect, Frame::Rect, Frame::Arch, Frame::Quatrefoil]),
                    // A fine pattern in ink, dark on the colour, under the gold.
                    pattern: r.pick(&[Pattern::None, Pattern::Diaper, Pattern::Hatch, Pattern::Dots]),
                    pat_slot: Slot::Ink,
                    pat_scale: r.range(36.0, 48.0),
                    pat_angle: r.pick(&[0.0, FRAC_PI_4]),
                    pat_w: 4.0,
                    orn_fill: Slot::Gilt,
                    detail_slot: ground,
                    detail_w: 4.0,
                    orn: Opts {
                        leaf: r.pick(&[Leaf::Trefoil, Leaf::Ivy, Leaf::Lance]),
                        flower: r.pick(&[Flower::Quatre, Flower::Rosette, Flower::Bud]),
                        density: r.range(0.65, 0.8),
                        stem_w: 17.0,
                        margin: 12.0,
                        leaf_size: 1.15,
                        berries: 0.4,
                        berry_size: 1.4,
                        dots: 0.1,
                        terminal: Terminal::Flower,
                        detail: Detail::Vein,
                        ..base.orn
                    },
                    edge: 0.12,
                    ..base
                }
            }
            Recipe::Penwork => {
                let (letter, pen) = if r.chance(0.5) {
                    (Slot::Azure, Slot::Rubric)
                } else {
                    (Slot::Rubric, Slot::Azure)
                };
                Style {
                    face: r.pick(&[Face::Lombardic, Face::Uncial, Face::Lombardic]),
                    size: r.range(0.7, 0.76),
                    letter,
                    gap: 24.0,
                    frame: Frame::None,
                    margin: 8.0,
                    ground: Slot::Paper,
                    echo: r.pick(&[1, 2]),
                    echo_slot: pen,
                    echo_gap: 18.0,
                    echo_w: 6.0,
                    halo: Some(r.range(110.0, 140.0)),
                    orn_fill: pen,
                    orn: Opts {
                        stem_w: 8.0,
                        taper: 0.2,
                        margin: 9.0,
                        curl: 0.95,
                        turns: 1.8,
                        branch: 0.35,
                        max_depth: 2,
                        length: 0.8,
                        leaf: Leaf::None,
                        flower: Flower::None,
                        beads: 0.55,
                        berries: 0.6,
                        berry_size: 0.9,
                        dots: 0.35,
                        terminal: Terminal::Ball,
                        density: r.range(0.45, 0.55),
                        sprout: 0.9,
                        mains: 4,
                        detail: Detail::None,
                        ..base.orn
                    },
                    edge: 0.15,
                    ..base
                }
            }
            Recipe::Engraved => {
                let filled = r.chance(0.5);
                Style {
                    face: r.pick(&[Face::Roman, Face::Garamond, Face::Roman]),
                    size: r.range(0.58, 0.64),
                    letter: if filled { Slot::Rubric } else { Slot::Paper },
                    outline: 9.0,
                    outline_slot: Slot::Ink,
                    inline: if filled { 0.0 } else { 6.0 },
                    inline_off: 14.0,
                    gap: 20.0,
                    ground: Slot::Paper,
                    rules: 1,
                    w1: 8.0,
                    rule_slot: Slot::Ink,
                    frame: r.pick(&[Frame::Rect, Frame::Arch]),
                    pattern: if r.chance(0.4) { Pattern::Hatch } else { Pattern::None },
                    pat_slot: Slot::Ink,
                    pat_scale: 16.0,
                    pat_w: 3.0,
                    seeds: Seeds::Root,
                    mirror: true,
                    orn_fill: Slot::Paper,
                    orn_stroke: Slot::Ink,
                    orn_stroke_w: 6.0,
                    detail_slot: Slot::Ink,
                    detail_w: 4.5,
                    orn: Opts {
                        leaf: r.pick(&[Leaf::Acanthus, Leaf::Acanthus, Leaf::Ivy]),
                        flower: r.pick(&[Flower::Rosette, Flower::Daisy, Flower::Bud]),
                        stem_w: 17.0,
                        margin: 15.0,
                        curl: 0.78,
                        turns: 1.35,
                        density: 0.6,
                        leaf_size: 1.15,
                        lobes: 3.0,
                        leaf_curl: 0.7,
                        flower_chance: 0.35,
                        petals: 5,
                        dots: 0.15,
                        terminal: Terminal::Flower,
                        mains: 1,
                        detail: Detail::Vein,
                        ..base.orn
                    },
                    edge: 0.2,
                    ..base
                }
            }
        }
    }
}

/// The frame's outline, in the box `x`, `y`, `w`, `h`.
fn frame_poly(kind: Frame, x: f32, y: f32, w: f32, h: f32) -> Vec<P> {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let mut pts = Vec::new();
    let arc = |pts: &mut Vec<P>, ax: f32, ay: f32, r: f32, a0: f32, a1: f32, n: usize| {
        for i in 0..=n {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            pts.push([ax + a.cos() * r, ay + a.sin() * r]);
        }
    };
    match kind {
        Frame::None | Frame::Rect => vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        Frame::Rounded => {
            let r = w.min(h) * 0.13;
            arc(&mut pts, x + w - r, y + r, r, -FRAC_PI_2, 0.0, 10);
            arc(&mut pts, x + w - r, y + h - r, r, 0.0, FRAC_PI_2, 10);
            arc(&mut pts, x + r, y + h - r, r, FRAC_PI_2, PI, 10);
            arc(&mut pts, x + r, y + r, r, PI, PI * 1.5, 10);
            pts
        }
        Frame::Arch => {
            let r = w / 2.0;
            pts.push([x + w, y + h]);
            pts.push([x, y + h]);
            arc(&mut pts, cx, y + r, r, PI, 2.0 * PI, 40);
            pts
        }
        Frame::Quatrefoil => {
            let (r, d) = (w * 0.27, w / 2.0 - w * 0.27);
            let m = 200;
            (0..m)
                .map(|i| {
                    let a = i as f32 / m as f32 * 2.0 * PI;
                    let (dx, dy) = (a.cos(), a.sin());
                    let mut best: f32 = 0.0;
                    for k in 0..4 {
                        let ca = k as f32 * FRAC_PI_2;
                        let (ox, oy) = (ca.cos() * d, ca.sin() * d);
                        let b = dx * ox + dy * oy;
                        let c = ox * ox + oy * oy - r * r;
                        let disc = b * b - c;
                        if disc >= 0.0 {
                            best = best.max(b + disc.sqrt());
                        }
                    }
                    [cx + dx * best, cy + dy * best * h / w]
                })
                .collect()
        }
    }
}

struct Rule {
    pts: Vec<P>,
    w: f32,
}

/// A built initial: its geometry, ready to paint at any moment of its growing in.
pub struct Scene {
    style: Style,
    seed: u32,
    letter: Vec<Vec<P>>,
    field: Vec<P>,
    rules: Vec<Rule>,
    orn: Items,
    /// When the ornament has all grown, in stem lengths.
    end: f32,
    grain: Mutex<HashMap<u32, Arc<Grain>>>,
    layers: Mutex<HashMap<u32, Arc<Layers>>>,
}

/// The parts of an initial that don't grow, at one size.
struct Layers {
    under: Vec<[f32; 4]>,
    /// Painted over a clear canvas, to lay over the ornament in one go.
    over: Canvas,
}

/// Rasterizes onto a grid of `n` × `m` cells, `cell` units each, as booleans.
fn grid_mask(nx: usize, ny: usize, cell: f32, draw: impl FnOnce(&mut Mask, Transform)) -> Vec<bool> {
    let mut m = Mask::new(nx as u32, ny as u32).expect("a grid");
    let t = Transform::from_row(1.0 / cell, 0.0, 0.0, 1.0 / cell, 0.5, 0.5);
    draw(&mut m, t);
    m.data().iter().map(|&v| v > 127).collect()
}

fn path_of(polys: &[Vec<P>], closed: bool) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for poly in polys {
        if poly.len() < 2 {
            continue;
        }
        pb.move_to(poly[0][0], poly[0][1]);
        for p in &poly[1..] {
            pb.line_to(p[0], p[1]);
        }
        if closed {
            pb.close();
        }
    }
    pb.finish()
}

fn dots_path(dots: impl IntoIterator<Item = [f32; 3]>, min_r: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for [x, y, r] in dots {
        pb.push_circle(x, y, r.max(min_r));
    }
    pb.finish()
}

fn stroked(path: &Path, w: f32) -> Option<Path> {
    path.stroke(
        &Stroke {
            width: w,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        },
        1.0,
    )
}

/// Lines across `box` at `spacing`, turned to `angle` about its middle.
fn hatch(b: Box2, spacing: f32, angle: f32) -> Vec<Vec<P>> {
    let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    let r = b.w().hypot(b.h()) / 2.0 + spacing;
    let (c, s) = (angle.cos(), angle.sin());
    let mut out = Vec::new();
    let mut v = -r;
    while v <= r {
        let p = |u: f32| [cx + c * u - s * v, cy + s * u + c * v];
        out.push(vec![p(-r), p(r)]);
        v += spacing;
    }
    out
}

impl Scene {
    /// The initial `letter` of the spell `id`.
    pub fn build(letter: char, id: &str) -> Scene {
        Scene::of(letter, Style::of(id), hash(&["cantrip", id, "growth"]))
    }

    fn of(letter: char, style: Style, seed: u32) -> Scene {
        let st = style;
        let m = st.margin;
        let base = frame_poly(st.frame, m, m, W - 2.0 * m, W - 2.0 * m);
        let mut rules = Vec::new();
        let mut inset = 0.0;
        let n = if st.frame == Frame::None { 0 } else { st.rules };
        for i in 0..n {
            let rw = if i == 0 { st.w1 } else { st.w2 };
            if i > 0 {
                inset += st.rule_gap;
            }
            if rw > 0.0 {
                rules.push(Rule {
                    pts: offset_polygon(&base, inset + rw / 2.0),
                    w: rw,
                });
            }
            inset += rw;
        }
        let field = if inset > 0.0 {
            offset_polygon(&base, inset)
        } else {
            base.clone()
        };
        let fb = Box2::of([&field]);

        // The letter, centred in the field.
        let face = caps(st.face);
        let glyph = face
            .letter(letter)
            .or_else(|| caps(Face::Roman).letter(letter))
            .cloned()
            .unwrap_or_default();
        let contours = if glyph.is_empty() {
            Vec::new()
        } else {
            let b = Box2::of(&glyph);
            let s = (st.size * fb.h() / face.cap_height.max(b.h() * 0.6))
                .min(st.size * 1.15 * fb.w() / b.w())
                .min(1.6 * fb.h() / b.h());
            let (cx, cy) = ((fb.x0 + fb.x1) / 2.0, (fb.y0 + fb.y1) / 2.0 + st.dy * fb.h());
            let (bx, by) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
            glyph
                .iter()
                .map(|c| c.iter().map(|p| [cx + (p[0] - bx) * s, cy + (p[1] - by) * s]).collect())
                .collect()
        };

        // Where the ornament may grow: inside the frame, kept off the letter.
        let o = st.orn;
        let cell = 5.0;
        let mut f = Field::new(W, W, cell, sizes(&o).cap);
        let (nx, ny) = f.dims();
        let letter_path = path_of(&contours, true);
        let inside = grid_mask(nx, ny, cell, |mk, t| {
            if let Some(p) = path_of(std::slice::from_ref(&field), true) {
                mk.fill_path(&p, FillRule::Winding, true, t);
            }
        });
        let keep_out = grid_mask(nx, ny, cell, |mk, t| {
            if let Some(p) = &letter_path {
                mk.fill_path(p, FillRule::Winding, true, t);
                if let Some(s) = stroked(p, 2.0 * (st.gap + st.embolden.max(0.0))) {
                    mk.fill_path(&s, FillRule::Winding, true, t);
                }
            }
        });
        let mut allowed: Vec<bool> = inside.iter().zip(&keep_out).map(|(a, k)| *a && !k).collect();
        if let (Some(halo), Some(p)) = (st.halo, &letter_path) {
            let near = grid_mask(nx, ny, cell, |mk, t| {
                mk.fill_path(p, FillRule::Winding, true, t);
                if let Some(s) = stroked(p, 2.0 * halo) {
                    mk.fill_path(&s, FillRule::Winding, true, t);
                }
            });
            for (a, n) in allowed.iter_mut().zip(&near) {
                *a = *a && *n;
            }
        }
        if st.mirror {
            // A wall down the middle; the other half is this one's reflection.
            let wall = W / 2.0 + o.margin / 2.0;
            let full = allowed.clone();
            for j in 0..ny {
                for i in 0..nx {
                    let k = j * nx + i;
                    let mi = ((W - i as f32 * cell) / cell).round() as isize;
                    if i as f32 * cell >= wall || mi < 0 || mi as usize >= nx || !full[j * nx + mi as usize] {
                        allowed[k] = false;
                    }
                }
            }
        }
        let blocked: Vec<bool> = allowed.iter().map(|a| !a).collect();
        f.set_blocked(&blocked);

        let inset = o.margin + o.stem_w + 4.0;
        let (x0, x1, y0, y1) = (fb.x0 + inset, fb.x1 - inset, fb.y0 + inset, fb.y1 - inset);
        let (cx, cy) = ((fb.x0 + fb.x1) / 2.0, (fb.y0 + fb.y1) / 2.0);
        let mut r = Rng::new(seed ^ 0x51ed);
        let towards = |x: f32, y: f32| (cy - y).atan2(cx - x);
        let seeds: Vec<Seed> = match st.seeds {
            Seeds::Gaps => Vec::new(),
            Seeds::Corners(n) => [[x0, y1], [x1, y1], [x0, y0], [x1, y0]]
                .iter()
                .take(n.max(1))
                .map(|&[x, y]| Seed {
                    x,
                    y,
                    dir: towards(x, y) + (r.next() - 0.5) * 0.6,
                    grace: None,
                })
                .collect(),
            Seeds::Root => vec![Seed {
                x: W / 2.0 - o.margin - o.stem_w,
                y: y1,
                dir: -FRAC_PI_2 - 0.25,
                grace: Some(0.0),
            }],
        };
        let noise = Noise::new(seed ^ 0x5bd1_e995);
        let seeds: Vec<Seed> = seeds
            .into_iter()
            .filter(|s| f.at(s.x, s.y) > o.margin * 0.5)
            .collect();
        let mut orn = grow(&mut f, o, seed, &noise, &seeds);
        if st.mirror {
            orn = orn.mirrored(W);
        }
        let end = orn.end();
        Scene {
            style: st,
            seed,
            letter: contours,
            field,
            rules,
            orn,
            end,
            grain: Mutex::new(HashMap::new()),
            layers: Mutex::new(HashMap::new()),
        }
    }

    /// The initial `t` of the way through growing in (0: the letter in its frame alone; 1: all
    /// of it), `px` pixels square, in `inks`: straight-alpha sRGB, rows top first.
    pub fn paint(&self, t: f32, px: u32, inks: &Inks) -> Vec<u8> {
        // Painted at twice the size and printed down, so the inks' edges are smooth.
        let n = px * 2;
        let layers = self.layers(n);
        let mut c = Canvas::new(n);
        c.cov.clone_from(&layers.under);
        let st = &self.style;
        let tf = Transform::from_scale(n as f32 / W, n as f32 / W);
        let min_line = 1.1 * W / n as f32;

        // The ornament as it stands.
        let items = self.orn.at(t.clamp(0.0, 1.0) * self.end);
        let mut polys: Vec<Vec<P>> = items.stems.iter().map(|s| s.outline(min_line)).collect();
        for s in items.leaves.iter().chain(&items.flowers) {
            polys.extend(s.polys.iter().cloned());
        }
        let dots: Vec<[f32; 3]> = items
            .dots
            .iter()
            .chain(&items.beads)
            .map(|d| [d[0], d[1], d[2]])
            .collect();
        let mut m = c.fill(path_of(&polys, true), tf);
        union(&mut m, &c.fill(dots_path(dots.iter().copied(), min_line / 2.0), tf));
        if st.orn_stroke != Slot::None {
            // Outlined: the shapes grown by the line's width, under them. (Stroking every
            // outline, every frame, would cost several times as much.)
            let out = dilate(&m, st.orn_stroke_w * n as f32 / W);
            c.put(&out, st.orn_stroke);
        }
        c.put(&m, st.orn_fill);
        if st.orn.detail != Detail::None {
            let mut lines: Vec<Vec<P>> = items.stems.iter().filter_map(|s| s.vein()).collect();
            for s in items.leaves.iter().chain(&items.flowers) {
                lines.extend(s.lines.iter().filter(|l| l.len() > 1).cloned());
            }
            let mut m = c.stroke(path_of(&lines, false), st.detail_w.max(min_line), tf);
            union(
                &mut m,
                &c.fill(dots_path(items.flowers.iter().flat_map(|f| f.dots.iter().copied()), min_line / 2.0), tf),
            );
            c.put(&m, st.detail_slot);
        }

        c.apply(&layers.over);
        let grain = self.grain(n);
        c.print(st, &grain, inks, px)
    }

    /// What doesn't grow, painted once at each size: the ground and its pattern under the
    /// ornament, and the letter and the rules over it.
    fn layers(&self, n: u32) -> Arc<Layers> {
        if let Some(l) = self.layers.lock().unwrap().get(&n) {
            return l.clone();
        }
        let mut c = Canvas::new(n);
        let st = &self.style;
        let tf = Transform::from_scale(n as f32 / W, n as f32 / W);
        let min_line = 1.1 * W / n as f32;

        // The field and its ground.
        let field_mask = c.fill(path_of(std::slice::from_ref(&self.field), true), tf);
        if st.frame != Frame::None {
            c.put(&field_mask, st.ground);
            if st.pattern != Pattern::None {
                let b = Box2::of([&self.field]);
                let mut p = match st.pattern {
                    Pattern::Hatch => c.stroke(path_of(&hatch(b, st.pat_scale, st.pat_angle), false), st.pat_w, tf),
                    Pattern::Diaper => {
                        let lines: Vec<_> = hatch(b, st.pat_scale, st.pat_angle + FRAC_PI_4)
                            .into_iter()
                            .chain(hatch(b, st.pat_scale, st.pat_angle - FRAC_PI_4))
                            .collect();
                        let mut m = c.stroke(path_of(&lines, false), st.pat_w, tf);
                        let d = st.pat_scale / std::f32::consts::SQRT_2 * 2.0;
                        let dots = lattice(b, d, st.pat_angle, false).map(|[x, y]| [x, y + d / 2.0, st.pat_w * 1.4]);
                        union(&mut m, &c.fill(dots_path(dots, 0.0), tf));
                        m
                    }
                    Pattern::Dots => {
                        let dots = lattice(b, st.pat_scale, st.pat_angle, true).map(|[x, y]| [x, y, st.pat_w * 0.9]);
                        c.fill(dots_path(dots, 0.0), tf)
                    }
                    Pattern::None => unreachable!(),
                };
                intersect(&mut p, &field_mask);
                c.put(&p, st.pat_slot);
            }
        }

        let under = std::mem::take(&mut c.cov);
        let mut c = Canvas::new(n);
        // The letter.
        if let Some(lp) = path_of(&self.letter, true) {
            let e = st.embolden;
            for i in 1..=st.echo {
                let d = e + st.echo_gap * i as f32;
                let band = c.band(&lp, -d - st.echo_w / 2.0, -d + st.echo_w / 2.0, tf);
                c.put(&band, st.echo_slot);
            }
            let body = c.region(&lp, -e, tf);
            c.put(&body, st.letter);
            if st.inline > 0.0 {
                let band = c.band(&lp, -e + st.inline_off, -e + st.inline_off + st.inline, tf);
                c.put(&band, st.inline_slot);
            }
            if st.outline > 0.0 {
                let band = c.band(&lp, -e - st.outline / 2.0, -e + st.outline / 2.0, tf);
                c.put(&band, st.outline_slot);
            }
        }

        // Rules on top.
        for rule in &self.rules {
            let m = c.stroke(path_of(std::slice::from_ref(&rule.pts), true), rule.w.max(min_line), tf);
            c.put(&m, st.rule_slot);
        }

        let layers = Arc::new(Layers { under, over: c });
        self.layers.lock().unwrap().insert(n, layers.clone());
        layers
    }

    fn grain(&self, n: u32) -> Arc<Grain> {
        let mut cache = self.grain.lock().unwrap();
        cache
            .entry(n)
            .or_insert_with(|| Arc::new(Grain::new(n, self.seed, &self.style)))
            .clone()
    }
}

/// Points of a (staggered) grid over `b`, `d` apart, turned to `angle`.
fn lattice(b: Box2, d: f32, angle: f32, stagger: bool) -> impl Iterator<Item = P> {
    let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    let r = b.w().hypot(b.h()) / 2.0 + d;
    let (c, s) = (angle.cos(), angle.sin());
    let rows = (2.0 * r / (d * if stagger { 0.87 } else { 1.0 })) as i32 + 1;
    let cols = (2.0 * r / d) as i32 + 1;
    (0..rows).flat_map(move |j| {
        let v = -r + j as f32 * d * if stagger { 0.87 } else { 1.0 };
        let off = if stagger && j % 2 == 1 { d / 2.0 } else { 0.0 };
        (0..cols).map(move |i| {
            let u = -r + off + i as f32 * d;
            [cx + c * u - s * v, cy + s * u + c * v]
        })
    })
}

/// `m` grown by `r` pixels (a square reach, softened at its edge).
fn dilate(m: &Mask, r: f32) -> Mask {
    let n = m.width() as usize;
    let reach = r.ceil() as usize;
    let frac = r - (reach as f32 - 1.0);
    let pass = |src: &[u8], step: usize, stride: usize| -> Vec<u8> {
        let mut out = vec![0u8; src.len()];
        for line in 0..n {
            for i in 0..n {
                let at = |j: usize| src[line * stride + j * step];
                let mut v = at(i) as f32;
                for d in 1..=reach {
                    let w = if d == reach { frac } else { 1.0 };
                    if i >= d {
                        v = v.max(at(i - d) as f32 * w);
                    }
                    if i + d < n {
                        v = v.max(at(i + d) as f32 * w);
                    }
                }
                out[line * stride + i * step] = v as u8;
            }
        }
        out
    };
    let h = pass(m.data(), 1, n);
    let v = pass(&h, n, 1);
    Mask::from_vec(v, tiny_skia::IntSize::from_wh(n as u32, n as u32).unwrap()).unwrap()
}

fn union(a: &mut Mask, b: &Mask) {
    for (x, y) in a.data_mut().iter_mut().zip(b.data()) {
        *x = (*x).max(*y);
    }
}

fn intersect(a: &mut Mask, b: &Mask) {
    for (x, y) in a.data_mut().iter_mut().zip(b.data()) {
        *x = ((*x as u16 * *y as u16) / 255) as u8;
    }
}

fn subtract(a: &mut Mask, b: &Mask) {
    for (x, y) in a.data_mut().iter_mut().zip(b.data()) {
        *x = ((*x as u16 * (255 - *y as u16)) / 255) as u8;
    }
}

/// Ink coverage, painted in order: each pixel how much of each ink is on it.
struct Canvas {
    n: u32,
    cov: Vec<[f32; 4]>,
    /// How much has been painted over each pixel, paper included.
    alpha: Vec<f32>,
}

impl Canvas {
    fn new(n: u32) -> Canvas {
        Canvas {
            n,
            cov: vec![[0.0; 4]; (n * n) as usize],
            alpha: vec![0.0; (n * n) as usize],
        }
    }

    fn blank(&self) -> Mask {
        Mask::new(self.n, self.n).expect("a mask")
    }

    fn fill(&self, path: Option<Path>, t: Transform) -> Mask {
        let mut m = self.blank();
        if let Some(p) = path {
            m.fill_path(&p, FillRule::Winding, true, t);
        }
        m
    }

    fn stroke(&self, path: Option<Path>, w: f32, t: Transform) -> Mask {
        self.fill(path.and_then(|p| stroked(&p, w)), t)
    }

    /// Where the letter's signed distance is at least `s` (inside positive): grown by a
    /// negative `s`, shrunk by a positive one.
    fn region(&self, p: &Path, s: f32, t: Transform) -> Mask {
        let mut m = self.fill(Some(p.clone()), t);
        if s < 0.0 {
            union(&mut m, &self.stroke(Some(p.clone()), -2.0 * s, t));
        } else if s > 0.0 {
            subtract(&mut m, &self.stroke(Some(p.clone()), 2.0 * s, t));
        }
        m
    }

    fn band(&self, p: &Path, lo: f32, hi: f32, t: Transform) -> Mask {
        let mut m = self.region(p, lo, t);
        subtract(&mut m, &self.region(p, hi, t));
        m
    }

    /// Paints `mask` in `slot`, over whatever is there.
    fn put(&mut self, mask: &Mask, slot: Slot) {
        if slot == Slot::None {
            return;
        }
        let ch = slot.channel();
        for ((c, al), &m) in self.cov.iter_mut().zip(&mut self.alpha).zip(mask.data()) {
            if m == 0 {
                continue;
            }
            let a = m as f32 / 255.0;
            for (i, v) in c.iter_mut().enumerate() {
                *v = *v * (1.0 - a) + if Some(i) == ch { a } else { 0.0 };
            }
            *al = *al * (1.0 - a) + a;
        }
    }

    /// Lays `over` (painted on a clear canvas) over this, as if painted here.
    fn apply(&mut self, over: &Canvas) {
        for ((c, o), &a) in self.cov.iter_mut().zip(&over.cov).zip(&over.alpha) {
            if a == 0.0 {
                continue;
            }
            for (v, ov) in c.iter_mut().zip(o) {
                *v = *v * (1.0 - a) + ov;
            }
        }
    }

    /// Prints the coverage into the page: ragged edges, mottled density, ink that didn't take,
    /// burnished gold, as initialis's ink pass does, then down to `px` square.
    fn print(&self, st: &Style, g: &Grain, inks: &Inks, px: u32) -> Vec<u8> {
        let n = self.n as usize;
        let colours = [
            inks.of(Slot::Ink).unwrap(),
            inks.of(Slot::Rubric).unwrap(),
            inks.of(Slot::Gilt).unwrap(),
            inks.of(Slot::Azure).unwrap(),
        ];
        // A light blur first, so the edge's noise has a ramp to bite into.
        let blur = |ch: usize| -> Vec<f32> {
            let mut h = vec![0.0f32; n * n];
            for y in 0..n {
                for x in 0..n {
                    let a = self.cov[y * n + x.saturating_sub(1)][ch];
                    let b = self.cov[y * n + x][ch];
                    let c = self.cov[y * n + (x + 1).min(n - 1)][ch];
                    h[y * n + x] = (a + 2.0 * b + c) / 4.0;
                }
            }
            let mut v = vec![0.0f32; n * n];
            for y in 0..n {
                for x in 0..n {
                    let a = h[y.saturating_sub(1) * n + x];
                    let b = h[y * n + x];
                    let c = h[(y + 1).min(n - 1) * n + x];
                    v[y * n + x] = (a + 2.0 * b + c) / 4.0;
                }
            }
            v
        };
        let chans: Vec<Vec<f32>> = (0..4)
            .map(|ch| {
                if self.cov.iter().any(|c| c[ch] > 0.0) {
                    blur(ch)
                } else {
                    Vec::new()
                }
            })
            .collect();
        let mut hi = vec![[0.0f32; 4]; n * n];
        for (i, out) in hi.iter_mut().enumerate() {
            let mut cov = [0.0f32; 4];
            for (ch, c) in chans.iter().enumerate() {
                if c.is_empty() {
                    continue;
                }
                let v = c[i];
                let t = 0.42 + g.edge[i] * st.edge;
                cov[ch] = smoothstep(t - 0.12, t + 0.12, v) * g.keep[i];
            }
            // The inks multiplied over white, as colour and alpha.
            let mut col = [1.0f32; 3];
            let mut clear = 1.0;
            for (ch, &a) in cov.iter().enumerate() {
                if a <= 0.0 {
                    continue;
                }
                let mut ink = colours[ch];
                if ch == 2 {
                    // Burnished: brighter where the light catches it.
                    let s = g.sheen[i];
                    ink = [
                        (ink[0] * s).min(1.0),
                        (ink[1] * s).min(1.0),
                        (ink[2] * s).min(1.0),
                    ];
                }
                for k in 0..3 {
                    col[k] *= 1.0 + (ink[k] - 1.0) * a;
                }
                clear *= 1.0 - a;
            }
            let al = 1.0 - clear;
            *out = if al > 0.002 {
                [
                    ((col[0] - clear) / al).clamp(0.0, 1.0),
                    ((col[1] - clear) / al).clamp(0.0, 1.0),
                    ((col[2] - clear) / al).clamp(0.0, 1.0),
                    al,
                ]
            } else {
                [0.0; 4]
            };
        }
        // Down to size: premultiplied averages of each 2 × 2.
        let px = px as usize;
        let mut rgba = vec![0u8; px * px * 4];
        for y in 0..px {
            for x in 0..px {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = hi[(y * 2 + dy) * n + x * 2 + dx];
                    acc[0] += p[0] * p[3];
                    acc[1] += p[1] * p[3];
                    acc[2] += p[2] * p[3];
                    acc[3] += p[3];
                }
                let o = (y * px + x) * 4;
                if acc[3] > 0.0 {
                    for k in 0..3 {
                        rgba[o + k] = (acc[k] / acc[3] * 255.0).round() as u8;
                    }
                    rgba[o + 3] = (acc[3] / 4.0 * 255.0).round() as u8;
                }
            }
        }
        rgba
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The paper and press under an initial at one size, worked out once: how the edge wavers,
/// where ink is thin or didn't take, and where the gold catches the light.
struct Grain {
    edge: Vec<f32>,
    keep: Vec<f32>,
    sheen: Vec<f32>,
}

impl Grain {
    fn new(n: u32, seed: u32, st: &Style) -> Grain {
        let noise = Noise::new(seed ^ 0x2545);
        let fbm = |x: f32, y: f32| {
            let (mut s, mut a, mut x, mut y) = (0.0, 0.5, x, y);
            for _ in 0..4 {
                s += a * (noise.at(x, y) * 0.5 + 0.5);
                x = x * 2.03 + 17.1;
                y = y * 2.03 + 17.1;
                a *= 0.5;
            }
            s
        };
        let n = n as usize;
        let k = W / n as f32;
        let mut edge = vec![0.0; n * n];
        let mut keep = vec![1.0; n * n];
        let mut sheen = vec![1.0; n * n];
        for y in 0..n {
            for x in 0..n {
                let (ux, uy) = (x as f32 * k, y as f32 * k);
                let i = y * n + x;
                edge[i] = (fbm(ux / st.edge_scale, uy / st.edge_scale) - 0.5) * 0.9;
                let mot = 1.0 - st.mottle * fbm(ux * 0.045, uy * 0.045);
                let sp = if st.speckle > 0.0 {
                    let v = noise.at(ux * 0.55 + 3.1, uy * 0.55) * 0.5 + 0.5;
                    smoothstep(1.0 - st.speckle * 0.35, 1.0 - st.speckle * 0.35 + 0.06, v)
                } else {
                    0.0
                };
                keep[i] = mot * (1.0 - sp);
                let diag = 1.0 - (ux + uy) / (2.0 * W);
                sheen[i] = 0.82 + 0.42 * diag + 0.25 * (fbm(ux * 0.012 + 40.0, uy * 0.012) - 0.5);
            }
        }
        Grain { edge, keep, sheen }
    }
}

/// Initials built so far, by spell id and letter.
type Built = HashMap<(String, char), Arc<Scene>>;

/// The book's initials, built and painted as they're wanted, and kept. They're built ahead on
/// a thread of their own (`prepare`), so leafing to one rarely waits for it.
#[derive(Default)]
pub struct Illuminations {
    built: Arc<Mutex<Built>>,
    asked: std::collections::HashSet<(String, char, u32)>,
    painted: HashMap<(String, char, u32, u64), (rune::Pixels, f32)>,
}

impl std::fmt::Debug for Illuminations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Illuminations({} painted)", self.painted.len())
    }
}

impl Illuminations {
    /// Builds the initials of these spells (id and letter), in order, `px` square, ahead of
    /// their being shown. Each is asked for once.
    pub fn prepare(&mut self, wants: impl IntoIterator<Item = (String, char)>, px: u32) {
        let new: Vec<(String, char)> = wants
            .into_iter()
            .filter(|(id, c)| self.asked.insert((id.clone(), *c, px)))
            .collect();
        if new.is_empty() {
            return;
        }
        let built = self.built.clone();
        std::thread::spawn(move || {
            for key in new {
                let have = built.lock().unwrap().get(&key).cloned();
                let scene = have.unwrap_or_else(|| {
                    let s = Arc::new(Scene::build(key.1, &key.0));
                    built.lock().unwrap().insert(key, s.clone());
                    s
                });
                scene.layers(px * 2);
                scene.grain(px * 2);
            }
        });
    }

    /// The initial `letter` of the spell `id`, `t` of the way grown in, `px` square: the
    /// same `Pixels` every time, repainted only when `t` has moved.
    pub fn pixels(&mut self, id: &str, letter: char, t: f32, px: u32, inks: &Inks) -> rune::Pixels {
        let t = t.clamp(0.0, 1.0);
        let key = (id.to_string(), letter, px, inks.key());
        let (pixels, at) = self
            .painted
            .entry(key)
            .or_insert_with(|| (rune::Pixels::new(), f32::NAN));
        if *at != t {
            let have = self
                .built
                .lock()
                .unwrap()
                .get(&(id.to_string(), letter))
                .cloned();
            let scene = have.unwrap_or_else(|| {
                let s = Arc::new(Scene::build(letter, id));
                self.built
                    .lock()
                    .unwrap()
                    .insert((id.to_string(), letter), s.clone());
                s
            });
            pixels.set(px, px, scene.paint(t, px, inks));
            *at = t;
        }
        pixels.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INKS: Inks = Inks {
        ink: [0.2, 0.12, 0.07],
        rubric: [0.58, 0.14, 0.09],
        gilt: [0.8, 0.62, 0.28],
        azure: [0.17, 0.27, 0.52],
    };

    #[test]
    fn every_face_has_every_capital() {
        for face in caps::FACES {
            for c in 'A'..='Z' {
                let g = caps(face).letter(c).unwrap_or_else(|| panic!("{face:?} {c}"));
                assert!(!g.is_empty() && g.iter().all(|c| c.len() > 2), "{face:?} {c}");
            }
        }
    }

    #[test]
    fn an_initial_grows_in_the_same_way_every_time() {
        let a = Scene::build('H', "spell:hello.ts:hello");
        let b = Scene::build('H', "spell:hello.ts:hello");
        assert!(a.end > 0.0 && !a.orn.stems.is_empty(), "it grows an ornament");
        assert_eq!(a.paint(0.5, 48, &INKS), b.paint(0.5, 48, &INKS));
        let bare = a.paint(0.0, 48, &INKS);
        let grown = a.paint(1.0, 48, &INKS);
        let inked = |p: &[u8]| p.chunks(4).filter(|p| p[3] > 128).count();
        assert!(inked(&bare) > 100, "the letter and its frame are there from the start");
        assert_ne!(bare, grown, "and the ornament grows in");
    }

    /// A sheet of initials on parchment, for looking at: `cargo test --release sheet -- --ignored`.
    #[test]
    #[ignore]
    fn sheet() {
        let ids = [
            "spell:arrange.ts:arrange", "spell:brew.ts:brew", "entry:claude-usage", "spell:clip.ts:history",
            "spell:uuid.ts:uuid", "spell:git.ts:branches", "spell:hello.ts:hello", "entry:insomnia",
            "spell:party.ts:party", "spell:sky.ts:sky", "spell:note.ts:note", "spell:timer.ts:timer",
        ];
        let px: u32 = std::env::var("PX").ok().and_then(|v| v.parse().ok()).unwrap_or(240);
        let cols = 6;
        let ts = [0.0f32, 0.5, 1.0];
        let rows = ids.len().div_ceil(cols) * ts.len();
        let (w, h) = (px * cols as u32, px * rows as u32);
        let mut sheet = tiny_skia::Pixmap::new(w, h).unwrap();
        sheet.fill(tiny_skia::Color::from_rgba8(232, 214, 176, 255));
        for (i, id) in ids.iter().enumerate() {
            let letter = id.rsplit(':').next().unwrap().chars().next().unwrap().to_ascii_uppercase();
            let began = std::time::Instant::now();
            let scene = Scene::build(letter, id);
            let built = began.elapsed();
            let began = std::time::Instant::now();
            let _ = scene.paint(0.4, 124, &INKS);
            let first = began.elapsed();
            let began = std::time::Instant::now();
            let _ = scene.paint(0.5, 124, &INKS);
            let _ = scene.paint(0.6, 124, &INKS);
            eprintln!("built in {built:?}, first painted in {first:?}, then in {:?}", began.elapsed() / 2);
            eprintln!("{id}: {:?}, {} stems", scene.style.face, scene.orn.stems.len());
            for (k, &t) in ts.iter().enumerate() {
                let rgba = scene.paint(t, px, &INKS);
                let (x0, y0) = ((i % cols) as u32 * px, ((i / cols) * ts.len() + k) as u32 * px);
                let data = sheet.data_mut();
                for y in 0..px {
                    for x in 0..px {
                        let s = ((y * px + x) * 4) as usize;
                        let d = (((y0 + y) * w + x0 + x) * 4) as usize;
                        let a = rgba[s + 3] as f32 / 255.0;
                        for c in 0..3 {
                            data[d + c] = (data[d + c] as f32 * (1.0 - a) + rgba[s + c] as f32 * a) as u8;
                        }
                    }
                }
            }
        }
        sheet.save_png(std::env::var("OUT").unwrap_or("/tmp/book/initials.png".into())).unwrap();
    }

    #[test]
    fn every_recipe_grows() {
        for (i, recipe) in [Recipe::Woodcut, Recipe::Gilt, Recipe::Penwork, Recipe::Engraved]
            .into_iter()
            .enumerate()
        {
            let style = Style::recipe(recipe, &mut Rng::new(7 + i as u32));
            let s = Scene::of('R', style, 11);
            assert!(!s.orn.stems.is_empty() || !s.orn.dots.is_empty(), "{recipe:?}");
        }
    }
}
