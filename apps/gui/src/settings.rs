//! Window settings kept between launches: fastener library folders, language.
//! File: `~/.config/dowel-craft/settings.json` (or `$XDG_CONFIG_HOME/dowel-craft`, on Windows `%APPDATA%\dowel-craft`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Folders with fastener models (.fr3d); empty means the examples folder, if present.
    pub library_dirs: Vec<String>,
    /// Window language: "en" (default) or "ru".
    pub language: String,
    /// Folders with texture images (.jpg, .png); empty means the example textures, if present.
    pub texture_dirs: Vec<String>,
    /// Grid (like cadcraft's "Drafting settings"): show, step in mm, every N-th line is a major one.
    pub grid: GridSettings,
    /// Custom hotkeys: action → shortcut ("" means unbound); the rest use defaults (`keys::ACTIONS`).
    pub keys: BTreeMap<String, String>,
    /// Size of the whole window's text and controls: 1 is normal, 1.25 is a quarter larger.
    pub scale: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct GridSettings {
    pub on: bool,
    pub step: f32,
    pub major: u32,
}

impl Default for GridSettings {
    fn default() -> Self {
        GridSettings { on: true, step: 10.0, major: 10 }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    // Tests neither read the user's settings and recent files (their language would change test output) nor overwrite them.
    if cfg!(test) {
        return None;
    }
    // Windows has no HOME, so settings go to %APPDATA%.
    let base = std::env::var_os("XDG_CONFIG_HOME").or_else(|| std::env::var_os("APPDATA")).map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let dir = base.join("dowel-craft");
    // Settings, recent files etc. from the program's former name are migrated once.
    let old = base.join("basis-craft");
    if !dir.exists() && old.is_dir() {
        let _ = std::fs::rename(&old, &dir);
    }
    Some(dir)
}

fn file() -> Option<PathBuf> {
    Some(config_dir()?.join("settings.json"))
}

impl Settings {
    /// Settings from disk; missing or corrupt file means defaults.
    pub fn load() -> Settings {
        let mut s: Settings = file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if s.library_dirs.is_empty() && std::path::Path::new("example").exists() {
            s.library_dirs.push("example".into());
        }
        if s.texture_dirs.is_empty() && std::path::Path::new("example/textures").exists() {
            s.texture_dirs.push("example/textures".into());
        }
        if s.language.is_empty() {
            s.language = "en".into();
        }
        if !(0.5..=3.0).contains(&s.scale) {
            s.scale = 1.0;
        }
        s
    }

    /// Shortcut for an action: custom or default; `None` means unassigned.
    pub fn shortcut(&self, action: &str) -> Option<eframe::egui::KeyboardShortcut> {
        let default = crate::keys::ACTIONS.iter().find(|(id, ..)| *id == action).map(|(.., k)| *k);
        crate::keys::parse(self.keys.get(action).map(String::as_str).or(default)?)
    }

    /// Shortcut as text for menus and tooltips ("" means unassigned).
    pub fn key_label(&self, action: &str) -> String {
        self.shortcut(action).map(|sc| crate::keys::format(sc.modifiers, sc.logical_key)).unwrap_or_default()
    }

    /// Write to disk; errors go to the terminal (they don't disturb the window).
    pub fn save(&self) {
        let Some(f) = file() else { return };
        let res = f.parent().map(std::fs::create_dir_all).transpose().and_then(|_| dowel::write(&f, serde_json::to_string_pretty(self).unwrap_or_default()));
        if let Err(e) = res {
            eprintln!("[settings] {}: {e}", f.display());
        }
    }
}
