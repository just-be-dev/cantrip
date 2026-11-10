//! `~/.config/cantrip/config.json`
//!
//! ```json
//! {
//!   "hotkey": "option+space",   // modifiers: cmd, option/alt, ctrl, shift
//!   "bun": "/path/to/bun",      // optional; found automatically otherwise
//!   "theme": "sigil"            // sigil or arcane; the one chosen in the bar wins
//! }
//! ```

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub hotkey: String,
    pub bun: Option<String>,
    /// The theme to start in until one is chosen in the bar (`views::theme`).
    pub theme: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            hotkey: "option+space".into(),
            bun: None,
            theme: None,
        }
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `~` and `~/…` to the home folder.
pub fn expand_tilde(path: &str) -> PathBuf {
    match path.strip_prefix('~') {
        Some("") => home(),
        Some(rest) if rest.starts_with('/') => home().join(&rest[1..]),
        _ => PathBuf::from(path),
    }
}

impl Config {
    pub fn directory() -> PathBuf {
        home().join(".config/cantrip")
    }

    /// The grimoire: the folder of spells.
    pub fn grimoire_directory() -> PathBuf {
        match std::env::var_os("CANTRIP_GRIMOIRE") {
            Some(dir) => PathBuf::from(dir),
            None => Self::directory().join("grimoire"),
        }
    }

    /// Where the spell `id` is written: its file or its directory's entry (`Registry.ts` names a
    /// file's spells after the file, without its extension, then `:export`). The grimoire
    /// itself when it can't be found.
    pub fn spell_file(id: &str) -> PathBuf {
        let dir = Self::grimoire_directory();
        let base = id.split(':').next().unwrap_or(id);
        let here = dir.join(base);
        if here.is_dir() {
            return ["index.ts", "index.tsx", "index.js"]
                .iter()
                .map(|entry| here.join(entry))
                .find(|p| p.is_file())
                .unwrap_or(here);
        }
        if here.is_file() {
            return here;
        }
        ["ts", "tsx", "js", "jsx", "mjs"]
            .iter()
            .map(|ext| dir.join(format!("{base}.{ext}")))
            .find(|p| p.is_file())
            .unwrap_or(dir)
    }

    pub fn load() -> Config {
        let path = Self::directory().join("config.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Config::default();
        };
        Self::parse(&text).unwrap_or_else(|e| {
            eprintln!("Cantrip: ignoring invalid {}: {e}", path.display());
            Config::default()
        })
    }

    pub fn parse(text: &str) -> Result<Config, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// The hotkey as a menu key equivalent (key, ⌘⌥⌃⇧ flags), so the status menu shows it the
    /// native way (⌥Space). The global shortcut itself is registered from `hotkey` as written.
    pub fn menu_shortcut(&self) -> Option<(String, MenuMods)> {
        let mut mods = MenuMods::default();
        let mut key = None;
        for part in self.hotkey.to_lowercase().split('+').map(str::trim) {
            match part {
                "cmd" | "command" => mods.cmd = true,
                "opt" | "option" | "alt" => mods.option = true,
                "ctrl" | "control" => mods.ctrl = true,
                "shift" => mods.shift = true,
                "space" => key = Some(" ".to_string()),
                "return" | "enter" => key = Some("\r".to_string()),
                "tab" => key = Some("\t".to_string()),
                "escape" => key = Some("\u{1b}".to_string()),
                other if other.chars().count() == 1 => key = Some(other.to_string()),
                _ => key = None,
            }
        }
        key.map(|k| (k, mods))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuMods {
    pub cmd: bool,
    pub option: bool,
    pub ctrl: bool,
    pub shift: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_keep_their_defaults() {
        assert_eq!(Config::parse("{}").unwrap(), Config::default());
        let c = Config::parse(r#"{"bun":"~/bin/bun"}"#).unwrap();
        assert_eq!(
            (c.hotkey.as_str(), c.bun.as_deref()),
            ("option+space", Some("~/bin/bun"))
        );
    }

    #[test]
    fn menu_shortcut_reads_modifiers_and_named_keys() {
        let c = Config {
            hotkey: "cmd+shift+space".into(),
            bun: None,
            theme: None,
        };
        assert_eq!(
            c.menu_shortcut(),
            Some((
                " ".into(),
                MenuMods {
                    cmd: true,
                    shift: true,
                    ..Default::default()
                }
            ))
        );
        let c = Config {
            hotkey: "ctrl+k".into(),
            bun: None,
            theme: None,
        };
        assert_eq!(c.menu_shortcut().unwrap().0, "k");
        let c = Config {
            hotkey: "ctrl+f13".into(),
            bun: None,
            theme: None,
        };
        assert_eq!(c.menu_shortcut(), None);
    }

    #[test]
    fn tilde_expands_to_home() {
        assert_eq!(expand_tilde("~/x"), home().join("x"));
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
    }
}
