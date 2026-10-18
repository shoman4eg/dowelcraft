//! Project object editors: each change is a command (`cabinet.set` etc.), applied on release.

use dowel::design::{self, Project};
use dowel::panel::Texture;
use eframe::egui;
use serde_json::{Value, json};

pub type Cmds = Vec<(&'static str, Value)>;

/// Width of the label column in property tables (as in cadcraft: label on the right, value is a field).
pub const KEY_W: f32 = 128.0;
/// Width of the value field (numbers, lists, text).
pub const VALUE_W: f32 = 150.0;

/// Property table: two columns, labels right-aligned, even spacing.
pub fn props<R>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Grid::new(id).num_columns(2).spacing([8.0, 5.0]).show(ui, |ui| {
        ui.spacing_mut().interact_size.x = 72.0;
        body(ui)
    })
    .inner
}

/// Property label: dimmed, right-aligned in a fixed-width column; long ones get "…".
pub fn caption(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let color = ui.visuals().weak_text_color();
    let text = &crate::i18n::t(text);
    let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Truncate), KEY_W, egui::TextStyle::Body);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(KEY_W, ui.spacing().interact_size.y), egui::Sense::hover());
    let at = egui::pos2(rect.right() - galley.size().x, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley.clone(), color);
    if galley.elided {
        return resp.on_hover_text(crate::i18n::t(text));
    }
    resp
}

/// Read-only value — in a field frame (like a disabled cadcraft field).
pub fn value_box(ui: &mut egui::Ui, text: impl Into<String>) {
    let v = ui.visuals();
    egui::Frame::new().fill(v.extreme_bg_color).stroke(egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color)).corner_radius(3).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
        ui.set_min_width(VALUE_W);
        ui.add(egui::Label::new(text.into()).truncate());
    });
}

/// A numeric field edit is finished: drag released or typed from the keyboard.
fn done(r: &egui::Response) -> bool {
    r.drag_stopped() || (r.changed() && !r.dragged())
}

/// Field in mm; returns the new value when the edit is finished.
pub fn mm(ui: &mut egui::Ui, value: f64, range: std::ops::RangeInclusive<f64>) -> Option<f64> {
    let mut v = value;
    let r = crate::expr::add(ui, &mut v, |d| d.range(range).suffix(crate::i18n::t(" mm")));
    (done(&r) && v != value).then_some(v)
}

/// Optional field in mm: a "custom value" checkbox and a number.
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

/// Position X, Y, Z (whole mm while dragging).
pub fn position_editor(ui: &mut egui::Ui, pos: [f64; 3]) -> Option<[f64; 3]> {
    let mut out = None;
    ui.horizontal(|ui| {
        let mut p = pos;
        let mut changed = false;
        for v in &mut p {
            let r = crate::expr::add(ui, v, |d| d.range(-20000.0..=20000.0).speed(1.0));
            changed |= done(&r);
        }
        if changed && p != pos {
            out = Some(p);
        }
    });
    out
}

/// Pick from a list of names; `none` — allow "—". Returns the choice if it changed.
pub fn choose(ui: &mut egui::Ui, key: impl std::hash::Hash + std::fmt::Debug, value: Option<&str>, names: &[String], none: bool) -> Option<Option<String>> {
    let mut chosen = value.map(str::to_owned);
    egui::ComboBox::from_id_salt(key).width(VALUE_W).selected_text(value.unwrap_or("—")).show_ui(ui, |ui| {
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

/// Fastener names from the project catalog (without article number).
pub fn fastener_names(p: &Project) -> Vec<String> {
    p.defs().iter().map(|d| d.name.split('\r').next().unwrap_or(&d.name).to_string()).collect()
}

fn edges(p: &Project) -> Vec<String> {
    p.edges.iter().map(|e| e.name.clone()).collect()
}

/// Corner module: all parameters.
pub fn corner_editor(ui: &mut egui::Ui, project: &Project, c: &design::Corner, cmds: &mut Cmds) {
    let id = c.id;
    let mut set = |key: &str, v: Value| cmds.push(("corner.set", json!({ "id": id, key: v })));
    props(ui, ("cg", id), |ui| {
        caption(ui, "Name");
        if let Some(n) = text_field(ui, ("corner_name", id), &c.name) {
            set("name", json!(n));
        }
        ui.end_row();
        for (key, label, value) in [("width_x", "Wing X", c.width_x), ("width_z", "Wing Z", c.width_z), ("height", "Height", c.height), ("depth", "Depth", c.depth), ("plinth", "Plinth", c.plinth), ("door_gap", "Door gap", c.door_gap)] {
            caption(ui, label);
            if let Some(v) = mm(ui, value, 0.0..=6000.0) {
                set(key, json!(v));
            }
            ui.end_row();
        }
        caption(ui, "Position X, Y, Z");
        if let Some(pos) = position_editor(ui, c.position) {
            set("position", json!(pos));
        }
        ui.end_row();
        caption(ui, "Doors");
        let mut doors = c.doors;
        if ui.checkbox(&mut doors, "").changed() {
            set("doors", json!(doors));
        }
        ui.end_row();
        caption(ui, "Fasteners");
        if let Some(v) = choose(ui, ("corner_fast", id), c.fastener.as_deref(), &fastener_names(project), true) {
            set("fastener", json!(v));
        }
        ui.end_row();
        let (mats, eds) = (materials(project), edges(project));
        for (key, label, value, list, none) in [
            ("material", "Material", Some(c.material.as_str()), &mats, false),
            ("door_material", "Door material", c.door_material.as_deref(), &mats, true),
            ("back", "Back panel", c.back.as_deref(), &mats, true),
            ("edge", "Edge banding", c.edge.as_deref(), &eds, true),
        ] {
            caption(ui, label);
            if let Some(v) = choose(ui, (key, "corner", id), value, list, none) {
                set(key, json!(v));
            }
            ui.end_row();
        }
    });
    ui.label(crate::i18n::t("Shelves (height from the bottom, mm):"));
    for (i, &h) in c.shelves.iter().enumerate() {
        ui.horizontal(|ui| {
            if let Some(v) = mm(ui, h, 0.0..=c.height) {
                let mut s = c.shelves.clone();
                s[i] = v;
                cmds.push(("corner.set", json!({ "id": id, "shelves": s })));
            }
            if ui.small_button(crate::i18n::t("delete")).clicked() {
                let mut s = c.shelves.clone();
                s.remove(i);
                cmds.push(("corner.set", json!({ "id": id, "shelves": s })));
            }
        });
    }
    if ui.button(crate::i18n::t("+ Shelf")).clicked() {
        let mut s = c.shelves.clone();
        s.push(s.iter().copied().fold(0.0, f64::max) + 300.0);
        cmds.push(("corner.set", json!({ "id": id, "shelves": s })));
    }
    ui.horizontal(|ui| {
        if ui.button(crate::i18n::t("Explode into panels")).on_hover_text(crate::i18n::t("Parts become separate panels of a block; module parameters stop applying")).clicked() {
            cmds.push(("corner.explode", json!({ "id": id })));
        }
        if ui.button(crate::i18n::t("Delete corner unit")).clicked() {
            cmds.push(("corner.remove", json!({ "id": id })));
        }
    });
}

/// Countertop: all parameters.
pub fn countertop_editor(ui: &mut egui::Ui, project: &Project, ct: &design::Countertop, cmds: &mut Cmds) {
    let id = ct.id;
    let mut set = |key: &str, v: Value| cmds.push(("countertop.set", json!({ "id": id, key: v })));
    props(ui, ("ctg", id), |ui| {
        caption(ui, "Name");
        if let Some(n) = text_field(ui, ("ct_name", id), &ct.name) {
            set("name", json!(n));
        }
        ui.end_row();
        for (key, label, value) in [("length", "Length", ct.length), ("depth", "Depth", ct.depth)] {
            caption(ui, label);
            if let Some(v) = mm(ui, value, 10.0..=6000.0) {
                set(key, json!(v));
            }
            ui.end_row();
        }
        caption(ui, "Position X, Y, Z");
        if let Some(pos) = position_editor(ui, ct.position) {
            set("position", json!(pos));
        }
        ui.end_row();
        let (mats, eds) = (materials(project), edges(project));
        caption(ui, "Material");
        if let Some(Some(v)) = choose(ui, ("ct_mat", id), Some(&ct.material), &mats, false) {
            set("material", json!(v));
        }
        ui.end_row();
        caption(ui, "Edge banding");
        if let Some(v) = choose(ui, ("ct_edge", id), ct.edge.as_deref(), &eds, true) {
            set("edge", json!(v));
        }
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.button(crate::i18n::t("Explode into a panel")).clicked() {
            cmds.push(("countertop.explode", json!({ "id": id })));
        }
        if ui.button(crate::i18n::t("Delete countertop")).clicked() {
            cmds.push(("countertop.remove", json!({ "id": id })));
        }
    });
}

/// Standalone tree panel: name, material, role, texture, position; sizes come from the contour.
pub fn panel_editor(ui: &mut egui::Ui, project: &Project, fp: &design::FreePanel, size: (f64, f64, f64), cmds: &mut Cmds) {
    let id = fp.id;
    let mut set = |key: &str, v: Value| cmds.push(("panel.set", json!({ "id": id, key: v })));
    props(ui, ("pg", id), |ui| {
        caption(ui, "Item name");
        if let Some(n) = text_field(ui, ("panel_name", id), &fp.name) {
            set("name", json!(n));
        }
        ui.end_row();
        caption(ui, "Material");
        if let Some(Some(v)) = choose(ui, ("pm", id), Some(&fp.material), &materials(project), false) {
            set("material", json!(v));
        }
        ui.end_row();
        caption(ui, "Purpose");
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
        caption(ui, "Texture");
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
        caption(ui, "Dimensions");
        ui.label(crate::i18n::t(format!("{} × {} × {} mm", size.0, size.1, size.2)));
        ui.end_row();
        caption(ui, "Panel origin").on_hover_text(crate::i18n::t("Contour corner (0, 0) on the back face; thickness runs along the panel Z axis"));
        if let Some(origin) = position_editor(ui, fp.frame.origin) {
            set("frame", json!(dowel::geom::Frame { origin, ..fp.frame }));
        }
        ui.end_row();
    });
    if ui.button(crate::i18n::t("Delete panel")).clicked() {
        cmds.push(("panel.remove", json!({ "id": id })));
    }
}

pub fn kind_name(k: design::Kind) -> &'static str {
    match k {
        design::Kind::Body => "Carcass",
        design::Kind::Back => "Back panel",
        design::Kind::Door => "Door",
        design::Kind::Drawer => "Drawer front",
    }
}

pub fn texture_name(t: Texture) -> &'static str {
    match t {
        Texture::Horizontal => "along contour X",
        Texture::Vertical => "along contour Y",
        Texture::Default => "no direction",
    }
}

/// Text field with a draft: typed text is kept while the field has focus
/// and returned on leaving the field or Enter, if it differs from `value`.
pub fn text_field(ui: &mut egui::Ui, key: impl egui::AsId, value: &str) -> Option<String> {
    let id = egui::Id::new(key);
    let mut draft = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| value.to_owned());
    let r = ui.add(egui::TextEdit::singleline(&mut draft).id(id.with("edit")).desired_width(ui.available_width().min(320.0)));
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        return (draft != value).then_some(draft);
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, draft));
    }
    None
}

/// Cabinet fields. Each change is a command `cabinet.set` etc.
pub fn cabinet_editor(ui: &mut egui::Ui, project: &Project, c: &design::Cabinet, cmds: &mut Cmds) {
    let id = c.id;
    let mut set = |key: &str, v: Value| cmds.push(("cabinet.set", json!({ "id": id, key: v })));
    props(ui, ("cab", id), |ui| {
        caption(ui, "Name");
        if let Some(name) = text_field(ui, ("cab_name", id), &c.name) {
            set("name", json!(name));
        }
        ui.end_row();
        for (key, label, value, min) in [
            ("width", "Width", c.width, 50.0),
            ("height", "Height", c.height, 50.0),
            ("depth", "Depth", c.depth, 50.0),
            ("plinth", "Plinth", c.plinth, 0.0),
            ("door_gap", "Door gap", c.door_gap, 0.0),
        ] {
            caption(ui, label);
            let mut v = value;
            // Apply on release so that a drag yields a single undo step.
            let r = crate::expr::add(ui, &mut v, |d| d.range(min..=6000.0).suffix(crate::i18n::t(" mm")));
            if (r.drag_stopped() || (r.changed() && !r.dragged())) && v != value {
                set(key, json!(v));
            }
            ui.end_row();
        }
        caption(ui, "Position X, Y, Z");
        if let Some(pos) = position_editor(ui, c.position) {
            set("position", json!(pos));
        }
        ui.end_row();
        caption(ui, "Drawer depth");
        if let Some(v) = optional_mm(ui, c.drawer_depth, "cabinet depth") {
            set("drawer_depth", json!(v));
        }
        ui.end_row();
        caption(ui, "Fasteners").on_hover_text(crate::i18n::t("Drilling for all cabinet joints; shelves on shelf supports"));
        if let Some(v) = choose(ui, ("cab_fast", id), c.fastener.as_deref(), &fastener_names(project), true) {
            set("fastener", json!(v));
        }
        ui.end_row();
        caption(ui, "Doors");
        let mut doors = c.doors;
        let r = crate::expr::add(ui, &mut doors, |d| d.range(0..=8));
        if (r.drag_stopped() || (r.changed() && !r.dragged())) && doors != c.doors {
            set("doors", json!(doors));
        }
        ui.end_row();
        for (key, label, value, edges) in [
            ("material", "Material", Some(c.material.clone()), false),
            ("door_material", "Door material", c.door_material.clone(), false),
            ("back", "Back panel", c.back.clone(), false),
            ("edge", "Edge banding", c.edge.clone(), true),
        ] {
            caption(ui, label);
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

    ui.label(crate::i18n::t("Partitions (left face from the left edge, mm):"));
    for (i, &x) in c.partitions.iter().enumerate() {
        ui.horizontal(|ui| {
            let mut v = x;
            let r = crate::expr::add(ui, &mut v, |d| d.range(0.0..=c.width).suffix(crate::i18n::t(" mm")));
            if (r.drag_stopped() || (r.changed() && !r.dragged())) && v != x {
                let mut xs = c.partitions.clone();
                xs[i] = v;
                cmds.push(("cabinet.set", json!({ "id": id, "partitions": xs })));
            }
            if ui.small_button(crate::i18n::t("delete")).clicked() {
                cmds.push(("partition.remove", json!({ "cabinet": id, "index": i })));
            }
        });
    }
    if ui.button(crate::i18n::t("+ Partition in the middle")).clicked() {
        let t = project.material(&c.material).map_or(16.0, |m| m.thickness);
        cmds.push(("partition.add", json!({ "cabinet": id, "x": ((c.width - t) / 2.0).round() })));
    }

    ui.label(crate::i18n::t("Shelves (section, height from the bottom, mm):"));
    for (i, sh) in c.shelves.iter().enumerate() {
        ui.horizontal(|ui| {
            let (mut sec, mut h) = (sh.section, sh.height);
            let r1 = crate::expr::add(ui, &mut sec, |d| d.range(0..=c.partitions.len()).prefix("section"));
            let r2 = crate::expr::add(ui, &mut h, |d| d.range(0.0..=c.height).suffix(crate::i18n::t(" mm")));
            let done = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
            if (done(&r1) || done(&r2)) && (sec != sh.section || h != sh.height) {
                let mut shelves = c.shelves.clone();
                shelves[i] = design::Shelf { section: sec, height: h };
                cmds.push(("cabinet.set", json!({ "id": id, "shelves": shelves })));
            }
            if ui.small_button(crate::i18n::t("delete")).clicked() {
                cmds.push(("shelf.remove", json!({ "cabinet": id, "index": i })));
            }
        });
    }
    ui.horizontal(|ui| {
        for sec in 0..=c.partitions.len() {
            if ui.button(crate::i18n::t(format!("+ Shelf in section {}", sec))).clicked() {
                let top = c.shelves.iter().filter(|s| s.section == sec).map(|s| s.height).fold(0.0, f64::max);
                cmds.push(("shelf.add", json!({ "cabinet": id, "section": sec, "height": top + 350.0 })));
            }
        }
    });
    ui.label(crate::i18n::t("Drawers (front heights from the bottom up, mm):"));
    if !c.partitions.is_empty() {
        let mut sec = c.drawer_section;
        let r = crate::expr::add(ui, &mut sec, |d| d.range(0..=c.partitions.len()).prefix("in section"));
        if (r.drag_stopped() || (r.changed() && !r.dragged())) && sec != c.drawer_section {
            cmds.push(("cabinet.set", json!({ "id": id, "drawer_section": sec })));
        }
    }
    for (i, &hf) in c.drawers.iter().enumerate() {
        ui.horizontal(|ui| {
            let mut v = hf;
            let r = crate::expr::add(ui, &mut v, |d| d.range(100.0..=1000.0).suffix(crate::i18n::t(" mm")));
            if (r.drag_stopped() || (r.changed() && !r.dragged())) && v != hf {
                let mut ds = c.drawers.clone();
                ds[i] = v;
                cmds.push(("cabinet.set", json!({ "id": id, "drawers": ds })));
            }
            if ui.small_button(crate::i18n::t("delete")).clicked() {
                let mut ds = c.drawers.clone();
                ds.remove(i);
                cmds.push(("cabinet.set", json!({ "id": id, "drawers": ds })));
            }
        });
    }
    if ui.button(crate::i18n::t("+ Drawer")).clicked() {
        let mut ds = c.drawers.clone();
        ds.push(if ds.is_empty() { 140.0 } else { 280.0 });
        cmds.push(("cabinet.set", json!({ "id": id, "drawers": ds })));
    }
    if ui.button(crate::i18n::t("Delete cabinet")).clicked() {
        cmds.push(("cabinet.remove", json!({ "id": id })));
    }
}

/// Segment points for drawing (arcs as a polyline).
fn seg_points(s: &dowel::contour::Seg) -> Vec<[f64; 2]> {
    use dowel::contour::Seg;
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

/// Click on the contour diagram: edge under the cursor, nearby vertex and point in mm of the panel axes.
#[derive(Clone, Copy, Debug)]
pub struct ContourClick {
    pub segment: Option<usize>,
    pub vertex: Option<[f64; 2]>,
    pub point: [f64; 2],
}

/// Panel contour diagram (as in Bazis when edge banding: panel parallel to the screen). Banded edges —
/// thick line with a thickness label, edge under the cursor — orange, vertices — circles.
pub fn contour_view(ui: &mut egui::Ui, segs: &[dowel::contour::Seg], edges: &[dowel::panel::Edge], height: f32) -> Option<ContourClick> {
    use egui::{Color32, Pos2, Stroke};
    let (resp, painter) = ui.allocate_painter(egui::vec2(ui.available_width(), height), egui::Sense::click());
    let (lo, hi) = dowel::contour::bounds(segs)?;
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
        // Middle of the polyline: for a line segment — between the ends.
        let mid = if pl.len() == 2 { pl[0] + (pl[1] - pl[0]) / 2.0 } else { pl[pl.len() / 2] };
        let label = match edge {
            Some(e) => format!("{} · {}", i, e.thickness),
            None => i.to_string(),
        };
        painter.text(mid, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), Color32::from_rgb(20, 60, 160));
    }
    // Vertices: ends of line segments and arcs.
    let vertices: Vec<[f64; 2]> = segs.iter().filter(|s| !matches!(s, dowel::contour::Seg::Circle { .. })).map(|s| dowel::contour::ends(s).0).collect();
    let vertex = mouse.and_then(|m| vertices.iter().copied().filter(|&v| (to(v) - m).length() < 9.0).min_by(|a, b| (to(*a) - m).length().total_cmp(&(to(*b) - m).length())));
    for &v in &vertices {
        let hot = Some(v) == vertex;
        painter.circle(to(v), if hot { 5.0 } else { 2.5 }, if hot { Color32::from_rgb(230, 120, 0) } else { Color32::from_gray(90) }, Stroke::NONE);
    }
    if let Some(v) = vertex {
        resp.clone().on_hover_text(crate::i18n::t(format!("Vertex ({}, {})", v[0], v[1])));
    } else if let Some(i) = hovered {
        let len = dowel::contour::length(&segs[i]);
        resp.clone().on_hover_text(crate::i18n::t(format!("Edge {i}: {len:.1} mm")));
    }
    let point = mouse.map(from)?;
    resp.clicked().then_some(ContourClick { segment: hovered, vertex, point })
}
