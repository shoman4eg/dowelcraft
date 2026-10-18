//! Hotkeys: action → shortcut ("Ctrl+S", "Alt+1", "F7"). Custom bindings live in settings
//! (`Settings::keys`), the rest are defaults. The "Settings" window rebinds them by pressing a key.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// (action, label, default shortcut).
pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("save", "Save", "Ctrl+S"),
    ("undo", "Undo", "Ctrl+Z"),
    ("redo", "Redo", "Ctrl+Y"),
    ("delete", "Delete selection", "Delete"),
    ("grid", "Grid: show or hide", "F7"),
    ("cam.front", "Camera: front", "1"),
    ("cam.left", "Camera: left", "2"),
    ("cam.top", "Camera: top", "3"),
    ("cam.iso", "Camera: axonometric", "4"),
    ("cam.fit", "Zoom to fit", "5"),
    ("view.3d", "Tab: 3D", "Ctrl+1"),
    ("view.front", "Tab: front", "Ctrl+2"),
    ("view.parts", "Tab: parts", "Ctrl+3"),
    ("view.cut", "Tab: cutting", "Ctrl+4"),
    ("mode.solid", "Display: hidden lines removed", "Alt+1"),
    ("mode.hidden", "Display: hidden thin", "Alt+2"),
    ("mode.wire", "Display: wireframe", "Alt+3"),
    ("mode.xray", "Display: transparent", "Alt+4"),
    ("mode.textured", "Display: textured", "Alt+5"),
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
        assert!(parse("Ctrl+Nonsense").is_none());
        // All default shortcuts parse and are unique.
        let all: Vec<String> = ACTIONS.iter().map(|(_, _, k)| format(parse(k).unwrap().modifiers, parse(k).unwrap().logical_key)).collect();
        let mut uniq = all.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), all.len());
    }
}
