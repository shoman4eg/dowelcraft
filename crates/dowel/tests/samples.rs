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

/// The only .b3d model in a folder of the repository (Bazis names are Russian, so it is found by extension).
fn first_b3d(dir: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(dir);
    std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "b3d")).unwrap_or(dir)
}

/// The wardrobe sample in `example/` (its name is Russian): the only top-level model with 11 slabs in 17 blocks.
fn wardrobe() -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example");
    let is_wardrobe = |p: &Path| {
        let Some(file) = std::fs::read(p).ok().and_then(|d| dowel::b3d::parse(&d).ok()) else { return false };
        let ex = dowel::panel::panels(&file);
        ex.panels.len() == 11 && ex.blocks.len() == 17
    };
    std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "b3d")).find(|p| is_wardrobe(p)).unwrap_or(dir)
}

/// A "model + Bazis estimate" pair: slab and edge banding consumption matches the estimate.
#[test]
fn materials_match_bazis_estimate() {
    let p = first_b3d("reference/pairs/sanuzly");
    let Ok(data) = std::fs::read(&p) else {
        eprintln!("no reference/pairs/sanuzly — test skipped");
        return;
    };
    let ex = dowel::panel::panels(&dowel::b3d::parse(&data).unwrap());
    // Factors from the estimate: fronts 1, everything else 1.2.
    let front = dowel::i18n::bazis("guess.door").split(',').next().unwrap();
    let rows = dowel::panel::materials(&ex.panels, |m| if m.to_lowercase().starts_with(front) { 1.0 } else { 1.2 });
    // Rows are found by the article after '\r' (the names are Russian).
    let qty = |article: &str| rows.iter().filter(|r| r.material.contains(&format!("\r{article}"))).map(|r| r.qty).collect::<Vec<_>>();
    for (article, want) in [
        ("810(PO)_", vec![8.5]),
        ("8685(BS)_", vec![2.3]),
        ("0101_", vec![0.6]),
        // Two enamel fronts, white and pearl.
        ("FL1_16_", vec![2.1, 5.6]),
        ("810 PO_0.4*19", vec![72.0]),
        ("8685 (BS)_0.4*19", vec![26.0]),
    ] {
        let got = qty(article);
        assert!(got.len() == want.len() && got.iter().zip(&want).all(|(g, w)| (g - w).abs() < 1e-6), "{article}: ours {got:?}, the estimate {want:?}");
    }
}

/// Hardware from the model matches the Bazis estimate for piece items.
#[test]
fn hardware_matches_bazis_estimate() {
    let p = first_b3d("reference/pairs/sanuzly");
    let Ok(data) = std::fs::read(&p) else { return };
    let hw = dowel::panel::hardware(&dowel::b3d::parse(&data).unwrap());
    let qty = |article: &str| hw.iter().filter(|(n, _)| n.ends_with(&format!("\r{article}"))).map(|r| r.1).sum::<usize>();
    // Values from the estimate.
    for (article, want) in [("113706", 50), ("4*16", 116), ("4*20", 24), ("4*30", 107), ("9071313/9073662", 16),
        ("9071576", 20), ("BU17PN", 40), ("FRM0598/1", 8), ("12535", 2), ("GOLA BALANCE RPK001", 6)] {
        assert_eq!(qty(article), want, "{article}");
    }
    // The wall plug article "DU-10x62" is written with a Cyrillic "x".
    assert_eq!(hw.iter().filter(|(n, _)| n.contains("\rDU-10") && n.ends_with("62")).map(|r| r.1).sum::<usize>(), 13);
    // Hinge cups sit in blocks flagged "not in estimate" — Bazis does not count them.
    assert!(!hw.iter().any(|(n, _)| n.contains("TH52 D35")), "Sensys hinge cups are not counted");
}

/// Importing a Bazis model into our project keeps the parts: same saw sizes, materials, placement.
#[test]
fn import_basis_model_into_project() {
    let p = first_b3d("reference/pairs/kuhnya-24-04");
    let Ok(data) = std::fs::read(&p) else { return };
    let ex = dowel::panel::panels(&dowel::b3d::parse(&data).unwrap());
    let src = &ex.panels;
    let project = dowel::design::import("Kitchen", src, &ex.blocks, &ex.panel_block);
    assert_eq!(project.materials.len(), 3);
    let project = dowel::design::Project::load(&project.save()).expect("saves and loads");
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
    assert!(blocks > 0 && !ex.blocks.is_empty(), "{blocks} blocks of {}", ex.blocks.len());
    // A part can be moved to another material with a command.
    let mut s = dowel::design::Session::new(project);
    let other = s.project.materials[1].name.clone();
    s.execute("panel.set", &serde_json::json!({"id": 1, "material": other})).unwrap();
    assert!(s.execute("panel.set", &serde_json::json!({"id": 1, "material": "no such"})).is_err());
}

/// Cabinet drilling: every fastener hole found its slab and lies on its surface; exported to Raskroy XML.
#[test]
fn drilling_of_wardrobe() {
    let p = wardrobe();
    let Ok(data) = std::fs::read(&p) else { return };
    let ex = dowel::panel::panels(&dowel::b3d::parse(&data).unwrap());
    let holes: usize = ex.panels.iter().map(|p| p.holes.len()).sum();
    assert_eq!((holes, ex.holes_lost), (80, 0));
    for p in &ex.panels {
        for h in &p.holes {
            let on_face = h.pos[2].abs() < 0.01 || (h.pos[2] - p.thickness).abs() < 0.01;
            let on_edge = [h.pos[0], h.pos[1]].iter().zip([p.size.0, p.size.1]).any(|(v, s)| v.abs() < 0.01 || (v - s).abs() < 0.01);
            assert!(on_face || on_edge, "“{}”: hole {:?} is not on the surface", p.name, h.pos);
            // An edge hole is at mid-thickness.
            if h.dir[2].abs() < 0.5 {
                assert!((h.pos[2] - p.thickness / 2.0).abs() < 0.5, "“{}”: edge hole is not at mid-thickness", p.name);
            }
        }
    }
    let xml = dowel::raskroy::project_string("wardrobe", &ex.panels, |_| (2800.0, 2070.0));
    let n: usize = xml.matches("<DRILLING>").count();
    // <DRILLING> — both the per-part wrapper (11) and each hole (80).
    assert_eq!(n, 11 + 80);
    assert!(xml.contains("<TYPE>EDGEDIR</TYPE>") && xml.contains("<TYPE>FACETOBACKDIR</TYPE>"));
}

#[test]
fn all_samples_parse() {
    let files = samples();
    if files.is_empty() {
        eprintln!("no samples in example/ — test skipped");
        return;
    }
    let mut panels = 0;
    let mut open_contours = Vec::new();
    for f in &files {
        let file = dowel::b3d::parse(&std::fs::read(f).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert!(file.section("Header").is_some(), "{}: no Header", f.display());
        // Writing and reading back gives the same tree (A3: writing .b3d).
        assert!(dowel::b3d::parse(&dowel::b3d::write(&file)).as_ref() == Ok(&file), "{}: writing changes the tree", f.display());
        let ex = dowel::panel::panels(&file);
        assert!(ex.skipped.is_empty(), "{}: contours not parsed {:?}", f.display(), ex.skipped);
        for p in &ex.panels {
            let (l, w) = p.cut_size();
            assert!(l > 0.0 && w > 0.0 && p.thickness > 0.0, "{}: “{}” {l}x{w}x{}", f.display(), p.name, p.thickness);
        }
        panels += ex.panels.len();
        // Every slab's contour chains into closed contours and yields a 3D mesh.
        let parts: Vec<dowel::design::Part> = ex
            .panels
            .iter()
            .map(|p| dowel::design::Part { owner: 0, id: 0, kind: dowel::design::Kind::Body, panel: p.clone(), origin: [0.0; 3], size: [0.0; 3] })
            .collect();
        let mesh = dowel::mesh::build(&parts, |_| false);
        if !mesh.errors.is_empty() {
            let errors = mesh.errors.iter().map(|(i, e)| format!("“{}”: {e}", ex.panels[*i].name)).collect::<Vec<_>>();
            open_contours.push((f, errors.join("\n")));
        }
    }
    eprintln!("files {}, slabs {panels}", files.len());
    // Exactly one model, "check contour" in example/telegram, has a deliberately open contour: the error must be found.
    let telegram = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/telegram");
    let report = open_contours.iter().map(|(f, e)| format!("{}: {e}", f.display())).collect::<Vec<_>>().join("\n");
    assert!(open_contours.len() == usize::from(telegram.exists()) && open_contours.iter().all(|(f, _)| f.parent() == Some(&telegram)), "open contours ({}):\n{report}", open_contours.len());
}

/// Bazis slab placement: in a cabinet the shelves sit between the sides rather than into them.
#[test]
fn panel_positions_in_space() {
    let p = wardrobe();
    let Ok(data) = std::fs::read(&p) else { return };
    let ex = dowel::panel::panels(&dowel::b3d::parse(&data).unwrap());
    let b = |name: &str| ex.panels.iter().filter(|p| p.name == name).map(|p| p.bounds().unwrap()).collect::<Vec<_>>();
    // Shelves share the most frequent slab name; the left side is the leftmost slab.
    let shelf = &ex.panels.iter().max_by_key(|p| ex.panels.iter().filter(|q| q.name == p.name).count()).unwrap().name;
    let left = ex.panels.iter().map(|p| p.bounds().unwrap()).min_by(|a, b| a.0[0].total_cmp(&b.0[0])).unwrap();
    let shelves = b(shelf);
    assert_eq!(shelves.len(), 8);
    for (lo, hi) in shelves {
        assert!((lo[0] - left.1[0]).abs() < 0.01, "the shelf starts at the right face of the left side: {} vs {}", lo[0], left.1[0]);
        assert!((hi[1] - lo[1] - 18.0).abs() < 0.01, "the shelf is horizontal, 18 thick vertically");
    }
}

/// Bazis model analysis: a back panel in a side's groove is not an intersection (grooves are read from the model).
#[test]
fn analysis_respects_grooves() {
    let p = wardrobe();
    let Ok(job) = dowel::job::Job::open(&p) else { return };
    assert!(job.panels.iter().any(|p| !p.grooves.is_empty()), "the wardrobe has grooves");
    let problems = dowel::design::analysis::analyze(&job.import()).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
}
