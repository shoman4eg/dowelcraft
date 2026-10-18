//! Window language: Russian (the source text is the key) and English (`i18n/en.tsv`: "russian<TAB>english").
//! Strings with numbers and names are translated by template: in the table `{}` marks an insertion,
//! e.g. "Delete ({})" (Russian key) → "Delete ({})" translates the Russian "Delete (3)" into "Delete (3)".

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

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
        for line in include_str!("i18n/en.tsv").lines() {
            let Some((ru, en)) = line.split_once('\t') else { continue };
            if ru.is_empty() || ru.starts_with('#') {
                continue;
            }
            if ru.contains("{}") {
                templates.push((ru.split("{}").collect(), en.split("{}").collect()));
            } else {
                exact.insert(ru, en);
            }
        }
        // Longer templates first: Russian "Delete cabinet ({})" before "Delete ({})".
        templates.sort_by_key(|(ru, _): &(Vec<&str>, Vec<&str>)| std::cmp::Reverse(ru.iter().map(|p| p.len()).sum::<usize>()));
        Table { exact, templates }
    })
}

/// Switch language: "en" — English, anything else — Russian.
pub fn set_language(lang: &str) {
    ENGLISH.store(lang == "en", Ordering::Relaxed);
}

pub fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// Insertions of template `parts` in `text` (the pieces between `{}`), if the text matches it.
fn fill<'a>(parts: &[&str], text: &'a str) -> Option<Vec<&'a str>> {
    let rest = text.strip_prefix(parts[0])?;
    let mut rest = rest;
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
    if english() { en(text) } else { text.to_string() }
}

/// Translate into English (regardless of the selected language).
fn en(text: &str) -> String {
    let tb = table();
    if let Some(en) = tb.exact.get(text) {
        return (*en).to_string();
    }
    for (ru, en_parts) in &tb.templates {
        if let Some(args) = fill(ru, text) {
            let mut out = String::from(en_parts[0]);
            for (k, piece) in en_parts.iter().enumerate().skip(1) {
                // An insertion may itself be a Russian phrase from the table.
                out.push_str(&args.get(k - 1).map(|a| en(a)).unwrap_or_default());
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
        // Without the global language switch: window tests run in parallel and change it.
        let t = en;
        assert_eq!(t("Новый проект"), "New project");
        assert_eq!(t("Удалить (3)"), "Delete (3)");
        assert_eq!(t("строка, которой нет в таблице"), "строка, которой нет в таблице");
        // Nested templates: the insertion is translated too.
        assert_eq!(t("Корпус: Корпус 2 (6)"), "Cabinet: Корпус 2 (6)");
        assert_eq!(t("600 × 400 × 16 мм"), "600 × 400 × 16 mm");
    }

    #[test]
    fn template_matching() {
        assert_eq!(fill(&["Удалить (", ")"], "Удалить (12)"), Some(vec!["12"]));
        assert_eq!(fill(&["", " × ", " мм"], "600 × 400 мм"), Some(vec!["600", "400"]));
        assert_eq!(fill(&["Удалить (", ")"], "Удалить корпус"), None);
    }
}
