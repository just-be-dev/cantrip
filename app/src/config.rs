//! `~/.config/cantrip/config.json`. Settings (`store::Settings`) writes `hotkey` back into it;
//! the rest is only read.
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

/// The hotkey when config.json names none.
pub const DEFAULT_HOTKEY: &str = "option+space";

impl Default for Config {
    fn default() -> Self {
        Config {
            hotkey: DEFAULT_HOTKEY.into(),
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

    /// Puts `hotkey` in config.json, keeping everything else in it as it was; None takes it
    /// out, so the default applies. The file and its folder are made if they're missing.
    pub fn save_hotkey(hotkey: Option<&str>) -> Result<(), String> {
        let dir = Self::directory();
        let path = dir.join("config.json");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("Can't read {}: {e}", path.display())),
        };
        let text = with_hotkey(text.as_deref(), hotkey)?;
        std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&path, text))
            .map_err(|e| format!("Can't write {}: {e}", path.display()))
    }
}

/// config.json's `text` (None when there's no file) with `hotkey` set, or taken out.
pub fn with_hotkey(text: Option<&str>, hotkey: Option<&str>) -> Result<String, String> {
    use serde_json::{Map, Value};
    let mut doc = match text.filter(|t| !t.trim().is_empty()) {
        Some(text) => serde_json::from_str::<Map<String, Value>>(text)
            .map_err(|e| format!("config.json isn't a JSON object ({e}); fix it to save"))?,
        None => Map::new(),
    };
    match hotkey {
        Some(hotkey) => doc.insert("hotkey".into(), Value::String(hotkey.into())),
        None => doc.remove("hotkey"),
    };
    let mut out = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    out.push('\n');
    Ok(out)
}

/// A chord as config.json spells it: `cmd`, `option`, `ctrl`, `shift`, then the key
/// ("cmd+shift+space"). Rune reads it back as the same chord.
pub fn hotkey_name(shortcut: &rune::Shortcut) -> String {
    // Rune writes ctrl+alt+shift+super+key.
    shortcut
        .to_string()
        .split('+')
        .map(|part| match part {
            "alt" => "option",
            "super" => "cmd",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// The hotkey as keycaps (⌥ Space), or as written when Rune can't read it.
pub fn hotkey_keycaps(hotkey: &str) -> Vec<String> {
    match hotkey.parse::<rune::Shortcut>() {
        Ok(shortcut) => shortcut.keycaps(),
        Err(_) => vec![hotkey.to_string()],
    }
}

/// The hotkey as a menu key equivalent (key, ⌘⌥⌃⇧ flags), so the status menu shows it the
/// native way (⌥Space). The global shortcut itself is registered from the hotkey as written.
pub fn menu_shortcut(hotkey: &str) -> Option<(String, MenuMods)> {
    let mut mods = MenuMods::default();
    let mut key = None;
    for part in hotkey.to_lowercase().split('+').map(str::trim) {
        key = None;
        match part {
            "cmd" | "command" | "super" => mods.cmd = true,
            "opt" | "option" | "alt" => mods.option = true,
            "ctrl" | "control" => mods.ctrl = true,
            "shift" => mods.shift = true,
            name => key = menu_key(name),
        }
    }
    key.map(|k| (k, mods))
}

/// A key's character as an NSMenuItem key equivalent: what it types, or AppKit's
/// function-key character (NSUpArrowFunctionKey, NSF1FunctionKey, ...).
fn menu_key(name: &str) -> Option<String> {
    const TYPED: &[(&str, &str)] = &[
        ("space", " "),
        ("return", "\r"),
        ("enter", "\r"),
        ("tab", "\t"),
        ("escape", "\u{1b}"),
        ("backspace", "\u{8}"),
        ("comma", ","),
        ("period", "."),
        ("slash", "/"),
        ("backslash", "\\"),
        ("semicolon", ";"),
        ("quote", "'"),
        ("backquote", "`"),
        ("minus", "-"),
        ("equal", "="),
        ("bracketleft", "["),
        ("bracketright", "]"),
    ];
    const FUNCTION: &[(&str, u32)] = &[
        ("up", 0xF700),
        ("down", 0xF701),
        ("left", 0xF702),
        ("right", 0xF703),
        ("delete", 0xF728),
    ];
    if let Some((_, typed)) = TYPED.iter().find(|(n, _)| *n == name) {
        return Some(typed.to_string());
    }
    let code = FUNCTION
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
        .or_else(|| {
            let n = name.strip_prefix('f')?.parse::<u32>().ok()?;
            (1..=20).contains(&n).then(|| 0xF704 + n - 1)
        });
    match code {
        Some(code) => char::from_u32(code).map(String::from),
        None => (name.chars().count() == 1).then(|| name.to_string()),
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
        assert_eq!(
            menu_shortcut("cmd+shift+space"),
            Some((
                " ".into(),
                MenuMods {
                    cmd: true,
                    shift: true,
                    ..Default::default()
                }
            ))
        );
        assert_eq!(menu_shortcut("ctrl+k").unwrap().0, "k");
        assert_eq!(menu_shortcut("cmd+comma").unwrap().0, ",");
        assert_eq!(menu_shortcut("ctrl+f13").unwrap().0, "\u{F710}");
        assert_eq!(menu_shortcut("ctrl+nonsense"), None);
    }

    #[test]
    fn hotkeys_are_saved_as_config_spells_them() {
        let shortcut: rune::Shortcut = "OPTION+Cmd+Space".parse().unwrap();
        assert_eq!(hotkey_name(&shortcut), "option+cmd+space");
        assert_eq!(
            hotkey_name(&"ctrl+shift+k".parse().unwrap()),
            "ctrl+shift+k"
        );
        // Read back, it's the same chord.
        assert_eq!(
            hotkey_name(&shortcut).parse::<rune::Shortcut>(),
            Ok(shortcut)
        );
    }

    #[test]
    fn saving_the_hotkey_keeps_the_rest_of_the_file() {
        let text = r#"{ "theme": "arcane", "hotkey": "option+space", "bun": "~/bin/bun" }"#;
        let saved = with_hotkey(Some(text), Some("cmd+k")).unwrap();
        let c = Config::parse(&saved).unwrap();
        assert_eq!(c.hotkey, "cmd+k");
        assert_eq!(
            (c.theme.as_deref(), c.bun.as_deref()),
            (Some("arcane"), Some("~/bin/bun"))
        );
        assert!(
            saved.find("theme") < saved.find("bun"),
            "in the order it was written"
        );
        // Reset takes it out, so the default applies.
        let reset = with_hotkey(Some(&saved), None).unwrap();
        assert_eq!(Config::parse(&reset).unwrap().hotkey, DEFAULT_HOTKEY);
        assert!(!reset.contains("hotkey"));
        // No file yet, or an empty one: just the hotkey.
        assert_eq!(
            with_hotkey(None, Some("cmd+k")).unwrap(),
            "{\n  \"hotkey\": \"cmd+k\"\n}\n"
        );
        assert!(with_hotkey(Some("  "), Some("cmd+k")).is_ok());
        // A broken file isn't overwritten.
        assert!(with_hotkey(Some("{ nope"), Some("cmd+k")).is_err());
    }

    #[test]
    fn tilde_expands_to_home() {
        assert_eq!(expand_tilde("~/x"), home().join("x"));
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
    }
}
