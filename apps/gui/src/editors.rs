//! Редакторы объектов проекта: каждое изменение — команда (`cabinet.set` и т.п.), применяется по отпусканию.

use basis::design::{self, Project};
use basis::panel::Texture;
use eframe::egui;
use serde_json::{Value, json};

pub type Cmds = Vec<(&'static str, Value)>;

/// Изменение числового поля завершено: отпустили перетаскивание или ввели с клавиатуры.
fn done(r: &egui::Response) -> bool {
    r.drag_stopped() || (r.changed() && !r.dragged())
}

/// Поле в мм; возвращает новое значение, когда изменение завершено.
pub fn mm(ui: &mut egui::Ui, value: f64, range: std::ops::RangeInclusive<f64>) -> Option<f64> {
    let mut v = value;
    let r = ui.add(egui::DragValue::new(&mut v).range(range).suffix(" мм"));
    (done(&r) && v != value).then_some(v)
}

/// Необязательное поле в мм: флажок «своё значение» и число.
fn optional_mm(ui: &mut egui::Ui, value: Option<f64>, default_text: &str) -> Option<Option<f64>> {
    let mut on = value.is_some();
    let mut out = None;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut on, "").changed() {
            out = Some(if on { Some(500.0) } else { None });
        }
        match value {
            Some(v) => {
                if let Some(n) = mm(ui, v, 1.0..=3000.0) {
                    out = Some(Some(n));
                }
            }
            None => {
                ui.weak(default_text);
            }
        }
    });
    out
}

/// Положение X, Y, Z (целые мм при перетаскивании).
pub fn position_editor(ui: &mut egui::Ui, pos: [f64; 3]) -> Option<[f64; 3]> {
    let mut out = None;
    ui.horizontal(|ui| {
        let mut p = pos;
        let mut changed = false;
        for v in &mut p {
            let r = ui.add(egui::DragValue::new(v).range(-20000.0..=20000.0).speed(1.0));
            changed |= done(&r);
        }
        if changed && p != pos {
            out = Some(p);
        }
    });
    out
}

/// Выбор из списка имён; `none` — разрешить «—». Возвращает выбор, если он изменился.
pub fn choose(ui: &mut egui::Ui, key: impl std::hash::Hash + std::fmt::Debug, value: Option<&str>, names: &[String], none: bool) -> Option<Option<String>> {
    let mut chosen = value.map(str::to_owned);
    egui::ComboBox::from_id_salt(key).selected_text(value.unwrap_or("—")).show_ui(ui, |ui| {
        if none {
            ui.selectable_value(&mut chosen, None, "—");
        }
        for n in names {
            ui.selectable_value(&mut chosen, Some(n.clone()), n);
        }
    });
    (chosen.as_deref() != value).then_some(chosen)
}

fn materials(p: &Project) -> Vec<String> {
    p.materials.iter().map(|m| m.name.clone()).collect()
}

/// Названия крепежа справочника проекта (без артикула).
pub fn fastener_names(p: &Project) -> Vec<String> {
    p.defs().iter().map(|d| d.name.split('\r').next().unwrap_or(&d.name).to_string()).collect()
}

fn edges(p: &Project) -> Vec<String> {
    p.edges.iter().map(|e| e.name.clone()).collect()
}

/// Угловой модуль: все параметры.
pub fn corner_editor(ui: &mut egui::Ui, project: &Project, c: &design::Corner, cmds: &mut Cmds) {
    let id = c.id;
    let mut set = |key: &str, v: Value| cmds.push(("corner.set", json!({ "id": id, key: v })));
    egui::Grid::new(("cg", id)).num_columns(2).show(ui, |ui| {
        ui.label("Название");
        if let Some(n) = text_field(ui, ("corner_name", id), &c.name) {
            set("name", json!(n));
        }
        ui.end_row();
        for (key, label, value) in [("width_x", "Крыло X", c.width_x), ("width_z", "Крыло Z", c.width_z), ("height", "Высота", c.height), ("depth", "Глубина", c.depth), ("plinth", "Цоколь", c.plinth), ("door_gap", "Зазор дверей", c.door_gap)] {
            ui.label(label);
            if let Some(v) = mm(ui, value, 0.0..=6000.0) {
                set(key, json!(v));
            }
            ui.end_row();
        }
        ui.label("Положение X, Y, Z");
        if let Some(pos) = position_editor(ui, c.position) {
            set("position", json!(pos));
        }
        ui.end_row();
        ui.label("Двери");
        let mut doors = c.doors;
        if ui.checkbox(&mut doors, "").changed() {
            set("doors", json!(doors));
        }
        ui.end_row();
        ui.label("Крепёж");
        if let Some(v) = choose(ui, ("corner_fast", id), c.fastener.as_deref(), &fastener_names(project), true) {
            set("fastener", json!(v));
        }
        ui.end_row();
        let (mats, eds) = (materials(project), edges(project));
        for (key, label, value, list, none) in [
            ("material", "Материал", Some(c.material.as_str()), &mats, false),
            ("door_material", "Материал дверей", c.door_material.as_deref(), &mats, true),
            ("back", "Задняя стенка", c.back.as_deref(), &mats, true),
            ("edge", "Кромка", c.edge.as_deref(), &eds, true),
        ] {
            ui.label(label);
            if let Some(v) = choose(ui, (key, "corner", id), value, list, none) {
                set(key, json!(v));
            }
            ui.end_row();
        }
    });
    ui.label("Полки (высота от дна, мм):");
    for (i, &h) in c.shelves.iter().enumerate() {
        ui.horizontal(|ui| {
            if let Some(v) = mm(ui, h, 0.0..=c.height) {
                let mut s = c.shelves.clone();
                s[i] = v;
                cmds.push(("corner.set", json!({ "id": id, "shelves": s })));
            }
            if ui.small_button("удалить").clicked() {
                let mut s = c.shelves.clone();
                s.remove(i);
                cmds.push(("corner.set", json!({ "id": id, "shelves": s })));
            }
        });
    }
    if ui.button("+ Полка").clicked() {
        let mut s = c.shelves.clone();
        s.push(s.iter().copied().fold(0.0, f64::max) + 300.0);
        cmds.push(("corner.set", json!({ "id": id, "shelves": s })));
    }
    ui.horizontal(|ui| {
        if ui.button("Разрушить в панели").on_hover_text("Детали станут отдельными панелями блока; параметры модуля больше не действуют").clicked() {
            cmds.push(("corner.explode", json!({ "id": id })));
        }
        if ui.button("Удалить угловой модуль").clicked() {
            cmds.push(("corner.remove", json!({ "id": id })));
        }
    });
}

/// Столешница: все параметры.
pub fn countertop_editor(ui: &mut egui::Ui, project: &Project, ct: &design::Countertop, cmds: &mut Cmds) {
    let id = ct.id;
    let mut set = |key: &str, v: Value| cmds.push(("countertop.set", json!({ "id": id, key: v })));
    egui::Grid::new(("ctg", id)).num_columns(2).show(ui, |ui| {
        ui.label("Название");
        if let Some(n) = text_field(ui, ("ct_name", id), &ct.name) {
            set("name", json!(n));
        }
        ui.end_row();
        for (key, label, value) in [("length", "Длина", ct.length), ("depth", "Глубина", ct.depth)] {
            ui.label(label);
            if let Some(v) = mm(ui, value, 10.0..=6000.0) {
                set(key, json!(v));
            }
            ui.end_row();
        }
        ui.label("Положение X, Y, Z");
        if let Some(pos) = position_editor(ui, ct.position) {
            set("position", json!(pos));
        }
        ui.end_row();
        let (mats, eds) = (materials(project), edges(project));
        ui.label("Материал");
        if let Some(Some(v)) = choose(ui, ("ct_mat", id), Some(&ct.material), &mats, false) {
            set("material", json!(v));
        }
        ui.end_row();
        ui.label("Кромка");
        if let Some(v) = choose(ui, ("ct_edge", id), ct.edge.as_deref(), &eds, true) {
            set("edge", json!(v));
        }
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.button("Разрушить в панель").clicked() {
            cmds.push(("countertop.explode", json!({ "id": id })));
        }
        if ui.button("Удалить столешницу").clicked() {
            cmds.push(("countertop.remove", json!({ "id": id })));
        }
    });
}

/// Отдельная панель дерева: имя, материал, назначение, текстура, положение; размеры — из контура.
pub fn panel_editor(ui: &mut egui::Ui, project: &Project, fp: &design::FreePanel, size: (f64, f64, f64), cmds: &mut Cmds) {
    let id = fp.id;
    let mut set = |key: &str, v: Value| cmds.push(("panel.set", json!({ "id": id, key: v })));
    egui::Grid::new(("pg", id)).num_columns(2).show(ui, |ui| {
        ui.label("Наименование");
        if let Some(n) = text_field(ui, ("panel_name", id), &fp.name) {
            set("name", json!(n));
        }
        ui.end_row();
        ui.label("Материал");
        if let Some(Some(v)) = choose(ui, ("pm", id), Some(&fp.material), &materials(project), false) {
            set("material", json!(v));
        }
        ui.end_row();
        ui.label("Назначение");
        let mut kind = fp.kind;
        egui::ComboBox::from_id_salt(("pk", id)).selected_text(kind_name(kind)).show_ui(ui, |ui| {
            for k in [design::Kind::Body, design::Kind::Back, design::Kind::Door, design::Kind::Drawer] {
                ui.selectable_value(&mut kind, k, kind_name(k));
            }
        });
        if kind != fp.kind {
            set("kind", json!(kind));
        }
        ui.end_row();
        ui.label("Текстура");
        let mut tex = fp.texture;
        egui::ComboBox::from_id_salt(("pt", id)).selected_text(texture_name(tex)).show_ui(ui, |ui| {
            for t in [Texture::Horizontal, Texture::Vertical, Texture::Default] {
                ui.selectable_value(&mut tex, t, texture_name(t));
            }
        });
        if tex != fp.texture {
            set("texture", json!(tex));
        }
        ui.end_row();
        ui.label("Размеры");
        ui.label(format!("{} × {} × {} мм", size.0, size.1, size.2));
        ui.end_row();
        ui.label("Начало осей панели").on_hover_text("Угол контура (0, 0) на тыльной пласти; толщина идёт вдоль оси Z панели");
        if let Some(origin) = position_editor(ui, fp.frame.origin) {
            set("frame", json!(basis::geom::Frame { origin, ..fp.frame }));
        }
        ui.end_row();
    });
    if ui.button("Удалить панель").clicked() {
        cmds.push(("panel.remove", json!({ "id": id })));
    }
}

pub fn kind_name(k: design::Kind) -> &'static str {
    match k {
        design::Kind::Body => "Корпусная",
        design::Kind::Back => "Задняя стенка",
        design::Kind::Door => "Дверь",
        design::Kind::Drawer => "Фасад ящика",
    }
}

pub fn texture_name(t: Texture) -> &'static str {
    match t {
        Texture::Horizontal => "вдоль X контура",
        Texture::Vertical => "вдоль Y контура",
        Texture::Default => "без направления",
    }
}

/// Текстовое поле с черновиком: набранное хранится, пока поле в фокусе,
/// и возвращается по выходу из поля или Enter, если отличается от `value`.
pub fn text_field(ui: &mut egui::Ui, key: impl egui::AsId, value: &str) -> Option<String> {
    let id = egui::Id::new(key);
    let mut draft = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| value.to_owned());
    let r = ui.add(egui::TextEdit::singleline(&mut draft).id(id.with("edit")));
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        return (draft != value).then_some(draft);
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, draft));
    }
    None
}

/// Поля корпуса. Каждое изменение — команда `cabinet.set` и т.п.
pub fn cabinet_editor(ui: &mut egui::Ui, project: &Project, c: &design::Cabinet, cmds: &mut Cmds) {
    let id = c.id;
    let mut set = |key: &str, v: Value| cmds.push(("cabinet.set", json!({ "id": id, key: v })));
    egui::Grid::new(("cab", id)).num_columns(2).show(ui, |ui| {
        ui.label("Название");
        if let Some(name) = text_field(ui, ("cab_name", id), &c.name) {
            set("name", json!(name));
        }
        ui.end_row();
        for (key, label, value, min) in [
            ("width", "Ширина", c.width, 50.0),
            ("height", "Высота", c.height, 50.0),
            ("depth", "Глубина", c.depth, 50.0),
            ("plinth", "Цоколь", c.plinth, 0.0),
            ("door_gap", "Зазор дверей", c.door_gap, 0.0),
        ] {
            ui.label(label);
            let mut v = value;
            // Применяем по отпусканию, чтобы перетаскивание давало один шаг отмены.
            let r = ui.add(egui::DragValue::new(&mut v).range(min..=6000.0).suffix(" мм"));
            if (r.drag_stopped() || (r.changed() && !r.dragged())) && v != value {
                set(key, json!(v));
            }
            ui.end_row();
        }
        ui.label("Положение X, Y, Z");
        if let Some(pos) = position_editor(ui, c.position) {
            set("position", json!(pos));
        }
        ui.end_row();
        ui.label("Глубина ящиков");
        if let Some(v) = optional_mm(ui, c.drawer_depth, "по глубине корпуса") {
            set("drawer_depth", json!(v));
        }
        ui.end_row();
        ui.label("Крепёж").on_hover_text("Присадка всех стыков корпуса; полки — на полкодержатели");
        if let Some(v) = choose(ui, ("cab_fast", id), c.fastener.as_deref(), &fastener_names(project), true) {
            set("fastener", json!(v));
        }
        ui.end_row();
        ui.label("Двери");
        let mut doors = c.doors;
        let r = ui.add(egui::DragValue::new(&mut doors).range(0..=8));
        if (r.drag_stopped() || (r.changed() && !r.dragged())) && doors != c.doors {
            set("doors", json!(doors));
        }
        ui.end_row();
        for (key, label, value, edges) in [
            ("material", "Материал", Some(c.material.clone()), false),
            ("door_material", "Материал дверей", c.door_material.clone(), false),
            ("back", "Задняя стенка", c.back.clone(), false),
            ("edge", "Кромка", c.edge.clone(), true),
        ] {
            ui.label(label);
            let names: Vec<String> = if edges { project.edges.iter().map(|e| e.name.clone()).collect() } else { project.materials.iter().map(|m| m.name.clone()).collect() };
            let mut chosen = value.clone();
            egui::ComboBox::from_id_salt((key, id)).selected_text(chosen.clone().unwrap_or_else(|| "—".into())).show_ui(ui, |ui| {
                if key != "material" {
                    ui.selectable_value(&mut chosen, None, "—");
                }
                for n in names {
                    ui.selectable_value(&mut chosen, Some(n.clone()), n);
                }
            });
            if chosen != value {
                set(key, json!(chosen));
            }
            ui.end_row();
        }
    });

    ui.label("Перегородки (левая грань от левого края, мм):");
    for (i, &x) in c.partitions.iter().enumerate() {
        ui.horizontal(|ui| {
            let mut v = x;
            let r = ui.add(egui::DragValue::new(&mut v).range(0.0..=c.width).suffix(" мм"));
            if (r.drag_stopped() || (r.changed() && !r.dragged())) && v != x {
                let mut xs = c.partitions.clone();
                xs[i] = v;
                cmds.push(("cabinet.set", json!({ "id": id, "partitions": xs })));
            }
            if ui.small_button("удалить").clicked() {
                cmds.push(("partition.remove", json!({ "cabinet": id, "index": i })));
            }
        });
    }
    if ui.button("+ Перегородка посередине").clicked() {
        let t = project.material(&c.material).map_or(16.0, |m| m.thickness);
        cmds.push(("partition.add", json!({ "cabinet": id, "x": ((c.width - t) / 2.0).round() })));
    }

    ui.label("Полки (секция, высота от дна, мм):");
    for (i, sh) in c.shelves.iter().enumerate() {
        ui.horizontal(|ui| {
            let (mut sec, mut h) = (sh.section, sh.height);
            let r1 = ui.add(egui::DragValue::new(&mut sec).range(0..=c.partitions.len()).prefix("секция "));
            let r2 = ui.add(egui::DragValue::new(&mut h).range(0.0..=c.height).suffix(" мм"));
            let done = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
            if (done(&r1) || done(&r2)) && (sec != sh.section || h != sh.height) {
                let mut shelves = c.shelves.clone();
                shelves[i] = design::Shelf { section: sec, height: h };
                cmds.push(("cabinet.set", json!({ "id": id, "shelves": shelves })));
            }
            if ui.small_button("удалить").clicked() {
                cmds.push(("shelf.remove", json!({ "cabinet": id, "index": i })));
            }
        });
    }
    ui.horizontal(|ui| {
        for sec in 0..=c.partitions.len() {
            if ui.button(format!("+ Полка в секцию {}", sec)).clicked() {
                let top = c.shelves.iter().filter(|s| s.section == sec).map(|s| s.height).fold(0.0, f64::max);
                cmds.push(("shelf.add", json!({ "cabinet": id, "section": sec, "height": top + 350.0 })));
            }
        }
    });
    ui.label("Ящики (высота фасада снизу вверх, мм):");
    if !c.partitions.is_empty() {
        let mut sec = c.drawer_section;
        let r = ui.add(egui::DragValue::new(&mut sec).range(0..=c.partitions.len()).prefix("в секции "));
        if (r.drag_stopped() || (r.changed() && !r.dragged())) && sec != c.drawer_section {
            cmds.push(("cabinet.set", json!({ "id": id, "drawer_section": sec })));
        }
    }
    for (i, &hf) in c.drawers.iter().enumerate() {
        ui.horizontal(|ui| {
            let mut v = hf;
            let r = ui.add(egui::DragValue::new(&mut v).range(100.0..=1000.0).suffix(" мм"));
            if (r.drag_stopped() || (r.changed() && !r.dragged())) && v != hf {
                let mut ds = c.drawers.clone();
                ds[i] = v;
                cmds.push(("cabinet.set", json!({ "id": id, "drawers": ds })));
            }
            if ui.small_button("удалить").clicked() {
                let mut ds = c.drawers.clone();
                ds.remove(i);
                cmds.push(("cabinet.set", json!({ "id": id, "drawers": ds })));
            }
        });
    }
    if ui.button("+ Ящик").clicked() {
        let mut ds = c.drawers.clone();
        ds.push(if ds.is_empty() { 140.0 } else { 280.0 });
        cmds.push(("cabinet.set", json!({ "id": id, "drawers": ds })));
    }
    if ui.button("Удалить корпус").clicked() {
        cmds.push(("cabinet.remove", json!({ "id": id })));
    }
}

/// Точки сегмента для рисования (дуги — ломаной).
fn seg_points(s: &basis::contour::Seg) -> Vec<[f64; 2]> {
    use basis::contour::Seg;
    match *s {
        Seg::Line { a, b } => vec![a, b],
        Seg::Arc { a, b, c, ccw } => {
            let r = (a[0] - c[0]).hypot(a[1] - c[1]);
            let ang = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
            let tau = std::f64::consts::TAU;
            let sweep = if ccw { ang(b) - ang(a) } else { ang(a) - ang(b) }.rem_euclid(tau);
            let sweep = if sweep < 1e-12 { tau } else { sweep };
            let sign = if ccw { 1.0 } else { -1.0 };
            (0..=32).map(|i| {
                let t = ang(a) + sign * sweep * i as f64 / 32.0;
                [c[0] + r * t.cos(), c[1] + r * t.sin()]
            }).collect()
        }
        Seg::Circle { c, r } => (0..=48).map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 48.0;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        }).collect(),
    }
}

/// Щелчок по схеме контура: торец под курсором, вершина рядом и точка в мм осей панели.
#[derive(Clone, Copy, Debug)]
pub struct ContourClick {
    pub segment: Option<usize>,
    pub vertex: Option<[f64; 2]>,
    pub point: [f64; 2],
}

/// Схема контура панели (как в Базисе при облицовке: панель параллельно экрану). Торцы с кромкой —
/// толстой линией с подписью толщины, торец под курсором — оранжевым, вершины — кружками.
pub fn contour_view(ui: &mut egui::Ui, segs: &[basis::contour::Seg], edges: &[basis::panel::Edge], height: f32) -> Option<ContourClick> {
    use egui::{Color32, Pos2, Stroke};
    let (resp, painter) = ui.allocate_painter(egui::vec2(ui.available_width(), height), egui::Sense::click());
    let (lo, hi) = basis::contour::bounds(segs)?;
    let area = resp.rect.shrink(18.0);
    let k = (area.width() as f64 / (hi[0] - lo[0]).max(1.0)).min(area.height() as f64 / (hi[1] - lo[1]).max(1.0));
    let off = egui::vec2((area.width() - ((hi[0] - lo[0]) * k) as f32) / 2.0, -(area.height() - ((hi[1] - lo[1]) * k) as f32) / 2.0);
    let to = |p: [f64; 2]| Pos2::new(area.left() + ((p[0] - lo[0]) * k) as f32, area.bottom() - ((p[1] - lo[1]) * k) as f32) + off;
    let from = |q: Pos2| {
        let q = q - off;
        [((q.x - area.left()) as f64 / k + lo[0]).round(), ((area.bottom() - q.y) as f64 / k + lo[1]).round()]
    };
    painter.rect_filled(resp.rect, 4.0, Color32::from_gray(245));
    let mouse = ui.ctx().pointer_hover_pos().filter(|m| resp.rect.contains(*m));
    let polylines: Vec<Vec<Pos2>> = segs.iter().map(|s| seg_points(s).into_iter().map(to).collect()).collect();
    let dist = |m: Pos2, pl: &[Pos2]| pl.windows(2).map(|w| {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let t = ((m - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
        (a + ab * t - m).length()
    }).fold(f32::MAX, f32::min);
    let hovered = mouse.and_then(|m| (0..segs.len()).map(|i| (i, dist(m, &polylines[i]))).filter(|(_, d)| *d < 10.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i));
    for (i, pl) in polylines.iter().enumerate() {
        let edge = edges.iter().find(|e| e.segment == i && e.thickness > 0.0);
        let stroke = match (Some(i) == hovered, edge) {
            (true, _) => Stroke::new(4.0, Color32::from_rgb(230, 120, 0)),
            (false, Some(_)) => Stroke::new(4.0, Color32::from_rgb(40, 40, 40)),
            (false, None) => Stroke::new(1.0, Color32::from_gray(90)),
        };
        painter.add(egui::Shape::line(pl.clone(), stroke));
        // Середина ломаной: у отрезка — между концами.
        let mid = if pl.len() == 2 { pl[0] + (pl[1] - pl[0]) / 2.0 } else { pl[pl.len() / 2] };
        let label = match edge {
            Some(e) => format!("{} · {}", i, e.thickness),
            None => i.to_string(),
        };
        painter.text(mid, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), Color32::from_rgb(20, 60, 160));
    }
    // Вершины: концы отрезков и дуг.
    let vertices: Vec<[f64; 2]> = segs.iter().filter(|s| !matches!(s, basis::contour::Seg::Circle { .. })).map(|s| basis::contour::ends(s).0).collect();
    let vertex = mouse.and_then(|m| vertices.iter().copied().filter(|&v| (to(v) - m).length() < 9.0).min_by(|a, b| (to(*a) - m).length().total_cmp(&(to(*b) - m).length())));
    for &v in &vertices {
        let hot = Some(v) == vertex;
        painter.circle(to(v), if hot { 5.0 } else { 2.5 }, if hot { Color32::from_rgb(230, 120, 0) } else { Color32::from_gray(90) }, Stroke::NONE);
    }
    if let Some(v) = vertex {
        resp.clone().on_hover_text(format!("Вершина ({}, {})", v[0], v[1]));
    } else if let Some(i) = hovered {
        let len = basis::contour::length(&segs[i]);
        resp.clone().on_hover_text(format!("Торец {i}: {len:.1} мм"));
    }
    let point = mouse.map(from)?;
    resp.clicked().then_some(ContourClick { segment: hovered, vertex, point })
}
