//! The Raskroy export structure matches the references in `reference/`.

use std::path::Path;

/// Sequence of direct children of the first `<tag>` element (tag names only).
fn children(xml: &str, tag: &str) -> Vec<String> {
    let open = xml.find(&format!("<{tag}>")).or_else(|| xml.find(&format!("<{tag} "))).expect(tag);
    let start = open + xml[open..].find('>').unwrap() + 1;
    let mut out = Vec::new();
    let mut depth = 0;
    let mut rest = &xml[start..];
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let end = rest.find('>').unwrap();
        let t = rest[..end].split(' ').next().unwrap();
        if let Some(close) = t.strip_prefix('/') {
            if depth == 0 {
                assert_eq!(close, tag);
                break;
            }
            depth -= 1;
        } else {
            if depth == 0 {
                out.push(t.to_string());
            }
            // Leaf like <A>1</A>: the next tag is its own closing tag.
            let after = &rest[end + 1..];
            let next = after.find('<').map(|j| &after[j + 1..]).unwrap_or("");
            if next.starts_with(&format!("/{t}>")) {
                // Skip the leaf's closing tag entirely.
                rest = &next[t.len() + 2..];
                continue;
            }
            depth += 1;
        }
        rest = &rest[end + 1..];
    }
    out
}

fn reference(name: &str) -> Option<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference").join(name);
    let bytes = std::fs::read(p).ok()?;
    Some(encoding_rs::WINDOWS_1251.decode(&bytes).0.into_owned())
}

fn ours() -> Option<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/обшивка.b3d");
    let file = dowel::b3d::parse(&std::fs::read(p).ok()?).unwrap();
    let ex = dowel::panel::panels(&file);
    let bytes = dowel::raskroy::project_xml("тест", &ex.panels, |_| (2800.0, 2070.0));
    Some(encoding_rs::WINDOWS_1251.decode(&bytes).0.into_owned())
}

#[test]
fn structure_matches_reference() {
    let (Some(r), Some(o)) = (reference("raskroy-project-tribuny.xml"), ours()) else {
        eprintln!("нет reference/ или example/ — тест пропущен");
        return;
    };
    for tag in ["PROJECT", "MATERIALROOTNODE", "BOARDROOTNODE", "BINDINGROOTNODE", "MATERIALCORE", "MATERIALOUTER", "BOARDSMP", "EDGEBOT", "CONTITEMPLINE"] {
        let mut want = children(&r, tag);
        // COMMENT is an optional comment; drilling is not exported.
        want.retain(|t| t != "COMMENT");
        let mut got = children(&o, tag);
        got.retain(|t| t != "COMMENT");
        if tag == "EDGEBOT" {
            // A side without edge banding is exported without MATERIAL — the reference has both variants.
            want.retain(|t| t != "MATERIAL");
            got.retain(|t| t != "MATERIAL");
        }
        if tag == "BOARDSMP" {
            want.retain(|t| t != "DRILLING");
            got.retain(|t| t != "DRILLING");
        }
        assert_eq!(got, want, "потомки <{tag}>");
    }
}
