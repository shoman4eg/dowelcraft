//! All samples from `example/` must read completely, and slab contours must parse.

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

/// A "model + Bazis estimate" pair: slab and edge banding consumption matches the estimate.
#[test]
fn materials_match_bazis_estimate() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/pairs/sanuzly/1.0711 Мебель в санузлы.b3d");
    let Ok(data) = std::fs::read(&p) else {
        eprintln!("нет reference/pairs/sanuzly — тест пропущен");
        return;
    };
    let ex = dowel::panel::panels(&dowel::b3d::parse(&data).unwrap());
    // Factors from the estimate: fronts 1, everything else 1.2.
    let rows = dowel::panel::materials(&ex.panels, |m| if m.starts_with("Фасад") { 1.0 } else { 1.2 });
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

/// Hardware from the model matches the Bazis estimate for piece items.
#[test]
fn hardware_matches_bazis_estimate() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/pairs/sanuzly/1.0711 Мебель в санузлы.b3d");
    let Ok(data) = std::fs::read(&p) else { return };
    let hw = dowel::panel::hardware(&dowel::b3d::parse(&data).unwrap());
    let qty = |article: &str| hw.iter().filter(|(n, _)| n.ends_with(&format!("\r{article}"))).map(|r| r.1).sum::<usize>();
    // Values from the estimate.
    for (article, want) in [("113706", 50), ("4*16", 116), ("4*20", 24), ("4*30", 107), ("9071313/9073662", 16),
        ("9071576", 20), ("BU17PN", 40), ("FRM0598/1", 8), ("DU-10х62", 13), ("12535", 2), ("GOLA BALANCE RPK001", 6)] {
        assert_eq!(qty(article), want, "{article}");
    }
    // Hinge cups sit in blocks flagged "not in estimate" — Bazis does not count them.
    assert!(!hw.iter().any(|(n, _)| n.starts_with("Чашка петли")));
}

/// Importing a Bazis model into our project keeps the parts: same saw sizes, materials, placement.
#[test]
fn import_basis_model_into_project() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/pairs/kuhnya-24-04/_Кухня 24.04.b3d");
    let Ok(data) = std::fs::read(&p) else { return };
    let ex = dowel::panel::panels(&dowel::b3d::parse(&data).unwrap());
    let src = &ex.panels;
    let project = dowel::design::import("Кухня", src, &ex.blocks, &ex.panel_block);
    assert_eq!(project.materials.len(), 3);
    let project = dowel::design::Project::load(&project.save()).expect("сохраняется и читается");
    let parts = dowel::design::build(&project).unwrap();
    assert_eq!(parts.len(), src.len());
    for (a, b) in src.iter().zip(&parts) {
        assert_eq!((a.cut_size(), &a.name, a.thickness), (b.panel.cut_size(), &b.panel.name, b.panel.thickness));
        // Placement is exact: same slab coordinate system, same bounding box.
        assert_eq!(a.frame, b.panel.frame);
        assert_eq!(a.bounds().unwrap().0, b.origin);
    }
    // Bazis blocks are preserved: there are blocks, and slabs lie in them.
    let blocks = project.objects.iter().filter(|o| matches!(o, dowel::design::Object::Block(_))).count();
    assert!(blocks > 0 && !ex.blocks.is_empty(), "блоков {blocks} из {}", ex.blocks.len());
    // A part can be moved to another material with a command.
    let mut s = dowel::design::Session::new(project);
    let other = s.project.materials[1].name.clone();
    s.execute("panel.set", &serde_json::json!({"id": 1, "material": other})).unwrap();
    assert!(s.execute("panel.set", &serde_json::json!({"id": 1, "material": "нет такого"})).is_err());
}

/// Cabinet drilling: every fastener hole found its slab and lies on its surface; exported to Raskroy XML.
#[test]
fn drilling_of_wardrobe() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/шкаф.b3d");
    let Ok(data) = std::fs::read(&p) else { return };
    let ex = dowel::panel::panels(&dowel::b3d::parse(&data).unwrap());
    let holes: usize = ex.panels.iter().map(|p| p.holes.len()).sum();
    assert_eq!((holes, ex.holes_lost), (80, 0));
    for p in &ex.panels {
        for h in &p.holes {
            let on_face = h.pos[2].abs() < 0.01 || (h.pos[2] - p.thickness).abs() < 0.01;
            let on_edge = [h.pos[0], h.pos[1]].iter().zip([p.size.0, p.size.1]).any(|(v, s)| v.abs() < 0.01 || (v - s).abs() < 0.01);
            assert!(on_face || on_edge, "«{}»: отверстие {:?} не на поверхности", p.name, h.pos);
            // An edge hole is at mid-thickness.
            if h.dir[2].abs() < 0.5 {
                assert!((h.pos[2] - p.thickness / 2.0).abs() < 0.5, "«{}»: торцевое отверстие не по центру толщины", p.name);
            }
        }
    }
    let xml = dowel::raskroy::project_string("шкаф", &ex.panels, |_| (2800.0, 2070.0));
    let n: usize = xml.matches("<DRILLING>").count();
    // <DRILLING> — both the per-part wrapper (11) and each hole (80).
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
    let mut open_contours = Vec::new();
    for f in &files {
        let file = dowel::b3d::parse(&std::fs::read(f).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert!(file.section("Header").is_some(), "{}: нет Header", f.display());
        // Writing and reading back gives the same tree (A3: writing .b3d).
        assert!(dowel::b3d::parse(&dowel::b3d::write(&file)).as_ref() == Ok(&file), "{}: запись меняет дерево", f.display());
        let ex = dowel::panel::panels(&file);
        assert!(ex.skipped.is_empty(), "{}: не разобраны контуры {:?}", f.display(), ex.skipped);
        for p in &ex.panels {
            let (l, w) = p.cut_size();
            assert!(l > 0.0 && w > 0.0 && p.thickness > 0.0, "{}: «{}» {l}x{w}x{}", f.display(), p.name, p.thickness);
        }
        panels += ex.panels.len();
        // Every slab's contour chains into closed contours and yields a 3D mesh.
        let parts: Vec<dowel::design::Part> = ex
            .panels
            .iter()
            .map(|p| dowel::design::Part { owner: 0, id: 0, kind: dowel::design::Kind::Body, panel: p.clone(), origin: [0.0; 3], size: [0.0; 3] })
            .collect();
        let mesh = dowel::mesh::build(&parts, |_| false);
        // The "proverit kontur.b3d" model has a deliberately open contour: the error must be found.
        let broken = f.file_name().is_some_and(|n| n.to_string_lossy().starts_with("проверить контур"));
        assert_eq!(broken, !mesh.errors.is_empty(), "{}: {:?}", f.display(), mesh.errors);
        for (i, e) in mesh.errors.iter().filter(|_| !broken) {
            open_contours.push(format!("{}: «{}»: {e}", f.display(), ex.panels[*i].name));
        }
    }
    eprintln!("файлов {}, плит {panels}", files.len());
    assert!(open_contours.is_empty(), "незамкнутые контуры ({}):\n{}", open_contours.len(), open_contours.join("\n"));
}

/// Bazis slab placement: in a cabinet the shelves sit between the sides rather than into them.
#[test]
fn panel_positions_in_space() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/шкаф.b3d");
    let Ok(data) = std::fs::read(&p) else { return };
    let ex = dowel::panel::panels(&dowel::b3d::parse(&data).unwrap());
    let b = |name: &str| ex.panels.iter().filter(|p| p.name == name).map(|p| p.bounds().unwrap()).collect::<Vec<_>>();
    let left = b("Вертикальная1")[0];
    let shelves = b("Горизонтальная1");
    assert_eq!(shelves.len(), 8);
    for (lo, hi) in shelves {
        assert!((lo[0] - left.1[0]).abs() < 0.01, "полка начинается у правой грани левой боковины: {} vs {}", lo[0], left.1[0]);
        assert!((hi[1] - lo[1] - 18.0).abs() < 0.01, "полка горизонтальна, толщина 18 по вертикали");
    }
}

/// Bazis model analysis: a back panel in a side's groove is not an intersection (grooves are read from the model).
#[test]
fn analysis_respects_grooves() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/шкаф.b3d");
    let Ok(job) = dowel::job::Job::open(&p) else { return };
    assert!(job.panels.iter().any(|p| !p.grooves.is_empty()), "в шкафу есть пазы");
    let problems = dowel::design::analysis::analyze(&job.import()).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
}
