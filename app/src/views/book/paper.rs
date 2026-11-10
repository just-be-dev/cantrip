//! The book's materials, painted on the CPU into `Pixels` once per display scale and kept:
//! the open book's two halves (leather boards with brass corners, the pages beneath showing at
//! the fore-edges, and the open pages: mottled, foxed and tide-marked parchment, burnt at the
//! edges, curving into the gutter, lit by a candle above), a loose leaf for turning, and the
//! front cover, outside (tooled in gilt round a sigil) and in.
//!
//! The book is the same in every theme; only its inks change (`book.rs`, `Ink`). Everything
//! here is in points of the spread (`BOOK_W` × `BOOK_H`), sampled at each pixel's centre, and
//! deterministic: noise is hashed, so headless renders repeat.

use std::sync::{Arc, Mutex};

use rune::Pixels;

use super::{BOOK_H, BOOK_W, PAGE_H, PAGE_INSET_X, PAGE_INSET_Y, PAGE_W, SPINE};

/// Leather showing round the page block, at the top and outer sides.
const BOARD: f32 = 7.0;
/// How far the pages beneath show past the open ones: outer side, bottom, top.
const STACK_SIDE: f32 = PAGE_INSET_X - BOARD;
const STACK_BOTTOM: f32 = BOOK_H - PAGE_INSET_Y - PAGE_H - BOARD;
const STACK_TOP: f32 = PAGE_INSET_Y - BOARD;
const CORNER: f32 = 7.0;

type Rgb = [f32; 3];

const LEATHER_DARK: Rgb = [0.11, 0.06, 0.035];
const LEATHER: Rgb = [0.27, 0.15, 0.08];
const LEATHER_WORN: Rgb = [0.47, 0.31, 0.17];
const BRASS: Rgb = [0.78, 0.60, 0.28];
const TARNISH: Rgb = [0.36, 0.27, 0.13];
const GILT: Rgb = [0.80, 0.62, 0.30];
const PARCH_LIGHT: Rgb = [0.95, 0.87, 0.69];
const PARCH_DARK: Rgb = [0.77, 0.63, 0.42];
const STAIN: Rgb = [0.55, 0.37, 0.19];
const FOX: Rgb = [0.60, 0.39, 0.20];
const BURN: Rgb = [0.47, 0.30, 0.14];
const SCORCH: Rgb = [0.24, 0.14, 0.07];
const EDGES: Rgb = [0.86, 0.74, 0.52];
const HEADBAND: [Rgb; 2] = [[0.55, 0.12, 0.08], [0.82, 0.66, 0.34]];

/// The book's pictures at one display scale.
pub struct Textures {
    /// The open book, either side of the spine.
    pub left: Pixels,
    pub right: Pixels,
    /// A loose leaf: face up on the right, and its back as it lands on the left.
    pub leaf_right: Pixels,
    pub leaf_left: Pixels,
    /// The front cover, and its inside as it lands on the left.
    pub cover: Pixels,
    pub inside: Pixels,
}

/// Paint the book for `scale` off the UI thread, so the first opening needn't wait for it.
pub fn prepare(scale: f32) {
    std::thread::spawn(move || {
        textures(scale);
    });
}

/// The book at display `scale`, painted the first time it's asked for (about 100 ms).
pub fn textures(scale: f32) -> Arc<Textures> {
    static CACHE: Mutex<Vec<(u32, Arc<Textures>)>> = Mutex::new(Vec::new());
    let scale = resolution(scale);
    let key = (scale * 100.0).round() as u32;
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, t)) = cache.iter().find(|(k, _)| *k == key) {
        return t.clone();
    }
    let t = Arc::new(Textures::paint(scale));
    cache.push((key, t.clone()));
    t
}

/// Pixels per point to paint at. Tests only need the shapes.
fn resolution(scale: f32) -> f32 {
    if cfg!(test) {
        0.25
    } else {
        scale.clamp(1.0, 3.0)
    }
}

impl Textures {
    fn paint(scale: f32) -> Textures {
        let half = |x0: f32| paint(SPINE, BOOK_H, scale, |x, y| spread(x0 + x, y)).pixels();
        let leaf = paint(PAGE_W, PAGE_H, scale, leaf);
        let cover = paint(SPINE, BOOK_H, scale, cover);
        let inside = paint(SPINE, BOOK_H, scale, inside);
        Textures {
            left: half(0.0),
            right: half(SPINE),
            leaf_left: leaf.mirrored().pixels(),
            leaf_right: leaf.pixels(),
            cover: cover.pixels(),
            inside: inside.pixels(),
        }
    }
}

// MARK: Painting

struct Image {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

impl Image {
    fn pixels(&self) -> Pixels {
        let p = Pixels::new();
        p.set(self.w as u32, self.h as u32, self.rgba.clone());
        p
    }

    fn mirrored(&self) -> Image {
        let mut rgba = Vec::with_capacity(self.rgba.len());
        for row in self.rgba.chunks(self.w * 4) {
            for px in row.chunks(4).rev() {
                rgba.extend_from_slice(px);
            }
        }
        Image {
            w: self.w,
            h: self.h,
            rgba,
        }
    }
}

/// Paints a `w` × `h` point picture at `scale`, `at` giving each pixel's sRGB colour and
/// straight alpha from its centre in points. Bands of rows paint on every core.
fn paint(w: f32, h: f32, scale: f32, at: impl Fn(f32, f32) -> [f32; 4] + Sync) -> Image {
    let (pw, ph) = ((w * scale).round() as usize, (h * scale).round() as usize);
    let mut rgba = vec![0u8; pw * ph * 4];
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let band = ph.div_ceil(cores).max(1);
    std::thread::scope(|scope| {
        for (b, chunk) in rgba.chunks_mut(band * pw * 4).enumerate() {
            let at = &at;
            scope.spawn(move || {
                for (n, px) in chunk.chunks_mut(4).enumerate() {
                    let (i, j) = (n % pw, b * band + n / pw);
                    let c = at((i as f32 + 0.5) / scale, (j as f32 + 0.5) / scale);
                    for (out, v) in px.iter_mut().zip(c) {
                        *out = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    }
                }
            });
        }
    });
    Image { w: pw, h: ph, rgba }
}

// MARK: Noise

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Value noise, 0 to 1.
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let (fx, fy) = (x - xf, y - yf);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (i, j) = (xf as i32, yf as i32);
    let a = hash(i, j, seed);
    let b = hash(i + 1, j, seed);
    let c = hash(i, j + 1, seed);
    let d = hash(i + 1, j + 1, seed);
    lerp(lerp(a, b, ux), lerp(c, d, ux), uy)
}

/// Fractal noise, 0 to 1.
fn fbm(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut f) = (0.0, 0.5, 0.0, 1.0);
    for o in 0..octaves {
        sum += amp * noise(x * f + o as f32 * 17.3, y * f - o as f32 * 9.1, seed + o);
        norm += amp;
        amp *= 0.5;
        f *= 2.03;
    }
    sum / norm
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

fn scale(c: Rgb, k: f32) -> Rgb {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn opaque(c: Rgb) -> [f32; 4] {
    [c[0], c[1], c[2], 1.0]
}

const CLEAR: [f32; 4] = [0.0; 4];

/// Signed distance to a rounded rectangle (`x0`, `y0`, `x1`, `y1`), negative inside.
fn rounded(x: f32, y: f32, r: [f32; 4], radius: f32) -> f32 {
    let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let (hx, hy) = ((r[2] - r[0]) / 2.0 - radius, (r[3] - r[1]) / 2.0 - radius);
    let (qx, qy) = ((x - cx).abs() - hx, (y - cy).abs() - hy);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

/// Distance to the segment `a`–`b`.
fn segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

/// Coverage of a line `width` wide at distance `d`.
fn stroke(d: f32, width: f32) -> f32 {
    smooth(width / 2.0 + 0.6, width / 2.0 - 0.3, d)
}

// MARK: Leather and brass

/// Calfskin: mottled, pebbled, creased, and rubbed pale where it's handled, `edge` points from
/// the board's edge.
fn leather(x: f32, y: f32, edge: f32) -> Rgb {
    let mottle = fbm(x * 0.018, y * 0.018, 4, 3);
    let grain = fbm(x * 0.55, y * 0.55, 3, 5);
    let warp = fbm(x * 0.03, y * 0.03, 2, 6) * 6.0;
    let crease = (1.0 - (noise(x * 0.09 + warp, y * 0.05, 7) * 2.0 - 1.0).abs()).powi(12);
    let mut c = mix(LEATHER_DARK, LEATHER, smooth(0.2, 0.8, mottle));
    c = scale(c, 0.78 + 0.38 * grain - 0.22 * crease);
    let rub = smooth(7.0, 0.0, edge + (fbm(x * 0.12, y * 0.12, 3, 8) - 0.5) * 9.0);
    let scuff = smooth(0.78, 0.9, fbm(x * 0.04, y * 0.25, 3, 9)) * 0.35;
    mix(
        c,
        scale(LEATHER_WORN, 0.75 + 0.4 * grain),
        (rub * 0.7 + scuff).min(1.0),
    )
}

/// A brass corner piece at a board's corner: (`cx`, `cy`) are distances from its two edges.
/// Its colour and coverage, and the shadow it casts.
fn brass(cx: f32, cy: f32, x: f32, y: f32) -> Option<(Rgb, f32)> {
    const LEG: f32 = 30.0;
    const THICK: f32 = 5.0;
    const IN: f32 = 0.8;
    // An L of two rounded bars along the edges, and a small square where they meet.
    let bar = |u: f32, v: f32| {
        let along = (u - LEG).max(0.0);
        let across = (v - IN - THICK / 2.0).abs() - THICK / 2.0;
        (along.powi(2) + across.max(0.0).powi(2)).sqrt() + across.min(0.0).max(-along)
    };
    let d = bar(cx, cy)
        .min(bar(cy, cx))
        .min(rounded(cx, cy, [IN, IN, 10.0, 10.0], 1.5));
    let cover = smooth(0.5, -0.5, d);
    if cover <= 0.0 {
        return None;
    }
    let tarnish = fbm(x * 0.3, y * 0.3, 3, 41);
    let mut c = mix(TARNISH, BRASS, smooth(0.25, 0.75, tarnish));
    // Bevelled: lit along its upper inside, dark at its rim.
    let bevel = smooth(0.0, 1.8, -d);
    c = scale(c, 0.55 + 0.65 * bevel);
    // Rivets.
    for (u, v) in [
        (LEG - 4.0, IN + THICK / 2.0),
        (IN + THICK / 2.0, LEG - 4.0),
        (5.5, 5.5),
    ] {
        let r = ((cx - u).powi(2) + (cy - v).powi(2)).sqrt();
        if r < 1.6 {
            let lit = smooth(
                1.6,
                0.0,
                ((cx - u + 0.5).powi(2) + (cy - v + 0.5).powi(2)).sqrt(),
            );
            c = mix(scale(TARNISH, 0.6), [1.0, 0.9, 0.62], lit);
        }
    }
    Some((c, cover))
}

/// The shadow a brass corner casts on the leather, down and in.
fn brass_shadow(cx: f32, cy: f32, x: f32, y: f32) -> f32 {
    brass(cx - 1.2, cy - 1.2, x, y).map_or(0.0, |(_, a)| a * 0.6)
}

// MARK: Parchment

/// Parchment at (`x`, `y`): mottled, fibrous, foxed and tide-marked, browning and burnt
/// within `edge` points of its edge.
fn parchment(x: f32, y: f32, edge: f32) -> Rgb {
    let wx = (fbm(x * 0.01, y * 0.01, 3, 21) - 0.5) * 70.0;
    let wy = (fbm(x * 0.01 + 5.0, y * 0.01, 3, 22) - 0.5) * 70.0;
    let mottle = fbm((x + wx) * 0.007, (y + wy) * 0.007, 5, 23);
    let fine = fbm(x * 0.08, y * 0.08, 3, 25);
    let fibre = noise(x * 0.03, y * 0.45, 27);
    let mut c = mix(PARCH_DARK, PARCH_LIGHT, smooth(0.28, 0.72, mottle));
    c = scale(c, 0.95 + 0.08 * fine + 0.03 * fibre);
    // Tide marks where water once dried.
    let tide = fbm(x * 0.005 + 3.0, y * 0.005, 4, 29);
    let ring = (-((tide - 0.6) / 0.012).powi(2)).exp();
    c = mix(c, STAIN, 0.13 * ring + 0.05 * smooth(0.6, 0.7, tide));
    // Foxing: rust-brown spots, in drifts.
    let drift = smooth(0.4, 0.7, fbm(x * 0.012, y * 0.012, 2, 30));
    let (wx, wy) = (
        (noise(x * 0.2, y * 0.2, 34) - 0.5) * 2.0,
        (noise(x * 0.2, y * 0.2, 35) - 0.5) * 2.0,
    );
    let fox = spots(x + wx, y + wy, 18.0, 31, 0.1 + 0.4 * drift);
    let speck = spots(x, y, 5.0, 32, 0.12 * drift);
    c = mix(c, FOX, (0.5 * fox + 0.45 * speck).min(1.0));
    // Browned toward the edges, scorched at the very edge.
    let ragged = edge + (fbm(x * 0.06, y * 0.06, 3, 33) - 0.5) * 18.0;
    c = mix(c, BURN, 0.72 * smooth(19.0, 0.0, ragged));
    mix(c, SCORCH, 0.85 * smooth(2.5, 0.0, edge))
}

/// Round spots scattered one to a `cell` with chance `density`: soft halos with darker
/// hearts, 0 to 1.
fn spots(x: f32, y: f32, cell: f32, seed: u32, density: f32) -> f32 {
    let (i, j) = ((x / cell).floor() as i32, (y / cell).floor() as i32);
    let mut v = 0.0f32;
    for dj in -1..=1 {
        for di in -1..=1 {
            let (ci, cj) = (i + di, j + dj);
            if hash(ci, cj, seed) >= density {
                continue;
            }
            let cx = (ci as f32 + hash(ci, cj, seed + 1)) * cell;
            let cy = (cj as f32 + hash(ci, cj, seed + 2)) * cell;
            let r = cell * (0.05 + 0.14 * hash(ci, cj, seed + 3).powi(2));
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            let halo = smooth(r * 2.2, r * 0.6, d) * 0.45;
            let heart = smooth(r, r * 0.5, d);
            v = v.max((halo + heart * 0.7) * (0.5 + 0.5 * hash(ci, cj, seed + 4)));
        }
    }
    v.min(1.0)
}

/// How a page's edge wanders: the margin to subtract from its distance to the edge.
fn tatter(x: f32, y: f32) -> f32 {
    (fbm(x * 0.3, y * 0.3, 3, 11) - 0.5) * 2.2 + (fbm(x * 0.06, y * 0.06, 2, 12) - 0.5) * 2.4
}

/// A page's curve as it rises out of the gutter, `g` points from the spine, and how its outer
/// edge falls away (`u`, 0 at the spine to 1 at the fore-edge).
fn curve(g: f32, u: f32, deep: f32) -> f32 {
    let gutter = 1.0 - deep * (0.62 * (-g / 7.0).exp() + 0.2 * (-g / 34.0).exp());
    let ridge = 0.05 * (-((g - 30.0) / 16.0).powi(2)).exp();
    gutter + ridge - 0.1 * smooth(0.82, 1.0, u)
}

/// Candlelight over the spread, from above the book's middle.
fn candle(x: f32, y: f32) -> f32 {
    let lx = (x - BOOK_W * 0.5) / BOOK_W;
    let ly = (y - BOOK_H * 0.15) / BOOK_H;
    1.08 - 0.5 * (lx * lx * 1.4 + ly * ly)
}

// MARK: The open book

/// The open book at (`x`, `y`) in the spread.
fn spread(x: f32, y: f32) -> [f32; 4] {
    let board = rounded(x, y, [0.0, 0.0, BOOK_W, BOOK_H], CORNER);
    if board > 0.5 {
        return CLEAR;
    }
    let left = x < SPINE;
    // Distances from the book's outer edge, and from the spine.
    let out = if left { x } else { BOOK_W - x };
    let g = (x - SPINE).abs();
    let (top, bottom) = (y - PAGE_INSET_Y, PAGE_INSET_Y + PAGE_H - y);
    let fore = out - PAGE_INSET_X;
    let light = candle(x, y);

    // The open page, its outer edges tattered.
    let edge = fore.min(top).min(bottom);
    if edge + tatter(x, y) > 0.0 {
        let u = 1.0 - g / PAGE_W;
        let c = parchment(x, y, edge.max(0.0));
        return opaque(scale(c, curve(g, u, 1.0) * light));
    }

    // The pages beneath, their edges in fine lines parallel to the fore-edge.
    let (dx, dy) = ((-fore).max(0.0), (-bottom).max(0.0));
    let dt = (-top).max(0.0);
    let depth = (dx / STACK_SIDE)
        .hypot((dy / STACK_BOTTOM).max(dt / STACK_TOP.max(0.5)))
        .min(2.0);
    let wander = (fbm(x * 0.02, y * 0.02, 2, 51) - 0.5) * 1.6;
    if depth + (fbm(x * 0.25, y * 0.25, 2, 52) - 0.5) * 0.25 < 1.0 && g > 0.5 {
        let d = dx.hypot(dy.max(dt));
        let line = noise(d * 2.2 + wander, (x + y) * 0.004, 53);
        let gap = smooth(0.25, 0.0, line) * 0.4;
        let mut c = scale(EDGES, 0.8 + 0.28 * line - gap);
        c = mix(c, BURN, 0.45 * fbm(x * 0.05, y * 0.05, 2, 54));
        // Deeper pages sit lower, in shadow; the open page shades the first few.
        let shade = (1.0 - 0.42 * depth) * (1.0 - 0.4 * (-d / 1.2).exp());
        return opaque(scale(c, shade * light));
    }

    // Leather, shadowed where the pages sit on it and darker in the hinge.
    let mut c = leather(x, y, -board);
    let ao = smooth(5.0, 0.0, (depth - 1.0) * STACK_SIDE.min(STACK_BOTTOM));
    let hinge = smooth(14.0, 0.0, g) * 0.45;
    c = scale(c, (1.0 - 0.6 * ao) * (1.0 - hinge) * light);
    // Headbands at the top and foot of the spine.
    let band = |yb: f32| g < 3.5 && (y - yb).abs() < 2.2;
    if band(PAGE_INSET_Y - 1.5) || band(PAGE_INSET_Y + PAGE_H + 1.5) {
        let stripe = HEADBAND[((x * 1.3).floor() as i32).rem_euclid(2) as usize];
        c = scale(stripe, 0.7 * light);
    }
    // Brass at the outer corners.
    let (cx, cy) = (out, y.min(BOOK_H - y));
    let c = scale(c, 1.0 - brass_shadow(cx, cy, x, y));
    match brass(cx, cy, x, y) {
        Some((b, a)) => opaque(mix(c, scale(b, light), a)),
        None => [c[0], c[1], c[2], smooth(0.5, -0.5, board)],
    }
}

/// A loose leaf, face up as it lies on the right: spine at its left.
fn leaf(x: f32, y: f32) -> [f32; 4] {
    // Elsewhere in the same skin than the open pages.
    let (sx, sy) = (x + 1300.0, y + 700.0);
    let edge = (PAGE_W - x).min(y).min(PAGE_H - y);
    if edge + tatter(sx, sy) <= 0.0 {
        return CLEAR;
    }
    let c = parchment(sx, sy, edge.max(0.0));
    opaque(scale(c, curve(x, x / PAGE_W, 0.6)))
}

// MARK: The cover

/// The front cover, with its spine at the left: gilt rules and a sigil tooled into the
/// leather, brass at its corners.
fn cover(x: f32, y: f32) -> [f32; 4] {
    let board = rounded(x, y, [-CORNER, 0.0, SPINE, BOOK_H], CORNER);
    if board > 0.5 {
        return CLEAR;
    }
    let mut c = leather(x + 2000.0, y, (-board).min(x + 30.0));
    // The hinge, where the board bends.
    c = scale(
        c,
        1.0 - 0.45 * smooth(10.0, 0.0, x) + 0.06 * smooth(10.0, 12.0, x) * smooth(16.0, 12.0, x),
    );
    let p = [x, y];
    let (cx, cy) = (SPINE * 0.5 + 4.0, BOOK_H * 0.42);
    let mut lines: Vec<(f32, f32)> = Vec::new(); // (distance, width)
    for (inset, w) in [(14.0, 1.2), (18.5, 0.7)] {
        let r = [22.0 + inset - 14.0, inset, SPINE - inset, BOOK_H - inset];
        lines.push((rounded(x, y, r, 2.0).abs(), w));
    }
    // Corner fleurons on the inner frame: small diamonds.
    for (fx, fy) in [
        (26.5, 18.5),
        (SPINE - 18.5, 18.5),
        (26.5, BOOK_H - 18.5),
        (SPINE - 18.5, BOOK_H - 18.5),
    ] {
        let d = (x - fx).abs() + (y - fy).abs();
        lines.push(((d - 5.0).abs(), 0.9));
    }
    // The sigil: two rings, a seven-pointed star, an eye of a ring.
    let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    lines.push(((r - 62.0).abs(), 1.3));
    lines.push(((r - 56.0).abs(), 0.7));
    lines.push(((r - 11.0).abs(), 1.0));
    let star: Vec<[f32; 2]> = (0..7)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 7.0;
            [cx + 54.0 * a.cos(), cy + 54.0 * a.sin()]
        })
        .collect();
    for i in 0..7 {
        lines.push((segment(p, star[i], star[(i + 3) % 7]), 1.0));
    }
    // Ticks round the outer ring, like a dial.
    let a = (y - cy).atan2(x - cx);
    let tick =
        ((a / std::f32::consts::TAU * 28.0).fract() - 0.5).abs() * std::f32::consts::TAU / 28.0 * r;
    if (r - 66.5).abs() < 3.0 {
        lines.push((tick, 0.8));
    }
    let gilt = lines
        .iter()
        .map(|(d, w)| stroke(*d, *w))
        .fold(0.0f32, f32::max);
    // Pressed in: a dark lip below each line, gold rubbed off in places.
    let rubbed = 0.45 + 0.55 * smooth(0.3, 0.6, fbm(x * 0.08, y * 0.08, 3, 61));
    let below = lines
        .iter()
        .map(|(d, w)| stroke(d - 0.9, *w + 0.6))
        .fold(0.0f32, f32::max);
    c = scale(c, 1.0 - 0.35 * below * (1.0 - gilt));
    let lit = scale(GILT, 0.8 + 0.4 * fbm(x * 0.4, y * 0.4, 2, 62));
    c = mix(c, lit, gilt * rubbed);
    let (bx, by) = (SPINE - x, y.min(BOOK_H - y));
    let c = scale(c, (1.0 - brass_shadow(bx, by, x, y)) * 0.95);
    match brass(bx, by, x, y) {
        Some((b, a)) => opaque(mix(c, b, a)),
        None => [c[0], c[1], c[2], smooth(0.5, -0.5, board)],
    }
}

/// The cover's inside, lying on the left: the leather turned in round a pasted-down sheet.
fn inside(x: f32, y: f32) -> [f32; 4] {
    let board = rounded(x, y, [0.0, 0.0, SPINE + CORNER, BOOK_H], CORNER);
    if board > 0.5 {
        return CLEAR;
    }
    let paste = rounded(x, y, [14.0, 12.0, SPINE - 4.0, BOOK_H - 12.0], 1.0);
    let c = if paste + tatter(x + 900.0, y) * 0.5 < 0.0 {
        scale(parchment(x + 900.0, y + 400.0, -paste), 0.8)
    } else {
        let lip = smooth(3.0, 0.0, paste.abs()) * 0.4;
        scale(leather(x + 3000.0, y, -board), 0.85 - lip)
    };
    [c[0], c[1], c[2], smooth(0.5, -0.5, board)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spread_is_opaque_inside_and_clear_past_its_corners() {
        assert_eq!(spread(SPINE + 40.0, BOOK_H / 2.0)[3], 1.0);
        assert_eq!(spread(0.0, 0.0)[3], 0.0, "rounded corners");
        let page = spread(SPINE + 60.0, BOOK_H / 2.0);
        let board = spread(BOOK_W - 2.5, BOOK_H / 2.0);
        assert!(page[0] > board[0], "parchment is lighter than leather");
    }

    #[test]
    fn the_textures_paint_at_the_asked_scale() {
        let t = textures(2.0);
        assert_eq!(
            t.left.size(),
            (
                (SPINE * 0.25).round() as u32,
                (BOOK_H * 0.25).round() as u32
            )
        );
        assert!(Arc::ptr_eq(&t, &textures(2.0)), "painted once");
    }
}
