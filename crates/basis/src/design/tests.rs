//! Проверки модели: генераторы, дерево объектов, команды.
use super::*;

fn wardrobe() -> Session {
    let mut s = Session::new(Project::new("Шкаф"));
    s.execute("material.add", &json!({"name": "ЛДСП Белый 16", "article": "W980", "thickness": 16})).unwrap();
    s.execute("material.add", &json!({"name": "ХДФ 3", "thickness": 3, "coef": 1.1})).unwrap();
    s.execute("edge.add", &json!({"name": "ПВХ 0,4 белая", "thickness": 0.4, "width": 19})).unwrap();
    let id = s
        .execute("cabinet.add", &json!({"width": 800, "height": 2000, "depth": 600, "material": "ЛДСП Белый 16",
            "edge": "ПВХ 0,4 белая", "plinth": 80, "back": "ХДФ 3", "doors": 2}))
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
    // 2 боковины, дно, крыша, цоколь, перегородка, 2 полки, задняя стенка, 2 двери.
    assert_eq!(parts.len(), 11);
    let side = by_name(&parts, "Боковина левая")[0];
    assert_eq!(side.panel.size, (600.0, 2000.0));
    // Лицевая кромка 0,4 подрезает заготовку по глубине; длина вдоль текстуры — высота.
    assert_eq!(side.panel.cut_size(), (2000.0, 599.6));
    assert_eq!(by_name(&parts, "Дно")[0].panel.size, (768.0, 600.0));
    assert_eq!(by_name(&parts, "Перегородка")[0].panel.size, (600.0, 2000.0 - 80.0 - 32.0));
    let shelves = by_name(&parts, "Полка");
    // Секции: 16..392 (376 мм) и 408..784 (376 мм).
    assert_eq!(shelves[0].panel.size.0, 376.0);
    assert_eq!(shelves[1].origin[0], 408.0);
    let doors = by_name(&parts, "Дверь");
    assert_eq!(doors.len(), 2);
    assert_eq!(doors[0].panel.size, (397.0, 1916.0));
    assert_eq!(doors[0].panel.edges.len(), 4);
    assert_eq!(by_name(&parts, "Задняя стенка")[0].panel.thickness, 3.0);
    assert_eq!(parts[0].panel.pos, "1");
}

#[test]
fn invalid_changes_are_rejected_and_project_stays_valid() {
    let mut s = wardrobe();
    let before = s.project.clone();
    assert!(s.execute("shelf.add", &json!({"cabinet": 1, "height": 5000})).is_err());
    assert!(s.execute("partition.add", &json!({"cabinet": 1, "x": 790})).is_err());
    assert!(s.execute("cabinet.set", &json!({"id": 1, "material": "нет такого"})).is_err());
    assert!(s.execute("cabinet.set", &json!({"id": 1, "width": 20})).is_err());
    assert!(s.execute("shelf.add", &json!({"cabinet": 1, "height": 100, "section": 7})).is_err());
    assert!(s.execute("bogus", &json!({})).is_err());
    assert_eq!(s.project, before);
}

#[test]
fn set_undo_redo() {
    let mut s = wardrobe();
    s.execute("cabinet.set", &json!({"id": 1, "width": 1000, "doors": 3})).unwrap();
    assert_eq!(by_name(&build(&s.project).unwrap(), "Дверь").len(), 3);
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.project.cabinets[0].width, 800.0);
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(s.project.cabinets[0].width, 1000.0);
    assert!(s.execute("redo", &json!({})).is_err());
}

#[test]
fn drawers_countertop_and_wall_module() {
    let mut s = wardrobe();
    s.execute("cabinet.add", &json!({"name": "Стол с ящиками", "width": 600, "height": 820, "depth": 560,
        "material": "ЛДСП Белый 16", "edge": "ПВХ 0,4 белая", "plinth": 100, "back": "ХДФ 3", "drawers": [140, 280, 280]})).unwrap();
    let parts = build(&s.project).unwrap();
    let mine: Vec<&Part> = parts.iter().filter(|p| p.owner == 2).collect();
    let count = |n: &str| mine.iter().filter(|p| p.panel.name == n).count();
    assert_eq!((count("Фасад ящика"), count("Боковина ящика"), count("Дно ящика")), (3, 6, 3));
    // Короб: 600 − 2·16 − 2·13 = 542 по ширине, направляющие 550 (глубина 560 → кратно 50).
    let bottom = mine.iter().find(|p| p.panel.name == "Дно ящика").unwrap();
    assert_eq!(bottom.panel.size, (542.0, 550.0));
    assert_eq!(bottom.panel.thickness, 3.0);
    let side = mine.iter().find(|p| p.panel.name == "Боковина ящика").unwrap();
    assert_eq!(side.panel.size, (550.0, 100.0));
    // Фасады не выходят за корпус: 100 + 2 + 140+2 + 280+2 + 280+2 = 808 ≤ 820.
    let top = mine.iter().filter(|p| p.kind == Kind::Drawer).map(|p| p.origin[1] + p.size[1]).fold(0.0, f64::max);
    assert!(top <= 820.0, "{top}");
    // Слишком высокие ящики и полка в зоне ящиков отклоняются.
    assert!(s.execute("cabinet.set", &json!({"id": 2, "drawers": [400, 400]})).is_err());
    assert!(s.execute("shelf.add", &json!({"cabinet": 2, "height": 100})).is_err());
    // Ящики в правой секции корпуса с перегородкой: короб по ширине секции.
    s.execute("cabinet.add", &json!({"name": "Тумба", "width": 1000, "height": 820, "depth": 560, "material": "ЛДСП Белый 16",
        "partitions": [392], "drawers": [200, 200], "drawer_section": 1, "doors": 0})).unwrap();
    let parts = build(&s.project).unwrap();
    let tumba: Vec<&Part> = parts.iter().filter(|p| p.owner == 3).collect();
    let bottom = tumba.iter().find(|p| p.panel.name == "Дно ящика").unwrap();
    // Секция 408…984 (576 мм) − 2·13 = 550; короб начинается у перегородки + 13.
    assert_eq!((bottom.panel.size.0, bottom.origin[0]), (550.0, 421.0));
    let facade = tumba.iter().find(|p| p.kind == Kind::Drawer).unwrap();
    // От середины перегородки (400) до правого края (1000), минус зазоры по 2.
    assert_eq!((facade.origin[0], facade.size[0]), (402.0, 596.0));
    assert!(s.execute("cabinet.set", &json!({"id": 3, "drawer_section": 5})).is_err());
    s.execute("cabinet.remove", &json!({"id": 3})).unwrap();

    s.execute("countertop.add", &json!({"material": "ЛДСП Белый 16", "length": 1400, "depth": 600,
        "position": [0, 2000, 0], "edge": "ПВХ 0,4 белая"})).unwrap();
    s.execute("cabinet.add", &json!({"name": "Навесной", "width": 600, "height": 720, "depth": 300,
        "material": "ЛДСП Белый 16", "doors": 1, "position": [0, 1400, 300]})).unwrap();
    let parts = build(&s.project).unwrap();
    let ct = parts.iter().find(|p| p.panel.name == "Столешница").unwrap();
    assert_eq!((ct.panel.size, ct.origin[1]), ((1400.0, 600.0), 2000.0));
    // Номера общие для всех объектов: корпуса 1 и 2, столешница 3, навесной 4.
    let wall = parts.iter().filter(|p| p.owner == 4).map(|p| p.origin[1]).fold(f64::MAX, f64::min);
    assert_eq!(wall, 1400.0);
    assert!(s.execute("countertop.set", &json!({"id": 3, "length": -5})).is_err());
}

#[test]
fn iso_view_shows_three_faces_per_box() {
    let parts = build(&wardrobe().project).unwrap();
    let faces = iso_view(&parts, true);
    // Спереди-справа-сверху видны фронт, правая грань и верх каждой детали.
    assert_eq!(faces.len(), parts.len() * 3);
    assert_eq!(iso_view(&parts, false).len(), (parts.len() - 2) * 3);
}

#[test]
fn corner_module() {
    let mut s = wardrobe();
    let id = s.execute("corner.add", &json!({"width_x": 1000, "width_z": 900, "height": 820, "depth": 560, "material": "ЛДСП Белый 16",
        "edge": "ПВХ 0,4 белая", "plinth": 100, "back": "ХДФ 3", "shelves": [300], "doors": true, "position": [3000, 0, 0]})).unwrap()["id"].clone();
    let parts: Vec<Part> = build(&s.project).unwrap().into_iter().filter(|p| p.origin[0] >= 2990.0 || p.panel.name.contains("углов")).collect();
    let bottom = parts.iter().find(|p| p.panel.name == "Дно угловое").unwrap();
    // Г-контур из 6 отрезков: это фигурная деталь (BOARDFIG), лицевая кромка на двух внутренних сторонах.
    assert_eq!(bottom.panel.segs.len(), 6);
    assert!(!bottom.panel.is_rect());
    assert_eq!(bottom.panel.size, (984.0, 884.0));
    assert_eq!(bottom.panel.edges.iter().map(|e| e.segment).collect::<Vec<_>>(), vec![3, 4]);
    assert_eq!(parts.iter().filter(|p| p.kind == Kind::Door).count(), 2);
    assert_eq!(parts.iter().filter(|p| p.kind == Kind::Back).count(), 2);
    // Двери не пересекаются во внутреннем углу.
    let doors: Vec<&Part> = parts.iter().filter(|p| p.kind == Kind::Door).collect();
    let overlap = (0..3).all(|k| doors[0].origin[k] < doors[1].origin[k] + doors[1].size[k] && doors[1].origin[k] < doors[0].origin[k] + doors[0].size[k]);
    assert!(!overlap, "двери пересекаются");
    assert!(s.execute("corner.set", &json!({"id": id, "width_x": 600})).is_err(), "крыло меньше глубины");
    let xml = crate::raskroy::project_string("т", &parts.iter().map(|p| p.panel.clone()).collect::<Vec<_>>(), |_| (2800.0, 2070.0));
    assert_eq!(xml.matches("<BOARDFIG>").count(), 3, "дно, крыша, полка");
    s.execute("corner.remove", &json!({"id": id})).unwrap();
    assert!(s.project.corners.is_empty());
}

/// Каждая деталь генераторов имеет точное положение: оси ровно 0/±1, габарит из положения совпадает
/// с заданным, толщина смотрит поперёк плоскости детали.
#[test]
fn generated_parts_have_exact_frames() {
    let mut s = wardrobe();
    s.execute("cabinet.add", &json!({"width": 600, "height": 820, "depth": 560, "material": "ЛДСП Белый 16",
        "edge": "ПВХ 0,4 белая", "plinth": 100, "back": "ХДФ 3", "drawers": [140, 280, 280], "position": [1000, 0, 0]})).unwrap();
    s.execute("corner.add", &json!({"width_x": 1000, "width_z": 900, "height": 820, "depth": 560, "material": "ЛДСП Белый 16",
        "plinth": 100, "back": "ХДФ 3", "shelves": [300], "doors": true, "position": [3000, 0, 0]})).unwrap();
    s.execute("countertop.add", &json!({"material": "ЛДСП Белый 16", "length": 1400, "depth": 600, "position": [0, 2000, 0]})).unwrap();
    let parts = build(&s.project).unwrap();
    assert!(parts.len() > 40);
    for p in &parts {
        let f = p.panel.frame.unwrap();
        f.check().unwrap();
        assert!(f.x.iter().chain(&f.y).chain(&f.z).all(|v| [0.0, 1.0, -1.0].contains(v)), "{}: {f:?}", p.panel.name);
        let thick_axis = (0..3).find(|&k| f.z[k] != 0.0).unwrap();
        assert_eq!(p.size[thick_axis], p.panel.thickness, "{}", p.panel.name);
    }
    let shelf = by_name(&parts, "Полка")[0];
    // Горизонтальная: контур X → X, Y → +Z (лицевая кромка к зрителю), толщина вниз от верхней пласти.
    let f = shelf.panel.frame.unwrap();
    assert_eq!((f.x, f.y, f.z), ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]));
    assert_eq!(f.origin, [16.0, 80.0 + 16.0 + 400.0 + 16.0, 0.0]);
}

#[test]
fn free_panels_in_three_planes_and_blocks() {
    let mut s = wardrobe();
    let add = |s: &mut Session, plane: &str, size: [f64; 2], pos: [f64; 3]| {
        s.execute("panel.add", &json!({"material": "ЛДСП Белый 16", "plane": plane, "size": size, "position": pos})).unwrap()["id"].as_u64().unwrap() as u32
    };
    let v = add(&mut s, "vertical", [500.0, 700.0], [2000.0, 0.0, 0.0]);
    let h = add(&mut s, "horizontal", [600.0, 500.0], [2016.0, 300.0, 0.0]);
    let fr = add(&mut s, "front", [600.0, 700.0], [2016.0, 0.0, 500.0]);
    let parts = build(&s.project).unwrap();
    let get = |id: u32| parts.iter().find(|p| p.owner == id).unwrap();
    assert_eq!((get(v).origin, get(v).size), ([2000.0, 0.0, 0.0], [16.0, 700.0, 500.0]));
    assert_eq!((get(h).origin, get(h).size), ([2016.0, 300.0, 0.0], [600.0, 16.0, 500.0]));
    assert_eq!((get(fr).origin, get(fr).size), ([2016.0, 0.0, 500.0], [600.0, 700.0, 16.0]));
    assert_eq!(get(v).panel.name, format!("Вертикальная {v}"));
    assert!(s.execute("panel.add", &json!({"material": "ЛДСП Белый 16", "plane": "front", "size": [0, 10]})).is_err());
    assert!(s.execute("panel.add", &json!({"material": "нет", "plane": "front", "size": [10, 10]})).is_err());

    // Блок с панелью внутри: положение блока сдвигает панель; удаление блока убирает всё содержимое.
    s.project.objects.push(Object::Block(Block { id: 100, name: "Блок".into(), frame: Frame::IDENTITY, children: Vec::new() }));
    let inner = s.execute("panel.add", &json!({"material": "ЛДСП Белый 16", "plane": "front", "size": [100, 100], "block": 100})).unwrap()["id"].as_u64().unwrap();
    s.execute("block.set", &json!({"id": 100, "name": "Тумба", "frame": {"origin": [0, 0, 1000], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}})).unwrap();
    let parts = build(&s.project).unwrap();
    assert_eq!(parts.iter().find(|p| p.panel.name == format!("Фронтальная {inner}")).unwrap().origin, [0.0, 0.0, 1000.0]);
    assert!(s.execute("block.set", &json!({"id": 100, "frame": {"origin": [0, 0, 0], "x": [1, 0, 0], "y": [1, 0, 0], "z": [0, 0, 1]}})).is_err(), "оси не перпендикулярны");
    assert!(s.execute("panel.remove", &json!({"id": 100})).is_err(), "это блок, а не деталь");
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
    // Теперь деталь правится отдельно.
    let side = s.project.panels().iter().find(|p| p.name == "Боковина левая").unwrap().id;
    s.execute("panel.set", &json!({"id": side, "material": "ХДФ 3"})).unwrap();
    assert!(s.execute("cabinet.explode", &json!({"id": 1})).is_err());
}

#[test]
fn old_format_loads_without_free_panels() {
    let v1 = r#"{"format":"basis-craft/1","name":"Старый","materials":[],"cabinets":[]}"#;
    assert_eq!(Project::load(v1).unwrap().format, FORMAT);
    let v1_panels = r#"{"format":"basis-craft/1","name":"Старый","panels":[{"id":1}]}"#;
    assert!(Project::load(v1_panels).unwrap_err().contains("импортируйте"));
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
    let ldsp = rows.iter().find(|r| r.material.starts_with("ЛДСП")).unwrap();
    assert!(ldsp.qty > 4.0 && ldsp.qty < 8.0, "{}", ldsp.qty);
    let xml = crate::raskroy::project_string("Шкаф", &panels, |_| (2800.0, 2070.0));
    assert_eq!(xml.matches("<BOARDSMP>").count(), 11);
}

/// Панель в проём: полка между боковиной и перегородкой, стойка между дном и крышей, вставка спереди.
#[test]
fn panel_fill_takes_the_opening() {
    let mut s = wardrobe();
    // Левая секция шкафа: от боковины (16) до перегородки (392), глубина 0…600.
    let id = |v: Value| v["id"].as_u64().unwrap() as u32;
    let shelf = id(s.execute("panel.fill", &json!({"material": "ЛДСП Белый 16", "plane": "horizontal", "at": [200, 1500], "gaps": [0, 0, 0, 20]})).unwrap());
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == shelf).unwrap();
    assert_eq!((p.origin, p.size), ([16.0, 1500.0, 0.0], [376.0, 16.0, 580.0]));
    // Стойка в правой секции между полкой (на 800 от дна → верх 80+16+800+16 = 912) и крышей (2000−16).
    let post = id(s.execute("panel.fill", &json!({"material": "ЛДСП Белый 16", "plane": "vertical", "at": [600, 1500]})).unwrap());
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == post).unwrap();
    assert_eq!((p.origin, p.size), ([600.0, 912.0, 0.0], [16.0, 1984.0 - 912.0, 600.0]));
    // Фронтальная в левой секции: между дном (верх 96) и полкой шкафа на 400 (низ 496), отступы по 2 мм.
    let front = id(s.execute("panel.fill", &json!({"material": "ЛДСП Белый 16", "plane": "front", "at": [200, 300], "gaps": [2, 2, 2, 2]})).unwrap());
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == front).unwrap();
    assert_eq!((p.origin, p.size), ([18.0, 98.0, 584.0], [372.0, 396.0, 16.0]));
    // Вне шкафа проёма нет.
    assert!(s.execute("panel.fill", &json!({"material": "ЛДСП Белый 16", "plane": "horizontal", "at": [5000, 500]})).is_err());
}

/// Кромка по торцам панели: наносится, подрезает заготовку, снимается; ошибки — понятные.
#[test]
fn panel_edges_by_segment() {
    let mut s = wardrobe();
    let id = s.execute("panel.add", &json!({"material": "ЛДСП Белый 16", "plane": "horizontal", "size": [600, 400], "position": [2000, 0, 0]})).unwrap()["id"].clone();
    let cut = |s: &Session| build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id).unwrap().panel.cut_size();
    assert_eq!(cut(&s), (600.0, 400.0));
    // Торцы прямоугольника: 0 — низ, 1 — лево, 2 — верх, 3 — право.
    s.execute("panel.edge", &json!({"id": id, "segments": [2], "edge": "ПВХ 0,4 белая"})).unwrap();
    s.execute("panel.edge", &json!({"id": id, "segments": [1, 3], "edge": "ПВХ 0,4 белая", "allowance": 1})).unwrap();
    // Длина: 600 − 2·0,4 + 2·1 (припуск); ширина: 400 − 0,4.
    let (l, w) = cut(&s);
    assert!((l - 601.2).abs() < 1e-9 && (w - 399.6).abs() < 1e-9, "{l} × {w}");
    s.execute("panel.edge", &json!({"id": id, "segments": [1, 2, 3], "edge": null})).unwrap();
    assert_eq!(cut(&s), (600.0, 400.0));
    assert!(s.execute("panel.edge", &json!({"id": id, "segments": [7], "edge": "ПВХ 0,4 белая"})).is_err());
    assert!(s.execute("panel.edge", &json!({"id": id, "segments": [0], "edge": "нет такой"})).is_err());
    assert!(s.execute("panel.edge", &json!({"id": 1, "segments": [0], "edge": "ПВХ 0,4 белая"})).unwrap_err().contains("Разрушить"));
}

/// Правка контура: скругление и фаска угла, вырез, размеры; деталь не смещается, ошибки отклоняются.
#[test]
fn contour_editing() {
    let mut s = wardrobe();
    let id = s.execute("panel.add", &json!({"material": "ЛДСП Белый 16", "plane": "front", "size": [600, 400], "position": [2000, 0, 0]})).unwrap()["id"].clone();
    let part = |s: &Session| build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id).unwrap();
    s.execute("panel.edge", &json!({"id": id, "segments": [0, 1, 2, 3], "edge": "ПВХ 0,4 белая"})).unwrap();
    // Скругление R50 в углу (600, 400): габарит тот же, торцов 5, новая дуга с той же кромкой.
    s.execute("panel.corner", &json!({"id": id, "at": [600, 400], "radius": 50})).unwrap();
    let p = part(&s);
    assert_eq!((p.origin, p.size), ([2000.0, 0.0, 0.0], [600.0, 400.0, 16.0]));
    assert_eq!(p.panel.segs.len(), 5);
    assert!(matches!(p.panel.segs[4], crate::contour::Seg::Arc { c, .. } if c == [550.0, 350.0]));
    assert_eq!(p.panel.edges.len(), 5);
    // Фаска 30 в углу (0, 0) и круглый вырез.
    s.execute("panel.corner", &json!({"id": id, "at": [0, 0], "chamfer": 30})).unwrap();
    s.execute("panel.cutout", &json!({"id": id, "center": [300, 200], "radius": 40})).unwrap();
    let m = crate::mesh::build(&[part(&s)], |_| false);
    assert!(m.errors.is_empty());
    // Ошибки: скругление больше торца, вырез вне панели, разрыв контура, размеры фигурной.
    assert!(s.execute("panel.corner", &json!({"id": id, "at": [600, 0], "radius": 900})).is_err());
    assert!(s.execute("panel.cutout", &json!({"id": id, "center": [900, 200], "radius": 10})).unwrap_err().contains("вне"));
    assert!(s.execute("panel.contour", &json!({"id": id, "segs": [{"Line": {"a": [0, 0], "b": [10, 0]}}]})).is_err());
    assert!(s.execute("panel.resize", &json!({"id": id, "size": [500, 300]})).is_err());
    // Размеры прямоугольной панели: кромка остаётся на тех же торцах.
    let id2 = s.execute("panel.add", &json!({"material": "ЛДСП Белый 16", "plane": "horizontal", "size": [600, 400], "position": [3000, 0, 0]})).unwrap()["id"].clone();
    s.execute("panel.edge", &json!({"id": id2, "segments": [2], "edge": "ПВХ 0,4 белая"})).unwrap();
    s.execute("panel.resize", &json!({"id": id2, "size": [800, 450]})).unwrap();
    let p = build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id2).unwrap();
    assert_eq!(p.panel.size, (800.0, 450.0));
    assert_eq!(p.panel.cut_size(), (800.0, 449.6));
    // Контур целиком: треугольник, смещённый от (0, 0), — нормализуется, начало осей переносится.
    s.execute("panel.contour", &json!({"id": id2, "segs": [
        {"Line": {"a": [10, 10], "b": [110, 10]}}, {"Line": {"a": [110, 10], "b": [10, 60]}}, {"Line": {"a": [10, 60], "b": [10, 10]}}]})).unwrap();
    let p = build(&s.project).unwrap().into_iter().find(|p| json!(p.id) == id2).unwrap();
    assert_eq!(p.panel.size, (100.0, 50.0));
    assert_eq!(p.origin, [3010.0, 0.0, 10.0]);
}

/// Действия с объектами: сдвиг, поворот на 90° (точно), зеркало с копией, массив, блок, удаление.
#[test]
fn object_actions() {
    let mut s = wardrobe();
    let id = |v: Value| v["id"].as_u64().unwrap() as u32;
    let p1 = id(s.execute("panel.add", &json!({"material": "ЛДСП Белый 16", "plane": "front", "size": [300, 200], "position": [2000, 0, 0]})).unwrap());
    s.execute("panel.corner", &json!({"id": p1, "at": [300, 200], "radius": 50})).unwrap();
    let part = |s: &Session, i: u32| build(&s.project).unwrap().into_iter().find(|p| p.id == i).unwrap();
    // Сдвиг шаблона и панели.
    s.execute("object.move", &json!({"ids": [1, p1], "d": [100, 0, -50]})).unwrap();
    assert_eq!(s.project.cabinets[0].position, [100.0, 0.0, -50.0]);
    assert_eq!(part(&s, p1).origin, [2100.0, 0.0, -50.0]);
    // Поворот на 90° вокруг Y: оси точные, габарит повёрнут.
    s.execute("object.rotate", &json!({"ids": [p1], "axis": "y", "angle": 90, "center": [2100, 0, -50]})).unwrap();
    let f = part(&s, p1).panel.frame.unwrap();
    assert!(f.x.iter().chain(&f.y).chain(&f.z).all(|v| [0.0, 1.0, -1.0].contains(v)), "{f:?}");
    assert_eq!(part(&s, p1).size, [16.0, 200.0, 300.0]);
    assert!(s.execute("object.rotate", &json!({"ids": [1], "axis": "y", "angle": 90})).unwrap_err().contains("Разрушить"));
    // Зеркальная копия плоскостью x = 3000: тот же габарит, отражённый; скругление — на другой стороне.
    let before = part(&s, p1);
    let m = s.execute("object.mirror", &json!({"ids": [p1], "axis": "x", "at": 3000, "copy": true})).unwrap()["ids"][0].as_u64().unwrap() as u32;
    let after = part(&s, m);
    assert_eq!(after.size, before.size);
    assert_eq!(after.origin[0], 6000.0 - before.origin[0] - before.size[0]);
    after.panel.frame.unwrap().check().unwrap();
    assert!(crate::mesh::build(std::slice::from_ref(&after), |_| false).errors.is_empty());
    // Пильный размер и кромка зеркальной копии — те же.
    assert_eq!(after.panel.cut_size(), before.panel.cut_size());
    // Массив: 3 копии корпуса через 800 мм.
    let ids = s.execute("object.copy", &json!({"ids": [1], "d": [800, 0, 0], "count": 3})).unwrap()["ids"].clone();
    assert_eq!(ids.as_array().unwrap().len(), 3);
    assert_eq!(s.project.cabinets.last().unwrap().position[0], 100.0 + 2400.0);
    // Блок из двух панелей; разрушение возвращает их на место.
    let b = id(s.execute("block.create", &json!({"ids": [p1, m], "name": "Боковины"})).unwrap());
    s.execute("block.set", &json!({"id": b, "frame": {"origin": [0, 100, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}})).unwrap();
    let lifted = part(&s, p1).origin;
    s.execute("block.explode", &json!({"id": b})).unwrap();
    assert_eq!(part(&s, p1).origin, lifted);
    assert!(s.execute("block.create", &json!({"ids": [1]})).is_err());
    s.execute("object.remove", &json!({"ids": [p1, 1]})).unwrap();
    assert!(s.project.cabinets.iter().all(|c| c.id != 1));
}

/// Присадка корпуса конфирматами: отверстия с точными координатами на обеих деталях стыка.
#[test]
fn cabinet_fasteners_drill_exactly() {
    let mut s = wardrobe();
    s.execute("cabinet.set", &json!({"id": 1, "fastener": "Конфирмат 7х50"})).unwrap();
    let parts = build(&s.project).unwrap();
    let side = by_name(&parts, "Боковина левая")[0];
    let bottom = by_name(&parts, "Дно")[0];
    // Стык дна с левой боковиной: длина 600, точки на 50 и 550 мм.
    // Боковина: сквозное ⌀8 с наружной пласти (z = 16 в осях боковины) на высоте середины дна (88).
    assert!(side.panel.holes.contains(&crate::panel::Hole { pos: [50.0, 88.0, 16.0], dir: [0.0, 0.0, -1.0], diameter: 8.0, depth: 16.0 }), "{:?}", side.panel.holes);
    // Дно: ⌀5 × 37 в торец, посередине толщины.
    assert!(bottom.panel.holes.contains(&crate::panel::Hole { pos: [0.0, 50.0, 8.0], dir: [1.0, 0.0, 0.0], diameter: 5.0, depth: 37.0 }), "{:?}", bottom.panel.holes);
    // Полки — на полкодержатели, по 2 с каждой стороны.
    let shelf = by_name(&parts, "Полка")[0];
    assert!(shelf.panel.holes.is_empty(), "полкодержатель сверлится только в стойке");
    let hw = hardware(&s.project).unwrap();
    let count = |n: &str| hw.iter().find(|(k, _)| k.starts_with(n)).map_or(0, |r| r.1);
    assert_eq!(count("Полкодержатель"), 2 * 2 * 2);
    assert!(count("Конфирмат") >= 12, "{hw:?}");
    // В XML Раскроя появляется присадка.
    let panels: Vec<Panel> = parts.iter().map(|p| p.panel.clone()).collect();
    assert!(crate::raskroy::project_string("Шкаф", &panels, |_| (2800.0, 2070.0)).contains("<DRILLING>"));
    // Неизвестный крепёж отклоняется.
    assert!(s.execute("cabinet.set", &json!({"id": 1, "fastener": "Гвоздь"})).is_err());
}

/// Крепёж между отдельными панелями: стык ищется сам, без стыка — отказ; раздвинутые панели — «висячий» крепёж.
#[test]
fn fasteners_between_panels() {
    let mut s = Session::new(Project::new("Тумба"));
    s.execute("material.add", &json!({"name": "ЛДСП 16", "thickness": 16})).unwrap();
    let id = |v: Value| v["id"].as_u64().unwrap() as u32;
    let side = id(s.execute("panel.add", &json!({"material": "ЛДСП 16", "plane": "vertical", "size": [500, 700], "position": [0, 0, 0]})).unwrap());
    let top = id(s.execute("panel.add", &json!({"material": "ЛДСП 16", "plane": "horizontal", "size": [600, 500], "position": [16, 684, 0]})).unwrap());
    let far = id(s.execute("panel.add", &json!({"material": "ЛДСП 16", "plane": "horizontal", "size": [600, 500], "position": [16, 1500, 0]})).unwrap());
    let f = id(s.execute("fastener.add", &json!({"kind": "Эксцентрик Minifix 15", "a": side, "b": top})).unwrap());
    assert!(s.execute("fastener.add", &json!({"kind": "Конфирмат 7х50", "a": side, "b": far})).unwrap_err().contains("не стыкуются"));
    let parts = build(&s.project).unwrap();
    let get = |i: u32| parts.iter().find(|p| p.id == i).unwrap();
    // Крышка: в торце ⌀8×34 и чашка ⌀15 в пласти; стойка: ⌀5×13. По 2 точки (стык 500: 50 и 450).
    assert_eq!(get(top).panel.holes.len(), 4);
    assert_eq!(get(side).panel.holes.len(), 2);
    assert!(get(top).panel.holes.iter().any(|h| h.diameter == 15.0 && h.depth == 13.0));
    assert_eq!(hardware(&s.project).unwrap(), vec![("Эксцентрик Minifix 15".to_string(), 2)]);
    // Раздвинули — крепёж «висит», отверстий нет, анализ это видит.
    s.execute("object.move", &json!({"ids": [top], "d": [100, 0, 0]})).unwrap();
    assert_eq!(hanging_fasteners(&s.project).len(), 1);
    assert!(build(&s.project).unwrap().iter().all(|p| p.panel.holes.is_empty()));
    s.execute("fastener.remove", &json!({"id": f})).unwrap();
    // Крепёж на все стыки: возвращаем крышку — один стык.
    s.execute("object.move", &json!({"ids": [top], "d": [-100, 0, 0]})).unwrap();
    let added = s.execute("fastener.auto", &json!({"kind": "Шкант 8х30"})).unwrap()["ids"].as_array().unwrap().len();
    assert_eq!(added, 1);
}

/// Фурнитура из библиотеки: отверстия садятся в прижатую панель; в смете — по названию; оторвали — анализ видит.
#[test]
fn furniture_drills_panels() {
    let mut s = Session::new(Project::new("Тумба"));
    s.execute("material.add", &json!({"name": "ЛДСП 16", "thickness": 16})).unwrap();
    let side = s.execute("panel.add", &json!({"material": "ЛДСП 16", "plane": "vertical", "size": [500, 700], "position": [0, 0, 0]})).unwrap()["id"].as_u64().unwrap() as u32;
    // Направляющая: два шурупа ⌀4×12 влево (в стойку), модель прижата к правой пласти стойки (x = 16).
    let hole = |z: f64| crate::panel::WorldHole { pos: [0.0, 10.0, z], dir: [-1.0, 0.0, 0.0], diameter: 4.0, depth: 12.0 };
    s.project.objects.push(Object::Furniture(Furniture {
        id: 50, name: "Направляющая".into(), kind: "Ящик".into(), source: String::new(),
        frame: Frame { origin: [16.0, 100.0, 0.0], ..Frame::IDENTITY },
        holes: vec![hole(37.0), hole(261.0)], lo: [0.0; 3], hi: [12.0, 45.0, 300.0],
    }));
    let parts = build(&s.project).unwrap();
    let p = parts.iter().find(|p| p.id == side).unwrap();
    assert_eq!(p.panel.holes.len(), 2);
    // В осях стойки: X — глубина (z проекта), Y — высота, Z — к левой пласти; вход — на правой пласти (z = 0).
    assert!(p.panel.holes.iter().any(|h| (h.pos[0] - 37.0).abs() < 1e-9 && (h.pos[1] - 110.0).abs() < 1e-9 && h.pos[2].abs() < 1e-9 && h.depth == 12.0), "{:?}", p.panel.holes);
    assert_eq!(hardware(&s.project).unwrap(), vec![("Направляющая".to_string(), 1)]);
    assert!(furniture_lost_holes(&s.project).is_empty());
    s.execute("object.move", &json!({"ids": [50], "d": [200, 0, 0]})).unwrap();
    assert_eq!(furniture_lost_holes(&s.project), vec![("Направляющая".to_string(), 2)]);
    assert!(s.execute("object.mirror", &json!({"ids": [50], "axis": "x", "at": 0})).is_err());
    // Настоящая модель из примеров ставится командой.
    let tandem = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/telegram/Ящик_300_мм_TANDEM_BLUMOTION_част,выд_11_16_мм_30_кг.fr3d");
    if tandem.exists() {
        let id = s.execute("furniture.add", &json!({"path": tandem, "position": [1000, 0, 0]})).unwrap()["id"].as_u64().unwrap() as u32;
        let f = furniture(&s.project).into_iter().find(|(f, _)| f.id == id).unwrap().0;
        assert!(f.name.contains("TANDEM") && !f.holes.is_empty());
    }
}
