//! Installed applications, found by scanning the usual folders.

use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppEntry {
    pub name: String,
    pub path: String,
}

pub fn scan() -> Vec<AppEntry> {
    let home = crate::config::home();
    let home_apps = home.join("Applications");
    let roots = [
        Path::new("/Applications"),
        Path::new("/System/Applications"),
        Path::new("/System/Applications/Utilities"),
        Path::new("/System/Library/CoreServices/Applications"),
        home_apps.as_path(),
    ];
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for root in roots {
        visit(root, 1, &mut seen, &mut apps);
    }
    // Finder lives somewhere special.
    if seen.insert("finder".to_string()) {
        apps.push(AppEntry {
            name: "Finder".into(),
            path: "/System/Library/CoreServices/Finder.app".into(),
        });
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

fn visit(dir: &Path, depth: u32, seen: &mut HashSet<String>, apps: &mut Vec<AppEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if let Some(stem) = name.strip_suffix(".app") {
            let display = crate::platform::display_name(&path).unwrap_or_else(|| stem.to_string());
            let display = display.strip_suffix(".app").unwrap_or(&display).to_string();
            if seen.insert(display.to_lowercase()) {
                apps.push(AppEntry {
                    name: display,
                    path: path.to_string_lossy().into_owned(),
                });
            }
        } else if depth > 0 && path.is_dir() {
            visit(&path, depth - 1, seen, apps);
        }
    }
}
