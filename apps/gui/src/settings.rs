//! Настройки окна между запусками: папки библиотеки фурнитуры, язык.
//! Файл — `~/.config/basis-craft/settings.json` (или `$XDG_CONFIG_HOME/basis-craft`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Папки с моделями фурнитуры (.fr3d); пусто — папка примеров, если есть.
    pub library_dirs: Vec<String>,
    /// Язык окна: "ru" или "en".
    pub language: String,
    /// Сетка (как «Настройки черчения» cadcraft): показывать, шаг в мм, основная линия — каждая N-я.
    pub grid: GridSettings,
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

/// Папка настроек программы.
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("basis-craft"))
}

fn file() -> Option<PathBuf> {
    Some(config_dir()?.join("settings.json"))
}

impl Settings {
    /// Настройки с диска; нет файла или он испорчен — по умолчанию.
    pub fn load() -> Settings {
        let mut s: Settings = file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if s.library_dirs.is_empty() && std::path::Path::new("example").exists() {
            s.library_dirs.push("example".into());
        }
        if s.language.is_empty() {
            s.language = "ru".into();
        }
        s
    }

    /// Записать на диск; ошибка — в терминал (окну она не мешает).
    pub fn save(&self) {
        let Some(f) = file() else { return };
        let res = f.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&f, serde_json::to_string_pretty(self).unwrap_or_default()));
        if let Err(e) = res {
            eprintln!("[настройки] {}: {e}", f.display());
        }
    }
}
