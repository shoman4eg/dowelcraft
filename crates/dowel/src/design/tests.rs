//! Model tests: generators, object tree, commands.
use super::*;

fn wardrobe() -> Session {
    let mut s = Session::new(Project::new("Wardrobe"));
    s.execute("material.add", &json!({"name": "Chipboard White 16", "article": "W980", "thickness": 16})).unwrap();
    s.execute("material.add", &json!({"name": "HDF 3", "thickness": 3, "coef": 1.1})).unwrap();
    s.execute("edge.add", &json!({"name": "PVC 0.4 white", "thickness": 0.4, "width": 19})).unwrap();
    let id = s
        .execute("cabinet.add", &json!({"width": 800, "height": 2000, "depth": 600, "material": "Chipboard White 16",
            "edge": "PVC 0.4 white", "plinth": 80, "back": "HDF 3", "doors": 2}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    s.execute("partition.add", &json!({"cabinet": id, "x": 392})).unwrap();
    s.execute("shelf.add", &json!({"cabinet": id, "height": 400, "section": 0})).unwrap();
    s.execute("shelf.add", &json!({"cabinet": id, "height": 800, "section": 1})).unwrap();
    s
}

fn by_name<'a>(parts: &'a [Part], name: &str) -> Vec<&'a Part> {
    parts.iter().filter(|p| p.panel.name == name).collect()
}

#[test]
fn wardrobe_parts_and_sizes() {
    let s = wardrobe();
    let parts = build(&s.project).unwrap();
    // 2 side panels, bottom, top, plinth, partition, 2 shelves, back panel, 2 doors.
    assert_eq!(parts.len(), 11);
    let side = by_name(&parts, "Left side")[0];
    assert_eq!(side.panel.size, (600.0, 2000.0));
    // Front edge banding 0.4 trims the blank in depth; length along the grain is the height.
    assert_eq!(side.panel.cut_size(), (2000.0, 599.6));
    assert_eq!(by_name(&parts, "Bottom")[0].panel.size, (768.0, 600.0));
    assert_eq!(by_name(&parts, "Partition")[0].panel.size, (600.0, 2000.0 - 80.0 - 32.0));
    let shelves = by_name(&parts, "Shelf");
    // Sections: 16..392 (376 mm) and 408..784 (376 mm).
    assert_eq!(shelves[0].panel.size.0, 376.0);
    assert_eq!(shelves[1].origin[0], 408.0);
    let doors = by_name(&parts, "Door");
    assert_eq!(doors.len(), 2);
    assert_eq!(doors[0].panel.size, (397.0, 1916.0));
    assert_eq!(doors[0].panel.edges.len(), 4);
    assert_eq!(by_name(&parts, "Back panel")[0].panel.thickness, 3.0);
    assert_eq!(parts[0].panel.pos, "1");
}

#[test]
fn invalid_changes_are_rejected_and_project_stays_valid() {
    let mut s = wardrobe();
    let before = s.project.clone();
    assert!(s.execute("shelf.add", &json!({"cabinet": 1, "height": 5000})).is_err());
    assert!(s.execute("partition.add", &json!({"cabinet": 1, "x": 790})).is_err());
    assert!(s.execute("cabinet.set", &json!({"id": 1, "material": "no such"})).is_err());
    assert!(s.execute("cabinet.set", &json!({"id": 1, "width": 20})).is_err());
    assert!(s.execute("shelf.add", &json!({"cabinet": 1, "height": 100, "section": 7})).is_err());
    assert!(s.execute("bogus", &json!({})).is_err());
    assert_eq!(s.project, before);
}

#[test]
fn set_undo_redo() {
    let mut s = wardrobe();
    s.execute("cabinet.set", &json!({"id": 1, "width": 1000, "doors": 3})).unwrap();
    assert_eq!(by_name(&build(&s.project).unwrap(), "Door").len(), 3);
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.project.cabinets[0].width, 800.0);
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(s.project.cabinets[0].width, 1000.0);
    assert!(s.execute("redo", &json!({})).is_err());
}

#[test]
fn drawers_countertop_and_wall_module() {
    let mut s = wardrobe();
    s.execute("cabinet.add", &json!({"name": "Desk with drawers", "width": 600, "height": 820, "depth": 560,
        "material": "Chipboard White 16", "edge": "PVC 0.4 white", "plinth": 100, "back": "HDF 3", "drawers": [140, 280, 280]})).unwrap();
    let parts = build(&s.project).unwrap();
    let mine: Vec<&Part> = parts.iter().filter(|p| p.owner == 2).collect();
    let count = |n: &str| mine.iter().filter(|p| p.panel.name == n).count();
    assert_eq!((count("Drawer front"), count("Drawer side"), count("Drawer bottom")), (3, 6, 3));
    // Box: 600 − 2·16 − 2·13 = 542 wide, drawer runners 550 (depth 560 → multiple of 50).
    let bottom = mine.iter().find(|p| p.panel.name == "Drawer bottom").unwrap();
    assert_eq!(bottom.panel.size, (542.0, 550.0));
    assert_eq!(bottom.panel.thickness, 3.0);
    let side = mine.iter().find(|p| p.panel.name == "Drawer side").unwrap();
    assert_eq!(side.panel.size, (550.0, 100.0));
    // Fronts don't extend beyond the cabinet: 100 + 2 + 140+2 + 280+2 + 280+2 = 808 ≤ 820.
    let top = mine.iter().filter(|p| p.kind == Kind::Drawer).map(|p| p.origin[1] + p.size[1]).fold(0.0, f64::max);
    assert!(top <= 820.0, "{top}");
    // Drawers that are too tall and a shelf in the drawer zone are rejected.
    assert!(s.execute("cabinet.set", &json!({"id": 2, "drawers": [400, 400]})).is_err());
    assert!(s.execute("shelf.add", &json!({"cabinet": 2, "height": 100})).is_err());
    // Drawers in the right section of a cabinet with a partition: box as wide as the section.
    s.execute("cabinet.add", &json!({"name": "Base unit", "width": 1000, "height": 820, "depth": 560, "material": "Chipboard White 16",
        "partitions": [392], "drawers": [200, 200], "drawer_section": 1, "doors": 0})).unwrap();
    let parts = build(&s.project).unwrap();
    let tumba: Vec<&Part> = parts.iter().filter(|p| p.owner == 3).collect();
    let bottom = tumba.iter().find(|p| p.panel.name == "Drawer bottom").unwrap();
    // Section 408…984 (576 mm) − 2·13 = 550; the box starts at the partition + 13.
    assert_eq!((bottom.panel.size.0, bottom.origin[0]), (550.0, 421.0));
    let facade = tumba.iter().find(|p| p.kind == Kind::Drawer).unwrap();
    // From the partition middle (400) to the right edge (1000), minus gaps of 2.
    assert_eq!((facade.origin[0], facade.size[0]), (402.0, 596.0));
    assert!(s.execute("cabinet.set", &json!({"id": 3, "drawer_section": 5})).is_err());
    s.execute("cabinet.remove", &json!({"id": 3})).unwrap();

    s.execute("countertop.add", &json!({"material": "Chipboard White 16", "length": 1400, "depth": 600,
        "position": [0, 2000, 0], "edge": "PVC 0.4 white"})).unwrap();
    s.execute("cabinet.add", &json!({"name": "Wall unit", "width": 600, "height": 720, "depth": 300,
        "material": "Chipboard White 16", "doors": 1, "position": [0, 1400, 300]})).unwrap();
    let parts = build(&s.project).unwrap();
    let ct = parts.iter().find(|p| p.panel.name == "Countertop").unwrap();
    assert_eq!((ct.panel.size, ct.origin[1]), ((1400.0, 600.0), 2000.0));
    // Ids are shared by all objects: cabinets 1 and 2, countertop 3, wall cabinet 4.
    let wall = parts.iter().filter(|p| p.owner == 4).map(|p| p.origin[1]).fold(f64::MAX, f64::min);
    assert_eq!(wall, 1400.0);
    assert!(s.execute("countertop.set", &json!({"id": 3, "length": -5})).is_err());
}

#[test]
fn iso_view_shows_three_faces_per_box() {
    let parts = build(&wardrobe().project).unwrap();
    let faces = iso_view(&parts, true);
    // From front-right-top, the front, right face and top of every part are visible.
    assert_eq!(faces.len(), parts.len() * 3);
    assert_eq!(iso_view(&parts, false).len(), (parts.len() - 2) * 3);
}

#[test]
fn corner_module() {
    let mut s = wardrobe();
    let id = s.execute("corner.add", &json!({"width_x": 1000, "width_z": 900, "height": 820, "depth": 560, "material": "Chipboard White 16",
        "edge": "PVC 0.4 white", "plinth": 100, "back": "HDF 3", "shelves": [300], "doors": true, "position": [3000, 0, 0]})).unwrap()["id"].clone();
    let parts: Vec<Part> = build(&s.project).unwrap().into_iter().filter(|p| p.origin[0] >= 2990.0 || p.panel.name.contains("Corner")).collect();
    let bottom = parts.iter().find(|p| p.panel.name == "Corner bottom").unwrap();
    // L-contour of 6 segments: a shaped part (BOARDFIG), front edge banding on the two inner sides.
    assert_eq!(bottom.panel.segs.len(), 6);
    assert!(!bottom.panel.is_rect());
    assert_eq!(bottom.panel.size, (984.0, 884.0));
    assert_eq!(bottom.panel.edges.iter().map(|e| e.segment).collect::<Vec<_>>(), vec![3, 4]);
    assert_eq!(parts.iter().filter(|p| p.kind == Kind::Door).count(), 2);
    assert_eq!(parts.iter().filter(|p| p.kind == Kind::Back).count(), 2);
    // Doors don't intersect in the inner corner.
    let doors: Vec<&Part> = parts.iter().filter(|p| p.kind == Kind::Door).collect();
    let overlap = (0..3).all(|k| doors[0].origin[k] < doors[1].origin[k] + doors[1].size[k] && doors[1].origin[k] < doors[0].origin[k] + doors[0].size[k]);
    assert!(!overlap, "doors overlap");
    assert!(s.execute("corner.set", &json!({"id": id, "width_x": 600})).is_err(), "wing shorter than the depth");
    let xml = crate::raskroy::project_string("t", &parts.iter().map(|p| p.panel.clone()).collect::<Vec<_>>(), |_| (2800.0, 2070.0));
    assert_eq!(xml.matches("<BOARDFIG>").count(), 3, "bottom, top, shelf");
    s.execute("corner.remove", &json!({"id": id})).unwrap();
    assert!(s.project.corners.is_empty());
}

/// Every generator part has an exact position: axes exactly 0/±1, the bounding box from the position matches
/// the given one, thickness points across the part plane.
#[test]
fn generated_parts_have_exact_frames() {
    let mut s = wardrobe();
    s.execute("cabinet.add", &json!({"width": 600, "height": 820, "depth": 560, "material": "Chipboard White 16",
        "edge": "PVC 0.4 white", "plinth": 100, "back": "HDF 3", "drawers": [140, 280, 280], "position": [1000, 0, 0]})).unwrap();
    s.execute("corner.add", &json!({"width_x": 1000, "width_z": 900, "height": 820, "depth": 560, "material": "Chipboard White 16",
        "plinth": 100, "back": "HDF 3", "shelves": [300], "doors": true, "position": [3000, 0, 0]})).unwrap();
    s.execute("countertop.add", &json!({"material": "Chipboard White 16", "length": 1400, "depth": 600, "position": [0, 2000, 0]})).unwrap();
    let parts = build(&s.project).unwrap();
    assert!(parts.len() > 40);
    for p in &parts {
        let f = p.panel.frame.unwrap();
        f.check().unwrap();
        assert!(f.x.iter().chain(&f.y).chain(&f.z).all(|v| [0.0, 1.0, -1.0].contains(v)), "{}: {f:?}", p.panel.name);
        let thick_axis = (0..3).find(|&k| f.z[k] != 0.0).unwrap();
        assert_eq!(p.size[thick_axis], p.panel.thickness, "{}", p.panel.name);
    }
    let shelf = by_name(&parts, "Shelf")[0];
    // Horizontal: contour X → X, Y → +Z (front edge towards the viewer), thickness down from the top face.
    let f = shelf.panel.frame.unwrap();
    assert_eq!((f.x, f.y, f.z), ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]));
    assert_eq!(f.origin, [16.0, 80.0 + 16.0 + 400.0 + 16.0, 0.0]);
}

#[test]
fn free_panels_in_three_planes_and_blocks() {
    let mut s = wardrobe();
    let add = |s: &mut Session, plane: &str, size: [f64; 2], pos: [f64; 3]| {
        s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": plane, "size": size, "position": pos})).unwrap()["id"].as_u64().unwrap() as u32
    };
    let v = add(&mut s, "vertical", [500.0, 700.0], [2000.0, 0.0, 0.0]);
    let h = add(&mut s, "horizontal", [600.0, 500.0], [2016.0, 300.0, 0.0]);
    let fr = add(&mut s, "front", [600.0, 700.0], [2016.0, 0.0, 500.0]);
    let parts = build(&s.project).unwrap();
    let get = |id: u32| parts.iter().find(|p| p.owner == id).unwrap();
    assert_eq!((get(v).origin, get(v).size), ([2000.0, 0.0, 0.0], [16.0, 700.0, 500.0]));
    assert_eq!((get(h).origin, get(h).size), ([2016.0, 300.0, 0.0], [600.0, 16.0, 500.0]));
    assert_eq!((get(fr).origin, get(fr).size), ([2016.0, 0.0, 500.0], [600.0, 700.0, 16.0]));
    assert_eq!(get(v).panel.name, format!("Vertical {v}"));
    assert!(s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": "front", "size": [0, 10]})).is_err());
    assert!(s.execute("panel.add", &json!({"material": "none", "plane": "front", "size": [10, 10]})).is_err());

    // Block with a panel inside: the block position shifts the panel; deleting the block removes all its contents.
    s.project.objects.push(Object::Block(Block { id: 100, name: "Block".into(), frame: Frame::IDENTITY, children: Vec::new() }));
    let inner = s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": "front", "size": [100, 100], "block": 100})).unwrap()["id"].as_u64().unwrap();
    s.execute("block.set", &json!({"id": 100, "name": "Base unit", "frame": {"origin": [0, 0, 1000], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}})).unwrap();
    let parts = build(&s.project).unwrap();
    assert_eq!(parts.iter().find(|p| p.panel.name == format!("Front {inner}")).unwrap().origin, [0.0, 0.0, 1000.0]);
    assert!(s.execute("block.set", &json!({"id": 100, "frame": {"origin": [0, 0, 0], "x": [1, 0, 0], "y": [1, 0, 0], "z": [0, 0, 1]}})).is_err(), "axes are not perpendicular");
    assert!(s.execute("panel.remove", &json!({"id": 100})).is_err(), "this is a block, not a part");
    s.execute("block.remove", &json!({"id": 100})).unwrap();
    assert_eq!(s.project.panels().len(), 3);
    let back = Project::load(&s.project.save()).unwrap();
    assert_eq!(back, s.project);
}

#[test]
fn explode_keeps_parts_exactly() {
    let mut s = wardrobe();
    let before = build(&s.project).unwrap();
    s.execute("cabinet.explode", &json!({"id": 1})).unwrap();
    assert!(s.project.cabinets.is_empty());
    let after = build(&s.project).unwrap();
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert_eq!((&a.panel, a.kind, a.origin, a.size, a.owner), (&b.panel, b.kind, b.origin, b.size, b.owner));
    }
    // Now the part is edited on its own.
    let side = s.project.panels().iter().find(|p| p.name == "Left side").unwrap().id;
    s.execute("panel.set", &json!({"id": side, "material": "HDF 3"})).unwrap();
    assert!(s.execute("cabinet.explode", &json!({"id": 1})).is_err());
}

#[test]
fn old_format_loads_without_free_panels() {
    let v1 = r#"{"format":"basis-craft/1","name":"Old","materials":[],"cabinets":[]}"#;
    assert_eq!(Project::load(v1).unwrap().format, FORMAT);
    let v1_panels = r#"{"format":"basis-craft/1","name":"Old","panels":[{"id":1}]}"#;
    assert!(Project::load(v1_panels).unwrap_err().contains("import the model again"));
    // A project saved under the old name basis-craft.
    let old_name = r#"{"format":"basis-craft/2","name":"Old","materials":[]}"#;
    assert_eq!(Project::load(old_name).unwrap().format, FORMAT);
    assert!(is_project_file(std::path::Path::new("wardrobe.DCRAFT")));
    assert!(!is_project_file(std::path::Path::new("wardrobe.b3d")));
}

#[test]
fn save_load_roundtrip() {
    let s = wardrobe();
    let back = Project::load(&s.project.save()).unwrap();
    assert_eq!(back, s.project);
    assert!(Project::load(r#"{"format":"x","name":"a"}"#).is_err());
}

#[test]
fn reports_work_on_own_model() {
    let parts = build(&wardrobe().project).unwrap();
    let panels: Vec<Panel> = parts.into_iter().map(|p| p.panel).collect();
    let rows = crate::panel::materials(&panels, |_| 1.0);
    let ldsp = rows.iter().find(|r| r.material.starts_with("Chipboard")).unwrap();
    assert!(ldsp.qty > 4.0 && ldsp.qty < 8.0, "{}", ldsp.qty);
    let xml = crate::raskroy::project_string("Wardrobe", &panels, |_| (2800.0, 2070.0));
    assert_eq!(xml.matches("<BOARDSMP>").count(), 11);
}

/// Panel into an opening: shelf between a side panel and a partition, upright between bottom and top, front insert.
#[test]
fn panel_fill_takes_the_opening() {
    let mut s = wardrobe();
    // Left section of the wardrobe: from the side panel (16) to the partition (392), depth 0…600.
    let id = |v: Value| v["id"].as_u64().unwrap() as u32;
    let shelf = id(s.execute("panel.fill", &json!({"material": "Chipboard White 16", "plane": "horizontal", "at": [200, 1500], "gaps": [0, 0, 0, 20]})).unwrap());
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == shelf).unwrap();
    assert_eq!((p.origin, p.size), ([16.0, 1500.0, 0.0], [376.0, 16.0, 580.0]));
    // Upright in the right section between the shelf (800 above the bottom → top 80+16+800+16 = 912) and the top (2000−16).
    let post = id(s.execute("panel.fill", &json!({"material": "Chipboard White 16", "plane": "vertical", "at": [600, 1500]})).unwrap());
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == post).unwrap();
    assert_eq!((p.origin, p.size), ([600.0, 912.0, 0.0], [16.0, 1984.0 - 912.0, 600.0]));
    // Front panel in the left section: between the bottom (top at 96) and the wardrobe shelf at 400 (bottom at 496), 2 mm gaps.
    let front = id(s.execute("panel.fill", &json!({"material": "Chipboard White 16", "plane": "front", "at": [200, 300], "gaps": [2, 2, 2, 2]})).unwrap());
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == front).unwrap();
    assert_eq!((p.origin, p.size), ([18.0, 98.0, 584.0], [372.0, 396.0, 16.0]));
    // No opening outside the wardrobe.
    assert!(s.execute("panel.fill", &json!({"material": "Chipboard White 16", "plane": "horizontal", "at": [5000, 500]})).is_err());
}

/// Edge banding on panel edges: applied, trims the blank, removed; errors are clear.
#[test]
fn panel_edges_by_segment() {
    let mut s = wardrobe();
    let id = s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": "horizontal", "size": [600, 400], "position": [2000, 0, 0]})).unwrap()["id"].clone();
    let cut = |s: &Session| build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id).unwrap().panel.cut_size();
    assert_eq!(cut(&s), (600.0, 400.0));
    // Rectangle edges: 0 — bottom, 1 — left, 2 — top, 3 — right.
    s.execute("panel.edge", &json!({"id": id, "segments": [2], "edge": "PVC 0.4 white"})).unwrap();
    s.execute("panel.edge", &json!({"id": id, "segments": [1, 3], "edge": "PVC 0.4 white", "allowance": 1})).unwrap();
    // Length: 600 − 2·0.4 + 2·1 (allowance); width: 400 − 0.4.
    let (l, w) = cut(&s);
    assert!((l - 601.2).abs() < 1e-9 && (w - 399.6).abs() < 1e-9, "{l} × {w}");
    s.execute("panel.edge", &json!({"id": id, "segments": [1, 2, 3], "edge": null})).unwrap();
    assert_eq!(cut(&s), (600.0, 400.0));
    assert!(s.execute("panel.edge", &json!({"id": id, "segments": [7], "edge": "PVC 0.4 white"})).is_err());
    assert!(s.execute("panel.edge", &json!({"id": id, "segments": [0], "edge": "no such"})).is_err());
    assert!(s.execute("panel.edge", &json!({"id": 1, "segments": [0], "edge": "PVC 0.4 white"})).unwrap_err().contains("Explode"));
}

/// Contour editing: corner fillet and chamfer, cut-out, resizing; the part doesn't move, errors are rejected.
#[test]
fn contour_editing() {
    let mut s = wardrobe();
    let id = s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": "front", "size": [600, 400], "position": [2000, 0, 0]})).unwrap()["id"].clone();
    let part = |s: &Session| build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id).unwrap();
    s.execute("panel.edge", &json!({"id": id, "segments": [0, 1, 2, 3], "edge": "PVC 0.4 white"})).unwrap();
    // R50 fillet at corner (600, 400): same bounding box, 5 edges, the new arc has the same edge banding.
    s.execute("panel.corner", &json!({"id": id, "at": [600, 400], "radius": 50})).unwrap();
    let p = part(&s);
    assert_eq!((p.origin, p.size), ([2000.0, 0.0, 0.0], [600.0, 400.0, 16.0]));
    assert_eq!(p.panel.segs.len(), 5);
    assert!(matches!(p.panel.segs[4], crate::contour::Seg::Arc { c, .. } if c == [550.0, 350.0]));
    assert_eq!(p.panel.edges.len(), 5);
    // 30 chamfer at corner (0, 0) and a round cut-out.
    s.execute("panel.corner", &json!({"id": id, "at": [0, 0], "chamfer": 30})).unwrap();
    s.execute("panel.cutout", &json!({"id": id, "center": [300, 200], "radius": 40})).unwrap();
    let m = crate::mesh::build(&[part(&s)], |_| false);
    assert!(m.errors.is_empty());
    // Errors: fillet larger than the edge, cut-out outside the panel, broken contour, resizing a shaped panel.
    assert!(s.execute("panel.corner", &json!({"id": id, "at": [600, 0], "radius": 900})).is_err());
    assert!(s.execute("panel.cutout", &json!({"id": id, "center": [900, 200], "radius": 10})).unwrap_err().contains("outside"));
    assert!(s.execute("panel.contour", &json!({"id": id, "segs": [{"Line": {"a": [0, 0], "b": [10, 0]}}]})).is_err());
    assert!(s.execute("panel.resize", &json!({"id": id, "size": [500, 300]})).is_err());
    // Resizing a rectangular panel: edge banding stays on the same edges.
    let id2 = s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": "horizontal", "size": [600, 400], "position": [3000, 0, 0]})).unwrap()["id"].clone();
    s.execute("panel.edge", &json!({"id": id2, "segments": [2], "edge": "PVC 0.4 white"})).unwrap();
    s.execute("panel.resize", &json!({"id": id2, "size": [800, 450]})).unwrap();
    let p = build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id2).unwrap();
    assert_eq!(p.panel.size, (800.0, 450.0));
    assert_eq!(p.panel.cut_size(), (800.0, 449.6));
    // Whole contour: a triangle offset from (0, 0) is normalized, the axes origin moves.
    s.execute("panel.contour", &json!({"id": id2, "segs": [
        {"Line": {"a": [10, 10], "b": [110, 10]}}, {"Line": {"a": [110, 10], "b": [10, 60]}}, {"Line": {"a": [10, 60], "b": [10, 10]}}]})).unwrap();
    let p = build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id2).unwrap();
    assert_eq!(p.panel.size, (100.0, 50.0));
    assert_eq!(p.origin, [3010.0, 0.0, 10.0]);
}

/// Object actions: move, exact 90° rotation, mirror with copy, array, block, delete.
#[test]
fn object_actions() {
    let mut s = wardrobe();
    let id = |v: Value| v["id"].as_u64().unwrap() as u32;
    let p1 = id(s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": "front", "size": [300, 200], "position": [2000, 0, 0]})).unwrap());
    s.execute("panel.corner", &json!({"id": p1, "at": [300, 200], "radius": 50})).unwrap();
    let part = |s: &Session, i: u32| build(&s.project).unwrap().into_iter().find(|p| p.id == i).unwrap();
    s.execute("object.move", &json!({"ids": [1, p1], "d": [100, 0, -50]})).unwrap();
    assert_eq!(s.project.cabinets[0].position, [100.0, 0.0, -50.0]);
    assert_eq!(part(&s, p1).origin, [2100.0, 0.0, -50.0]);
    // 90° rotation about Y: exact axes, rotated bounding box.
    s.execute("object.rotate", &json!({"ids": [p1], "axis": "y", "angle": 90, "center": [2100, 0, -50]})).unwrap();
    let f = part(&s, p1).panel.frame.unwrap();
    assert!(f.x.iter().chain(&f.y).chain(&f.z).all(|v| [0.0, 1.0, -1.0].contains(v)), "{f:?}");
    assert_eq!(part(&s, p1).size, [16.0, 200.0, 300.0]);
    assert!(s.execute("object.rotate", &json!({"ids": [1], "axis": "y", "angle": 90})).unwrap_err().contains("Explode"));
    // Mirror copy in the plane x = 3000: same bounding box, mirrored; the fillet is on the other side.
    let before = part(&s, p1);
    let m = s.execute("object.mirror", &json!({"ids": [p1], "axis": "x", "at": 3000, "copy": true})).unwrap()["ids"][0].as_u64().unwrap() as u32;
    let after = part(&s, m);
    assert_eq!(after.size, before.size);
    assert_eq!(after.origin[0], 6000.0 - before.origin[0] - before.size[0]);
    after.panel.frame.unwrap().check().unwrap();
    assert!(crate::mesh::build(std::slice::from_ref(&after), |_| false).errors.is_empty());
    // Cut size and edge banding of the mirror copy are the same.
    assert_eq!(after.panel.cut_size(), before.panel.cut_size());
    // Array: 3 copies of the cabinet 800 mm apart.
    let ids = s.execute("object.copy", &json!({"ids": [1], "d": [800, 0, 0], "count": 3})).unwrap()["ids"].clone();
    assert_eq!(ids.as_array().unwrap().len(), 3);
    assert_eq!(s.project.cabinets.last().unwrap().position[0], 100.0 + 2400.0);
    // Block of two panels; exploding puts them back in place.
    let b = id(s.execute("block.create", &json!({"ids": [p1, m], "name": "Sides"})).unwrap());
    s.execute("block.set", &json!({"id": b, "frame": {"origin": [0, 100, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}})).unwrap();
    let lifted = part(&s, p1).origin;
    s.execute("block.explode", &json!({"id": b})).unwrap();
    assert_eq!(part(&s, p1).origin, lifted);
    assert!(s.execute("block.create", &json!({"ids": [1]})).is_err());
    s.execute("object.remove", &json!({"ids": [p1, 1]})).unwrap();
    assert!(s.project.cabinets.iter().all(|c| c.id != 1));
}

/// Cabinet drilling with confirmat screws: holes with exact coordinates on both parts of the joint.
#[test]
fn cabinet_fasteners_drill_exactly() {
    let mut s = wardrobe();
    s.execute("cabinet.set", &json!({"id": 1, "fastener": "Confirmat 7x50"})).unwrap();
    let parts = build(&s.project).unwrap();
    let side = by_name(&parts, "Left side")[0];
    let bottom = by_name(&parts, "Bottom")[0];
    // Joint of the bottom with the left side panel: length 600; on the 32 mm grid 480 apart, centred: 60 and 540 mm.
    // Side panel: through ⌀8 from the outer face (z = 16 in side panel axes) at the height of the bottom's middle (88).
    assert!(side.panel.holes.contains(&crate::panel::Hole { pos: [60.0, 88.0, 16.0], dir: [0.0, 0.0, -1.0], diameter: 8.0, depth: 16.0 }), "{:?}", side.panel.holes);
    // Bottom: ⌀5 × 37 into the edge, at mid-thickness.
    assert!(bottom.panel.holes.contains(&crate::panel::Hole { pos: [0.0, 60.0, 8.0], dir: [1.0, 0.0, 0.0], diameter: 5.0, depth: 37.0 }), "{:?}", bottom.panel.holes);
    // Shelves sit on shelf supports, 2 on each side.
    let shelf = by_name(&parts, "Shelf")[0];
    assert!(shelf.panel.holes.is_empty(), "a shelf support is drilled only in the upright");
    // Under the shelf (the top of the ⌀5 hole touches its bottom face), plus spare holes 32 and 64 mm below and above.
    let mut supports: Vec<f64> = side.panel.holes.iter().filter(|h| h.diameter == 5.0 && h.depth == 9.0).map(|h| side.origin[1] + h.pos[1] - (shelf.origin[1] - 2.5)).collect();
    supports.sort_by(f64::total_cmp);
    supports.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert_eq!(supports, [-64.0, -32.0, 0.0, 32.0, 64.0], "shelf support heights from the shelf one");
    let hw = hardware(&s.project).unwrap();
    let count = |n: &str| hw.iter().find(|(k, _)| k.starts_with(n)).map_or(0, |r| r.1);
    assert_eq!(count("Shelf support"), 2 * 2 * 2);
    assert!(count("Confirmat") >= 12, "{hw:?}");
    // Drilling appears in the Raskroy XML.
    let panels: Vec<Panel> = parts.iter().map(|p| p.panel.clone()).collect();
    assert!(crate::raskroy::project_string("Wardrobe", &panels, |_| (2800.0, 2070.0)).contains("<DRILLING>"));
    // An unknown fastener is rejected.
    assert!(s.execute("cabinet.set", &json!({"id": 1, "fastener": "Nail"})).is_err());
}

/// The 32 mm system: spacing is a multiple of 32 and the points are centred on the joint.
#[test]
fn fasteners_on_the_32_mm_grid() {
    let lib = crate::fasteners::library();
    let confirmat = lib.iter().find(|d| d.name == "Confirmat 7x50").unwrap();
    for len in [300.0, 560.0, 600.0, 1234.0, 2000.0] {
        let ps = crate::fasteners::positions(confirmat, len);
        let gaps: Vec<f64> = ps.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|g| (g / 32.0).fract().abs() < 1e-9 && *g > 0.0), "{len}: {ps:?}");
        assert!((ps[0] - (len - ps[ps.len() - 1])).abs() < 1e-9, "{len}: symmetric {ps:?}");
        assert!(ps[0] >= confirmat.offset - 1e-9, "{len}: not closer to the end than the offset {ps:?}");
    }
    // Two shelves 64 mm apart: their spare holes meet but are drilled once.
    let mut s = wardrobe();
    s.execute("cabinet.set", &json!({"id": 1, "fastener": "Confirmat 7x50"})).unwrap();
    s.execute("shelf.add", &json!({"cabinet": 1, "height": 464, "section": 0})).unwrap();
    let parts = build(&s.project).unwrap();
    let side = by_name(&parts, "Left side")[0];
    let mut seen = std::collections::HashSet::new();
    for h in &side.panel.holes {
        assert!(seen.insert(format!("{:?}{:?}{}", h.pos, h.dir, h.diameter)), "drilled twice: {h:?}");
    }
    let mut huge = serde_json::to_value(lib.iter().find(|d| d.name == "Shelf support 5").unwrap()).unwrap();
    huge["rows"] = json!([1_000_000, 0]);
    assert!(s.execute("fastener.def", &huge).is_err(), "a million spare holes");
    // Without a grid (a custom catalogue from an older project): evenly between the offsets, as before.
    let plain = crate::fasteners::Def { grid: 0.0, ..confirmat.clone() };
    assert_eq!(crate::fasteners::positions(&plain, 600.0), vec![50.0, 550.0]);
}

/// Fasteners between separate panels: the joint is found automatically, no joint — rejected; panels moved apart — a "dangling" fastener.
#[test]
fn fasteners_between_panels() {
    let mut s = Session::new(Project::new("Base unit"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    let id = |v: Value| v["id"].as_u64().unwrap() as u32;
    let side = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "vertical", "size": [500, 700], "position": [0, 0, 0]})).unwrap());
    let top = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "horizontal", "size": [600, 500], "position": [16, 684, 0]})).unwrap());
    let far = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "horizontal", "size": [600, 500], "position": [16, 1500, 0]})).unwrap());
    let f = id(s.execute("fastener.add", &json!({"kind": "Minifix 15 cam", "a": side, "b": top})).unwrap());
    assert!(s.execute("fastener.add", &json!({"kind": "Confirmat 7x50", "a": side, "b": far})).unwrap_err().contains("do not join"));
    let parts = build(&s.project).unwrap();
    let get = |i: u32| parts.iter().find(|p| p.id == i).unwrap();
    // Lid: ⌀8×34 in the edge and a ⌀15 cup in the face; upright: ⌀5×13. 2 points each (joint 500: 50 and 450).
    assert_eq!(get(top).panel.holes.len(), 4);
    assert_eq!(get(side).panel.holes.len(), 2);
    assert!(get(top).panel.holes.iter().any(|h| h.diameter == 15.0 && h.depth == 13.0));
    assert_eq!(hardware(&s.project).unwrap(), vec![("Minifix 15 cam".to_string(), 2)]);
    // Moved apart — the fastener "dangles", no holes, analysis detects it.
    s.execute("object.move", &json!({"ids": [top], "d": [100, 0, 0]})).unwrap();
    assert_eq!(hanging_fasteners(&s.project).len(), 1);
    assert!(build(&s.project).unwrap().iter().all(|p| p.panel.holes.is_empty()));
    s.execute("fastener.remove", &json!({"id": f})).unwrap();
    // Fasteners on all joints: put the lid back — one joint.
    s.execute("object.move", &json!({"ids": [top], "d": [-100, 0, 0]})).unwrap();
    let added = s.execute("fastener.auto", &json!({"kind": "Dowel 8x30"})).unwrap()["ids"].as_array().unwrap().len();
    assert_eq!(added, 1);
}

/// Each fastener has its own id: otherwise `fastener.remove` removes all with the same id at once.
#[test]
fn fasteners_get_distinct_ids() {
    let mut s = Session::new(Project::new("Base unit"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    let id = |v: Value| v["id"].as_u64().unwrap();
    let side = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "vertical", "size": [500, 700], "position": [0, 0, 0]})).unwrap());
    let top = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "horizontal", "size": [600, 500], "position": [16, 684, 0]})).unwrap());
    let bottom = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "horizontal", "size": [600, 500], "position": [16, 0, 0]})).unwrap());
    let a = id(s.execute("fastener.add", &json!({"kind": "Dowel 8x30", "a": side, "b": top})).unwrap());
    let b = id(s.execute("fastener.add", &json!({"kind": "Dowel 8x30", "a": side, "b": bottom})).unwrap());
    assert_ne!(a, b);
    s.execute("fastener.remove", &json!({"id": a})).unwrap();
    assert_eq!(s.project.fasteners.len(), 1);
}

/// Composite fastener Minifix (cam lock) + dowel: the dowel is 32 mm from the cam lock towards the joint middle; in the estimate — both separately.
#[test]
fn minifix_with_dowel() {
    let mut s = Session::new(Project::new("Base unit"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    let id = |v: Value| v["id"].as_u64().unwrap() as u32;
    let side = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "vertical", "size": [500, 700], "position": [0, 0, 0]})).unwrap());
    let top = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "horizontal", "size": [600, 500], "position": [16, 684, 0]})).unwrap());
    s.execute("fastener.add", &json!({"kind": "Minifix 15 + dowel 8x30", "a": side, "b": top})).unwrap();
    let parts = build(&s.project).unwrap();
    let get = |i: u32| parts.iter().find(|p| p.id == i).unwrap();
    assert_eq!(get(top).panel.holes.len(), 6);
    assert_eq!(get(side).panel.holes.len(), 4);
    let pos = |d: f64, depth: f64| -> Vec<[f64; 3]> { get(top).panel.holes.iter().filter(|h| h.diameter == d && h.depth == depth).map(|h| h.pos).collect() };
    let (m, w) = (pos(8.0, 34.0), pos(8.0, 20.0));
    // The axis along the joint is the one along which the cam lock points diverge.
    let k = (0..3).find(|&k| (m[0][k] - m[1][k]).abs() > 1.0).unwrap();
    let sorted = |v: &[[f64; 3]]| { let mut a: Vec<f64> = v.iter().map(|p| p[k]).collect(); a.sort_by(f64::total_cmp); a };
    let (minifix, dowel) = (sorted(&m), sorted(&w));
    assert_eq!(minifix.len(), 2);
    assert_eq!(dowel.len(), 2);
    // Cam lock points 50 and 450 along the joint, dowels — 82 and 418 (inward).
    let gaps: Vec<f64> = minifix.iter().zip(&dowel).map(|(m, d)| d - m).collect();
    assert_eq!(gaps, vec![32.0, -32.0], "cams {minifix:?}, dowels {dowel:?}");
    assert_eq!(hardware(&s.project).unwrap(), vec![("Dowel 8x30".to_string(), 2), ("Minifix 15 cam".to_string(), 2)]);
}

/// The cam lock cup is inside the cabinet: on the bottom from above, on the top from below.
#[test]
fn minifix_cups_face_inside() {
    let mut s = Session::new(Project::new("Base unit"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    let id = |v: Value| v["id"].as_u64().unwrap() as u32;
    let side = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "vertical", "size": [500, 700], "position": [0, 0, 0]})).unwrap());
    let top = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "horizontal", "size": [600, 500], "position": [16, 684, 0]})).unwrap());
    let bottom = id(s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "horizontal", "size": [600, 500], "position": [16, 0, 0]})).unwrap());
    for b in [top, bottom] {
        s.execute("fastener.add", &json!({"kind": "Minifix 15 cam", "a": side, "b": b})).unwrap();
    }
    let parts = build(&s.project).unwrap();
    // World height of the ⌀15 cup entry.
    let cups = |i: u32| -> Vec<f64> {
        let p = parts.iter().find(|p| p.id == i).unwrap();
        let f = p.panel.frame.unwrap();
        p.panel.holes.iter().filter(|h| h.diameter == 15.0).map(|h| f.apply(h.pos)[1]).collect()
    };
    assert_eq!(cups(top), vec![684.0, 684.0], "top 684…700: cup underneath");
    assert_eq!(cups(bottom), vec![16.0, 16.0], "bottom 0…16: cup on top");
    // Setting: for the top, the cup goes outside, upwards.
    let f = s.project.fasteners.iter().find(|f| f.b == top).unwrap().id;
    s.execute("fastener.set", &json!({"id": f, "outside": true})).unwrap();
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == top).unwrap();
    let up: Vec<f64> = p.panel.holes.iter().filter(|h| h.diameter == 15.0).map(|h| p.panel.frame.unwrap().apply(h.pos)[1]).collect();
    assert_eq!(up, vec![700.0, 700.0], "cup on the upper face of the top");
}

/// A material and edge banding can be deleted only if they are not used anywhere.
#[test]
fn unused_materials_and_edges_are_removed() {
    let mut s = Session::new(Project::new("Base unit"));
    for m in ["Chipboard 16", "Chipboard 18"] {
        s.execute("material.add", &json!({"name": m, "thickness": 16})).unwrap();
    }
    s.execute("edge.add", &json!({"name": "PVC 2", "thickness": 2})).unwrap();
    s.execute("edge.add", &json!({"name": "PVC 1", "thickness": 1})).unwrap();
    let p = s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "front", "size": [400, 600]})).unwrap()["id"].clone();
    s.execute("panel.edge", &json!({"id": p, "segments": [0], "edge": "PVC 2"})).unwrap();
    assert!(s.execute("material.remove", &json!({"name": "Chipboard 16"})).unwrap_err().contains("is used by parts"));
    assert!(s.execute("edge.remove", &json!({"name": "PVC 2"})).unwrap_err().contains("is on parts"));
    s.execute("material.remove", &json!({"name": "Chipboard 18"})).unwrap();
    s.execute("edge.remove", &json!({"name": "PVC 1"})).unwrap();
    assert_eq!((s.project.materials.len(), s.project.edges.len()), (1, 1));
}

/// Hardware from the library: holes are drilled into the touching panel; in the estimate — by name; detached — analysis detects it.
#[test]
fn furniture_drills_panels() {
    let mut s = Session::new(Project::new("Base unit"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    let side = s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "vertical", "size": [500, 700], "position": [0, 0, 0]})).unwrap()["id"].as_u64().unwrap() as u32;
    // Drawer runner: two ⌀4×12 screws to the left (into the upright), the model touches the upright's right face (x = 16).
    let hole = |z: f64| crate::panel::WorldHole { pos: [0.0, 10.0, z], dir: [-1.0, 0.0, 0.0], diameter: 4.0, depth: 12.0 };
    s.project.objects.push(Object::Furniture(Furniture {
        id: 50, name: "Runner".into(), kind: "Drawer".into(), source: String::new(),
        frame: Frame { origin: [16.0, 100.0, 0.0], ..Frame::IDENTITY },
        holes: vec![hole(37.0), hole(261.0)], lo: [0.0; 3], hi: [12.0, 45.0, 300.0],
    }));
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == side).unwrap();
    assert_eq!(p.panel.holes.len(), 2);
    // In upright axes: X — depth (project z), Y — height, Z — towards the left face; entry on the right face (z = 0).
    assert!(p.panel.holes.iter().any(|h| (h.pos[0] - 37.0).abs() < 1e-9 && (h.pos[1] - 110.0).abs() < 1e-9 && h.pos[2].abs() < 1e-9 && h.depth == 12.0), "{:?}", p.panel.holes);
    assert_eq!(hardware(&s.project).unwrap(), vec![("Runner".to_string(), 1)]);
    assert!(furniture_lost_holes(&s.project).is_empty());
    s.execute("object.move", &json!({"ids": [50], "d": [200, 0, 0]})).unwrap();
    assert_eq!(furniture_lost_holes(&s.project), vec![("Runner".to_string(), 2)]);
    assert!(s.execute("object.mirror", &json!({"ids": [50], "axis": "x", "at": 0})).is_err());
    // A real model from the examples is placed by a command.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/telegram");
    let tandem = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.to_string_lossy().contains("TANDEM") && p.extension().is_some_and(|x| x == "fr3d"));
    if let Some(tandem) = tandem {
        let id = s.execute("furniture.add", &json!({"path": tandem, "position": [1000, 0, 0]})).unwrap()["id"].as_u64().unwrap() as u32;
        let f = furniture(&s.project).into_iter().find(|(f, _)| f.id == id).unwrap().0;
        assert!(f.name.contains("TANDEM") && !f.holes.is_empty());
    }
}

/// Doors and drawers in an opening: sizes by mount type, hinges with drilling in the door and upright, drawers with runners.
#[test]
fn doors_and_drawers_in_opening() {
    let mut s = Session::new(Project::new("Base unit"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    s.execute("material.add", &json!({"name": "HDF 3", "thickness": 3})).unwrap();
    // Box without doors: uprights 0…16 and 584…600, bottom 0…16, top 704…720, depth 500.
    s.execute("cabinet.add", &json!({"width": 600, "height": 720, "depth": 500, "material": "Chipboard 16", "doors": 0})).unwrap();
    let block = s.execute("door.add", &json!({"at": [300, 360], "material": "Chipboard 16", "mount": "overlay"})).unwrap()["id"].as_u64().unwrap() as u32;
    let parts = build(&s.project).unwrap();
    let door = parts.iter().find(|p| p.owner == block && p.kind == Kind::Door).unwrap();
    // Overlay: at the outer faces of the uprights minus a 2 gap.
    assert_eq!((door.origin, door.size), ([2.0, 2.0, 500.0], [596.0, 716.0, 16.0]));
    // Two hinges: ⌀35 cup in the door 22 mm from the left edge, 100 mm from the bottom and top.
    let cups: Vec<_> = door.panel.holes.iter().filter(|h| h.diameter == 35.0).collect();
    assert_eq!(cups.len(), 2, "{:?}", door.panel.holes);
    let side = parts.iter().find(|p| p.panel.name == "Left side").unwrap();
    // Mounting plates: 4 ⌀2 holes in the left upright, 37 mm from the front edge (z = 463).
    let plate: Vec<_> = side.panel.holes.iter().filter(|h| h.diameter == 2.0).collect();
    assert_eq!(plate.len(), 4, "{:?}", side.panel.holes);
    assert!(plate.iter().all(|h| (h.pos[0] - 463.0).abs() < 1e-9), "{plate:?}");
    assert!(hardware(&s.project).unwrap().contains(&("Overlay hinge with soft close".to_string(), 2)));
    assert!(furniture_lost_holes(&s.project).is_empty());
    // Opening closed: a second door and drawers don't fit into it.
    let again = s.execute("door.add", &json!({"at": [300, 360], "material": "Chipboard 16"})).unwrap_err();
    assert!(again.contains("already has"), "{again}");
    assert!(s.execute("drawers.add", &json!({"at": [300, 360], "material": "Chipboard 16", "heights": [300]})).unwrap_err().contains("already has"));
    s.execute("block.remove", &json!({"id": block})).unwrap();
    let block = s.execute("door.add", &json!({"at": [300, 360], "material": "Chipboard 16", "mount": "inset", "double": true})).unwrap()["id"].as_u64().unwrap() as u32;
    let parts = build(&s.project).unwrap();
    let leaves: Vec<_> = parts.iter().filter(|p| p.owner == block && p.kind == Kind::Door).collect();
    assert_eq!(leaves.len(), 2);
    assert_eq!((leaves[0].origin, leaves[0].size), ([18.0, 18.0, 484.0], [281.0, 684.0, 16.0]));
    s.execute("block.remove", &json!({"id": block})).unwrap();
    // Drawers: two fronts, boxes as wide as the opening minus 2·13.
    let block = s.execute("drawers.add", &json!({"at": [300, 360], "material": "Chipboard 16", "heights": [300, 400], "bottom": "HDF 3"})).unwrap()["id"].as_u64().unwrap() as u32;
    let parts = build(&s.project).unwrap();
    let mine: Vec<_> = parts.iter().filter(|p| p.owner == block).collect();
    assert_eq!(mine.iter().filter(|p| p.kind == Kind::Drawer).count(), 2);
    let bottom = mine.iter().find(|p| p.panel.name == "Drawer bottom").unwrap();
    assert_eq!(bottom.panel.size, (568.0 - 26.0, 450.0));
    assert!(furniture_lost_holes(&s.project).is_empty(), "{:?}", furniture_lost_holes(&s.project));
    assert!(s.execute("drawers.add", &json!({"at": [300, 360], "material": "Chipboard 16", "heights": [500, 500]})).is_err());
}

/// Model analysis: own templates are clean; intersection, part larger than the sheet and dangling fastener are found.
#[test]
fn analysis_finds_real_problems_only() {
    let mut s = wardrobe();
    s.execute("cabinet.add", &json!({"width": 600, "height": 820, "depth": 560, "material": "Chipboard White 16", "plinth": 100, "back": "HDF 3",
        "drawers": [140, 280, 280], "fastener": "Confirmat 7x50", "position": [1000, 0, 0]})).unwrap();
    s.execute("corner.add", &json!({"width_x": 1000, "width_z": 900, "height": 820, "depth": 560, "material": "Chipboard White 16",
        "plinth": 100, "back": "HDF 3", "shelves": [300], "doors": true, "position": [3000, 0, 0]})).unwrap();
    s.execute("countertop.add", &json!({"material": "Chipboard White 16", "length": 600, "depth": 600, "position": [1000, 820, 0]})).unwrap();
    let clean = analysis::analyze(&s.project).unwrap();
    assert!(clean.is_empty(), "{clean:#?}");
    // Panel inside the wardrobe's side panel — intersection.
    s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": "horizontal", "size": [300, 300], "position": [-100, 1000, 100]})).unwrap();
    // Panel larger than the 2800×2070 sheet (trim 10).
    s.execute("panel.add", &json!({"material": "Chipboard White 16", "plane": "front", "size": [3000, 500], "position": [6000, 0, 0]})).unwrap();
    let found = analysis::analyze(&s.project).unwrap();
    let kinds: Vec<&str> = found.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, vec!["Overlap", "Oversize"], "{found:#?}");
    assert_eq!(found[0].parts.len(), 2);
}

/// Dimension markers: a panel occupies the given bounding box in any plane, with a single command.
#[test]
fn panel_box_resizes_exactly() {
    let mut s = Session::new(Project::new("T"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    for (plane, size, pos, lo, hi) in [
        ("front", [600, 400], [0, 0, 0], [10.0, 20.0, 0.0], [710.0, 520.0, 16.0]),
        ("horizontal", [600, 400], [0, 0, 0], [-50.0, 0.0, 0.0], [550.0, 16.0, 450.0]),
        ("vertical", [400, 700], [0, 0, 0], [0.0, 100.0, 30.0], [16.0, 900.0, 530.0]),
    ] {
        let id = s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": plane, "size": size, "position": pos})).unwrap()["id"].clone();
        s.execute("panel.box", &json!({"id": id, "lo": lo, "hi": hi})).unwrap();
        let p = build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id).unwrap();
        assert_eq!((p.origin, std::array::from_fn::<f64, 3, _>(|k| p.origin[k] + p.size[k])), (lo, hi), "{plane}");
        assert!(s.execute("panel.box", &json!({"id": id, "lo": [0, 0, 0], "hi": [100, 100, 100]})).is_err(), "thickness does not match");
    }
}

/// A project saved with the Russian built-in catalogue (it once ran fastener.def) still opens, with shelf supports.
#[test]
fn legacy_fastener_catalogue_opens() {
    let mut s = Session::new(Project::new("Legacy"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    s.execute("cabinet.add", &json!({"width": 600, "height": 720, "depth": 560, "material": "Chipboard 16", "fastener": "Confirmat 7x50"})).unwrap();
    s.execute("shelf.add", &json!({"cabinet": 1, "height": 300})).unwrap();
    let mut p = s.project.clone();
    p.fastener_defs = crate::fasteners::library();
    // Write it the way an old version did: every built-in name in Russian.
    let old = |n: &str| crate::i18n::bazis(&format!("fastener.{n}")).to_string();
    for d in &mut p.fastener_defs {
        for (n, _) in &mut d.items {
            *n = old(n);
        }
        d.name = old(&d.name);
    }
    p.cabinets[0].fastener = Some(old("Confirmat 7x50"));
    let back = Project::load(&p.save()).expect("an old project opens");
    assert_eq!(back.cabinets[0].fastener.as_deref(), Some("Confirmat 7x50"));
    assert!(back.def("Shelf support 5").is_ok() && back.def("Minifix 15 + dowel 8x30").is_ok());
}

/// A back panel in grooves: four grooves 4 mm wide (HDF 3 + 1), 16 mm from the back, 8 deep on the inner faces;
/// the back panel goes 7 mm into each, shelves stop in front of it, and the model check finds no overlaps.
#[test]
fn back_panel_in_grooves() {
    let mut s = Session::new(Project::new("Grooves"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    s.execute("material.add", &json!({"name": "HDF 3", "thickness": 3})).unwrap();
    s.execute("cabinet.add", &json!({"width": 800, "height": 2000, "depth": 600, "material": "Chipboard 16", "plinth": 80,
        "back": "HDF 3", "back_groove": {}, "doors": 2, "drawers": [200]})).unwrap();
    s.execute("shelf.add", &json!({"cabinet": 1, "height": 900})).unwrap();
    let parts = build(&s.project).unwrap();
    let one = |name: &str| *by_name(&parts, name).first().unwrap_or_else(|| panic!("{name}"));
    for name in ["Left side", "Right side", "Bottom", "Top panel"] {
        let g = &one(name).panel.grooves;
        assert_eq!(g.len(), 1, "{name}");
        assert_eq!(g[0].across, [0.0, 4.0], "{name}");
    }
    assert_eq!(one("Left side").panel.grooves[0].z, [0.0, 8.0]);
    assert_eq!(one("Right side").panel.grooves[0].z, [8.0, 16.0]);
    let back = one("Back panel");
    // Inner 768 × 1888 plus 7 mm into each groove; 16.5 mm from the back edge.
    assert_eq!(back.size, [782.0, 1902.0, 3.0]);
    assert_eq!(back.origin, [9.0, 89.0, 16.5]);
    let shelf = one("Shelf");
    assert_eq!((shelf.origin[2], shelf.size[2]), (20.0, 580.0));
    let problems = analysis::analyze(&s.project).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
    // Without a back panel material there is nothing to put into grooves: no grooves.
    s.execute("cabinet.set", &json!({"id": 1, "back": null})).unwrap();
    assert!(build(&s.project).unwrap().iter().all(|p| p.panel.grooves.is_empty()));
    // A wider groove (glass, a 4 mm cutter for HDF 3.2): the back panel stays in its middle.
    s.execute("cabinet.set", &json!({"id": 1, "back": "HDF 3", "back_groove": {"width": 5}})).unwrap();
    let parts = build(&s.project).unwrap();
    assert_eq!(by_name(&parts, "Left side")[0].panel.grooves[0].across, [0.0, 5.0]);
    assert_eq!(by_name(&parts, "Back panel")[0].origin[2], 17.0);
    assert_eq!(by_name(&parts, "Shelf")[0].origin[2], 21.0);
    assert!(s.execute("cabinet.set", &json!({"id": 1, "back_groove": {"width": 2}})).is_err(), "narrower than the back panel");
    // A groove deeper than the board is refused.
    assert!(s.execute("cabinet.set", &json!({"id": 1, "back": "HDF 3", "back_groove": {"depth": 20}})).is_err());
}

/// A groove along an edge of a free panel lies inside the panel, on the chosen face; it can be removed.
#[test]
fn groove_on_a_panel() {
    let mut s = Session::new(Project::new("Panel groove"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
    let id = s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "front", "size": [600, 400]})).unwrap()["id"].as_u64().unwrap();
    // Edge 0 is the bottom one: the groove runs 10..14 mm above it, 8 mm into the front face.
    let r = s.execute("panel.groove", &json!({"id": id, "segment": 0, "width": 4, "depth": 8, "offset": 10, "face": "front"})).unwrap();
    assert_eq!(r["index"], 0);
    let parts = build(&s.project).unwrap();
    let g = &parts[0].panel.grooves[0];
    assert_eq!(g.z, [8.0, 16.0]);
    assert!(g.contains([300.0, 12.0, 12.0], 1e-9) && !g.contains([300.0, -12.0, 12.0], 1e-9), "{g:?}");
    assert!(s.execute("panel.groove", &json!({"id": id, "segment": 0, "width": 4, "depth": 16})).is_err(), "as deep as the board");
    assert!(s.execute("panel.groove", &json!({"id": id, "segment": 0, "width": 4, "depth": 8, "offset": 400})).is_err(), "beyond the panel");
    // On the other edges the groove goes inside the panel too (the contour runs the other way round there).
    for seg in 1..4 {
        let i = s.execute("panel.groove", &json!({"id": id, "segment": seg, "width": 4, "depth": 8, "offset": 10})).unwrap()["index"].as_u64().unwrap() as usize;
        let g = build(&s.project).unwrap()[0].panel.grooves[i].clone();
        let probe = match seg { 1 => [12.0, 200.0, 4.0], 2 => [300.0, 388.0, 4.0], _ => [588.0, 200.0, 4.0] };
        assert!(g.contains(probe, 1e-9), "edge {seg}: {g:?}");
    }
    s.execute("panel.groove_remove", &json!({"id": id, "index": 0})).unwrap();
    assert_eq!(build(&s.project).unwrap()[0].panel.grooves.len(), 3);
}

/// Prices and weights reach the estimate: boards by m² (with the factor), edge banding by metres, fasteners by pieces.
#[test]
fn estimate_has_prices_and_weight() {
    let mut s = Session::new(Project::new("Priced"));
    s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16, "coef": 1.0, "price": 500, "weight": 11})).unwrap();
    s.execute("edge.add", &json!({"name": "PVC 2", "thickness": 2})).unwrap();
    s.execute("edge.set", &json!({"name": "PVC 2", "price": 30})).unwrap();
    s.execute("cabinet.add", &json!({"width": 600, "height": 720, "depth": 500, "material": "Chipboard 16", "edge": "PVC 2", "fastener": "Confirmat 7x50"})).unwrap();
    let mut confirmat = serde_json::to_value(crate::fasteners::library().into_iter().find(|d| d.name == "Confirmat 7x50").unwrap()).unwrap();
    confirmat["price"] = json!(3);
    s.execute("fastener.def", &confirmat).unwrap();
    let job = crate::job::Job::from_project(s.project.clone());
    let rows = job.estimate(1.0);
    let row = |unit: &str| rows.iter().find(|r| r.unit == unit).unwrap();
    let board = row("m²");
    assert_eq!(board.price, 500.0);
    assert!((board.weight - board.net * 11.0).abs() < 1e-9 && board.weight > 0.0);
    assert_eq!(row("m").price, 30.0);
    assert_eq!(row("pcs").price, 3.0);
    let (cost, weight) = crate::report::totals(&rows);
    let expect: f64 = rows.iter().map(|r| r.qty * r.price).sum();
    assert!((cost - expect).abs() < 1e-9 && cost > 0.0);
    assert!((weight - board.weight).abs() < 0.005, "{weight} vs {}", board.weight);
    // Composite fasteners are priced by their parts.
    s.execute("cabinet.set", &json!({"id": 1, "fastener": "Minifix 15 + dowel 8x30"})).unwrap();
    let mut dowel = serde_json::to_value(s.project.def("Dowel 8x30").unwrap()).unwrap();
    dowel["price"] = json!(1.5);
    s.execute("fastener.def", &dowel).unwrap();
    let parts_rows = crate::job::Job::from_project(s.project.clone()).estimate(1.0);
    assert!(parts_rows.iter().any(|r| r.material == "Dowel 8x30" && r.price == 1.5), "{parts_rows:?}");
    // Negative prices are refused and leave the material as it was.
    assert!(s.execute("material.set", &json!({"name": "Chipboard 16", "price": -1})).is_err());
    assert_eq!(s.project.materials[0].price, 500.0);
    let csv = crate::report::materials_csv(&rows);
    assert!(csv.lines().last().unwrap().contains(&crate::report::money(cost)), "{csv}");
}

/// Mass replacement: a material everywhere or only in chosen objects; an edge banding on templates and free panels.
#[test]
fn materials_and_edges_are_replaced() {
    let mut s = Session::new(Project::new("Replace"));
    for (name, t) in [("White 16", 16), ("Oak 16", 16)] {
        s.execute("material.add", &json!({"name": name, "thickness": t})).unwrap();
    }
    s.execute("edge.add", &json!({"name": "PVC white", "thickness": 0.4})).unwrap();
    s.execute("edge.add", &json!({"name": "PVC oak", "thickness": 2, "width": 22})).unwrap();
    s.execute("cabinet.add", &json!({"width": 600, "height": 720, "depth": 500, "material": "White 16", "door_material": "White 16", "edge": "PVC white", "doors": 1})).unwrap();
    s.execute("cabinet.add", &json!({"width": 600, "height": 720, "depth": 500, "material": "White 16", "edge": "PVC white", "position": [700, 0, 0]})).unwrap();
    let panel = s.execute("panel.add", &json!({"material": "White 16", "plane": "front", "size": [400, 300], "position": [0, 1000, 0]})).unwrap()["id"].as_u64().unwrap();
    s.execute("panel.edge", &json!({"id": panel, "segments": [0, 2], "edge": "PVC white"})).unwrap();
    // Only the first cabinet: its carcass and door material.
    let r = s.execute("material.replace", &json!({"from": "White 16", "to": "Oak 16", "ids": [1]})).unwrap();
    assert_eq!(r["changed"], 2);
    assert_eq!((s.project.cabinets[0].material.as_str(), s.project.cabinets[1].material.as_str()), ("Oak 16", "White 16"));
    // Everywhere: the second cabinet and the free panel.
    assert_eq!(s.execute("material.replace", &json!({"from": "White 16", "to": "Oak 16"})).unwrap()["changed"], 2);
    assert!(build(&s.project).unwrap().iter().all(|p| crate::panel::material_name(&p.panel.material) == "Oak 16"));
    // Edge banding: two cabinets and two edges of the panel; the panel edges take the new thickness.
    // Two cabinets and one panel (with two banded edges).
    assert_eq!(s.execute("edge.replace", &json!({"from": "PVC white", "to": "PVC oak"})).unwrap()["changed"], 3);
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == panel as u32).unwrap();
    assert!(p.panel.edges.iter().all(|e| crate::panel::material_name(&e.material) == "PVC oak" && e.thickness == 2.0));
    assert!(s.execute("material.replace", &json!({"from": "White 16", "to": "Nothing"})).is_err());
    assert!(s.execute("material.replace", &json!({"from": "Oak 16", "to": "White 16", "ids": [999]})).is_err(), "unknown object");
    assert_eq!(s.execute("material.replace", &json!({"from": "Oak 16", "to": "Oak 16"})).unwrap()["changed"], 0);
}
