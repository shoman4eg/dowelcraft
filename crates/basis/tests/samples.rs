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

/// Фурнитура из модели совпадает со сметой Базиса по штучным позициям.
#[test]
fn hardware_matches_bazis_estimate() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/pairs/sanuzly/1.0711 Мебель в санузлы.b3d");
    let Ok(data) = std::fs::read(&p) else { return };
    let hw = basis::panel::hardware(&basis::b3d::parse(&data).unwrap());
    let qty = |article: &str| hw.iter().filter(|(n, _)| n.ends_with(&format!("\r{article}"))).map(|r| r.1).sum::<usize>();
    // Значения из сметы.
    for (article, want) in [("113706", 50), ("4*16", 116), ("4*20", 24), ("4*30", 107), ("9071313/9073662", 16),
        ("9071576", 20), ("BU17PN", 40), ("FRM0598/1", 8), ("DU-10х62", 13), ("12535", 2), ("GOLA BALANCE RPK001", 6)] {
        assert_eq!(qty(article), want, "{article}");
    }
    // Чашки петель лежат в блоках с флагом «не в смету» — Базис их не считает.
    assert!(!hw.iter().any(|(n, _)| n.starts_with("Чашка петли")));
}

/// Импорт модели Базиса в свой проект сохраняет детали: те же пильные размеры, материалы, положение.
#[test]
fn import_basis_model_into_project() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/pairs/kuhnya-24-04/_Кухня 24.04.b3d");
    let Ok(data) = std::fs::read(&p) else { return };
    let src = basis::panel::panels(&basis::b3d::parse(&data).unwrap()).panels;
    let project = basis::design::import("Кухня", &src);
    assert_eq!(project.materials.len(), 3);
    let project = basis::design::Project::load(&project.save()).expect("сохраняется и читается");
    let parts = basis::design::build(&project).unwrap();
    assert_eq!(parts.len(), src.len());
    for (a, b) in src.iter().zip(&parts) {
        assert_eq!((a.cut_size(), &a.name, a.thickness), (b.panel.cut_size(), &b.panel.name, b.panel.thickness));
        let (lo, _) = a.bounds.unwrap();
        assert_eq!(lo, b.origin);
    }
    // Деталь можно перевести в другой материал командой.
    let mut s = basis::design::Session::new(project);
    let other = s.project.materials[1].name.clone();
    s.execute("panel.set", &serde_json::json!({"id": 1, "material": other})).unwrap();
    assert!(s.execute("panel.set", &serde_json::json!({"id": 1, "material": "нет такого"})).is_err());
}

/// Присадка шкафа: все отверстия крепежа нашли плиту и лежат на её поверхности; выгружаются в XML Раскроя.
#[test]
fn drilling_of_wardrobe() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/шкаф.b3d");
    let Ok(data) = std::fs::read(&p) else { return };
    let ex = basis::panel::panels(&basis::b3d::parse(&data).unwrap());
    let holes: usize = ex.panels.iter().map(|p| p.holes.len()).sum();
    assert_eq!((holes, ex.holes_lost), (80, 0));
    for p in &ex.panels {
        for h in &p.holes {
            let on_face = h.pos[2].abs() < 0.01 || (h.pos[2] - p.thickness).abs() < 0.01;
            let on_edge = [h.pos[0], h.pos[1]].iter().zip([p.size.0, p.size.1]).any(|(v, s)| v.abs() < 0.01 || (v - s).abs() < 0.01);
            assert!(on_face || on_edge, "«{}»: отверстие {:?} не на поверхности", p.name, h.pos);
            // Торцевое отверстие — посередине толщины.
            if h.dir[2].abs() < 0.5 {
                assert!((h.pos[2] - p.thickness / 2.0).abs() < 0.5, "«{}»: торцевое отверстие не по центру толщины", p.name);
            }
        }
    }
    let xml = basis::raskroy::project_string("шкаф", &ex.panels, |_| (2800.0, 2070.0));
    let n: usize = xml.matches("<DRILLING>").count();
    // <DRILLING> — и обёртка у каждой детали (11), и каждое отверстие (80).
    assert_eq!(n, 11 + 80);
    assert!(xml.contains("<TYPE>EDGEDIR</TYPE>") && xml.contains("<TYPE>FACETOBACKDIR</TYPE>"));
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
