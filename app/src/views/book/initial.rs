//! Illuminated initials: a capital in its frame on a ground of ink, and around it a figure
//! inscribed stroke by stroke. One kind to each theme (`Kind`), and each spell's drawn its own
//! way from its id, so every page is a little different and always the same.
//!
//! The capitals' outlines, and the way the inks are printed into the page (ragged edges,
//! mottled density, burnished gold), come from initialis (github.com/azlen/initialis, MIT, by
//! Azlen Elza); the figures are the grimoire's own. An initial is built once (`Scene`) and
//! painted at any moment of its inscribing (`Scene::paint`), from the letter in its frame alone
//! (0) to the figure all drawn (1).

mod caps;
mod geom;

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::sync::{Arc, Mutex};

use tiny_skia::{FillRule, LineCap, LineJoin, Mask, Path, PathBuilder, Stroke, Transform};

use caps::{Face, caps};
use geom::{Box2, Noise, P, Rng, circle_poly, hash, offset_polygon};

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
            Slot::Paper => None,
        }
    }

    fn key(&self) -> u64 {
        let mut h: u64 = 1469598103934665603;
        for v in [self.ink, self.rubric, self.gilt, self.azure]
            .iter()
            .flatten()
        {
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
}

impl Slot {
    fn channel(self) -> Option<usize> {
        match self {
            Slot::Ink => Some(0),
            Slot::Rubric => Some(1),
            Slot::Gilt => Some(2),
            Slot::Azure => Some(3),
            Slot::Paper => None,
        }
    }
}

/// The grimoire's initials: one kind to each theme.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    /// A seal: a gold blackletter capital over a magic circle on ink, a band of runes round
    /// it, a star drawn through it, and sigils in the corners.
    Seal,
    /// A star chart: a gold Roman capital under an arch of night, with constellations, orbits, a
    /// crescent moon and stardust.
    Chart,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Frame {
    Rect,
    Arch,
}

/// How a kind of initial is set: its letter, frame and ground, and how it's printed.
#[derive(Clone, Copy, Debug)]
struct Style {
    face: Face,
    size: f32,
    letter: Slot,
    /// The ground kept clear round the letter, so the figure passes behind it.
    halo: f32,
    echo_gap: f32,
    echo_w: f32,
    echo_slot: Slot,
    frame: Frame,
    margin: f32,
    w1: f32,
    rule_gap: f32,
    w2: f32,
    rule_slot: Slot,
    ground: Slot,
    edge: f32,
    edge_scale: f32,
    mottle: f32,
    speckle: f32,
}

impl Style {
    fn of(kind: Kind) -> Style {
        match kind {
            Kind::Seal => Style {
                face: Face::Fraktur,
                size: 0.6,
                letter: Slot::Gilt,
                halo: 22.0,
                echo_gap: 34.0,
                echo_w: 8.0,
                echo_slot: Slot::Rubric,
                frame: Frame::Rect,
                margin: 18.0,
                w1: 18.0,
                rule_gap: 10.0,
                w2: 7.0,
                rule_slot: Slot::Gilt,
                ground: Slot::Ink,
                edge: 0.22,
                edge_scale: 7.0,
                mottle: 0.14,
                speckle: 0.12,
            },
            Kind::Chart => Style {
                face: Face::Roman,
                size: 0.52,
                letter: Slot::Gilt,
                halo: 22.0,
                echo_gap: 36.0,
                echo_w: 6.0,
                echo_slot: Slot::Paper,
                frame: Frame::Arch,
                margin: 18.0,
                w1: 16.0,
                rule_gap: 10.0,
                w2: 7.0,
                rule_slot: Slot::Gilt,
                ground: Slot::Azure,
                edge: 0.18,
                edge_scale: 7.0,
                mottle: 0.16,
                speckle: 0.08,
            },
        }
    }
}

/// The frame's outline, in the box `x`, `y`, `w`, `h`.
fn frame_poly(kind: Frame, x: f32, y: f32, w: f32, h: f32) -> Vec<P> {
    match kind {
        Frame::Rect => vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        Frame::Arch => {
            let r = w / 2.0;
            let mut pts = vec![[x + w, y + h], [x, y + h]];
            for i in 0..=40 {
                let a = PI + PI * i as f32 / 40.0;
                pts.push([x + r + a.cos() * r, y + r + a.sin() * r]);
            }
            pts
        }
    }
}

/// One stroke of the figure, drawn in over its own stretch of the inscribing.
#[derive(Clone, Debug)]
struct Mark {
    shape: Shape,
    slot: Slot,
    /// When it starts, and how long it takes, as parts of the whole.
    at: f32,
    dur: f32,
}

#[derive(Clone, Debug)]
enum Shape {
    /// A line drawn along its length, as a pen would.
    Line(Vec<P>, f32),
    /// A dot that swells into place.
    Dot(P, f32),
    /// A shape that's washed in.
    Fill(Vec<P>),
    /// A crescent: a disc (x, y, r) less one beside it, offset by (dx, dy) and `k` its size.
    Crescent([f32; 3], P, f32),
}

impl Mark {
    fn line(pts: Vec<P>, w: f32, slot: Slot, at: f32, dur: f32) -> Mark {
        Mark {
            shape: Shape::Line(pts, w),
            slot,
            at,
            dur,
        }
    }

    fn dot(p: P, r: f32, slot: Slot, at: f32, dur: f32) -> Mark {
        Mark {
            shape: Shape::Dot(p, r),
            slot,
            at,
            dur,
        }
    }

    /// How far it's drawn at `t`.
    fn progress(&self, t: f32) -> f32 {
        if t >= 1.0 {
            1.0
        } else {
            ((t - self.at) / self.dur.max(1e-3)).clamp(0.0, 1.0)
        }
    }
}

/// The start of `pts`, `u` of its length.
fn partial(pts: &[P], u: f32) -> Vec<P> {
    if u >= 1.0 || pts.len() < 2 {
        return pts.to_vec();
    }
    let len = |a: P, b: P| (b[0] - a[0]).hypot(b[1] - a[1]);
    let total: f32 = pts.windows(2).map(|w| len(w[0], w[1])).sum();
    let mut left = total * u;
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let d = len(w[0], w[1]);
        if d >= left {
            let k = if d > 0.0 { left / d } else { 0.0 };
            out.push([
                w[0][0] + (w[1][0] - w[0][0]) * k,
                w[0][1] + (w[1][1] - w[0][1]) * k,
            ]);
            return out;
        }
        out.push(w[1]);
        left -= d;
    }
    out
}

/// An arc about (`cx`, `cy`), radius `r`, from `a0` through `span`.
fn arc(cx: f32, cy: f32, r: f32, a0: f32, span: f32) -> Vec<P> {
    let n = ((span.abs() * r / 12.0).ceil() as usize).max(8);
    (0..=n)
        .map(|i| {
            let a = a0 + span * i as f32 / n as f32;
            [cx + a.cos() * r, cy + a.sin() * r]
        })
        .collect()
}

/// Where the figure may go: inside the field, and clear of the letter.
struct Room {
    cell: f32,
    n: usize,
    free: Vec<bool>,
}

impl Room {
    fn at(&self, x: f32, y: f32) -> bool {
        let (i, j) = ((x / self.cell).round(), (y / self.cell).round());
        if i < 0.0 || j < 0.0 || i as usize >= self.n || j as usize >= self.n {
            return false;
        }
        self.free[j as usize * self.n + i as usize]
    }

    /// A disc of radius `r` about (`x`, `y`) is clear.
    fn clear(&self, x: f32, y: f32, r: f32) -> bool {
        self.at(x, y)
            && (0..8).all(|k| {
                let a = k as f32 * TAU / 8.0;
                self.at(x + a.cos() * r, y + a.sin() * r)
            })
    }

    /// How far from (`x`, `y`) it stays clear, up to `max`.
    fn clearance(&self, x: f32, y: f32, max: f32) -> f32 {
        let mut r = 0.0;
        while r < max && self.clear(x, y, r + 8.0) {
            r += 8.0;
        }
        r
    }
}

/// A rune of no alphabet: a stave with twigs, or a figure without one, in a box 0.6 wide and
/// 1 tall about the origin, up being -y.
fn rune(r: &mut Rng) -> Vec<Vec<P>> {
    let mut out = Vec::new();
    if r.chance(0.82) {
        let x = if r.chance(0.2) { -0.18 } else { 0.0 };
        out.push(vec![[x, -0.5], [x, 0.5]]);
        if x != 0.0 {
            // Two staves, tied.
            out.push(vec![[0.18, -0.5], [0.18, 0.5]]);
            let y = r.pick(&[-0.2, 0.0, 0.2]);
            out.push(vec![[-0.18, y - 0.15], [0.18, y + 0.15]]);
            return out;
        }
        for _ in 0..r.pick(&[1, 1, 2]) {
            let y0 = r.pick(&[-0.5, -0.25, 0.0, 0.15]);
            let side = r.pick(&[-1.0, 1.0]);
            let dy = r.pick(&[-0.3, 0.25, 0.3]);
            if r.chance(0.3) {
                // A bow.
                out.push(vec![[0.0, y0], [side * 0.28, y0 + 0.17], [0.0, y0 + 0.34]]);
            } else {
                out.push(vec![[0.0, y0], [side * 0.3, y0 + dy]]);
            }
        }
    } else {
        match r.pick(&[0, 1, 2]) {
            0 => {
                out.push(vec![[-0.3, -0.5], [0.3, 0.5]]);
                out.push(vec![[0.3, -0.5], [-0.3, 0.5]]);
            }
            1 => out.push(vec![
                [0.0, -0.5],
                [0.3, 0.0],
                [0.0, 0.5],
                [-0.3, 0.0],
                [0.0, -0.5],
            ]),
            _ => out.push(vec![[0.3, -0.5], [-0.25, 0.0], [0.3, 0.5]]),
        }
    }
    out
}

/// A sigil: a path through points of a three-by-three grid ending in a bar across it, and
/// where it starts (for a ring), in a unit box about the origin.
fn sigil(r: &mut Rng) -> (Vec<P>, P) {
    let mut cells: Vec<(i32, i32)> = vec![(r.pick(&[-1, 0, 1]), r.pick(&[-1, 0, 1]))];
    let steps = r.pick(&[3, 4, 4, 5]);
    let mut tries = 0;
    while cells.len() < steps && tries < 60 {
        tries += 1;
        let c = (r.pick(&[-1, 0, 1]), r.pick(&[-1, 0, 1]));
        let last = *cells.last().unwrap();
        if !cells.contains(&c) && (c.0 - last.0).abs() + (c.1 - last.1).abs() <= 3 {
            cells.push(c);
        }
    }
    let mut path: Vec<P> = cells
        .iter()
        .map(|&(x, y)| [x as f32 * 0.4, y as f32 * 0.4])
        .collect();
    let start = path[0];
    if let [.., a, b] = path[..] {
        let d = [b[0] - a[0], b[1] - a[1]];
        let l = d[0].hypot(d[1]).max(1e-3);
        let n = [-d[1] / l * 0.16, d[0] / l * 0.16];
        path.push([b[0] + n[0], b[1] + n[1]]);
        path.push([b[0] - n[0], b[1] - n[1]]);
    }
    (path, start)
}

/// A four-pointed star, (`x`, `y`), `r` to its points.
fn sparkle(x: f32, y: f32, r: f32) -> Vec<P> {
    (0..16)
        .map(|i| {
            let a = i as f32 * TAU / 16.0 - FRAC_PI_2;
            let k = match i % 4 {
                0 => 1.0,
                2 => 0.22,
                _ => 0.3,
            };
            [x + a.cos() * r * k, y + a.sin() * r * k]
        })
        .collect()
}

struct Rule {
    pts: Vec<P>,
    w: f32,
}

/// A built initial: its geometry, ready to paint at any moment of its inscribing.
pub struct Scene {
    style: Style,
    seed: u32,
    letter: Vec<Vec<P>>,
    field: Vec<P>,
    rules: Vec<Rule>,
    marks: Vec<Mark>,
    grain: Mutex<HashMap<u32, Arc<Grain>>>,
    layers: Mutex<HashMap<u32, Arc<Layers>>>,
}

/// The parts of an initial that don't change, at one size.
struct Layers {
    /// The ground.
    under: Vec<[f32; 4]>,
    /// Where the figure may be drawn.
    field: Mask,
    /// Painted over a clear canvas, to lay over the figure in one go: the letter, kept clear
    /// round, and the rules.
    over: Canvas,
}

/// Rasterizes onto a grid of `n` × `n` cells, `cell` units each, as booleans.
fn grid_mask(n: usize, cell: f32, draw: impl FnOnce(&mut Mask, Transform)) -> Vec<bool> {
    let mut m = Mask::new(n as u32, n as u32).expect("a grid");
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

impl Scene {
    /// The initial `letter` of the spell `id`, of `kind`.
    pub fn build(letter: char, id: &str, kind: Kind) -> Scene {
        Scene::of(letter, kind, hash(&["cantrip", id, "figure"]))
    }

    fn of(letter: char, kind: Kind, seed: u32) -> Scene {
        let st = Style::of(kind);
        let m = st.margin;
        let base = frame_poly(st.frame, m, m, W - 2.0 * m, W - 2.0 * m);
        let mut rules = vec![Rule {
            pts: offset_polygon(&base, st.w1 / 2.0),
            w: st.w1,
        }];
        let mut inset = st.w1 + st.rule_gap;
        rules.push(Rule {
            pts: offset_polygon(&base, inset + st.w2 / 2.0),
            w: st.w2,
        });
        inset += st.w2;
        let field = offset_polygon(&base, inset);
        let fb = Box2::of([&field]);

        // The letter, centred in the field.
        let face = caps(st.face);
        let glyph = face.letter(letter).cloned().unwrap_or_default();
        let contours: Vec<Vec<P>> = if glyph.is_empty() {
            Vec::new()
        } else {
            let b = Box2::of(&glyph);
            let s = (st.size * fb.h() / face.cap_height.max(b.h() * 0.6))
                .min(st.size * 1.15 * fb.w() / b.w())
                .min(1.6 * fb.h() / b.h());
            let (cx, cy) = (
                (fb.x0 + fb.x1) / 2.0,
                (fb.y0 + fb.y1) / 2.0
                    + if st.frame == Frame::Arch {
                        0.06 * fb.h()
                    } else {
                        0.0
                    },
            );
            let (bx, by) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
            glyph
                .iter()
                .map(|c| {
                    c.iter()
                        .map(|p| [cx + (p[0] - bx) * s, cy + (p[1] - by) * s])
                        .collect()
                })
                .collect()
        };

        // Where the figure has room: in the field, clear of the letter and its echo.
        let cell = 10.0;
        let n = (W / cell) as usize + 1;
        let inside = grid_mask(n, cell, |mk, t| {
            if let Some(p) = path_of(std::slice::from_ref(&field), true) {
                mk.fill_path(&p, FillRule::Winding, true, t);
            }
        });
        let near = grid_mask(n, cell, |mk, t| {
            if let Some(p) = path_of(&contours, true) {
                mk.fill_path(&p, FillRule::Winding, true, t);
                if let Some(s) = stroked(&p, 2.0 * (st.echo_gap + st.echo_w + 10.0)) {
                    mk.fill_path(&s, FillRule::Winding, true, t);
                }
            }
        });
        let room = Room {
            cell,
            n,
            free: inside.iter().zip(&near).map(|(a, b)| *a && !b).collect(),
        };

        let mut r = Rng::new(seed);
        let marks = match kind {
            Kind::Seal => seal(fb, &room, &mut r),
            Kind::Chart => chart(fb, &room, &mut r),
        };
        Scene {
            style: st,
            seed,
            letter: contours,
            field,
            rules,
            marks,
            grain: Mutex::new(HashMap::new()),
            layers: Mutex::new(HashMap::new()),
        }
    }

    /// The initial `t` of the way through its inscribing (0: the letter in its frame alone; 1:
    /// all of it), `px` pixels square, in `inks`: straight-alpha sRGB, rows top first.
    pub fn paint(&self, t: f32, px: u32, inks: &Inks) -> Vec<u8> {
        // Painted at twice the size and printed down, so the inks' edges are smooth.
        let n = px * 2;
        let layers = self.layers(n);
        let mut c = Canvas::new(n);
        c.cov.clone_from(&layers.under);
        let tf = Transform::from_scale(n as f32 / W, n as f32 / W);
        let min_line = 1.1 * W / n as f32;

        // The figure as far as it's drawn, a run of marks of one ink at a time.
        let mut i = 0;
        while i < self.marks.len() {
            let slot = self.marks[i].slot;
            let mut m = c.blank();
            let mut any = false;
            while i < self.marks.len() && self.marks[i].slot == slot {
                let mark = &self.marks[i];
                i += 1;
                let u = mark.progress(t);
                if u <= 0.0 {
                    continue;
                }
                any = true;
                match &mark.shape {
                    Shape::Line(pts, w) => {
                        if let Some(p) = path_of(&[partial(pts, u)], false)
                            .and_then(|p| stroked(&p, w.max(min_line)))
                        {
                            m.fill_path(&p, FillRule::Winding, true, tf);
                        }
                    }
                    Shape::Dot([x, y], rad) => {
                        // Swells a little past its size, and settles.
                        let s = 1.0 - (1.0 - u).powi(3) + 0.25 * (u * PI).sin();
                        let mut pb = PathBuilder::new();
                        pb.push_circle(*x, *y, (rad * s).max(min_line / 2.0));
                        if let Some(p) = pb.finish() {
                            m.fill_path(&p, FillRule::Winding, true, tf);
                        }
                    }
                    Shape::Fill(poly) => {
                        let mut f = c.fill(path_of(std::slice::from_ref(poly), true), tf);
                        fade(&mut f, u);
                        union(&mut m, &f);
                    }
                    Shape::Crescent([x, y, rad], [dx, dy], k) => {
                        let disc = |x: f32, y: f32, r: f32| {
                            let mut pb = PathBuilder::new();
                            pb.push_circle(x, y, r);
                            c.fill(pb.finish(), tf)
                        };
                        let mut f = disc(*x, *y, *rad);
                        subtract(&mut f, &disc(x + dx, y + dy, rad * k));
                        fade(&mut f, u);
                        union(&mut m, &f);
                    }
                }
            }
            if any {
                intersect(&mut m, &layers.field);
                c.put(&m, slot);
            }
        }

        c.apply(&layers.over);
        let grain = self.grain(n);
        c.print(&self.style, &grain, inks, px)
    }

    /// What doesn't change, painted once at each size: the ground under the figure, and the
    /// letter and the rules over it.
    fn layers(&self, n: u32) -> Arc<Layers> {
        if let Some(l) = self.layers.lock().unwrap().get(&n) {
            return l.clone();
        }
        let mut c = Canvas::new(n);
        let st = &self.style;
        let tf = Transform::from_scale(n as f32 / W, n as f32 / W);
        let min_line = 1.1 * W / n as f32;

        let field = c.fill(path_of(std::slice::from_ref(&self.field), true), tf);
        c.put(&field, st.ground);
        let under = std::mem::take(&mut c.cov);

        let mut c = Canvas::new(n);
        if let Some(lp) = path_of(&self.letter, true) {
            // The ground kept clear round it, so the figure passes behind.
            let mut clear = c.region(&lp, -st.halo, tf);
            intersect(&mut clear, &field);
            c.put(&clear, st.ground);
            let band = c.band(
                &lp,
                -st.echo_gap - st.echo_w / 2.0,
                -st.echo_gap + st.echo_w / 2.0,
                tf,
            );
            c.put(&band, st.echo_slot);
            let body = c.region(&lp, 0.0, tf);
            c.put(&body, st.letter);
        }
        for rule in &self.rules {
            let m = c.stroke(
                path_of(std::slice::from_ref(&rule.pts), true),
                rule.w.max(min_line),
                tf,
            );
            c.put(&m, st.rule_slot);
        }

        let layers = Arc::new(Layers {
            under,
            field,
            over: c,
        });
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

/// Scales a mask's coverage by `u`.
fn fade(m: &mut Mask, u: f32) {
    if u < 1.0 {
        for v in m.data_mut() {
            *v = (*v as f32 * u) as u8;
        }
    }
}

/// The seal's figure: a double ring round the field with runes between, a star drawn through
/// it in vermilion, gold at the star's points, and a sigil in each corner. Each spell's has its
/// own star, runes, ticks and sigils.
fn seal(fb: Box2, room: &Room, r: &mut Rng) -> Vec<Mark> {
    let (cx, cy) = ((fb.x0 + fb.x1) / 2.0, (fb.y0 + fb.y1) / 2.0);
    let big = fb.w().min(fb.h()) / 2.0;
    let r_out = big * 0.95;
    let band = big * r.range(0.17, 0.2);
    let r_in = r_out - band;
    let turn = r.range(0.0, TAU);
    let mut marks = Vec::new();

    // The rings, traced from the top round.
    let start = -FRAC_PI_2 + r.range(-0.4, 0.4);
    marks.push(Mark::line(
        arc(cx, cy, r_out, start, TAU),
        13.0,
        Slot::Gilt,
        0.0,
        0.3,
    ));
    marks.push(Mark::line(
        arc(cx, cy, r_in, start, -TAU),
        11.0,
        Slot::Gilt,
        0.06,
        0.3,
    ));

    // The star, {n/k}, each of its loops drawn in one stroke.
    let (points, step) = r.pick(&[(5, 2), (6, 2), (7, 2), (7, 3), (8, 3), (9, 2), (9, 4)]);
    let r_star = r_in - 8.0;
    let a0 = -FRAC_PI_2
        + if r.chance(0.5) {
            0.0
        } else {
            PI / points as f32
        };
    let vertex = |i: usize| {
        let a = a0 + TAU * i as f32 / points as f32;
        [cx + a.cos() * r_star, cy + a.sin() * r_star]
    };
    let loops = gcd(points, step);
    for l in 0..loops {
        let pts: Vec<P> = (0..=points / loops).map(|j| vertex(l + j * step)).collect();
        let at = 0.22 + 0.35 * l as f32 / loops as f32;
        marks.push(Mark::line(pts, 13.0, Slot::Rubric, at, 0.4 / loops as f32));
    }
    // And sometimes the circle its inner points touch.
    if r.chance(0.5) {
        let ri = r_star * (PI * step as f32 / points as f32).cos();
        marks.push(Mark::line(
            arc(cx, cy, ri, turn, TAU),
            7.0,
            Slot::Rubric,
            0.5,
            0.25,
        ));
    }
    // Ticks inside the inner ring, between the star's points.
    if r.chance(0.6) {
        let per = r.pick(&[2, 3, 4]);
        let count = points * per;
        for i in 0..count {
            if i % per == 0 {
                continue;
            }
            let a = a0 + TAU * i as f32 / count as f32;
            let (c, s) = (a.cos(), a.sin());
            let pts = vec![
                [cx + c * (r_in - 4.0), cy + s * (r_in - 4.0)],
                [cx + c * (r_in - 30.0), cy + s * (r_in - 30.0)],
            ];
            let at = 0.45 + 0.2 * i as f32 / count as f32;
            marks.push(Mark::line(pts, 8.0, Slot::Gilt, at, 0.06));
        }
    }
    // Gold at the star's points.
    for i in 0..points {
        let at = 0.55 + 0.12 * i as f32 / points as f32;
        marks.push(Mark::dot(vertex(i), 15.0, Slot::Gilt, at, 0.12));
    }

    // The runes between the rings, upright, read clockwise from the top.
    let mid = (r_out + r_in) / 2.0;
    let h = band * 0.6;
    let count = ((TAU * mid) / (h * 1.0)).floor() as usize;
    let mut runes = Vec::new();
    for i in 0..count {
        let a = start + TAU * (i as f32 + 0.5) / count as f32;
        let (o, tg) = ([a.cos(), a.sin()], [-a.sin(), a.cos()]);
        let at = 0.32 + 0.55 * i as f32 / count as f32;
        let p = [cx + o[0] * mid, cy + o[1] * mid];
        if r.chance(0.14) {
            // A stop between words.
            marks.push(Mark::dot(p, 9.0, Slot::Gilt, at, 0.08));
            continue;
        }
        for stroke in rune(r) {
            let pts = stroke
                .iter()
                .map(|q| {
                    [
                        p[0] + (q[0] * tg[0] - q[1] * o[0]) * h,
                        p[1] + (q[0] * tg[1] - q[1] * o[1]) * h,
                    ]
                })
                .collect();
            runes.push(Mark::line(pts, 9.0, Slot::Gilt, at, 0.08));
        }
    }
    marks.extend(runes);

    // A sigil in each corner, where it has room.
    let corner = big * 0.85;
    let size = big * 0.2;
    for (k, (sx, sy)) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let (x, y) = (cx + sx * corner, cy + sy * corner);
        if !room.at(x, y) {
            continue;
        }
        let (path, ring) = sigil(r);
        let place = |q: &P| [x + q[0] * size, y + q[1] * size];
        let at = 0.7 + 0.06 * k as f32;
        marks.push(Mark::line(
            circle_poly(ring[0] * size + x, ring[1] * size + y, size * 0.13),
            8.0,
            Slot::Gilt,
            at,
            0.06,
        ));
        marks.push(Mark::line(
            path.iter().map(place).collect(),
            9.0,
            Slot::Gilt,
            at + 0.05,
            0.16,
        ));
    }
    marks
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The chart's figure: orbits swung round the letter with a planet on each, two or three
/// constellations, a crescent moon where there's room, and stardust. Each spell's sky is its
/// own.
fn chart(fb: Box2, room: &Room, r: &mut Rng) -> Vec<Mark> {
    let (cx, cy) = ((fb.x0 + fb.x1) / 2.0, (fb.y0 + fb.y1) / 2.0 + 0.06 * fb.h());
    let big = fb.w().min(fb.h()) / 2.0;
    let mut marks = Vec::new();
    let mut taken: Vec<[f32; 3]> = Vec::new();
    let near = |taken: &[[f32; 3]], x: f32, y: f32, d: f32| {
        taken.iter().any(|t| (t[0] - x).hypot(t[1] - y) < t[2] + d)
    };

    // Orbits: tilted ellipses, part drawn, a planet where each ends.
    for k in 0..r.pick(&[1, 2, 2]) {
        let rx = big * r.range(0.8, 0.98);
        let ry = rx * r.range(0.3, 0.48);
        let tilt = r.range(-0.7, 0.7) + k as f32 * 0.9;
        let a0 = r.range(0.0, TAU);
        let span = TAU * r.range(0.6, 0.82) * if r.chance(0.5) { 1.0 } else { -1.0 };
        let n = 120;
        let (c, s) = (tilt.cos(), tilt.sin());
        let pts: Vec<P> = (0..=n)
            .map(|i| {
                let a = a0 + span * i as f32 / n as f32;
                let (ex, ey) = (a.cos() * rx, a.sin() * ry);
                [cx + ex * c - ey * s, cy + ex * s + ey * c]
            })
            .collect();
        let end = *pts.last().unwrap();
        let at = 0.04 * k as f32;
        marks.push(Mark::line(pts, 8.0, Slot::Gilt, at, 0.5));
        if room.clear(end[0], end[1], 22.0) {
            marks.push(Mark::dot(end, 17.0, Slot::Gilt, at + 0.48, 0.12));
            marks.push(Mark::line(
                circle_poly(end[0], end[1], 32.0),
                6.0,
                Slot::Gilt,
                at + 0.55,
                0.12,
            ));
            taken.push([end[0], end[1], 40.0]);
        }
    }

    // A crescent moon, where there's most room.
    let mut best = (0.0, [0.0, 0.0]);
    for _ in 0..60 {
        let (x, y) = (r.range(fb.x0, fb.x1), r.range(fb.y0, fb.y1));
        let c = room.clearance(x, y, 90.0);
        if c > best.0 && !near(&taken, x, y, c) {
            best = (c, [x, y]);
        }
    }
    if best.0 >= 40.0 {
        let rad = (best.0 - 8.0).min(70.0);
        let a = r.range(0.0, TAU);
        let [x, y] = best.1;
        marks.push(Mark {
            shape: Shape::Crescent(
                [x, y, rad],
                [a.cos() * rad * 0.42, a.sin() * rad * 0.42],
                0.9,
            ),
            slot: Slot::Gilt,
            at: 0.35,
            dur: 0.35,
        });
        taken.push([x, y, rad + 16.0]);
    }

    // Constellations: a few bright stars, joined.
    let mut lines = Vec::new();
    let mut stars = Vec::new();
    for k in 0..r.pick(&[2, 3, 3]) {
        let mut chain: Vec<P> = Vec::new();
        for _ in 0..40 {
            let (x, y) = (r.range(fb.x0, fb.x1), r.range(fb.y0, fb.y1));
            if room.clear(x, y, 20.0) && !near(&taken, x, y, 20.0) {
                chain.push([x, y]);
                break;
            }
        }
        let Some(&first) = chain.first() else {
            continue;
        };
        let mut dir = r.range(0.0, TAU);
        let mut last = first;
        for _ in 0..r.pick(&[3, 4, 5]) {
            let mut placed = false;
            for _ in 0..24 {
                let d = dir + r.range(-1.1, 1.1);
                let l = r.range(90.0, 160.0);
                let p = [last[0] + d.cos() * l, last[1] + d.sin() * l];
                let midway = [(p[0] + last[0]) / 2.0, (p[1] + last[1]) / 2.0];
                if room.clear(p[0], p[1], 20.0)
                    && room.at(midway[0], midway[1])
                    && !near(&taken, p[0], p[1], 20.0)
                    && chain
                        .iter()
                        .all(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 70.0)
                {
                    chain.push(p);
                    last = p;
                    dir = d;
                    placed = true;
                    break;
                }
            }
            if !placed {
                break;
            }
        }
        if chain.len() < 3 {
            continue;
        }
        let begin = 0.15 + 0.22 * k as f32;
        let each = 0.4 / chain.len() as f32;
        for (j, &p) in chain.iter().enumerate() {
            let at = begin + each * j as f32;
            if j == 0 || r.chance(0.25) {
                stars.push(Mark {
                    shape: Shape::Fill(sparkle(p[0], p[1], r.range(30.0, 40.0))),
                    slot: Slot::Paper,
                    at,
                    dur: 0.12,
                });
            } else {
                stars.push(Mark::dot(p, r.range(10.0, 15.0), Slot::Paper, at, 0.1));
            }
            if j > 0 {
                lines.push(Mark::line(
                    vec![chain[j - 1], p],
                    6.0,
                    Slot::Gilt,
                    at - each,
                    each,
                ));
            }
            taken.push([p[0], p[1], 26.0]);
        }
    }
    marks.extend(lines);
    marks.extend(stars);

    // Stardust in what's left of the sky.
    let mut dust = Vec::new();
    for _ in 0..500 {
        if dust.len() >= 70 {
            break;
        }
        let (x, y) = (r.range(fb.x0, fb.x1), r.range(fb.y0, fb.y1));
        let rad = if r.chance(0.15) {
            r.range(8.0, 11.0)
        } else {
            r.range(4.5, 7.0)
        };
        if room.clear(x, y, rad + 6.0) && !near(&taken, x, y, 30.0) {
            taken.push([x, y, 0.0]);
            let slot = if r.chance(0.6) {
                Slot::Paper
            } else {
                Slot::Gilt
            };
            dust.push((slot, Mark::dot([x, y], rad, slot, r.range(0.0, 0.85), 0.15)));
        }
    }
    // One run per ink.
    dust.sort_by_key(|d| d.0 == Slot::Gilt);
    marks.extend(dust.into_iter().map(|d| d.1));
    marks
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

/// Initials built so far, by spell id, letter and kind.
type Built = HashMap<(String, char, Kind), Arc<Scene>>;

/// A painted initial: spell id, letter, kind, size and inks.
type Painted = (String, char, Kind, u32, u64);

/// The book's initials, built and painted as they're wanted, and kept. They're built ahead on
/// a thread of their own (`prepare`), so leafing to one rarely waits for it.
#[derive(Default)]
pub struct Illuminations {
    built: Arc<Mutex<Built>>,
    asked: std::collections::HashSet<(String, char, Kind, u32)>,
    painted: HashMap<Painted, (rune::Pixels, f32)>,
}

impl std::fmt::Debug for Illuminations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Illuminations({} painted)", self.painted.len())
    }
}

fn scene(built: &Mutex<Built>, id: &str, letter: char, kind: Kind) -> Arc<Scene> {
    let key = (id.to_string(), letter, kind);
    if let Some(s) = built.lock().unwrap().get(&key) {
        return s.clone();
    }
    let s = Arc::new(Scene::build(letter, id, kind));
    built.lock().unwrap().insert(key, s.clone());
    s
}

impl Illuminations {
    /// Builds the initials of these spells (id and letter) of `kind`, in order, `px` square,
    /// ahead of their being shown. Each is asked for once.
    pub fn prepare(
        &mut self,
        wants: impl IntoIterator<Item = (String, char)>,
        kind: Kind,
        px: u32,
    ) {
        let new: Vec<(String, char)> = wants
            .into_iter()
            .filter(|(id, c)| self.asked.insert((id.clone(), *c, kind, px)))
            .collect();
        if new.is_empty() {
            return;
        }
        let built = self.built.clone();
        std::thread::spawn(move || {
            for (id, letter) in new {
                let s = scene(&built, &id, letter, kind);
                s.layers(px * 2);
                s.grain(px * 2);
            }
        });
    }

    /// The initial `letter` of the spell `id`, of `kind`, `t` of the way inscribed, `px`
    /// square: the same `Pixels` every time, repainted only when `t` has moved.
    pub fn pixels(
        &mut self,
        id: &str,
        letter: char,
        kind: Kind,
        t: f32,
        px: u32,
        inks: &Inks,
    ) -> rune::Pixels {
        let t = t.clamp(0.0, 1.0);
        let key = (id.to_string(), letter, kind, px, inks.key());
        let (pixels, at) = self
            .painted
            .entry(key)
            .or_insert_with(|| (rune::Pixels::new(), f32::NAN));
        if *at != t {
            let s = scene(&self.built, id, letter, kind);
            pixels.set(px, px, s.paint(t, px, inks));
            *at = t;
        }
        pixels.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGIL: Inks = Inks {
        ink: [0.20, 0.12, 0.07],
        rubric: [0.58, 0.14, 0.09],
        gilt: [0.80, 0.62, 0.28],
        azure: [0.16, 0.26, 0.52],
    };
    const ARCANE: Inks = Inks {
        ink: [0.16, 0.09, 0.24],
        rubric: [0.45, 0.14, 0.52],
        gilt: [0.80, 0.62, 0.28],
        azure: [0.14, 0.24, 0.48],
    };
    const KINDS: [(Kind, Inks); 2] = [(Kind::Seal, SIGIL), (Kind::Chart, ARCANE)];

    #[test]
    fn every_face_has_every_capital() {
        for face in caps::FACES {
            for c in 'A'..='Z' {
                let g = caps(face)
                    .letter(c)
                    .unwrap_or_else(|| panic!("{face:?} {c}"));
                assert!(
                    !g.is_empty() && g.iter().all(|c| c.len() > 2),
                    "{face:?} {c}"
                );
            }
        }
    }

    #[test]
    fn an_initial_is_inscribed_the_same_way_every_time() {
        for (kind, inks) in KINDS {
            let a = Scene::build('H', "spell:hello.ts:hello", kind);
            let b = Scene::build('H', "spell:hello.ts:hello", kind);
            assert!(a.marks.len() > 10, "{kind:?} draws a figure");
            assert_eq!(a.paint(0.5, 48, &inks), b.paint(0.5, 48, &inks));
            let bare = a.paint(0.0, 48, &inks);
            let drawn = a.paint(1.0, 48, &inks);
            let inked = |p: &[u8]| p.chunks(4).filter(|p| p[3] > 128).count();
            assert!(
                inked(&bare) > 100,
                "the letter and its frame are there from the start"
            );
            assert_ne!(bare, drawn, "and the figure is drawn in");
        }
    }

    #[test]
    fn each_spell_has_a_figure_of_its_own() {
        for (kind, _) in KINDS {
            let a = Scene::build('H', "spell:hello.ts:hello", kind);
            let b = Scene::build('H', "spell:hello.ts:hi", kind);
            assert_eq!(a.letter, b.letter, "one kind: the same letter in the same frame");
            assert_ne!(
                format!("{:?}", a.marks),
                format!("{:?}", b.marks),
                "{kind:?}: drawn round it its own way"
            );
        }
    }

    /// A sheet of initials on parchment, for looking at: `cargo test --release sheet -- --ignored`.
    /// For each kind, its spells a third, two thirds and all the way inscribed.
    #[test]
    #[ignore]
    fn sheet() {
        let ids = [
            "spell:arrange.ts:arrange",
            "spell:brew.ts:brew",
            "entry:claude-usage",
            "spell:clip.ts:history",
            "spell:uuid.ts:uuid",
            "spell:git.ts:branches",
            "spell:hello.ts:hello",
            "entry:insomnia",
        ];
        let px: u32 = std::env::var("PX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200);
        let ts = [0.33f32, 0.66, 1.0];
        let cols = ids.len();
        let rows = KINDS.len() * ts.len();
        let (w, h) = (px * cols as u32, px * rows as u32);
        let mut sheet = tiny_skia::Pixmap::new(w, h).unwrap();
        sheet.fill(tiny_skia::Color::from_rgba8(232, 214, 176, 255));
        let mut cells = Vec::new();
        for (k, (kind, inks)) in KINDS.into_iter().enumerate() {
            for (j, &t) in ts.iter().enumerate() {
                for (i, id) in ids.iter().enumerate() {
                    cells.push((i, k * ts.len() + j, kind, inks, *id, t));
                }
            }
        }
        {
            for (col, row, kind, inks, id, t) in cells {
                let letter = id
                    .rsplit(':')
                    .next()
                    .unwrap()
                    .chars()
                    .next()
                    .unwrap()
                    .to_ascii_uppercase();
                let began = std::time::Instant::now();
                let scene = Scene::build(letter, id, kind);
                if t == 1.0 {
                    let built = began.elapsed();
                    let began = std::time::Instant::now();
                    let _ = scene.paint(0.5, 124, &inks);
                    eprintln!(
                        "{kind:?} {id}: built in {built:?}, painted in {:?}, {} marks",
                        began.elapsed(),
                        scene.marks.len()
                    );
                }
                let rgba = scene.paint(t, px, &inks);
                let (x0, y0) = (col as u32 * px, row as u32 * px);
                let data = sheet.data_mut();
                for y in 0..px {
                    for x in 0..px {
                        let s = ((y * px + x) * 4) as usize;
                        let d = (((y0 + y) * w + x0 + x) * 4) as usize;
                        let a = rgba[s + 3] as f32 / 255.0;
                        for c in 0..3 {
                            data[d + c] =
                                (data[d + c] as f32 * (1.0 - a) + rgba[s + c] as f32 * a) as u8;
                        }
                    }
                }
            }
        }
        sheet
            .save_png(std::env::var("OUT").unwrap_or("/tmp/book/initials.png".into()))
            .unwrap();
    }
}
