//! Arcane: Cantrip's look. Deep ink-violet glass, an aurora behind the search, a glowing
//! violet→magenta selection, and coloured chips. Always dark: the glow is the point.
//! Every colour is authored in sRGB (as design tools write them) and converted for Rune.

use rune::Color;

/// A colour from sRGB components (Rune's own are linear).
pub fn srgb(r: f32, g: f32, b: f32, a: f32) -> Color {
    let dec = rune::color::srgb_decode;
    Color {
        r: dec(r),
        g: dec(g),
        b: dec(b),
        a,
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    /// The panel's ink.
    pub bg: Color,
    /// Raised surfaces: the actions menu, the HUD.
    pub raised: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    /// The two ends of every glow and gradient.
    pub violet: Color,
    pub magenta: Color,
    /// The aurora's cool edge, low behind the footer.
    pub cyan: Color,
    pub red: Color,
    pub green: Color,
    pub amber: Color,
}

pub fn arcane() -> Palette {
    Palette {
        bg: srgb(0.055, 0.048, 0.092, 0.97),
        raised: srgb(0.085, 0.072, 0.14, 0.985),
        text: srgb(0.95, 0.94, 1.0, 1.0),
        muted: srgb(0.70, 0.66, 0.86, 0.62),
        faint: srgb(0.70, 0.66, 0.86, 0.38),
        violet: srgb(0.50, 0.33, 1.0, 1.0),
        magenta: srgb(0.93, 0.30, 0.76, 1.0),
        cyan: srgb(0.20, 0.80, 1.0, 1.0),
        red: srgb(1.0, 0.33, 0.36, 1.0),
        green: srgb(0.45, 0.95, 0.65, 1.0),
        amber: srgb(1.0, 0.78, 0.42, 1.0),
    }
}

impl Palette {
    /// The colour a chip of this kind of result wears.
    pub fn kind(&self, label: &str) -> Color {
        match label {
            "Spell" => self.violet.mix(Color::WHITE, 0.35),
            "Application" => self.cyan.mix(Color::WHITE, 0.2),
            "Cantrip" => self.amber,
            // Daemons' titles and checkmarks, and whatever spells put there.
            _ => self.green,
        }
    }

    /// The selection's sweep, left to right.
    pub fn sweep(&self, strength: f32) -> [(f32, Color); 2] {
        [
            (0.0, self.violet.alpha(0.34 * strength)),
            (1.0, self.magenta.alpha(0.08 * strength)),
        ]
    }

    /// A hairline that glows in the middle and fades out at both ends.
    pub fn rule(&self) -> [(f32, Color); 4] {
        [
            (0.0, Color::TRANSPARENT),
            (0.25, self.violet.alpha(0.55)),
            (0.75, self.magenta.alpha(0.35)),
            (1.0, Color::TRANSPARENT),
        ]
    }
}
