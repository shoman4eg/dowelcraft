//! Hotkeys: action → shortcut ("Ctrl+S", "Alt+1", "F7"). Custom bindings live in settings
//! (`Settings::keys`), the rest are defaults. The "Settings" window rebinds them by pressing a key.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// (action, label, default shortcut).
pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("save", "Сохранить", "Ctrl+S"),
    ("undo", "Отменить", "Ctrl+Z"),
    ("redo", "Вернуть", "Ctrl+Y"),
    ("delete", "Удалить выделенное", "Delete"),
    ("grid", "Сетка: показать или скрыть", "F7"),
    ("cam.front", "Камера: спереди", "1"),
    ("cam.left", "Камера: слева", "2"),
    ("cam.top", "Камера: сверху", "3"),
    ("cam.iso", "Камера: аксонометрия", "4"),
    ("cam.fit", "Показать всё", "5"),
    ("view.3d", "Вкладка: 3D", "Ctrl+1"),
    ("view.front", "Вкладка: спереди", "Ctrl+2"),
    ("view.parts", "Вкладка: детали", "Ctrl+3"),
    ("view.cut", "Вкладка: раскрой", "Ctrl+4"),
    ("mode.solid", "Показ: без невидимых", "Alt+1"),
    ("mode.hidden", "Показ: невидимые тонкие", "Alt+2"),
    ("mode.wire", "Показ: каркас", "Alt+3"),
    ("mode.xray", "Показ: прозрачность", "Alt+4"),
    ("mode.textured", "Показ: в текстурах", "Alt+5"),
];

/// "Ctrl+Shift+Z" → shortcut; `None` means empty (key unbound) or unparseable.
pub fn parse(s: &str) -> Option<KeyboardShortcut> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" => mods |= Modifiers::COMMAND,
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            _ => key = Some(Key::from_name(part)?),
        }
    }
    Some(KeyboardShortcut::new(mods, key?))
}

/// Shortcut as text, in the form `parse` understands.
pub fn format(mods: Modifiers, key: Key) -> String {
    let mut s = String::new();
    for (on, name) in [(mods.command || mods.ctrl, "Ctrl+"), (mods.alt, "Alt+"), (mods.shift, "Shift+")] {
        if on {
            s += name;
        }
    }
    s + key.name()
}

/// Number of modifier keys: longer shortcuts are checked first so that "1" doesn't fire on Alt+1.
pub fn weight(sc: &KeyboardShortcut) -> usize {
    [sc.modifiers.command, sc.modifiers.alt, sc.modifiers.shift].into_iter().filter(|&b| b).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_parse_and_format_back() {
        for s in ["Ctrl+S", "Alt+1", "F7", "Delete", "Ctrl+Shift+Z", "4"] {
            let sc = parse(s).unwrap_or_else(|| panic!("{s}"));
            assert_eq!(format(sc.modifiers, sc.logical_key), s);
        }
        assert!(parse("").is_none());
        assert!(parse("Ctrl+Непонятно").is_none());
        // All default shortcuts parse and are unique.
        let all: Vec<String> = ACTIONS.iter().map(|(_, _, k)| format(parse(k).unwrap().modifiers, parse(k).unwrap().logical_key)).collect();
        let mut uniq = all.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), all.len());
    }
}
