//! Icon strings from spells and the bar: an SF Symbol name, an emoji, or a file path (its
//! Finder icon). Emoji draw as text; symbols and file icons are rasterized natively and cached.

use std::cell::RefCell;
use std::collections::HashMap;

use rune::{Color, El, Image, ImageCache, Pixels, el, text};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Symbol(String),
    Emoji(String),
    File(String),
}

impl Icon {
    pub fn parse(raw: Option<&str>) -> Icon {
        let Some(raw) = raw.filter(|r| !r.is_empty()) else {
            return Icon::Symbol("sparkles".into());
        };
        if raw.starts_with('/') || raw.starts_with('~') {
            Icon::File(
                crate::config::expand_tilde(raw)
                    .to_string_lossy()
                    .into_owned(),
            )
        } else if raw.chars().any(|c| c as u32 > 0x2000) {
            Icon::Emoji(raw.into())
        } else {
            Icon::Symbol(raw.into())
        }
    }
}

type FileIcons = HashMap<(String, u32), (Option<Image>, Pixels)>;
type SymbolIcons = HashMap<(String, u32, [u8; 4]), Option<Pixels>>;

thread_local! {
    static IMAGES: RefCell<Option<ImageCache>> = const { RefCell::new(None) };
    /// The image handle keeps a decode alive; its pixels are what draws.
    static FILES: RefCell<FileIcons> = RefCell::new(HashMap::new());
    static SYMBOLS: RefCell<SymbolIcons> = RefCell::new(HashMap::new());
}

/// Give file icons a decoder; call from `AppSpec::setup` with its waker.
pub fn install(waker: rune::Waker) {
    IMAGES.with(|c| *c.borrow_mut() = Some(ImageCache::new(waker)));
}

/// `icon` drawn `size` points square. Symbols take `tint`.
pub fn view(icon: &Icon, size: f32, tint: Color, scale: f32) -> El {
    let frame = el().size(size, size).center().a11y_hidden();
    match icon {
        Icon::Emoji(t) => frame.child(
            text(t.clone())
                .font(rune::TextStyle::new(size * 0.8, 400.0).lh(size))
                .color(Color::WHITE),
        ),
        Icon::File(path) => match file_pixels(path, size, scale) {
            Some(px) => frame.pixels(&px),
            None => frame,
        },
        Icon::Symbol(name) => match symbol_pixels(name, size, tint, scale)
            .or_else(|| symbol_pixels("sparkle", size, tint, scale))
        {
            Some(px) => frame.child(el().size(size, size).pixels(&px)),
            None => frame,
        },
    }
}

fn file_pixels(path: &str, size: f32, scale: f32) -> Option<Pixels> {
    let px = (size * scale).ceil() as u32;
    let key = (path.to_string(), px);
    if let Some((_, pixels)) = FILES.with(|f| f.borrow().get(&key).cloned()) {
        return Some(pixels);
    }
    let entry = if rune::tooling::headless_run().is_some() {
        // Headless renders decode on the spot, so a snapshot never races the decoder.
        let (w, h, rgba) = rune::image::app_icon_rgba(std::path::Path::new(path), px).ok()?;
        let pixels = Pixels::new();
        pixels.set(w, h, rgba);
        (None, pixels)
    } else {
        let image = IMAGES.with(|c| {
            c.borrow()
                .as_ref()
                .map(|cache| cache.app_icon_scaled(path, size, scale))
        })?;
        let pixels = image.pixels().clone();
        (Some(image), pixels)
    };
    FILES.with(|f| f.borrow_mut().insert(key, entry.clone()));
    Some(entry.1)
}

fn symbol_pixels(name: &str, size: f32, tint: Color, scale: f32) -> Option<Pixels> {
    // Rune's colours are linear; AppKit takes sRGB.
    let enc = rune::color::srgb_encode;
    let rgba = [enc(tint.r), enc(tint.g), enc(tint.b), tint.a];
    let key = (
        name.to_string(),
        (size * scale).ceil() as u32,
        rgba.map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8),
    );
    if let Some(hit) = SYMBOLS.with(|s| s.borrow().get(&key).cloned()) {
        return hit;
    }
    let made =
        crate::platform::symbol_rgba(name, (size * 0.7) as f64, size as f64, scale as f64, rgba)
            .map(|(w, h, data)| {
                let px = Pixels::new();
                px.set(w, h, data);
                px
            });
    SYMBOLS.with(|s| s.borrow_mut().insert(key, made.clone()));
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_symbols_emoji_and_paths() {
        assert_eq!(Icon::parse(Some("number")), Icon::Symbol("number".into()));
        assert_eq!(
            Icon::parse(Some("\u{1F389}")),
            Icon::Emoji("\u{1F389}".into())
        );
        assert_eq!(
            Icon::parse(Some("/Applications/Safari.app")),
            Icon::File("/Applications/Safari.app".into())
        );
        assert_eq!(Icon::parse(None), Icon::Symbol("sparkles".into()));
        assert_eq!(Icon::parse(Some("")), Icon::Symbol("sparkles".into()));
    }
}
