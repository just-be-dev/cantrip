//! Magical themes: how the bar's magic looks. A theme picks the palette and type (`palette.rs`),
//! the wand's materials, what orbits it and what it sheds (`wand.rs`), how typed letters arrive
//! and deleted ones leave (`wand/spells.rs`), and the flourish as the bar opens and closes. The
//! wand's motion is the same in every theme.
//!
//! To add one: a variant here, its `Palette` in `palette.rs`, and an arm wherever a `match` on
//! `Theme` asks for its look.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Occult geometry and ink: tarnished gold on ink, serif labels. Letters arrive as orbs of
    /// energy that bloom into runes and dissolve into the letter; deleted letters turn back into
    /// runes and the wand draws them in.
    #[default]
    Sigil,
    /// Violet and magenta glass and starlight: sparkles, sparks that write the letters, and
    /// stardust.
    Arcane,
}

impl Theme {
    pub const ALL: [Theme; 2] = [Theme::Sigil, Theme::Arcane];

    /// As written in config.json and user defaults.
    pub fn id(self) -> &'static str {
        match self {
            Theme::Sigil => "sigil",
            Theme::Arcane => "arcane",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Theme::Sigil => "Sigil",
            Theme::Arcane => "Arcane",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Theme::Sigil => "Ink and tarnished gold: runes, sigils and orbs of energy",
            Theme::Arcane => "Violet glass and starlight: sparkles, sparks and stardust",
        }
    }

    pub fn parse(id: &str) -> Option<Theme> {
        Theme::ALL
            .into_iter()
            .find(|t| t.id().eq_ignore_ascii_case(id.trim()))
    }

    pub fn palette(self) -> super::palette::Palette {
        super::palette::of(self)
    }
}

#[cfg(test)]
mod tests {
    use super::Theme;

    #[test]
    fn themes_round_trip_by_id() {
        for t in Theme::ALL {
            assert_eq!(Theme::parse(t.id()), Some(t));
        }
        assert_eq!(Theme::parse(" SIGIL "), Some(Theme::Sigil));
        assert_eq!(Theme::parse("nope"), None);
        assert_eq!(Theme::default(), Theme::Sigil);
    }
}
