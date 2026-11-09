//! Each theme's palette and type (`theme.rs`). Always dark: the glow is the point. Every colour
//! is authored in sRGB (as design tools write them) and converted for Rune.
//!
//! Arcane: deep ink-violet glass, an aurora behind the search, a glowing violet→magenta
//! selection, and coloured chips. Sigil: ink and tarnished gold, quiet light, serif small caps,
//! a hairline frame inside the glass, and outlined chips.

use rune::{Color, Family, TextStyle};

use super::theme::Theme;

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

const SERIF: Family = Family::Named("Iowan Old Style, Palatino, Georgia");

#[derive(Clone, Copy)]
pub struct Palette {
    pub theme: Theme,
    /// The panel's ink.
    pub bg: Color,
    /// Raised surfaces: the actions menu, the HUD.
    pub raised: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    /// The two ends of every glow and gradient (Arcane: violet → magenta; Sigil: tarnished gold
    /// → dusk violet).
    pub glow: Color,
    pub glow2: Color,
    /// The aurora's cool edge, low behind the footer.
    pub cool: Color,
    pub red: Color,
    pub green: Color,
    pub amber: Color,
    /// How strongly things glow: halos, blooms and shadows of light scale by it.
    pub radiance: f32,
    /// Labels, the brand, and the query, by family.
    pub display: Family,
    pub query: Family,
    /// A hairline frame inset in the glass.
    pub frame: Option<Color>,
    /// The glass's edge.
    pub edge: (Color, Color),
}

pub fn of(theme: Theme) -> Palette {
    match theme {
        Theme::Arcane => Palette {
            theme,
            bg: srgb(0.055, 0.048, 0.092, 0.97),
            raised: srgb(0.085, 0.072, 0.14, 0.985),
            text: srgb(0.95, 0.94, 1.0, 1.0),
            muted: srgb(0.70, 0.66, 0.86, 0.62),
            faint: srgb(0.70, 0.66, 0.86, 0.38),
            glow: srgb(0.50, 0.33, 1.0, 1.0),
            glow2: srgb(0.93, 0.30, 0.76, 1.0),
            cool: srgb(0.20, 0.80, 1.0, 1.0),
            red: srgb(1.0, 0.33, 0.36, 1.0),
            green: srgb(0.45, 0.95, 0.65, 1.0),
            amber: srgb(1.0, 0.78, 0.42, 1.0),
            radiance: 1.0,
            display: Family::Sans,
            query: Family::Sans,
            frame: None,
            edge: (Color::WHITE.alpha(0.20), Color::WHITE.alpha(0.05)),
        },
        Theme::Sigil => {
            let gold = srgb(0.784, 0.663, 0.416, 1.0);
            Palette {
                theme,
                bg: srgb(0.055, 0.047, 0.071, 0.975),
                raised: srgb(0.082, 0.07, 0.098, 0.985),
                text: srgb(0.914, 0.886, 0.824, 1.0),
                muted: srgb(0.914, 0.886, 0.824, 0.55),
                faint: srgb(0.914, 0.886, 0.824, 0.33),
                glow: gold,
                glow2: srgb(0.49, 0.416, 0.659, 1.0),
                cool: srgb(0.42, 0.38, 0.55, 1.0),
                red: srgb(0.86, 0.38, 0.34, 1.0),
                green: srgb(0.62, 0.76, 0.52, 1.0),
                amber: srgb(0.85, 0.75, 0.53, 1.0),
                radiance: 0.3,
                display: SERIF,
                query: SERIF,
                frame: Some(gold.alpha(0.09)),
                edge: (gold.alpha(0.26), gold.alpha(0.08)),
            }
        }
    }
}

impl Palette {
    /// The colour a chip of this kind of result wears.
    pub fn kind(&self, label: &str) -> Color {
        match (self.theme, label) {
            (Theme::Sigil, "Spell") => self.glow,
            (Theme::Sigil, "Application") => self.text.alpha(0.62),
            (Theme::Sigil, "Cantrip") => self.glow2.mix(Color::WHITE, 0.35),
            (Theme::Sigil, _) => self.green,
            (_, "Spell") => self.glow.mix(Color::WHITE, 0.35),
            (_, "Application") => self.cool.mix(Color::WHITE, 0.2),
            (_, "Cantrip") => self.amber,
            // Daemons' titles and checkmarks, and whatever spells put there.
            _ => self.green,
        }
    }

    /// The selection's sweep, left to right.
    pub fn sweep(&self, strength: f32) -> [(f32, Color); 2] {
        let (a, b) = match self.theme {
            Theme::Sigil => (0.09, 0.015),
            Theme::Arcane => (0.34, 0.08),
        };
        [
            (0.0, self.glow.alpha(a * strength)),
            (1.0, self.glow2.alpha(b * strength)),
        ]
    }

    /// A hairline that glows in the middle and fades out at both ends.
    pub fn rule(&self) -> [(f32, Color); 4] {
        let (a, b) = match self.theme {
            Theme::Sigil => (self.glow.alpha(0.42), self.glow.alpha(0.42)),
            Theme::Arcane => (self.glow.alpha(0.55), self.glow2.alpha(0.35)),
        };
        [
            (0.0, Color::TRANSPARENT),
            (0.25, a),
            (0.75, b),
            (1.0, Color::TRANSPARENT),
        ]
    }

    /// The query's type.
    pub fn query_style(&self) -> TextStyle {
        match self.theme {
            Theme::Sigil => TextStyle::new(22.0, 400.0).lh(28.0).family(self.query),
            Theme::Arcane => TextStyle::new(21.0, 400.0).lh(28.0).tracking(-0.2),
        }
    }

    /// Small capital labels: section headers, metadata labels.
    pub fn label_style(&self) -> TextStyle {
        match self.theme {
            Theme::Sigil => TextStyle::new(11.0, 600.0)
                .tracking(2.2)
                .family(self.display),
            Theme::Arcane => TextStyle::new(10.5, 650.0).tracking(1.1),
        }
    }
}
