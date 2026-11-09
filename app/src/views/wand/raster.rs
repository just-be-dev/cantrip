//! A small software rasterizer for the wand: z-buffered triangles with perspective-correct
//! attributes, drawn supersampled and averaged down, then soft star sprites over the result.
//! Colours are linear and premultiplied until `encode` turns them into what `Pixels` takes.

pub type V3 = [f32; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn norm(a: V3) -> V3 {
    let len = dot(a, a).sqrt();
    if len > 0.0 { mul(a, 1.0 / len) } else { a }
}
pub fn mix(a: V3, b: V3, t: f32) -> V3 {
    add(a, mul(sub(b, a), t))
}

/// A rotation, row-major.
#[derive(Clone, Copy)]
pub struct M3(pub [V3; 3]);

impl M3 {
    pub fn x(a: f32) -> M3 {
        let (s, c) = a.sin_cos();
        M3([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]])
    }
    pub fn y(a: f32) -> M3 {
        let (s, c) = a.sin_cos();
        M3([[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]])
    }
    pub fn z(a: f32) -> M3 {
        let (s, c) = a.sin_cos();
        M3([[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]])
    }
    pub fn then(self, outer: M3) -> M3 {
        let mut m = [[0.0; 3]; 3];
        for (r, row) in m.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| outer.0[r][k] * self.0[k][c]).sum();
            }
        }
        M3(m)
    }
    pub fn apply(&self, v: V3) -> V3 {
        [dot(self.0[0], v), dot(self.0[1], v), dot(self.0[2], v)]
    }
}

/// A projected vertex: device position, view depth (larger is farther), and attributes.
pub struct Vert<const A: usize> {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub attr: [f32; A],
}

/// Premultiplied linear RGBA with a depth per pixel.
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub color: Vec<[f32; 4]>,
    pub depth: Vec<f32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            color: vec![[0.0; 4]; w * h],
            depth: vec![f32::INFINITY; w * h],
        }
    }

    /// Fills a triangle (either winding), shading each pixel nearer than what's there.
    pub fn triangle<const A: usize>(
        &mut self,
        v: [&Vert<A>; 3],
        shade: &impl Fn(&[f32; A]) -> [f32; 4],
    ) {
        let edge = |a: &Vert<A>, b: &Vert<A>, x: f32, y: f32| {
            (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
        };
        let area = edge(v[0], v[1], v[2].x, v[2].y);
        if area.abs() < 1e-6 || v.iter().any(|v| v.z <= 0.0) {
            return;
        }
        let lo = |f: fn(&Vert<A>) -> f32| v.iter().map(|v| f(v)).fold(f32::INFINITY, f32::min);
        let hi = |f: fn(&Vert<A>) -> f32| v.iter().map(|v| f(v)).fold(f32::NEG_INFINITY, f32::max);
        let x0 = (lo(|v| v.x) - 0.5).floor().max(0.0) as usize;
        let y0 = (lo(|v| v.y) - 0.5).floor().max(0.0) as usize;
        let x1 = ((hi(|v| v.x) - 0.5).ceil() as isize).min(self.w as isize - 1);
        let y1 = ((hi(|v| v.y) - 0.5).ceil() as isize).min(self.h as isize - 1);
        if x1 < 0 || y1 < 0 {
            return;
        }
        let inv = [1.0 / v[0].z, 1.0 / v[1].z, 1.0 / v[2].z];
        for py in y0..=y1 as usize {
            for px in x0..=x1 as usize {
                let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                let b0 = edge(v[1], v[2], x, y) / area;
                let b1 = edge(v[2], v[0], x, y) / area;
                let b2 = 1.0 - b0 - b1;
                if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                    continue;
                }
                let (w0, w1, w2) = (b0 * inv[0], b1 * inv[1], b2 * inv[2]);
                let z = 1.0 / (w0 + w1 + w2);
                let i = py * self.w + px;
                if z >= self.depth[i] {
                    continue;
                }
                let mut attr = [0.0; A];
                for (k, a) in attr.iter_mut().enumerate() {
                    *a = (w0 * v[0].attr[k] + w1 * v[1].attr[k] + w2 * v[2].attr[k]) * z;
                }
                self.depth[i] = z;
                self.color[i] = shade(&attr);
            }
        }
    }

    /// Averages `f`×`f` blocks: colour by mean, depth by the nearest.
    pub fn downsample(&self, f: usize) -> Canvas {
        let (w, h) = (self.w / f, self.h / f);
        let mut out = Canvas::new(w, h);
        let n = (f * f) as f32;
        for y in 0..h {
            for x in 0..w {
                let (mut c, mut d) = ([0.0; 4], f32::INFINITY);
                for sy in 0..f {
                    for sx in 0..f {
                        let i = (y * f + sy) * self.w + x * f + sx;
                        for (sum, v) in c.iter_mut().zip(self.color[i]) {
                            *sum += v;
                        }
                        d = d.min(self.depth[i]);
                    }
                }
                out.color[y * w + x] = c.map(|v| v / n);
                out.depth[y * w + x] = d;
            }
        }
        out
    }

    /// A four-point star with a soft glow and a white-hot middle, centred on `at` (device x
    /// and y, view depth), `radius` device pixels to its points, hidden behind anything nearer.
    pub fn star(&mut self, at: V3, radius: f32, angle: f32, color: V3, alpha: f32) {
        let [cx, cy, z] = at;
        if radius < 0.3 || alpha <= 0.0 {
            return;
        }
        let at =
            |x: f32, y: f32| (y as usize).min(self.h - 1) * self.w + (x as usize).min(self.w - 1);
        if cx >= 0.0 && cy >= 0.0 && self.depth[at(cx, cy)] < z {
            return;
        }
        let (s, c) = (-angle).sin_cos();
        let reach = radius + 1.0;
        let x0 = (cx - reach).floor().max(0.0) as usize;
        let y0 = (cy - reach).floor().max(0.0) as usize;
        let x1 = ((cx + reach).ceil() as isize).min(self.w as isize - 1);
        let y1 = ((cy + reach).ceil() as isize).min(self.h as isize - 1);
        if x1 < 0 || y1 < 0 {
            return;
        }
        // A pixel's width, in the star's unit square: the antialiasing ramp.
        let soft = 1.4 / radius;
        for py in y0..=y1 as usize {
            for px in x0..=x1 as usize {
                let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
                let (x, y) = ((dx * c - dy * s) / radius, (dx * s + dy * c) / radius);
                // An astroid: concave sides pinched to the middle, points on the axes.
                let d = (x.abs().sqrt() + y.abs().sqrt()).powi(2);
                let body = ((1.0 - d) / soft).clamp(0.0, 1.0);
                let r2 = x * x + y * y;
                let glow = 0.45 * (-r2 / 0.06).exp();
                let core = (-r2 / 0.012).exp();
                let a = body.max(glow).max(core) * alpha;
                if a <= 0.002 {
                    continue;
                }
                let tint = mix(color, [1.0; 3], core * 0.85);
                let src = [tint[0] * a, tint[1] * a, tint[2] * a, a];
                let dst = &mut self.color[py * self.w + px];
                for k in 0..4 {
                    dst[k] = src[k] + dst[k] * (1.0 - a);
                }
            }
        }
    }

    /// Straight-alpha sRGB bytes, rows top first.
    pub fn encode(&self) -> Vec<u8> {
        let enc = |v: f32| {
            let v = v.clamp(0.0, 1.0);
            let s = if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            (s * 255.0 + 0.5) as u8
        };
        let mut out = Vec::with_capacity(self.w * self.h * 4);
        for c in &self.color {
            let a = c[3].clamp(0.0, 1.0);
            if a <= 0.0 {
                out.extend_from_slice(&[0; 4]);
                continue;
            }
            out.extend_from_slice(&[
                enc(c[0] / a),
                enc(c[1] / a),
                enc(c[2] / a),
                (a * 255.0 + 0.5) as u8,
            ]);
        }
        out
    }
}
