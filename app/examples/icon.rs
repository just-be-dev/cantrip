//! Cantrip's app icon, drawn with Rune in the bar's Arcane palette: a squircle of ink glass
//! with an aurora, a glowing orb, and a wand throwing four-point sparkles.
//!
//! `scripts/icon.sh` renders it to `app/Cantrip.icns`:
//! ```sh
//! cargo run --release --example icon -- --snap icon.png --scale 2   # 1024 px
//! ```

use rune::{App, AppSpec, Color, Cx, El, LineCap, Path, Shadow, Shape, WindowOptions, el};

/// The canvas, in points (1024 px at 2×).
const SIZE: f32 = 512.0;
/// Apple's icon grid: an 824/1024 body with a 22.37% continuous corner.
const BODY: f32 = SIZE * 824.0 / 1024.0;
const CORNER: f32 = BODY * 0.2237;

fn srgb(r: f32, g: f32, b: f32, a: f32) -> Color {
    let dec = rune::color::srgb_decode;
    Color { r: dec(r), g: dec(g), b: dec(b), a }
}

/// A four-point sparkle centred on (`x`, `y`), `r` from centre to tip, with curved waists.
fn sparkle(x: f32, y: f32, r: f32) -> Path {
    let k = r * 0.16;
    Path::new()
        .move_to([x, y - r])
        .quad_to([x + k, y - k], [x + r, y])
        .quad_to([x + k, y + k], [x, y + r])
        .quad_to([x - k, y + k], [x - r, y])
        .quad_to([x - k, y - k], [x, y - r])
        .close()
}

/// A soft round glow centred on (`x`, `y`).
fn glow(x: f32, y: f32, r: f32, color: Color, strength: f32) -> El {
    el().absolute(x - r, y - r).size(2.0 * r, 2.0 * r).radial_gradient(
        [0.5, 0.5],
        r,
        [(0.0, color.alpha(strength)), (0.4, color.alpha(strength * 0.4)), (1.0, color.alpha(0.0))],
    )
}

fn view(_: &(), _: &mut Cx<()>) -> El {
    let violet = srgb(0.50, 0.33, 1.0, 1.0);
    let magenta = srgb(0.93, 0.30, 0.76, 1.0);
    let cyan = srgb(0.20, 0.80, 1.0, 1.0);
    let white = Color::WHITE;
    let pool = |center: [f32; 2], radius: [f32; 2], color: Color, strength: f32| {
        el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radial_gradient(
            center,
            radius,
            [(0.0, color.alpha(strength)), (0.5, color.alpha(strength * 0.35)), (1.0, color.alpha(0.0))],
        )
    };

    // The orb, the wand across it, and its sparkles, in body points.
    let (cx, cy) = (BODY * 0.47, BODY * 0.54);
    let orb_r = BODY * 0.26;
    let tip = [cx + BODY * 0.13, cy - BODY * 0.13];
    let handle = [cx - BODY * 0.2, cy + BODY * 0.2];
    let wand = Shape::new()
        .line(handle, tip, BODY * 0.05, LineCap::Round, srgb(0.16, 0.12, 0.28, 1.0))
        .line([handle[0] + 4.0, handle[1] - 4.0], [tip[0] - 3.0, tip[1] + 3.0], BODY * 0.022, LineCap::Round, white.alpha(0.9))
        .fill_path(&sparkle(tip[0] + 6.0, tip[1] - 6.0, BODY * 0.16), white)
        .fill_path(&sparkle(BODY * 0.78, BODY * 0.2, BODY * 0.07), white.alpha(0.95))
        .fill_path(&sparkle(BODY * 0.83, BODY * 0.5, BODY * 0.045), magenta.mix(white, 0.5))
        .fill_path(&sparkle(BODY * 0.26, BODY * 0.24, BODY * 0.04), violet.mix(white, 0.55))
        .fill_path(&sparkle(BODY * 0.7, BODY * 0.76, BODY * 0.03), cyan.mix(white, 0.5));

    let body = el()
        .size(BODY, BODY)
        .radius(CORNER)
        .continuous_corners()
        .clip_rounded(CORNER)
        .fill(srgb(0.065, 0.052, 0.11, 1.0))
        .border_gradient(2.0, white.alpha(0.32), white.alpha(0.05))
        .box_shadow(Shadow { x: 0.0, y: 0.0, blur: 70.0, spread: -20.0, color: violet.alpha(0.35), inset: true })
        .child(pool([0.15, -0.05], [BODY * 0.95, BODY * 0.7], violet, 0.75))
        .child(pool([0.95, 0.1], [BODY * 0.75, BODY * 0.6], magenta, 0.5))
        .child(pool([0.5, 1.15], [BODY * 0.9, BODY * 0.5], cyan, 0.28))
        .child(
            el().absolute(cx - orb_r, cy - orb_r).size(2.0 * orb_r, 2.0 * orb_r).radius(orb_r)
                .radial_gradient([0.35, 0.3], orb_r * 1.9, [(0.0, violet.alpha(0.95)), (0.6, magenta.alpha(0.45)), (1.0, magenta.alpha(0.12))])
                .border_gradient(2.0, white.alpha(0.5), white.alpha(0.05))
                .box_shadow(Shadow { x: 0.0, y: 0.0, blur: 90.0, spread: 0.0, color: violet.alpha(0.7), inset: false }),
        )
        .child(glow(tip[0] + 6.0, tip[1] - 6.0, BODY * 0.24, white, 0.55))
        .child(glow(tip[0] + 6.0, tip[1] - 6.0, BODY * 0.4, magenta, 0.35))
        .child(el().absolute(0.0, 0.0).size(BODY, BODY).shape(wand));
    el().w_full().h_full().center().child(
        el().box_shadow(Shadow { x: 0.0, y: 10.0, blur: 24.0, spread: 0.0, color: Color::BLACK.alpha(0.35), inset: false })
            .radius(CORNER)
            .continuous_corners()
            .child(body),
    )
}

fn main() {
    let transparent = WindowOptions { transparent: true, ..WindowOptions::default() };
    App::new(AppSpec::new("Cantrip icon", (SIZE, SIZE), || (), view)).window_options(transparent).run();
}
