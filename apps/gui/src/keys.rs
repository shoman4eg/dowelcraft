//! Горячие клавиши: действие → сочетание («Ctrl+S», «Alt+1», «F7»). Свои назначения — в настройках
//! (`Settings::keys`), остальное — по умолчанию. Окно «Настройки» меняет их нажатием клавиши.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// (действие, название, сочетание по умолчанию).
pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("save", "Сохранить", "Ctrl+S"),
    ("undo", "Отменить", "Ctrl+Z"),
    ("redo", "Вернуть", "Ctrl+Y"),
    ("delete", "Удалить выделенное", "Delete"),
    ("grid", "Сетка: показать или скрыть", "F7"),
    ("view.3d", "Вид: 3D", "1"),
    ("view.front", "Вид: спереди", "2"),
    ("view.parts", "Вид: детали", "3"),
    ("view.cut", "Вид: раскрой", "4"),
    ("mode.solid", "Показ: без невидимых", "Alt+1"),
    ("mode.hidden", "Показ: невидимые тонкие", "Alt+2"),
    ("mode.wire", "Показ: каркас", "Alt+3"),
    ("mode.xray", "Показ: прозрачность", "Alt+4"),
    ("mode.textured", "Показ: в текстурах", "Alt+5"),
];

/// «Ctrl+Shift+Z» → сочетание; `None` — пусто (клавиша снята) или не разобрать.
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

/// Сочетание текстом, как его понимает `parse`.
pub fn format(mods: Modifiers, key: Key) -> String {
    let mut s = String::new();
    for (on, name) in [(mods.command || mods.ctrl, "Ctrl+"), (mods.alt, "Alt+"), (mods.shift, "Shift+")] {
        if on {
            s += name;
        }
    }
    s + key.name()
}

/// Сколько клавиш-модификаторов: сочетания длиннее проверяются первыми, чтобы «1» не сработала на Alt+1.
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
        // Все сочетания по умолчанию разбираются и не повторяются.
        let all: Vec<String> = ACTIONS.iter().map(|(_, _, k)| format(parse(k).unwrap().modifiers, parse(k).unwrap().logical_key)).collect();
        let mut uniq = all.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), all.len());
    }
}
