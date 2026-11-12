//! The growing ornament, after initialis's `render/src/ornament.js`. Stems step forward through
//! the clearance field, curl more as they age, branch to one side, carry leaves, and end in
//! curls, leaves or flowers. Then the biggest gaps left are filled one at a time (stems
//! sprouting off whatever is nearest, flowers, leaves, berries) until none is left above the
//! density. Everything carries the time it grows in at, in stem lengths, so the ornament can be
//! drawn as it stands at any moment (`Items::at`).
//!
//! Initialis stops growing after a time budget; here the budget is a count of steps, so an
//! initial comes out the same every time on every machine.

use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};

use super::field::{Field, Log};
use super::geom::{Noise, P, Rng, lerp};
use super::shapes::{Blade, Detail, Flower, Leaf, Shape, flower, leaf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Terminal {
    Curl,
    Ball,
    Leaf,
    Flower,
}

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub margin: f32,
    pub stem_w: f32,
    pub taper: f32,
    pub curl: f32,
    pub turns: f32,
    pub undulate: f32,
    pub wander: f32,
    pub thin: f32,
    pub branch: f32,
    pub max_depth: u32,
    pub branch_angle: f32,
    pub length: f32,
    pub leaf: Leaf,
    pub leaf_size: f32,
    pub leaf_every: f32,
    pub leaf_w: f32,
    pub lobes: f32,
    pub lobe_depth: f32,
    pub leaf_curl: f32,
    pub leaf_angle: f32,
    pub leaf_outer: bool,
    pub flower: Flower,
    pub flower_chance: f32,
    pub flower_size: f32,
    pub petals: usize,
    pub berries: f32,
    pub berry_size: f32,
    pub dots: f32,
    pub beads: f32,
    pub terminal: Terminal,
    pub density: f32,
    pub sprout: f32,
    pub mains: usize,
    pub fill_leaves: bool,
    pub detail: Detail,
}

pub struct Sizes {
    pub leaf_max: f32,
    pub flower_max: f32,
    pub fill_min: f32,
    pub sprout_min: f32,
    pub cap: f32,
}

/// The sizes growth works with; the field's clearance cap has to cover all of them.
pub fn sizes(o: &Opts) -> Sizes {
    let m = o.margin;
    let leaf_max = o.leaf_size * (55.0 + o.stem_w * 5.5);
    let flower_max = o.flower_size * (28.0 + o.stem_w * 2.6);
    let fill_min = lerp(70.0, m * 0.55 + 1.5, o.density);
    let sprout_min = 140f32.min(fill_min.max(o.stem_w * 2.0 + m * 2.0).max(leaf_max * 0.3));
    let cap = 180f32.min(
        40f32
            .max(flower_max + m + 4.0)
            .max(leaf_max * o.leaf_w * 0.5 + m + 4.0)
            .max(fill_min + 4.0)
            .max(sprout_min + 4.0),
    );
    Sizes {
        leaf_max,
        flower_max,
        fill_min,
        sprout_min,
        cap,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum End {
    Len,
    Blocked,
    Curl,
}

#[derive(Clone, Debug)]
pub struct Stem {
    pub pts: Vec<P>,
    pub ws: Vec<f32>,
    pub dirs: Vec<f32>,
    pub ks: Vec<f32>,
    pub sign: f32,
    end: End,
    pub depth: u32,
    /// Its length, and how far along it each point is.
    pub len: f32,
    pub acc: Vec<f32>,
    wave: bool,
    pub t0: f32,
}

impl Stem {
    /// Cut to length `l` (how far it has grown).
    fn cut(&self, l: f32) -> Stem {
        let acc = &self.acc;
        let mut i = 0;
        while i + 2 < acc.len() && acc[i + 1] < l {
            i += 1;
        }
        let f = ((l - acc[i]) / (acc[i + 1] - acc[i]).max(1e-6)).min(1.0);
        let mix = |a: f32, b: f32| a + (b - a) * f;
        let tip = [
            mix(self.pts[i][0], self.pts[i + 1][0]),
            mix(self.pts[i][1], self.pts[i + 1][1]),
        ];
        let mut pts = self.pts[..=i].to_vec();
        pts.push(tip);
        let mut ws = self.ws[..=i].to_vec();
        ws.push(mix(self.ws[i], self.ws[i + 1]));
        let mut dirs = self.dirs[..=i].to_vec();
        dirs.push(self.dirs[i + 1]);
        let mut ks = self.ks[..=i].to_vec();
        ks.push(self.ks[i + 1]);
        Stem {
            pts,
            ws,
            dirs,
            ks,
            len: l,
            acc: Vec::new(),
            ..self.clone()
        }
    }

    /// Its outline, round at both ends.
    pub fn outline(&self, min_w: f32) -> Vec<P> {
        let n = self.pts.len();
        let mut left = Vec::with_capacity(n * 2 + 20);
        let mut right = Vec::with_capacity(n);
        let wid: Vec<f32> = self.ws.iter().map(|w| w.max(min_w)).collect();
        for ((&a, &w), &[x, y]) in self.dirs.iter().zip(&wid).zip(&self.pts) {
            let (nx, ny, h) = (-a.sin(), a.cos(), w / 2.0);
            left.push([x + nx * h, y + ny * h]);
            right.push([x - nx * h, y - ny * h]);
        }
        let cap = |poly: &mut Vec<P>, c: P, start: f32, h: f32| {
            let steps = h.round().clamp(3.0, 10.0) as usize;
            for k in 1..steps {
                let t = start - k as f32 / steps as f32 * PI;
                poly.push([c[0] + t.cos() * h, c[1] + t.sin() * h]);
            }
        };
        let e = n - 1;
        cap(&mut left, self.pts[e], self.dirs[e] + PI / 2.0, wid[e] / 2.0);
        left.extend(right.iter().rev());
        cap(&mut left, self.pts[0], self.dirs[0] - PI / 2.0, wid[0] / 2.0);
        left
    }

    /// The line down its middle, for a vein.
    pub fn vein(&self) -> Option<Vec<P>> {
        let n = self.pts.len();
        if self.ws[0] < 3.0 || n < 4 {
            return None;
        }
        let (a, b) = (
            (n as f32 * 0.06).round() as usize,
            (n as f32 * 0.94).round() as usize,
        );
        Some(self.pts[a..b].to_vec())
    }
}

#[derive(Clone, Default, Debug)]
pub struct Items {
    pub stems: Vec<Stem>,
    pub leaves: Vec<Shape>,
    pub flowers: Vec<Shape>,
    /// `[x, y, r, t0]`.
    pub dots: Vec<[f32; 4]>,
    pub beads: Vec<[f32; 4]>,
}

fn ease(s: f32) -> f32 {
    1.0 - (1.0 - s).powi(3)
}

fn scaled(s: &Shape, e: f32) -> Shape {
    let [cx, cy] = s.at;
    let m = |p: &P| [cx + (p[0] - cx) * e, cy + (p[1] - cy) * e];
    Shape {
        polys: s.polys.iter().map(|p| p.iter().map(m).collect()).collect(),
        lines: s.lines.iter().map(|p| p.iter().map(m).collect()).collect(),
        dots: s
            .dots
            .iter()
            .map(|d| {
                let [x, y] = m(&[d[0], d[1]]);
                [x, y, d[2] * e]
            })
            .collect(),
        ..s.clone()
    }
}

impl Items {
    /// When the last of it has grown, in stem lengths.
    pub fn end(&self) -> f32 {
        let mut m: f32 = 0.0;
        for s in &self.stems {
            m = m.max(s.t0 + s.len);
        }
        for s in self.leaves.iter().chain(&self.flowers) {
            m = m.max(s.t0 + s.dur);
        }
        for d in self.dots.iter().chain(&self.beads) {
            m = m.max(d[3] + 12.0);
        }
        m
    }

    /// As it stands at time `t` (stem lengths): stems cut short, leaves and flowers swelling in.
    pub fn at(&self, t: f32) -> Items {
        let mut out = Items::default();
        for s in &self.stems {
            let l = t - s.t0;
            if l > 0.0 {
                out.stems.push(if l >= s.len { s.clone() } else { s.cut(l) });
            }
        }
        for (from, to) in [
            (&self.leaves, &mut out.leaves),
            (&self.flowers, &mut out.flowers),
        ] {
            for s in from {
                let u = (t - s.t0) / if s.dur > 0.0 { s.dur } else { 30.0 };
                if u > 0.0 {
                    to.push(if u >= 1.0 { s.clone() } else { scaled(s, ease(u)) });
                }
            }
        }
        for (from, to) in [(&self.dots, &mut out.dots), (&self.beads, &mut out.beads)] {
            for d in from {
                let u = (t - d[3]) / 12.0;
                if u > 0.0 {
                    to.push([d[0], d[1], d[2] * ease(u.min(1.0)), d[3]]);
                }
            }
        }
        out
    }

    /// Doubled across the middle (`w` wide), for a symmetric ornament.
    pub fn mirrored(mut self, w: f32) -> Items {
        let mp = |p: &P| [w - p[0], p[1]];
        let stems: Vec<Stem> = self
            .stems
            .iter()
            .map(|s| Stem {
                pts: s.pts.iter().map(mp).collect(),
                dirs: s.dirs.iter().map(|a| PI - a).collect(),
                ks: s.ks.iter().map(|k| -k).collect(),
                sign: -s.sign,
                ..s.clone()
            })
            .collect();
        let shape = |s: &Shape| Shape {
            polys: s.polys.iter().map(|p| p.iter().map(mp).collect()).collect(),
            lines: s.lines.iter().map(|p| p.iter().map(mp).collect()).collect(),
            dots: s.dots.iter().map(|d| [w - d[0], d[1], d[2]]).collect(),
            cover: Vec::new(),
            at: mp(&s.at),
            ..s.clone()
        };
        let leaves: Vec<Shape> = self.leaves.iter().map(shape).collect();
        let flowers: Vec<Shape> = self.flowers.iter().map(shape).collect();
        let flip = |d: &[f32; 4]| [w - d[0], d[1], d[2], d[3]];
        let dots: Vec<_> = self.dots.iter().map(flip).collect();
        let beads: Vec<_> = self.beads.iter().map(flip).collect();
        self.stems.extend(stems);
        self.leaves.extend(leaves);
        self.flowers.extend(flowers);
        self.dots.extend(dots);
        self.beads.extend(beads);
        self
    }
}

/// Where a main stem starts.
#[derive(Clone, Copy, Debug)]
pub struct Seed {
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub grace: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct Want {
    x: f32,
    y: f32,
    dir: f32,
    w: f32,
    len: f32,
    sign: f32,
    depth: u32,
    grace: f32,
    parent: Option<usize>,
    t0: f32,
}

struct Grower<'a> {
    field: &'a mut Field,
    o: Opts,
    r: Rng,
    noise: &'a Noise,
    items: Items,
    logs: Vec<Option<Log>>,
    queue: VecDeque<Want>,
    r_min: f32,
    r_max: f32,
    branch_every: f32,
    leaf_every: f32,
    base_len: f32,
    sz: Sizes,
    thin: f32,
    end_max: f32,
    fill_n: f32,
    /// Steps left before growth stops, whatever is still to come.
    work: i64,
}

/// Grows the ornament through `field` from `seeds` (or, with none, into its biggest openings).
pub fn grow(field: &mut Field, o: Opts, seed: u32, noise: &Noise, seeds: &[Seed]) -> Items {
    let m = o.margin;
    let r_min = (o.stem_w * 0.8 + m * 0.6).max(3.0);
    let mut g = Grower {
        r_min,
        r_max: lerp(900.0, r_min * 1.5, o.curl.sqrt()),
        branch_every: lerp(420.0, 36.0, o.branch),
        leaf_every: lerp(240.0, 22.0, o.leaf_every),
        base_len: 650.0 * o.length,
        sz: sizes(&o),
        thin: 1.0 - 0.45 * o.thin,
        field,
        o,
        r: Rng::new(seed),
        noise,
        items: Items::default(),
        logs: Vec::new(),
        queue: VecDeque::new(),
        end_max: 0.0,
        fill_n: 0.0,
        work: 120_000,
    };
    g.run(seeds);
    g.items
}

impl Grower<'_> {
    fn late(&self) -> bool {
        self.work <= 0
    }

    fn fill_t(&mut self) -> f32 {
        let t = self.end_max * 0.45 + self.fill_n * 2.5;
        self.fill_n += 1.0;
        t
    }

    /// The largest circle on the curling side the stem could follow without hitting anything.
    fn fit_radius(&self, x: f32, y: f32, th: f32, sgn: f32, need: f32) -> f32 {
        let (nx, ny) = (-th.sin() * sgn, th.cos() * sgn);
        let mut r = self.r_max;
        while r > self.r_min {
            let (cx, cy) = (x + nx * r, y + ny * r);
            let ok = [0.9f32, 1.7, 2.6, 3.5].iter().all(|a| {
                let ang = (y - cy).atan2(x - cx) + a * sgn;
                self.field.at(cx + ang.cos() * r, cy + ang.sin() * r) >= need
            });
            if ok {
                return r;
            }
            r /= 1.35;
        }
        self.r_min
    }

    /// Distance from (`px`, `py`) to the edge of a stem, ignoring its points within `skip` of
    /// (`sx`, `sy`) (where a leaf may touch it).
    fn stem_dist(&self, px: f32, py: f32, st: usize, s: P, skip: f32) -> f32 {
        let st = &self.items.stems[st];
        let s2 = skip * skip;
        let mut best = f32::INFINITY;
        for (p, w) in st.pts.iter().zip(&st.ws) {
            if s2 > 0.0 && (p[0] - s[0]).powi(2) + (p[1] - s[1]).powi(2) < s2 {
                continue;
            }
            best = best.min((p[0] - px).hypot(p[1] - py) - w / 2.0);
        }
        best
    }

    fn try_stem(&mut self, s: Want) -> Option<usize> {
        let o = self.o;
        let m = o.margin;
        let w0 = s.w.max(0.6);
        let step = (self.r_min * 0.35).min(w0 * 0.6 + 1.5).clamp(1.2, 5.0);
        let lag = ((w0 + m) / step).ceil() as usize + 2;
        let (mut x, mut y, mut th) = (s.x, s.y, s.dir);
        let mut sign = s.sign;
        if sign == 0.0 {
            let look = (self.r_min * 4.0 + 30.0).min(90.0);
            let l = self
                .field
                .at(x + (th - 1.0).cos() * look, y + (th - 1.0).sin() * look);
            let r = self
                .field
                .at(x + (th + 1.0).cos() * look, y + (th + 1.0).sin() * look);
            sign = if r > l { 1.0 } else { -1.0 } * if self.r.chance(0.15) { -1.0 } else { 1.0 };
        }
        let id = self.items.stems.len() as f32;
        let wave = s.depth == 0 && o.undulate > 0.0;
        let lambda = 160.0 + self.r_min * 6.0;
        let phase = self.r.next() * TAU;
        let ramp = (s.len * lerp(0.7, 0.18, o.curl) * if s.depth > 0 { 0.6 } else { 1.0 }).max(20.0);
        let (mut pts, mut ws, mut dirs, mut ks) = (vec![[x, y]], vec![w0], vec![th], vec![0.0]);
        let mut pend: VecDeque<[f32; 3]> = VecDeque::new();
        let (mut len, mut turned, mut end, mut n, mut fit_r) = (0.0, 0.0, End::Len, 0, self.r_max);
        self.field.begin();
        while len < s.len {
            self.work -= 1;
            let t = len / s.len;
            let w = (w0 * (1.0 - o.taper * t)).max(0.5);
            let age = (len / ramp).min(1.0);
            let mut k = if wave {
                o.undulate * (len / lambda * TAU + phase).sin() * (TAU / lambda) * 1.4
            } else {
                // The fitting circle changes slowly; refit every few steps.
                if n % 3 == 0 {
                    fit_r = self.fit_radius(x, y, th, sign, w / 2.0 + m);
                }
                n += 1;
                sign / fit_r * o.curl * (0.12 + 0.88 * age * age)
            };
            k += o.wander * self.noise.at(len * 0.006 + id * 7.3, id * 3.1) * 0.02;
            let need = w / 2.0 + m;
            let grace = len < s.grace;
            let look = need * 0.9 + step;
            // Near its start, the parent it springs from doesn't count as an obstacle.
            let clear = |g: &Grower, px: f32, py: f32| {
                let c = g.field.at(px, py);
                if !grace || c >= need {
                    return c;
                }
                match s.parent {
                    None => g.field.at_base(px, py),
                    Some(p) => {
                        if g.stem_dist(px, py, p, [0.0, 0.0], 0.0) - c < g.field.cell * 1.6 + 1.0 {
                            g.field.at_base(px, py).min(need)
                        } else {
                            c
                        }
                    }
                }
            };
            let mut chosen = None;
            for c in 0..15 {
                let dev = if c == 0 {
                    0.0
                } else {
                    (c as f32 / 2.0).ceil() * 0.075 * if c % 2 == 1 { sign } else { -sign }
                };
                let a = th + k * step + dev;
                let (ca, sa) = (a.cos(), a.sin());
                let (nx, ny) = (x + ca * step, y + sa * step);
                if clear(self, nx, ny) < need {
                    continue;
                }
                if clear(self, x + ca * look, y + sa * look) < need * 0.5 {
                    continue;
                }
                chosen = Some((a, nx, ny));
                break;
            }
            let Some((a, nx, ny)) = chosen else {
                end = End::Blocked;
                break;
            };
            turned += a - th;
            th = a;
            x = nx;
            y = ny;
            pts.push([x, y]);
            ws.push(w);
            dirs.push(th);
            ks.push(k);
            pend.push_back([x, y, w / 2.0]);
            if pend.len() > lag {
                let q = pend.pop_front().unwrap();
                self.field.stamp(q[0], q[1], q[2]);
            }
            len += step;
            if !wave && turned.abs() > o.turns * TAU + 0.6 {
                end = End::Curl;
                break;
            }
            if len > 4000.0 || self.late() {
                break;
            }
        }
        let min_len = (o.stem_w * 2.5).max(m * 3.0).max(14.0);
        if len < min_len {
            self.field.rollback();
            return None;
        }
        for q in pend {
            self.field.stamp(q[0], q[1], q[2]);
        }
        self.field.stamp(pts[0][0], pts[0][1], ws[0] / 2.0);
        let log = self.field.detach();
        let mut acc = vec![0.0];
        for w in pts.windows(2) {
            acc.push(acc.last().unwrap() + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
        }
        self.end_max = self.end_max.max(s.t0 + len);
        self.items.stems.push(Stem {
            pts,
            ws,
            dirs,
            ks,
            sign,
            end,
            depth: s.depth,
            len: *acc.last().unwrap(),
            acc,
            wave,
            t0: s.t0,
        });
        self.logs.push(Some(log));
        Some(self.items.stems.len() - 1)
    }

    /// Fits a leaf: shrinks it until everything past its base clears its own half width.
    #[allow(clippy::too_many_arguments)]
    fn try_leaf(&mut self, x: f32, y: f32, dir: f32, l_max: f32, side: f32, stem: Option<usize>, t0: f32) -> bool {
        let o = self.o;
        let mut l = l_max;
        while l >= (l_max * 0.3).max(6.0) {
            let bend = -side * o.leaf_curl * 0.9 * (0.7 + self.r.next() * 0.6);
            let shape = leaf(
                o.leaf,
                &Blade {
                    x,
                    y,
                    dir,
                    len: l,
                    width: o.leaf_w,
                    bend,
                    curl: -side * o.leaf_curl * 1.3,
                    lobes: o.lobes,
                    depth: o.lobe_depth,
                    n: 26,
                },
                o.detail,
            );
            let ok = shape.cover.iter().all(|&[px, py, r, t]| {
                if t < 0.12 {
                    return true;
                }
                if self.field.at(px, py) < r + o.margin {
                    return false;
                }
                !(stem.is_some_and(|st| {
                    t > 0.25 && self.stem_dist(px, py, st, [x, y], l * 0.55) < r + o.margin
                }))
            });
            if ok {
                for &[px, py, r, _] in &shape.cover {
                    if r > 0.5 {
                        self.field.stamp(px, py, r);
                    }
                }
                self.items.leaves.push(Shape {
                    at: [x, y],
                    t0,
                    dur: l * 0.9,
                    ..shape
                });
                return true;
            }
            l *= 0.84;
        }
        false
    }

    fn try_flower(&mut self, x: f32, y: f32, dir: f32, r_max: f32, t0: f32) -> bool {
        let o = self.o;
        let r = r_max.min(self.sz.flower_max);
        if r < 5.0 {
            return false;
        }
        let rot = self.r.next() * TAU;
        let shape = flower(o.flower, x, y, r, dir, rot, o.petals);
        if o.flower.head() {
            let l = shape.radius;
            let stride = (r * 0.3).max(3.0);
            let mut u = r * 0.4;
            while u <= l {
                if self.field.at(x + dir.cos() * u, y + dir.sin() * u) < r * 0.55 + o.margin {
                    return false;
                }
                u += stride;
            }
            let mut u = 0.0;
            while u <= l {
                self.field.stamp(x + dir.cos() * u, y + dir.sin() * u, r * 0.6);
                u += stride;
            }
        } else {
            self.field.stamp(x, y, r);
        }
        self.items.flowers.push(Shape {
            at: [x, y],
            t0,
            dur: r * 1.6,
            ..shape
        });
        true
    }

    fn dot(&mut self, x: f32, y: f32, r: f32, t0: f32) {
        self.items.dots.push([x, y, r, t0]);
        self.field.stamp(x, y, r);
    }

    fn decorate(&mut self, si: usize) {
        let o = self.o;
        let m = o.margin;
        // Leaves (and the end) are placed with the stem lifted out of the field, so its own
        // clearance doesn't refuse what grows from it; distance to it is checked directly.
        if let Some(log) = self.logs[si].take() {
            self.field.lift(&log);
        }
        let st = self.items.stems[si].clone();
        let n = st.pts.len();
        let [ex, ey] = st.pts[n - 1];
        let (ed, ew) = (st.dirs[n - 1], st.ws[n - 1]);
        if o.leaf != Leaf::None && o.leaf_every > 0.0 {
            let mut next = self.leaf_every * (0.4 + self.r.next() * 0.6);
            let mut along = 0.0;
            let mut side = if self.r.chance(0.5) { 1.0 } else { -1.0 };
            for i in 1..n.saturating_sub(2) {
                if self.late() {
                    break;
                }
                along += (st.pts[i][0] - st.pts[i - 1][0]).hypot(st.pts[i][1] - st.pts[i - 1][1]);
                if along < next {
                    continue;
                }
                next = along + self.leaf_every * (0.75 + self.r.next() * 0.5);
                let sd = if o.leaf_outer {
                    if st.ks[i] >= 0.0 { -1.0 } else { 1.0 }
                } else {
                    side = -side;
                    side
                };
                let a = st.dirs[i] + sd * o.leaf_angle;
                let (nx, ny) = (-st.dirs[i].sin() * sd, st.dirs[i].cos() * sd);
                let (bx, by) = (st.pts[i][0] + nx * st.ws[i] * 0.3, st.pts[i][1] + ny * st.ws[i] * 0.3);
                let l = self.sz.leaf_max * (0.75 + self.r.next() * 0.4);
                self.work -= 20;
                self.try_leaf(bx, by, a, l, sd, Some(si), st.t0 + along);
            }
        }
        let term = o.terminal;
        let mut end_dot = false;
        if st.end == End::Curl || term == Terminal::Ball {
            end_dot = matches!(term, Terminal::Ball | Terminal::Curl);
        } else {
            // Flowers end some stems whatever the ends are set to; flower ends, more often.
            let mut done = false;
            let chance = if term == Terminal::Flower {
                o.flower_chance + 0.25
            } else {
                o.flower_chance * 0.6
            };
            if o.flower != Flower::None && self.r.chance(chance) {
                let half = self.sz.flower_max * 0.5;
                let room = self.field.at(ex + ed.cos() * half, ey + ed.sin() * half);
                if o.flower.head() {
                    done = self.try_flower(ex, ey, ed, (room * 0.8).max(6.0), st.t0 + st.len);
                } else if room > 10.0 {
                    let r = (room - m).min(self.sz.flower_max);
                    let (cx, cy) = (ex + ed.cos() * (r + 1.0), ey + ed.sin() * (r + 1.0));
                    if self.field.at(cx, cy) > r * 0.8 + m {
                        done = self.try_flower(cx, cy, ed, r, st.t0 + st.len);
                    }
                }
            }
            if !done && matches!(term, Terminal::Leaf | Terminal::Flower) && o.leaf != Leaf::None {
                self.try_leaf(ex, ey, ed, self.sz.leaf_max, st.sign, Some(si), st.t0 + st.len);
            }
        }
        for i in 0..n {
            self.field.stamp(st.pts[i][0], st.pts[i][1], st.ws[i] / 2.0);
        }
        if end_dot {
            let k = if term == Terminal::Ball { 1.25 } else { 0.7 };
            self.dot(ex, ey, (ew * k).max(1.6), st.t0 + st.len);
        }
        // Penwork beads along the outer side.
        if o.beads > 0.0 {
            let every = lerp(46.0, 7.0, o.beads);
            let (mut along, mut next) = (0.0, every);
            for i in 1..n.saturating_sub(1) {
                along += (st.pts[i][0] - st.pts[i - 1][0]).hypot(st.pts[i][1] - st.pts[i - 1][1]);
                if along < next {
                    continue;
                }
                next = along + every;
                let sd = if st.ks[i] >= 0.0 { -1.0 } else { 1.0 };
                let r = (o.stem_w * 0.7 + 1.0).max(1.2);
                let off = st.ws[i] / 2.0 + m * 0.6 + r;
                let bx = st.pts[i][0] - st.dirs[i].sin() * sd * off;
                let by = st.pts[i][1] + st.dirs[i].cos() * sd * off;
                if self.field.at(bx, by) >= r + m * 0.5 {
                    self.items.beads.push([bx, by, r, st.t0 + along]);
                    self.field.stamp(bx, by, r);
                }
            }
        }
        // Branches, for later.
        if st.depth < o.max_depth && o.branch > 0.0 {
            let mut along = 0.0;
            let mut next = self.branch_every * (0.35 + self.r.next() * 0.5);
            for i in 1..n.saturating_sub(3) {
                along += (st.pts[i][0] - st.pts[i - 1][0]).hypot(st.pts[i][1] - st.pts[i - 1][1]);
                if along < next || along > st.len - 20.0 {
                    continue;
                }
                next = along + self.branch_every * (0.7 + self.r.next() * 0.6);
                let sd = if st.ks[i].abs() < 1e-4 {
                    if self.r.chance(0.5) { 1.0 } else { -1.0 }
                } else if st.wave {
                    st.ks[i].signum()
                } else {
                    -st.ks[i].signum()
                };
                let off = st.ws[i] * 0.35;
                let len = st.len * lerp(0.4, 0.8, self.r.next()) * if st.depth == 0 { 0.9 } else { 1.0 };
                self.queue.push_back(Want {
                    x: st.pts[i][0] - st.dirs[i].sin() * sd * off,
                    y: st.pts[i][1] + st.dirs[i].cos() * sd * off,
                    dir: st.dirs[i] + sd * o.branch_angle,
                    w: (st.ws[i] * self.thin).max(0.6),
                    len,
                    sign: sd,
                    depth: st.depth + 1,
                    grace: st.ws[i] + m + 3.0,
                    parent: Some(si),
                    t0: st.t0 + along,
                });
            }
        }
    }

    fn drain(&mut self) {
        while let Some(s) = self.queue.pop_front() {
            if self.late() {
                break;
            }
            if let Some(st) = self.try_stem(s) {
                self.decorate(st);
            }
        }
        self.queue.clear();
    }

    /// Starts a stem into the gap at (`bx`, `by`): off the nearest stem when one is close,
    /// otherwise out of whatever is nearest (the frame, the letter).
    #[allow(clippy::too_many_arguments)]
    fn sprout_into(&mut self, bx: f32, by: f32, bd: f32, w: f32, len: f32, depth: u32, t0: f32) -> bool {
        let o = self.o;
        let m = o.margin;
        let mut best = None;
        let mut best_d = (bd * 2.4 + 40.0).powi(2);
        for (si, st) in self.items.stems.iter().enumerate() {
            if st.ws[0] < w * 0.5 {
                continue;
            }
            let mut i = 2;
            while i + 2 < st.pts.len() {
                let d = (st.pts[i][0] - bx).powi(2) + (st.pts[i][1] - by).powi(2);
                if d < best_d {
                    best_d = d;
                    best = Some((si, i));
                }
                i += 2;
            }
        }
        if let Some((si, i)) = best {
            let st = &self.items.stems[si];
            let [px, py] = st.pts[i];
            let a = st.dirs[i];
            let to_gap = (by - py).atan2(bx - px);
            let fwd = if (to_gap - a).cos() >= 0.0 { a } else { a + PI };
            let diff = (to_gap - fwd).sin().atan2((to_gap - fwd).cos());
            let sd = if diff >= 0.0 { 1.0 } else { -1.0 };
            let dir = fwd + sd * diff.abs().min(o.branch_angle * 1.2);
            let (nx, ny) = (-a.sin(), a.cos());
            let side = if dir.cos() * nx + dir.sin() * ny >= 0.0 { 1.0 } else { -1.0 };
            let want = Want {
                x: px + nx * side * st.ws[i] * 0.35,
                y: py + ny * side * st.ws[i] * 0.35,
                dir,
                w: w.min(st.ws[i] * self.thin),
                len,
                sign: sd,
                depth: depth.max(st.depth + 1),
                grace: st.ws[i] + m + 3.0,
                parent: Some(si),
                t0: st.t0 + st.acc[i],
            };
            if let Some(ns) = self.try_stem(want) {
                self.decorate(ns);
                self.drain();
                return true;
            }
        }
        let (mut x, mut y) = (bx, by);
        for _ in 0..80 {
            let c = self.field.at(x, y);
            if c <= w / 2.0 + m + 1.5 {
                break;
            }
            let g = self.field.grad(x, y);
            let s = (c - (w / 2.0 + m)).min(6.0);
            x -= g[0] * s;
            y -= g[1] * s;
        }
        let dir = (by - y).atan2(bx - x) + (self.r.next() - 0.5) * 0.9;
        let want = Want {
            x,
            y,
            dir,
            w,
            len,
            sign: 0.0,
            depth,
            grace: 0.0,
            parent: None,
            t0,
        };
        match self.try_stem(want) {
            Some(st) => {
                self.decorate(st);
                self.drain();
                true
            }
            None => false,
        }
    }

    fn run(&mut self, seeds: &[Seed]) {
        let o = self.o;
        let m = o.margin;
        for (i, sd) in seeds.iter().enumerate() {
            let len = self.base_len * (0.8 + self.r.next() * 0.4);
            self.queue.push_back(Want {
                x: sd.x,
                y: sd.y,
                dir: sd.dir,
                w: o.stem_w,
                len,
                sign: 0.0,
                depth: 0,
                grace: sd.grace.unwrap_or(m + o.stem_w),
                parent: None,
                t0: i as f32 * 25.0,
            });
        }
        self.drain();
        // No starts given: the first stems sprout into the biggest openings.
        if seeds.is_empty() && o.mains > 0 && o.sprout > 0.0 {
            let (mut i, mut tries) = (0, 0);
            while i < o.mains && tries < o.mains * 4 && !self.late() {
                tries += 1;
                let (bx, by, bd) = self.field.biggest();
                if bd < self.sz.sprout_min {
                    break;
                }
                let len = self.base_len * (0.8 + self.r.next() * 0.4);
                if self.sprout_into(bx, by, bd, o.stem_w, len, 0, i as f32 * 30.0) {
                    i += 1;
                } else {
                    self.field.stamp(bx, by, bd * 0.3);
                }
            }
        }
        // Fill the biggest gaps until none is left above the threshold.
        if o.density > 0.0 {
            let mut guard = 0;
            while guard < 3000 && !self.late() {
                guard += 1;
                self.work -= 10;
                let (bx, by, bd) = self.field.biggest();
                if bd < self.sz.fill_min {
                    break;
                }
                let mut placed = false;
                if o.sprout > 0.0 && bd > self.sz.sprout_min && self.r.chance(o.sprout) {
                    let w = o.stem_w * lerp(0.55, 0.85, self.r.next());
                    let len = (bd * lerp(4.0, 8.0, self.r.next())).clamp(50.0, self.base_len.max(50.0));
                    let t = self.fill_t();
                    placed = self.sprout_into(bx, by, bd, w, len, 1, t);
                }
                if !placed && o.flower != Flower::None && !o.flower.head() && bd > 12.0 + m && self.r.chance(o.flower_chance) {
                    let (rot, t) = (self.r.next() * TAU, self.fill_t());
                    placed = self.try_flower(bx, by, rot, bd - m, t);
                }
                if !placed && o.leaf != Leaf::None && o.fill_leaves && bd > 8.0 + m {
                    let (mut x, mut y) = (bx, by);
                    for _ in 0..60 {
                        let c = self.field.at(x, y);
                        if c <= m + 1.5 {
                            break;
                        }
                        let g = self.field.grad(x, y);
                        let s = (c - m).min(6.0);
                        x -= g[0] * s;
                        y -= g[1] * s;
                    }
                    let dir = (by - y).atan2(bx - x);
                    let side = if self.r.chance(0.5) { 1.0 } else { -1.0 };
                    let t = self.fill_t();
                    let l = (self.sz.leaf_max * 1.2).min(bd * 2.6);
                    placed = self.try_leaf(x, y, dir, l, side, None, t);
                }
                if !placed && o.berries > 0.0 && self.r.chance(o.berries) && bd > m + 3.0 {
                    // A little cluster.
                    let r = ((bd - m) * 0.42).clamp(1.5, 7.0 * o.berry_size + 2.0);
                    let n = if bd > r * 3.0 + m { 3 + (self.r.next() * 4.0) as usize } else { 1 };
                    let tc = self.fill_t();
                    for i in 0..n {
                        let a = i as f32 * 2.4 + self.r.next();
                        let q = if i > 0 { r * 1.15 * (i as f32).sqrt() } else { 0.0 };
                        let (x, y) = (bx + a.cos() * q, by + a.sin() * q);
                        if self.field.at(x, y) >= r + m * 0.6 {
                            self.items.dots.push([x, y, r, tc + i as f32 * 3.0]);
                            self.field.stamp(x, y, r);
                            placed = true;
                        }
                    }
                }
                if !placed {
                    self.field.stamp(bx, by, (bd * 0.3).max(1.5));
                }
            }
        }
        // Tiny gaps become dots.
        if o.dots > 0.0 {
            let d_min = lerp(30.0, m * 0.5 + 1.2, o.dots);
            let mut guard = 0;
            while guard < 1500 && !self.late() {
                guard += 1;
                self.work -= 5;
                let (bx, by, bd) = self.field.biggest();
                if bd < d_min {
                    break;
                }
                let r = (bd - m).clamp(1.2, 4.0 + o.berry_size * 6.0);
                let t = self.fill_t();
                self.dot(bx, by, r, t);
            }
        }
    }
}
