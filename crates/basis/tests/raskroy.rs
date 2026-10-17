//! Структура выгрузки для Раскроя совпадает с эталонами из `reference/`.

use std::path::Path;

/// Последовательность прямых потомков первого элемента `<tag>` (только имена тегов).
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
            // Лист вида <A>1</A>: следующий тег — его же закрывающий.
            let after = &rest[end + 1..];
            let next = after.find('<').map(|j| &after[j + 1..]).unwrap_or("");
            if next.starts_with(&format!("/{t}>")) {
                // Пропускаем закрывающий тег листа целиком.
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
    let file = basis::b3d::parse(&std::fs::read(p).ok()?).unwrap();
    let ex = basis::panel::panels(&file);
    let bytes = basis::raskroy::project_xml("тест", &ex.panels, |_| (2800.0, 2070.0));
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
        // COMMENT — необязательный комментарий, сверловку не выгружаем.
        want.retain(|t| t != "COMMENT");
        let mut got = children(&o, tag);
        got.retain(|t| t != "COMMENT");
        if tag == "EDGEBOT" {
            // Сторона без кромки выгружается без MATERIAL — в эталоне есть оба варианта.
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
