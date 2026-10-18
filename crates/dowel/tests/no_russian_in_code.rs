//! Russian text lives only in `crates/dowel/lang/*.tsv`, the help and the docs: code stays English.
//! Fails when Cyrillic appears in a tracked `.rs`, `.js` or `.toml` file of the repository.

use std::path::Path;

#[test]
fn code_has_no_cyrillic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut bad = Vec::new();
    for dir in ["apps", "crates", "Cargo.toml"] {
        walk(&root.join(dir), &mut bad);
    }
    assert!(bad.is_empty(), "Cyrillic in code (move it to crates/dowel/lang/ru.tsv or bazis.tsv):\n{}", bad.join("\n"));
}

fn walk(p: &Path, bad: &mut Vec<String>) {
    if p.is_dir() {
        if p.file_name().is_some_and(|n| n == "target") {
            return;
        }
        for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
            walk(&e.path(), bad);
        }
        return;
    }
    if !p.extension().is_some_and(|e| e == "rs" || e == "js" || e == "toml") {
        return;
    }
    let text = std::fs::read_to_string(p).unwrap_or_default();
    for (i, line) in text.lines().enumerate() {
        if line.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c)) {
            bad.push(format!("{}:{}: {}", p.display(), i + 1, line.trim()));
        }
    }
}
