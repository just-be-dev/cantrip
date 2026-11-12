//! Seeded randomness, gradient noise and the small geometry the initials are built from, as in
//! initialis's `render/src/util.js`. Units are a 1000 × 1000 box, y down.

use std::f32::consts::TAU;

pub type P = [f32; 2];

/// mulberry32: the same stream initialis draws from, so a seed means the same thing.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed)
    }

    /// The next number in [0, 1).
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let a = self.0;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64 / 4_294_967_296.0) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }

    pub fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[((self.next() * of.len() as f32) as usize).min(of.len() - 1)]
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.next() < p
    }
}

/// FNV-1a over the parts, as initialis's `hash`.
pub fn hash(parts: &[&str]) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for part in parts {
        for unit in part.encode_utf16() {
            h ^= unit as u32;
            h = h.wrapping_mul(16_777_619);
        }
        h ^= 0x9e37;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

/// 2D gradient noise, about −1 to 1.
pub struct Noise {
    p: [u8; 512],
    g: [[f32; 2]; 256],
}

impl Noise {
    pub fn new(seed: u32) -> Noise {
        let mut r = Rng::new(seed);
        let mut base: Vec<u8> = (0..=255).collect();
        for i in (1..256).rev() {
            let j = (r.next() * (i + 1) as f32) as usize;
            base.swap(i, j.min(i));
        }
        let mut p = [0u8; 512];
        for (i, v) in p.iter_mut().enumerate() {
            *v = base[i & 255];
        }
        let mut g = [[0.0; 2]; 256];
        for v in &mut g {
            let a = r.next() * TAU;
            *v = [a.cos(), a.sin()];
        }
        Noise { p, g }
    }

    pub fn at(&self, x: f32, y: f32) -> f32 {
        let (xi, yi) = (x.floor(), y.floor());
        let (xf, yf) = (x - xi, y - yi);
        let (xx, yy) = ((xi as i64 & 255) as usize, (yi as i64 & 255) as usize);
        let p = &self.p;
        let grad = |h: u8, dx: f32, dy: f32| {
            let g = self.g[h as usize];
            g[0] * dx + g[1] * dy
        };
        let aa = p[p[xx] as usize + yy];
        let ab = p[p[xx] as usize + yy + 1];
        let ba = p[p[xx + 1] as usize + yy];
        let bb = p[p[xx + 1] as usize + yy + 1];
        let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let (u, v) = (fade(xf), fade(yf));
        let x1 = grad(aa, xf, yf) + u * (grad(ba, xf - 1.0, yf) - grad(aa, xf, yf));
        let x2 =
            grad(ab, xf, yf - 1.0) + u * (grad(bb, xf - 1.0, yf - 1.0) - grad(ab, xf, yf - 1.0));
        (x1 + v * (x2 - x1)) * 1.42
    }
}

pub fn signed_area(pts: &[P]) -> f32 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    a / 2.0
}

/// A closed polygon moved inward by `d` (outward when negative), mitred with a limit.
pub fn offset_polygon(pts: &[P], d: f32) -> Vec<P> {
    let n = pts.len();
    let s = if signed_area(pts) > 0.0 { 1.0 } else { -1.0 };
    (0..n)
        .map(|i| {
            let (p0, p1, p2) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            let (mut ax, mut ay) = (p1[0] - p0[0], p1[1] - p0[1]);
            let (mut bx, mut by) = (p2[0] - p1[0], p2[1] - p1[1]);
            let la = ax.hypot(ay).max(1e-6);
            let lb = bx.hypot(by).max(1e-6);
            ax /= la;
            ay /= la;
            bx /= lb;
            by /= lb;
            let (nax, nay, nbx, nby) = (-ay * s, ax * s, -by * s, bx * s);
            let (mut mx, mut my) = (nax + nbx, nay + nby);
            let ml = mx.hypot(my).max(1e-6);
            mx /= ml;
            my /= ml;
            let k = d / (mx * nax + my * nay).max(0.35);
            [p1[0] + mx * k, p1[1] + my * k]
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Box2 {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Box2 {
    pub fn of<'a>(contours: impl IntoIterator<Item = &'a Vec<P>>) -> Box2 {
        let mut b = Box2 {
            x0: f32::INFINITY,
            y0: f32::INFINITY,
            x1: f32::NEG_INFINITY,
            y1: f32::NEG_INFINITY,
        };
        for c in contours {
            for p in c {
                b.x0 = b.x0.min(p[0]);
                b.y0 = b.y0.min(p[1]);
                b.x1 = b.x1.max(p[0]);
                b.y1 = b.y1.max(p[1]);
            }
        }
        b
    }

    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }

    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }
}

pub fn circle_poly(x: f32, y: f32, r: f32) -> Vec<P> {
    let n = (r * 1.2).round().clamp(10.0, 64.0) as usize;
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * TAU;
            [x + a.cos() * r, y + a.sin() * r]
        })
        .collect()
}

