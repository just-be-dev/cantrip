//! Leaves and flowers, from initialis's `render/src/shapes.js` and the leaf and flower kinds it
//! ships (`kinds/leaf/`, `kinds/flower/`): only the ones the grimoire's initials grow.

use std::f32::consts::{PI, TAU};

use super::geom::{P, circle_poly};

/// What a leaf or flower is drawn as: shapes to fill, detail lines, dots, and the discs that
/// cover it for fitting it among the rest (`[x, y, r, t]`, `t` how far from its base).
#[derive(Clone, Default, Debug)]
pub struct Shape {
    pub polys: Vec<Vec<P>>,
    pub lines: Vec<Vec<P>>,
    pub dots: Vec<[f32; 3]>,
    pub cover: Vec<[f32; 4]>,
    /// How far a flower reaches from where it sits.
    pub radius: f32,
    /// Where it grows from, when it starts growing, and for how long (in stem lengths).
    pub at: P,
    pub t0: f32,
    pub dur: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Profile {
    Lance,
    Willow,
    Ovate,
    Teardrop,
    Petal,
    Acanthus,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Detail {
    None,
    Vein,
    Veins,
    Hatch,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Leaf {
    None,
    Acanthus,
    Trefoil,
    Ivy,
    Lance,
    Ovate,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flower {
    None,
    Rosette,
    Quatre,
    Daisy,
    /// A closed bud between two sepals, on the end of a stem.
    Bud,
}

impl Flower {
    /// Sits on the end of a stem, pointing along it.
    pub fn head(self) -> bool {
        self == Flower::Bud
    }
}

/// The leaf settings a blade is cut to.
#[derive(Clone, Copy, Debug)]
pub struct Blade {
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub len: f32,
    pub width: f32,
    pub bend: f32,
    pub curl: f32,
    pub lobes: f32,
    pub depth: f32,
    pub n: usize,
}

fn frac(x: f32) -> f32 {
    x - x.floor()
}

/// Half width along the blade, 0 to 1, for one side.
fn profile(kind: Profile, t: f32, side: f32, o: &Blade) -> f32 {
    let s = f32::sin;
    match kind {
        Profile::Lance => s(PI * t).max(0.0).powf(0.9),
        Profile::Willow => s(PI * t).max(0.0).powf(1.2),
        Profile::Ovate => s(PI * t.powf(0.68)).max(0.0).powf(0.7),
        Profile::Teardrop => {
            if t < 0.68 {
                s(PI / 2.0 * t / 0.68).max(0.0).powf(0.75)
            } else {
                (1.0 - ((t - 0.68) / 0.32).powi(2)).max(0.0).sqrt()
            }
        }
        Profile::Petal => {
            if t < 0.55 {
                (t / 0.55).powf(0.65)
            } else {
                (1.0 - ((t - 0.55) / 0.45).powi(2)).max(0.0).sqrt()
            }
        }
        Profile::Acanthus => {
            let env = s(PI * t.powf(0.8)).max(0.0).powf(0.7);
            let n = o.lobes.max(1.0);
            let f = frac(t * n + if side > 0.0 { 0.0 } else { 0.5 } + 0.15);
            let lobe = if f < 0.78 {
                (f / 0.78).powf(0.6)
            } else {
                1.0 - (f - 0.78) / 0.22 * 0.92
            };
            env * (1.0 - o.depth + o.depth * lobe)
        }
    }
}

/// A single blade along a bending axis.
pub struct Cut {
    pub poly: Vec<P>,
    pub axis: Vec<P>,
    pub dirs: Vec<f32>,
    pub half: Vec<f32>,
}

pub fn blade(kind: Profile, o: &Blade) -> Cut {
    let n = o.n.max(2);
    let step = o.len / n as f32;
    let (mut x, mut y) = (o.x, o.y);
    let mut axis = Vec::with_capacity(n + 1);
    let mut dirs = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = o.dir + o.bend * t + o.curl * t * t * t;
        axis.push([x, y]);
        dirs.push(a);
        x += a.cos() * step;
        y += a.sin() * step;
    }
    let w = o.len * o.width * 0.5;
    let mut left = Vec::with_capacity(n + 1);
    let mut right = Vec::with_capacity(n + 1);
    let mut half = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let (hl, hr) = (profile(kind, t, 1.0, o), profile(kind, t, -1.0, o));
        let (nx, ny) = (-dirs[i].sin(), dirs[i].cos());
        let [ax, ay] = axis[i];
        left.push([ax + nx * hl * w, ay + ny * hl * w]);
        right.push([ax - nx * hr * w, ay - ny * hr * w]);
        half.push(hl.max(hr) * w);
    }
    let mut poly = left;
    poly.extend(right.iter().rev().skip(1).take(n.saturating_sub(1)));
    Cut {
        poly,
        axis,
        dirs,
        half,
    }
}

/// Veins or hatching for a blade.
fn blade_detail(b: &Cut, kind: Detail) -> Vec<Vec<P>> {
    let mut lines = Vec::new();
    let n = b.axis.len() - 1;
    if matches!(kind, Detail::Vein | Detail::Veins) {
        lines.push(b.axis[1..((n as f32 * 0.82).round() as usize).max(2)].to_vec());
    }
    if kind == Detail::Veins {
        let m = (n / 5).max(2);
        for k in 1..=m {
            let i = (k as f32 / (m + 1) as f32 * n as f32 * 0.8).round() as usize;
            let j = (i + 3).min(n);
            for sd in [1.0f32, -1.0] {
                let [ax, ay] = b.axis[i];
                let (nx, ny) = (-b.dirs[j].sin() * sd, b.dirs[j].cos() * sd);
                let [bx, by] = b.axis[j];
                let r = b.half[j] * 0.7;
                lines.push(vec![[ax, ay], [bx + nx * r, by + ny * r]]);
            }
        }
    }
    if kind == Detail::Hatch {
        let m = ((n as f32 * 0.7).round() as usize).max(3);
        for k in 1..m {
            let i = (k as f32 / m as f32 * n as f32 * 0.92).round() as usize;
            let [ax, ay] = b.axis[i];
            let a = b.dirs[i];
            let r = b.half[i] * 0.85;
            if r < 1.0 {
                continue;
            }
            let (nx, ny) = (-a.sin(), a.cos());
            lines.push(vec![
                [ax, ay],
                [ax + nx * r + a.cos() * r * 0.35, ay + ny * r + a.sin() * r * 0.35],
            ]);
        }
    }
    lines
}

fn blade_cover(b: &Cut, step: usize) -> Vec<[f32; 4]> {
    let last = (b.axis.len() - 1) as f32;
    (0..b.axis.len())
        .step_by(step)
        .map(|i| [b.axis[i][0], b.axis[i][1], b.half[i], i as f32 / last])
        .collect()
}

fn plain(kind: Profile, o: &Blade, detail: Detail) -> Shape {
    let b = blade(kind, o);
    Shape {
        lines: blade_detail(&b, detail),
        cover: blade_cover(&b, 2),
        polys: vec![b.poly],
        ..Shape::default()
    }
}

pub fn leaf(kind: Leaf, o: &Blade, detail: Detail) -> Shape {
    match kind {
        Leaf::None | Leaf::Lance => plain(Profile::Lance, o, detail),
        Leaf::Ovate => plain(Profile::Ovate, o, detail),
        Leaf::Acanthus => plain(Profile::Acanthus, o, detail),
        Leaf::Trefoil => trefoil(o, detail),
        Leaf::Ivy => ivy(o, detail),
    }
}

/// Three teardrops on a short stalk.
fn trefoil(o: &Blade, detail: Detail) -> Shape {
    let mut out = Shape::default();
    let stalk = o.len * 0.18;
    let (cx, cy) = (o.x + o.dir.cos() * stalk, o.y + o.dir.sin() * stalk);
    for i in 0..3 {
        let a = o.dir + (i as f32 / 2.0 - 0.5) * 2.0 * 0.85;
        let len = o.len * if i == 1 { 0.82 } else { 0.66 };
        let b = blade(
            Profile::Teardrop,
            &Blade {
                x: cx,
                y: cy,
                dir: a + o.bend * 0.3,
                len,
                width: (o.width * 0.95).max(0.4),
                bend: o.bend * 0.3,
                curl: 0.0,
                n: 14,
                ..*o
            },
        );
        let vein = if detail == Detail::None {
            Detail::None
        } else {
            Detail::Vein
        };
        out.lines.extend(blade_detail(&b, vein));
        for d in blade_cover(&b, 2) {
            out.cover.push([d[0], d[1], d[2], 0.18 + d[3] * len / o.len]);
        }
        out.polys.push(b.poly);
    }
    let st = blade(
        Profile::Willow,
        &Blade {
            len: stalk * 1.4,
            width: 0.18,
            curl: 0.0,
            bend: 0.0,
            n: 6,
            ..*o
        },
    );
    out.polys.push(st.poly);
    out
}

/// Palmate, lobed, notched at the stalk.
fn ivy(o: &Blade, detail: Detail) -> Shape {
    let lobes = if o.lobes >= 4.0 { 5.0 } else { 3.0 };
    let r = o.len * 0.52;
    let (cx, cy) = (o.x + o.dir.cos() * r * 0.95, o.y + o.dir.sin() * r * 0.95);
    let m = 90;
    let poly: Vec<P> = (0..m)
        .map(|i| {
            let a = o.dir + PI + i as f32 / m as f32 * TAU;
            let rel = a - o.dir;
            let lob = (rel * lobes / 2.0).cos().abs().powf(1.6);
            let notch = 1.0 - 0.55 * (-((rel.cos() + 1.0) * 18.0)).exp();
            let rr = r * (0.58 + 0.42 * lob) * notch * (1.0 + o.width * 0.2 - 0.1);
            let b = a + o.bend * 0.2;
            [cx + b.cos() * rr, cy + b.sin() * rr]
        })
        .collect();
    let mut lines = Vec::new();
    if detail != Detail::None {
        for k in 0..lobes as usize {
            let spread = if lobes == 3.0 { 2.1 } else { 3.4 };
            let a = o.dir + (k as f32 / (lobes - 1.0) - 0.5) * spread;
            lines.push(vec![
                [o.x + o.dir.cos() * r * 0.5, o.y + o.dir.sin() * r * 0.5],
                [cx + a.cos() * r * 0.75, cy + a.sin() * r * 0.75],
            ]);
        }
    }
    let mut cover = vec![[cx, cy, r * 0.55, 0.5]];
    for p in poly.iter().step_by(6) {
        let (px, py) = (p[0], p[1]);
        cover.push([
            px + (cx - px) * 0.2,
            py + (cy - py) * 0.2,
            (px - cx).hypot(py - cy) * 0.2,
            0.4 + 0.6 * (px - o.x).hypot(py - o.y) / (2.0 * r),
        ]);
    }
    Shape {
        polys: vec![poly],
        lines,
        cover,
        ..Shape::default()
    }
}

/// A flower at (`x`, `y`), radius `r`, turned by `rot`; a head points along `dir`.
pub fn flower(kind: Flower, x: f32, y: f32, r: f32, dir: f32, rot: f32, petals: usize) -> Shape {
    let mut out = Shape {
        radius: r,
        ..Shape::default()
    };
    let petal = |a: f32, r0: f32, r1: f32, width: f32, kind: Profile| {
        blade(
            kind,
            &Blade {
                x: x + a.cos() * r0,
                y: y + a.sin() * r0,
                dir: a,
                len: r1 - r0,
                width,
                bend: 0.0,
                curl: 0.0,
                lobes: 1.0,
                depth: 0.0,
                n: 14,
            },
        )
        .poly
    };
    match kind {
        Flower::None => {}
        Flower::Rosette => {
            let n = petals.max(3);
            let w = (2.6 * (PI / n as f32).sin() * 1.25).min(1.25);
            for i in 0..n {
                let a = rot + i as f32 / n as f32 * TAU;
                out.polys.push(petal(a, r * 0.36, r, w, Profile::Petal));
            }
            out.polys.push(circle_poly(x, y, r * 0.27));
            if r > 10.0 {
                let mut ring = circle_poly(x, y, r * 0.15);
                ring.push(ring[0]);
                out.lines.push(ring);
            }
        }
        Flower::Quatre => {
            for i in 0..4 {
                let a = rot + i as f32 / 4.0 * TAU;
                out.polys.push(petal(a, r * 0.12, r, 1.3, Profile::Petal));
            }
            out.polys.push(circle_poly(x, y, r * 0.18));
        }
        Flower::Daisy => {
            let n = ((petals as f32 * 2.2).round() as usize).max(10);
            let w = (2.4 * (PI / n as f32).sin()).min(0.5);
            for i in 0..n {
                let a = rot + i as f32 / n as f32 * TAU;
                out.polys.push(petal(a, r * 0.32, r, w, Profile::Lance));
            }
            out.polys.push(circle_poly(x, y, r * 0.25));
            for i in 0..7 {
                let a = i as f32 / 7.0 * TAU;
                let q = if i == 0 { 0.0 } else { r * 0.13 };
                out.dots.push([x + a.cos() * q, y + a.sin() * q, r * 0.035]);
            }
        }
        Flower::Bud => {
            let bl = |dir: f32, len: f32, width: f32, bend: f32, kind: Profile, n: usize| {
                blade(
                    kind,
                    &Blade {
                        x,
                        y,
                        dir,
                        len,
                        width,
                        bend,
                        curl: 0.0,
                        lobes: 1.0,
                        depth: 0.0,
                        n,
                    },
                )
            };
            let c = bl(dir, r * 1.6, 0.6, 0.0, Profile::Teardrop, 14);
            out.lines.push(c.axis[4..12].to_vec());
            out.polys.push(c.poly);
            for sd in [1.0f32, -1.0] {
                let s = bl(dir + sd * 0.55, r * 0.95, 0.35, -sd * 0.6, Profile::Lance, 10);
                out.polys.push(s.poly);
            }
            out.radius = r * 1.6;
        }
    }
    out
}
