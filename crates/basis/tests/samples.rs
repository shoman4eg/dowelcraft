//! Все образцы из `example/` должны читаться целиком, а контуры плит — разбираться.

use std::path::Path;

fn samples() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example")];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if matches!(p.extension().and_then(|e| e.to_str()), Some("b3d" | "fr3d")) {
                out.push(p);
            }
        }
    }
    out
}

/// Пара «модель + смета Базиса»: расход плит и кромки совпадает со сметой.
#[test]
fn materials_match_bazis_estimate() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/pairs/sanuzly/1.0711 Мебель в санузлы.b3d");
    let Ok(data) = std::fs::read(&p) else {
        eprintln!("нет reference/pairs/sanuzly — тест пропущен");
        return;
    };
    let ex = basis::panel::panels(&basis::b3d::parse(&data).unwrap());
    // Коэффициенты из сметы: фасады 1, остальное 1,2.
    let rows = basis::panel::materials(&ex.panels, |m| if m.starts_with("Фасад") { 1.0 } else { 1.2 });
    let qty = |prefix: &str| rows.iter().find(|r| r.material.starts_with(prefix)).map(|r| r.qty).unwrap_or(f64::NAN);
    for (name, want) in [
        ("ЛДСП Персидский Жемчуг 810 PO", 8.5),
        ("ДСП 16 Белый Снег", 2.3),
        ("ХДФ Белый фон 0101", 0.6),
        ("Фасад Эмаль матовый односторонний 16 мм_Белый", 2.1),
        ("Фасад Эмаль матовый односторонний 16 мм_Перси", 5.6),
        ("Кромка ПВХ 0,4*19 Персидский Жемчуг", 72.0),
        ("Кромка ПВХ 0,4*19 Белый Снег", 26.0),
    ] {
        assert!((qty(name) - want).abs() < 1e-6, "{name}: у нас {}, в смете {want}", qty(name));
    }
}

#[test]
fn all_samples_parse() {
    let files = samples();
    if files.is_empty() {
        eprintln!("нет образцов в example/ — тест пропущен");
        return;
    }
    let mut panels = 0;
    for f in &files {
        let file = basis::b3d::parse(&std::fs::read(f).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert!(file.section("Header").is_some(), "{}: нет Header", f.display());
        let ex = basis::panel::panels(&file);
        assert!(ex.skipped.is_empty(), "{}: не разобраны контуры {:?}", f.display(), ex.skipped);
        for p in &ex.panels {
            let (l, w) = p.cut_size();
            assert!(l > 0.0 && w > 0.0 && p.thickness > 0.0, "{}: «{}» {l}x{w}x{}", f.display(), p.name, p.thickness);
        }
        panels += ex.panels.len();
    }
    eprintln!("файлов {}, плит {panels}", files.len());
}

/// Положение плит Базиса: в шкафу полки стоят между боковинами, а не заходят в них.
#[test]
fn panel_positions_in_space() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/шкаф.b3d");
    let Ok(data) = std::fs::read(&p) else { return };
    let ex = basis::panel::panels(&basis::b3d::parse(&data).unwrap());
    let b = |name: &str| ex.panels.iter().filter(|p| p.name == name).map(|p| p.bounds.unwrap()).collect::<Vec<_>>();
    let left = b("Вертикальная1")[0];
    let shelves = b("Горизонтальная1");
    assert_eq!(shelves.len(), 8);
    for (lo, hi) in shelves {
        assert!((lo[0] - left.1[0]).abs() < 0.01, "полка начинается у правой грани левой боковины: {} vs {}", lo[0], left.1[0]);
        assert!((hi[1] - lo[1] - 18.0).abs() < 0.01, "полка горизонтальна, толщина 18 по вертикали");
    }
}
