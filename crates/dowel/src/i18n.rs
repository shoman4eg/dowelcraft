//! Interface language. The source text (the key) is English; `lang/ru.tsv` ("english<TAB>russian") translates it.
//! Strings with numbers and names are translated by template: `{}` in the table marks an insertion,
//! e.g. "Delete ({})" → "Удалить ({})" turns "Delete (3)" into "Удалить (3)".
//! The window, the CLI and the MCP server share it; English is the default.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static RUSSIAN: AtomicBool = AtomicBool::new(false);

/// Translation table: exact strings and templates with `{}`.
struct Table {
    exact: HashMap<&'static str, &'static str>,
    templates: Vec<(Vec<&'static str>, Vec<&'static str>)>,
}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| {
        let mut exact = HashMap::new();
        let mut templates = Vec::new();
        for line in include_str!("../lang/ru.tsv").lines() {
            let Some((en, ru)) = line.split_once('\t') else { continue };
            if en.is_empty() || en.starts_with('#') {
                continue;
            }
            if en.contains("{}") {
                templates.push((en.split("{}").collect(), ru.split("{}").collect()));
            } else {
                exact.insert(en, ru);
            }
        }
        // Longer templates first: "Delete cabinet ({})" before "Delete ({})".
        templates.sort_by_key(|(en, _): &(Vec<&str>, Vec<&str>)| std::cmp::Reverse(en.iter().map(|p| p.len()).sum::<usize>()));
        Table { exact, templates }
    })
}

/// Switch language: "ru" — Russian, anything else — English.
pub fn set_language(lang: &str) {
    RUSSIAN.store(lang == "ru", Ordering::Relaxed);
}

pub fn russian() -> bool {
    RUSSIAN.load(Ordering::Relaxed)
}

/// Language from the `DOWEL_LANG` environment variable (CLI, MCP); English when unset.
pub fn from_env() {
    if let Ok(l) = std::env::var("DOWEL_LANG") {
        set_language(&l);
    }
}

/// A Bazis compatibility name from `lang/bazis.tsv` (formula aliases "alias.W", the unit "unit.mm", …); "" if unknown.
pub fn bazis(key: &str) -> &'static str {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    let t = T.get_or_init(|| include_str!("../lang/bazis.tsv").lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once('\t')).collect());
    t.get(key).copied().unwrap_or("")
}

/// Insertions of template `parts` in `text` (the pieces between `{}`), if the text matches it.
fn fill<'a>(parts: &[&str], text: &'a str) -> Option<Vec<&'a str>> {
    let mut rest = text.strip_prefix(parts[0])?;
    let mut args = Vec::new();
    for (k, lit) in parts.iter().enumerate().skip(1) {
        let last = k == parts.len() - 1;
        let at = if last {
            if !rest.ends_with(lit) || rest.len() < lit.len() {
                return None;
            }
            rest.len() - lit.len()
        } else if lit.is_empty() {
            return None;
        } else {
            rest.find(lit)?
        };
        args.push(&rest[..at]);
        rest = &rest[at + lit.len()..];
    }
    rest.is_empty().then_some(args)
}

/// Translate a string into the current language (no translation — as is).
pub fn t(text: impl AsRef<str>) -> String {
    let text = text.as_ref();
    if russian() { ru(text) } else { text.to_string() }
}

/// Translate into Russian (regardless of the selected language).
pub fn ru(text: &str) -> String {
    let tb = table();
    if let Some(ru) = tb.exact.get(text) {
        return (*ru).to_string();
    }
    for (en, ru_parts) in &tb.templates {
        if let Some(args) = fill(en, text) {
            let mut out = String::from(ru_parts[0]);
            for (k, piece) in ru_parts.iter().enumerate().skip(1) {
                // An insertion may itself be a phrase from the table.
                out.push_str(&args.get(k - 1).map(|a| ru(a)).unwrap_or_default());
                out.push_str(piece);
            }
            return out;
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_exact_and_templates() {
        // Without the global language switch: tests run in parallel.
        assert_eq!(ru("New project"), ru("New project").as_str());
        assert_ne!(ru("New project"), "New project");
        assert_eq!(ru("a line that is not in the table"), "a line that is not in the table");
        // A template: the number is kept, the words are translated.
        assert!(ru("Delete (3)").ends_with("(3)") && ru("Delete (3)") != "Delete (3)");
    }

    #[test]
    fn bazis_names_are_there() {
        for k in ["alias.W", "alias.H", "alias.D", "alias.P", "alias.T", "alias.L", "alias.WZ", "unit.mm"] {
            assert!(!bazis(k).is_empty(), "{k}");
        }
        assert_eq!(bazis("no.such.key"), "");
    }

    #[test]
    fn template_matching() {
        assert_eq!(fill(&["Delete (", ")"], "Delete (12)"), Some(vec!["12"]));
        assert_eq!(fill(&["", " × ", " mm"], "600 × 400 mm"), Some(vec!["600", "400"]));
        assert_eq!(fill(&["Delete (", ")"], "Delete cabinet"), None);
    }
}
