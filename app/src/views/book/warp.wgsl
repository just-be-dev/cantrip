// The open book's pages as they really lie: each rises out of the gutter, bulges toward you
// and settles toward its fore-edge. Everything Rune drew under the slot (the painted pages and
// all the writing on them) is redrawn through that surface: the writing crowds into the
// gutter, its lines bow with the bulge, and the pages' edges lift with it. Leather, brass and
// anything else outside the open pages pass through unchanged.
//
// Coordinates are points from the book's top-left; `warp.rs` explains the uniforms.

struct U {
    // The slot in pass pixels: x, y, w, h.
    rect: vec4<f32>,
    // Pixels per point, how much of the curve to apply (0 to 1), the pass's size.
    k: vec4<f32>,
    // The book's width and height, the spine's x, the open pages' inset from the sides.
    book: vec4<f32>,
    // The open pages' top and bottom; the light's sideways reach; how far the slot reaches
    // past the book on every side, for pages bulging over its edges.
    page: vec4<f32>,
    // The sheets turning (`Sheet` in `book.rs`): the one face up on the right and the leaf
    // back up on the left, each drawn from its capture. Each: where
    // it lies flat, where it shows now, and (whether there is one, how upright it stands, how
    // far its shadow falls, whether it's the cover).
    recto_flat: vec4<f32>,
    recto_now: vec4<f32>,
    recto_k: vec4<f32>,
    verso_flat: vec4<f32>,
    verso_now: vec4<f32>,
    verso_k: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var backdrop: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;
@group(0) @binding(3) var leaf: texture_2d<f32>;
@group(0) @binding(4) var leaf_left: texture_2d<f32>;

// How high a page stands (points) and how fast it rises out of the gutter.
const RISE: f32 = 17.0;
const RISE_IN: f32 = 30.0;
// How much of a page's width the gutter swallows, and over how many points.
const CROWD: f32 = 12.0;
const CROWD_IN: f32 = 14.0;
// Perspective: a point `z` up looks `1 + z / NEAR` its size about the page's middle, and
// shifts `z * LIFT` up the screen.
const NEAR: f32 = 280.0;
const LIFT: f32 = 0.24;

struct Out {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let corner = vec2<f32>(f32(i & 1u), f32(i >> 1u));
    let px = u.rect.xy + corner * u.rect.zw;
    let ndc = vec2<f32>(px.x / u.k.z * 2.0 - 1.0, 1.0 - px.y / u.k.w * 2.0);
    var o: Out;
    o.pos = vec4<f32>(ndc, 0.0, 1.0);
    return o;
}

// The page's height `g` points from the spine on a page `w` wide, and its slope.
fn height(g: f32, w: f32) -> f32 {
    let r = g / w;
    return RISE * (1.0 - exp(-g / RISE_IN)) * (1.0 - 0.8 * r * r);
}
fn slope(g: f32, w: f32) -> f32 {
    let r = g / w;
    return RISE * (exp(-g / RISE_IN) / RISE_IN * (1.0 - 0.8 * r * r)
        - (1.0 - exp(-g / RISE_IN)) * 1.6 * g / (w * w));
}

// Where a point `g` along the page from the spine shows, seen from above: crowded into the
// gutter where the page dives into it, its fore-edge where it was.
fn across(g: f32, w: f32) -> f32 {
    return g - CROWD * ((1.0 - exp(-g / CROWD_IN)) - g / w * (1.0 - exp(-w / CROWD_IN)));
}
fn across_rate(g: f32, w: f32) -> f32 {
    return 1.0 - CROWD * (exp(-g / CROWD_IN) / CROWD_IN - (1.0 - exp(-w / CROWD_IN)) / w);
}

// Where a point of the book is in the pass's textures.
fn at_uv(b: vec2<f32>) -> vec2<f32> {
    return (u.rect.xy + (b + vec2<f32>(u.page.w)) * u.k.x) / u.k.zw;
}

fn inside(q: vec2<f32>, r: vec4<f32>) -> bool {
    return q.x >= r.x && q.x < r.x + r.z && q.y >= r.y && q.y < r.y + r.w;
}

// A standing sheet's light, `a` across it from the spine: darkest at the spine.
fn lit(a: f32, stand: f32) -> f32 {
    return 1.0 - 0.5 * stand * (1.0 - a) - 0.12 * stand * a;
}

// How much a sheet (`k`: there is one, how upright, how far its shadow falls) shades a point
// `d` past its free edge, on the pages.
fn shadow_of(d: f32, y: f32, k: vec4<f32>) -> f32 {
    if (k.x < 0.5 || k.z < 0.5 || d < 0.0 || d > k.z || y < u.page.x || y > u.page.y) {
        return 0.0;
    }
    return 0.22 * k.y * (1.0 - d / k.z);
}

struct Found {
    // The point of the page that shows, how far along it is from the spine, and whether it's
    // on an open page at all.
    src: vec2<f32>,
    g: f32,
    on_page: bool,
};

// Which point of the page shows at `p`: undo the crowding (Newton's method; `across` only
// grows), then the perspective.
fn locate(p: vec2<f32>) -> Found {
    let spine = u.book.z;
    let inset = u.book.w;
    let w = spine - inset;
    let top = u.page.x;
    let bottom = u.page.y;
    let mid = (top + bottom) * 0.5;
    let side = select(-1.0, 1.0, p.x >= spine);
    let seen = abs(p.x - spine);
    var g = seen;
    for (var n = 0; n < 6; n++) {
        g = clamp(g - (across(g, w) - seen) / across_rate(g, w), 0.0, w * 1.5);
    }
    let z = height(g, w);
    let y = mid + (p.y - mid + z * LIFT) / (1.0 + z / NEAR);
    var f: Found;
    f.src = vec2<f32>(spine + side * g, y);
    f.g = g;
    f.on_page = f.src.x > inset && f.src.x < u.book.x - inset && f.src.y > top && f.src.y < bottom;
    return f;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let scale = u.k.x;
    let strength = u.k.y;
    let w = u.book.z - u.book.w;
    let p = (in.pos.xy - u.rect.xy) / scale - vec2<f32>(u.page.w);
    let side = select(-1.0, 1.0, p.x >= u.book.z);
    let f = locate(p);

    var src = p;
    var shade = 1.0;
    if (f.on_page) {
        src = f.src;
        // Lit from the upper left: the slope facing it brightens, the other darkens.
        let s = slope(f.g, w);
        let nx = -side * s / sqrt(1.0 + s * s);
        shade = 1.0 - u.page.z * nx;
    } else {
        // Off the pages: in the shade of a page bulging over it, cast down and away from the
        // light.
        var covered = 0.0;
        for (var n = 1; n <= 4; n++) {
            let d = f32(n) * 1.6;
            covered += select(0.0, 1.0, locate(p - vec2<f32>(d * 0.4, d)).on_page);
        }
        shade = 1.0 - 0.13 * covered;
    }
    src = mix(p, src, strength);
    shade = mix(1.0, shade, strength);

    // The sheets in the air: each squeezed to the width it shows at and lifted as it stands,
    // writing and all, curved with the pages (the cover lies over them, flat). Shaded only as
    // it stands, so lying flat it's the page it lifts off or lands on, exactly.
    let sheet_at = select(src, p, u.recto_k.w > 0.5);
    let r = u.recto_now;
    if (u.recto_k.x > 0.5 && inside(sheet_at, r)) {
        let a = (sheet_at - r.xy) / r.zw;
        let flat = u.recto_flat.xy + a * u.recto_flat.zw;
        let c = textureSampleLevel(leaf, smp, at_uv(flat), 0.0);
        let k = select(shade, 1.0, u.recto_k.w > 0.5) * lit(a.x, u.recto_k.y);
        return vec4<f32>(c.rgb * k, c.a);
    }
    let v = u.verso_now;
    if (u.verso_k.x > 0.5 && inside(src, v)) {
        let a = (src - v.xy) / v.zw;
        let flat = u.verso_flat.xy + a * u.verso_flat.zw;
        let c = textureSampleLevel(leaf_left, smp, at_uv(flat), 0.0);
        return vec4<f32>(c.rgb * shade * lit(1.0 - a.x, u.verso_k.y), c.a);
    }
    // Their shadows, on the pages beside their free edges.
    shade *= 1.0 - shadow_of(src.x - (r.x + r.z), src.y, u.recto_k) - shadow_of(v.x - src.x, src.y, u.verso_k);

    let c = textureSampleLevel(backdrop, smp, at_uv(src), 0.0);
    return vec4<f32>(c.rgb * shade, c.a);
}
