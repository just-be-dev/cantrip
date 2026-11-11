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
    // The sheet turning face up on the right (`leaf_on.x` > 0.5 when there is one): where it
    // lies flat in the capture, where it shows now, and how upright it stands (`leaf_on.y`).
    leaf_flat: vec4<f32>,
    leaf_now: vec4<f32>,
    leaf_on: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var backdrop: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;
@group(0) @binding(3) var leaf: texture_2d<f32>;

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

    // The turning sheet: its capture, squeezed to the width it shows at and lifted as it
    // stands, writing and all, then curved with the pages like everything else.
    let r = u.leaf_now;
    if (u.leaf_on.x > 0.5 && src.x >= r.x && src.x < r.x + r.z && src.y >= r.y && src.y < r.y + r.w) {
        let a = (src.x - r.x) / r.z;
        let b = (src.y - r.y) / r.w;
        let flat = u.leaf_flat.xy + vec2<f32>(a, b) * u.leaf_flat.zw;
        let luv = (u.rect.xy + (flat + vec2<f32>(u.page.w)) * scale) / u.k.zw;
        let lc = textureSampleLevel(leaf, smp, luv, 0.0);
        // Darkest at the spine as it stands; a little shaded all over.
        let stand = u.leaf_on.y;
        let lit = 1.0 - (0.1 + 0.45 * stand) * (1.0 - a) - 0.12 * stand * a;
        return vec4<f32>(lc.rgb * lit, lc.a);
    }

    let uv = (u.rect.xy + (src + vec2<f32>(u.page.w)) * scale) / u.k.zw;
    let c = textureSampleLevel(backdrop, smp, uv, 0.0);
    return vec4<f32>(c.rgb * shade, c.a);
}
