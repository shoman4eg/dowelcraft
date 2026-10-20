//! The "Tools" palette (like Tool Sets in cadcraft): icons by group, below them the parameters
//! of the active tool. The hint on what to do next is in the status bar (`App::hint`).

use dowel::design;
use eframe::egui::{self, Color32};
use serde_json::{Value, json};

use crate::icons::{Icon, tool};
use crate::{App, FillTool, MenuAct, Pane};


/// Tool that modifies the selection: parameters below the palette, Enter applies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modify {
    Move,
    Copy,
    Rotate,
    Mirror,
    MirrorCopy,
}

impl Modify {
    fn title(self) -> &'static str {
        match self {
            Modify::Move => "Move",
            Modify::Copy => "Copy",
            Modify::Rotate => "Rotate",
            Modify::Mirror => "Mirror",
            Modify::MirrorCopy => "Mirror copy",
        }
    }
}

/// What was picked in the palette this frame.
enum Pick {
    Add(&'static str),
    Fill(&'static str),
    Modify(Modify),
    Menu(MenuAct),
    Library,
    Fasteners,
    Analyze,
    Script,
    Ruler,
    Guides,
}

/// Palette group: a header and icons in a wrapping row.
fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    // No content indent: three labelled buttons fit in a row.
    egui::CollapsingHeader::new(egui::RichText::new(crate::i18n::t(title)).strong()).default_open(true).show_unindented(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            body(ui);
        });
    });
}

impl App {
    pub(crate) fn tools_panel(&mut self, ui: &mut egui::Ui) {
        let editable = self.session.is_some();
        let has_material = self.session.as_ref().is_some_and(|s| !s.project.materials.is_empty());
        let selected = !self.targets().is_empty();
        let deletable = !self.delete_targets().is_empty();
        let explodable = self.targets().iter().any(|&id| self.explode_cmd(id).is_some());
        if !editable {
            ui.colored_label(Color32::from_rgb(0xf5, 0x9e, 0x3b), crate::i18n::t("The Bazis model is read-only. Press the lock at the top to edit it."));
        } else if !has_material {
            ui.colored_label(Color32::from_rgb(0xf5, 0x9e, 0x3b), crate::i18n::t("First add a material in the “Project” panel — modules need it."));
        }
        let fill = self.fill.as_ref().map(|f| f.plane);
        let mut pick = None;
        let can_add = editable && has_material;
        group(ui, "Modules", |ui| {
            for (icon, kind, tip) in [
                (Icon::Cabinet, "cabinet", "Floor cabinet — placed to the right of the modules"),
                (Icon::WallCabinet, "wall", "Wall cabinet — above the floor ones"),
                (Icon::Corner, "corner", "Corner unit (L-shaped)"),
                (Icon::Countertop, "countertop", "Countertop over the floor cabinets"),
                (Icon::PanelFree, "panel", "Free panel — size and position in “Properties”"),
            ] {
                if tool(ui, can_add, icon, tip, false).clicked() {
                    pick = Some(Pick::Add(kind));
                }
            }
        });
        group(ui, "Panels and fronts into an opening", |ui| {
            for (icon, plane, tip) in [
                (Icon::PanelH, "horizontal", "Shelf: click into an opening in the front view"),
                (Icon::PanelV, "vertical", "Partition: click into an opening in the front view"),
                (Icon::PanelF, "front", "Front panel (back, false panel) into an opening"),
                (Icon::Door, "door", "Door into an opening, with hinges and drilling"),
                (Icon::Drawers, "drawers", "Drawers into an opening, with runners"),
            ] {
                if tool(ui, can_add, icon, tip, fill == Some(plane)).clicked() {
                    pick = Some(Pick::Fill(plane));
                }
            }
        });
        group(ui, "Hardware", |ui| {
            if tool(ui, true, Icon::Furniture, "Hardware library (.fr3d): baskets, hinges, runners", false).clicked() {
                pick = Some(Pick::Library);
            }
            if tool(ui, editable, Icon::Fastener, "Fasteners: choose a type (confirmat, cam lock, dowel…) and apply to the selected cabinet or panels", self.fastening).clicked() {
                pick = Some(Pick::Fasteners);
            }
        });
        group(ui, "Modify selection", |ui| {
            for (icon, m, tip) in [
                (Icon::Move, Modify::Move, "Move by dX, dY, dZ (or drag in the front view)"),
                (Icon::Copy, Modify::Copy, "Copy with offset, as an array"),
                (Icon::Rotate, Modify::Rotate, "Rotate about an axis through the selection centre"),
                (Icon::Mirror, Modify::Mirror, "Mirror"),
                (Icon::MirrorCopy, Modify::MirrorCopy, "Mirror copy"),
            ] {
                if tool(ui, editable && selected, icon, tip, self.modify == Some(m)).clicked() {
                    pick = Some(Pick::Modify(m));
                }
            }
            if tool(ui, editable && selected, Icon::Block, "Group into block", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Block));
            }
            if tool(ui, editable && explodable, Icon::Explode, "Explode a cabinet, corner unit, countertop or block into separate panels", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Explode));
            }
            if tool(ui, editable && deletable, Icon::Erase, "Delete (Del)", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Delete));
            }
        });
        group(ui, "Checks and scripts", |ui| {
            if tool(ui, true, Icon::Analyze, "Model analysis: intersections, fasteners off panels, parts larger than the sheet", false).clicked() {
                pick = Some(Pick::Analyze);
            }
            if tool(ui, editable, Icon::Script, "Run a Bazis script (.js)", false).clicked() {
                pick = Some(Pick::Script);
            }
            if tool(ui, true, Icon::Ruler, "Ruler (M): click two points for each measurement — part corners and contour points snap; they stay until you leave (Esc or M)", self.ruler.is_some()).clicked() {
                pick = Some(Pick::Ruler);
            }
            if tool(ui, editable, Icon::Guides, "Guide lines (G): click in the front view — a vertical one, with Shift — a horizontal one; drag a line to move it. Parts snap to them", self.guiding.is_some()).clicked() {
                pick = Some(Pick::Guides);
            }
        });
        match pick {
            Some(Pick::Add(kind)) => self.add_object(kind),
            Some(Pick::Fill(plane)) => self.toggle_fill(plane),
            Some(Pick::Modify(m)) => {
                self.fill = None;
                self.fastening = false;
                self.ruler = None;
                self.guiding = None;
                self.modify = (self.modify != Some(m)).then_some(m);
            }
            Some(Pick::Menu(a)) => self.menu_apply(a),
            Some(Pick::Library) => self.show_pane(Pane::Library),
            Some(Pick::Fasteners) => {
                (self.fill, self.modify, self.ruler, self.guiding) = (None, None, None, None);
                self.fastening = !self.fastening;
            }
            Some(Pick::Analyze) => self.analyze(),
            Some(Pick::Script) => self.pick_script(),
            Some(Pick::Ruler) => self.toggle_ruler(),
            Some(Pick::Guides) => self.toggle_guides(),
            None => {}
        }
        // Parameters of the active tool.
        if self.fill.is_some() || self.modify.is_some() || self.fastening || self.guiding.is_some() {
            ui.separator();
            if self.guiding.is_some() {
                self.guide_params(ui);
            } else if self.fill.is_some() {
                self.fill_params(ui);
            } else if self.modify.is_some() {
                self.modify_params(ui);
            } else {
                self.fastener_params(ui);
            }
        }
    }

    /// The ruler on or off; the other tools step aside.
    pub(crate) fn toggle_ruler(&mut self) {
        (self.fill, self.modify, self.fastening, self.guiding) = (None, None, false, None);
        self.ruler = if self.ruler.is_some() { None } else { Some(crate::viewport::Ruler::default()) };
    }

    /// The guide line tool on or off (in the front view, where it is clicked); the other tools step aside.
    pub(crate) fn toggle_guides(&mut self) {
        (self.fill, self.modify, self.fastening, self.ruler) = (None, None, false, None);
        self.guiding = if self.guiding.is_some() || self.session.is_none() { None } else { Some(crate::GuideTool::default()) };
        if self.guiding.is_some() {
            self.show_pane(Pane::Front);
        }
    }

    /// The guide line tool's form: a line by number along any axis (Z too) and the list with removal.
    fn guide_params(&mut self, ui: &mut egui::Ui) {
        let Some(mut t) = self.guiding else { return };
        ui.strong(crate::i18n::t("Guide lines"));
        ui.horizontal(|ui| {
            for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                ui.radio_value(&mut t.axis, axis, name);
            }
            ui.add(egui::DragValue::new(&mut t.at).speed(1.0).suffix(crate::i18n::t(" mm")));
            if ui.button(crate::i18n::t("Add")).clicked() {
                self.exec("guide.add", json!({ "axis": (["x", "y", "z"][t.axis]), "at": t.at }));
            }
        });
        self.guiding = Some(t);
        let guides = self.session.as_ref().map(|s| s.project.guides.clone()).unwrap_or_default();
        let mut remove = None;
        for (i, g) in guides.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{} {}", ["X", "Y", "Z"][g.axis], crate::num(g.at)));
                if ui.small_button("✕").on_hover_text(crate::i18n::t("Remove the guide line")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.exec("guide.remove", json!({ "index": i }));
        }
        if !guides.is_empty() && ui.button(crate::i18n::t("Remove all guide lines")).clicked() {
            self.exec("guide.clear", json!({}));
        }
    }

    /// Toggle the "into opening" tool and show the front view, where it is clicked.
    pub(crate) fn toggle_fill(&mut self, plane: &'static str) {
        self.modify = None;
        self.fastening = false;
        self.ruler = None;
        self.guiding = None;
        if self.fill.as_ref().is_some_and(|f| f.plane == plane) {
            self.fill = None;
            return;
        }
        let material = match (&self.fill, self.session.as_ref().and_then(|s| s.project.materials.first())) {
            (Some(f), _) => f.material.clone(),
            (None, Some(m)) => m.name.clone(),
            (None, None) => return,
        };
        let prev = self.fill.take();
        self.fill = Some(FillTool {
            plane,
            material,
            gaps: prev.as_ref().map_or([0.0; 4], |f| f.gaps),
            mount: prev.as_ref().map_or("overlay", |f| f.mount),
            double: prev.as_ref().is_some_and(|f| f.double),
            heights: prev.map_or_else(|| "140 280 280".into(), |f| f.heights),
        });
        self.show_pane(Pane::Front);
    }

    /// New module or panel with default dimensions; placed to the right of the modules and selected at once.
    pub(crate) fn add_object(&mut self, kind: &str) {
        let Some(s) = &self.session else { return };
        let project = &s.project;
        let Some(material) = project.materials.first().map(|m| m.name.clone()) else { return };
        let edge = project.edges.first().map(|e| e.name.clone());
        let floor: Vec<&design::Cabinet> = project.cabinets.iter().filter(|c| c.position[1] <= 0.0).collect();
        // To the right of everything already there (cabinets, panels, furniture) with a 50 mm gap, otherwise the new one lands on top.
        let right = self.free_x();
        let (cmd, args) = match kind {
            "cabinet" => ("cabinet.add", json!({ "width": 600, "height": 820, "depth": 560, "material": material, "plinth": 100, "edge": edge, "doors": 1, "position": [right, 0, 0] })),
            "wall" => {
                // 600 mm above the countertop (or floor units); without them, 1400 mm above the floor.
                let top = project.countertops.iter().map(|c| c.position[1] + project.material(&c.material).map_or(38.0, |m| m.thickness)).chain(floor.iter().map(|c| c.height)).fold(0.0, f64::max);
                let y = if top > 0.0 { top + 600.0 } else { 1400.0 };
                let x = project.cabinets.iter().filter(|c| c.position[1] > 0.0).map(|c| c.position[0] + c.width).fold(0.0, f64::max);
                ("cabinet.add", json!({ "name": "Wall", "width": 600, "height": 720, "depth": 300, "material": material, "plinth": 0, "edge": edge, "doors": 1, "position": [x, y, 0] }))
            }
            "corner" => ("corner.add", json!({ "width_x": 1000, "width_z": 1000, "height": 820, "depth": 560, "material": material, "plinth": 100, "doors": true, "edge": edge, "position": [right, 0, 0] })),
            "countertop" => {
                let x0 = floor.iter().map(|c| c.position[0]).fold(f64::MAX, f64::min).min(0.0);
                let x1 = floor.iter().map(|c| c.position[0] + c.width).fold(600.0, f64::max);
                let top = floor.iter().map(|c| c.height).fold(720.0, f64::max);
                let depth = floor.iter().map(|c| c.depth).fold(560.0, f64::max) + 20.0;
                ("countertop.add", json!({ "material": material, "length": x1 - x0, "depth": depth, "position": [x0, top, 0], "edge": edge }))
            }
            _ => {
                let (plane, a, b) = self.new_panel;
                ("panel.add", json!({ "material": material, "plane": plane, "size": [a, b], "position": [right, 0, 0] }))
            }
        };
        self.fill = None;
        self.modify = None;
        if let Some(id) = self.exec_ret(cmd, args).and_then(|v| v.get("id").and_then(Value::as_u64)) {
            self.select_object(id as u32);
        }
    }

    /// X from which a new object won't overlap existing ones: right edge of all parts and furniture + 50 mm.
    pub(crate) fn free_x(&self) -> f64 {
        let parts = self.parts.as_ref().map(|p| p.iter().map(|p| p.origin[0] + p.size[0]).fold(f64::MIN, f64::max)).unwrap_or(f64::MIN);
        let furniture = self.session.as_ref().map_or(f64::MIN, |s| design::furniture(&s.project).iter().map(|(f, fr)| (0..8).map(|c| fr.apply([if c & 1 == 0 { f.lo[0] } else { f.hi[0] }, if c & 2 == 0 { f.lo[1] } else { f.hi[1] }, if c & 4 == 0 { f.lo[2] } else { f.hi[2] }])[0]).fold(f64::MIN, f64::max)).fold(f64::MIN, f64::max));
        let right = parts.max(furniture);
        if right == f64::MIN { 0.0 } else { (right + 50.0).round() }
    }

    /// Select an object by number: its first part.
    pub(crate) fn select_object(&mut self, id: u32) {
        let Ok(parts) = &self.parts else { return };
        if let Some(i) = parts.iter().position(|p| p.owner == id || p.id == id) {
            self.selection.clear();
            self.furniture_sel = None;
            self.select(Some(i));
        }
    }

    /// "Into opening" parameters: material, gaps or front type.
    fn fill_params(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.session else { return };
        let names: Vec<String> = s.project.materials.iter().map(|m| m.name.clone()).collect();
        let Some(t) = &mut self.fill else { return };
        let title = match t.plane {
            "horizontal" => "Shelf into an opening",
            "vertical" => "Partition into an opening",
            "front" => "Front panel into an opening",
            "door" => "Door into an opening",
            _ => "Drawers into an opening",
        };
        ui.strong(crate::i18n::t(title));
        crate::editors::props(ui, "fill_params", |ui| {
            crate::editors::caption(ui, "Material");
            if let Some(Some(m)) = crate::editors::choose(ui, "fill_mat", Some(&t.material), &names, false) {
                t.material = m;
            }
            ui.end_row();
            if matches!(t.plane, "door" | "drawers") {
                crate::editors::caption(ui, "Mounting");
                ui.horizontal_wrapped(|ui| {
                    for (m, label) in [("overlay", "overlay"), ("half", "half overlay"), ("inset", "inset")] {
                        ui.selectable_value(&mut t.mount, m, crate::i18n::t(label));
                    }
                });
                ui.end_row();
                if t.plane == "door" {
                    crate::editors::caption(ui, "Leaves");
                    ui.checkbox(&mut t.double, crate::i18n::t("double"));
                } else {
                    crate::editors::caption(ui, "Fronts, mm");
                    ui.add(egui::TextEdit::singleline(&mut t.heights).desired_width(120.0)).on_hover_text(crate::i18n::t("Front heights from the bottom up, separated by spaces; 0 or “auto” — the auto fronts share the rest of the opening"));
                }
                ui.end_row();
                return;
            }
            let labels = match t.plane {
                "horizontal" => ["Gap left", "Gap right", "Gap back", "Gap front"],
                "vertical" => ["Gap back", "Gap front", "Gap bottom", "Gap top"],
                _ => ["Gap left", "Gap right", "Gap bottom", "Gap top"],
            };
            for (g, l) in t.gaps.iter_mut().zip(labels) {
                crate::editors::caption(ui, l);
                crate::expr::add(ui, g, |d| d.range(-100.0..=500.0).suffix(crate::i18n::t(" mm")));
                ui.end_row();
            }
        });
    }

    /// Parameters for move, copy, rotate, mirror and the "Apply" button (or Enter).
    fn modify_params(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.modify else { return };
        ui.strong(crate::i18n::t(m.title()));
        let ids = self.targets();
        if ids.is_empty() {
            ui.weak(crate::i18n::t("Select parts by clicking, then “Apply”."));
            return;
        }
        let Some((lo, hi)) = self.selection_bounds() else { return };
        let center: [f64; 3] = std::array::from_fn(|k| ((lo[k] + hi[k]) / 2.0).round());
        let (d, axis, angle, count) = &mut self.actions;
        crate::editors::props(ui, "modify_params", |ui| {
            match m {
                Modify::Move | Modify::Copy => {
                    for (k, l) in ["dX", "dY", "dZ"].iter().enumerate() {
                        crate::editors::caption(ui, l);
                        crate::expr::add(ui, &mut d[k], |d| d.speed(1.0).suffix(crate::i18n::t(" mm")));
                        ui.end_row();
                    }
                    if m == Modify::Copy {
                        crate::editors::caption(ui, "Copies");
                        crate::expr::add(ui, count, |d| d.range(1..=100));
                        ui.end_row();
                    }
                }
                Modify::Rotate | Modify::Mirror | Modify::MirrorCopy => {
                    crate::editors::caption(ui, if m == Modify::Rotate { "Axis" } else { "Across axis" });
                    ui.horizontal(|ui| {
                        for a in ['x', 'y', 'z'] {
                            ui.selectable_value(axis, a, a.to_ascii_uppercase().to_string());
                        }
                    });
                    ui.end_row();
                    if m == Modify::Rotate {
                        crate::editors::caption(ui, "Angle");
                        crate::expr::add(ui, angle, |d| d.range(-360.0..=360.0).suffix("°"));
                        ui.end_row();
                    }
                }
            }
        });
        let k = match *axis {
            'x' => 0,
            'y' => 1,
            _ => 2,
        };
        let apply = ui.button(crate::i18n::t("Apply (Enter)")).clicked() || (!ui.ctx().text_edit_focused() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        if !apply {
            return;
        }
        let (cmd, args) = match m {
            Modify::Move => ("object.move", json!({ "ids": ids, "d": *d })),
            Modify::Copy => ("object.copy", json!({ "ids": ids, "d": *d, "count": *count })),
            Modify::Rotate => ("object.rotate", json!({ "ids": ids, "axis": axis.to_string(), "angle": *angle, "center": center })),
            Modify::Mirror => ("object.mirror", json!({ "ids": ids, "axis": axis.to_string(), "at": center[k] })),
            Modify::MirrorCopy => ("object.mirror", json!({ "ids": ids, "axis": axis.to_string(), "at": hi[k], "copy": true })),
        };
        self.exec(cmd, args);
    }

    /// Fasteners: type from the catalog, a description and "Place on selection".
    /// A cabinet or corner module gets fasteners as a whole (all joints), individual panels on their own joints.
    fn fastener_params(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.session else { return };
        let project = s.project.clone();
        ui.strong(crate::i18n::t("Fasteners"));
        let names = crate::editors::fastener_names(&project);
        let mut cmds_def: Option<dowel::fasteners::Def> = None;
        crate::editors::props(ui, "fastener_params", |ui| {
            crate::editors::caption(ui, "Type");
            if let Some(Some(v)) = crate::editors::choose(ui, "fast_tool_kind", Some(&self.fastener_kind), &names, false) {
                self.fastener_kind = v;
            }
            ui.end_row();
            crate::editors::caption(ui, "Cam cup");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.fastener_outside, false, crate::i18n::t("inside"));
                ui.selectable_value(&mut self.fastener_outside, true, crate::i18n::t("outside"));
            })
            .response
            .on_hover_text(crate::i18n::t("Which face gets the ⌀15 cup: inside the carcass (top of the bottom, underside of the top) or outside. For free panels; “Apply” also changes fasteners already there"));
            ui.end_row();
            // The chosen catalogue entry: the 32 mm system and spare holes (shelf supports); a change rewrites the entry in the project.
            if let Ok(def) = project.def(&self.fastener_kind) {
                let mut changed = def.clone();
                crate::editors::caption(ui, "32 mm system").on_hover_text(crate::i18n::t("Spacing between points is a multiple of 32 mm and the points are centred on the joint, as drilling heads need"));
                let mut on = def.grid > 0.0;
                if ui.checkbox(&mut on, "").changed() {
                    changed.grid = if on { dowel::fasteners::GRID } else { 0.0 };
                }
                ui.end_row();
                // Spare holes make sense for a fastener drilled into the upright only (a shelf support).
                let single = def.holes.len() == 1 && def.holes[0].panel == 'a';
                if changed.grid > 0.0 && single {
                    crate::editors::caption(ui, "Spare holes").on_hover_text(crate::i18n::t("Extra holes 32 mm apart under and over each shelf support, so the shelf can be moved"));
                    ui.horizontal(|ui| {
                        for k in 0..2 {
                            let mut n = changed.rows[k];
                            let r = crate::expr::add(ui, &mut n, |d| d.range(0..=10).fixed_decimals(0).prefix(crate::i18n::t(if k == 0 { "below " } else { "above " })));
                            if (r.drag_stopped() || (r.changed() && !r.dragged())) && n != def.rows[k] {
                                changed.rows[k] = n;
                            }
                        }
                    });
                    ui.end_row();
                }
                crate::editors::caption(ui, "Price per piece");
                if def.items.is_empty() {
                    let mut price = changed.price;
                    let r = crate::expr::add(ui, &mut price, |d| d.range(0.0..=1e6).speed(0.1));
                    if (r.drag_stopped() || (r.changed() && !r.dragged())) && price != def.price {
                        changed.price = price;
                    }
                } else {
                    // A composite fastener is bought as its parts: the estimate prices those.
                    ui.weak(crate::i18n::t("by its parts"));
                }
                ui.end_row();
                if changed != def {
                    cmds_def = Some(changed);
                }
            }
        });
        if let Some(d) = cmds_def.take() {
            self.exec("fastener.def", serde_json::to_value(d).unwrap_or_default());
        }
        ui.weak(crate::i18n::t("Select a cabinet (click any of its parts) or several free panels (Ctrl+click) and press “Apply”. Holes with coordinates appear on both parts of every joint."));
        let ids = self.targets();
        let go = ui.add_enabled(!ids.is_empty(), egui::Button::new(crate::i18n::t("Apply to selection"))).clicked();
        if !go {
            return;
        }
        let kind = self.fastener_kind.clone();
        let mut panels = Vec::new();
        let mut modules = 0;
        for id in ids {
            if project.cabinets.iter().any(|c| c.id == id) {
                self.exec("cabinet.set", json!({ "id": id, "fastener": kind }));
                modules += 1;
            } else if project.corners.iter().any(|c| c.id == id) {
                self.exec("corner.set", json!({ "id": id, "fastener": kind }));
                modules += 1;
            } else {
                // Panel or block: all individual panels inside.
                let Ok(parts) = &self.parts else { continue };
                panels.extend(parts.iter().filter(|p| p.id != 0 && (p.id == id || p.owner == id)).map(|p| p.id));
            }
        }
        panels.sort_unstable();
        panels.dedup();
        let mut joints = 0;
        if panels.len() >= 2
            && let Some(r) = self.exec_ret("fastener.auto", json!({ "kind": kind, "ids": panels, "outside": self.fastener_outside }))
        {
            joints = r.get("ids").and_then(Value::as_array).map_or(0, Vec::len);
        }
        // Fasteners already placed between the selected panels go to the chosen side.
        let stale: Vec<u32> = self.session.as_ref().map_or(Vec::new(), |s| {
            s.project.fasteners.iter().filter(|f| panels.contains(&f.a) && panels.contains(&f.b) && f.outside != self.fastener_outside).map(|f| f.id).collect()
        });
        for id in stale {
            self.exec("fastener.set", json!({ "id": id, "outside": self.fastener_outside }));
        }
        self.status = crate::i18n::t(format!("“{kind}” fasteners: cabinets {modules}, panel joints {joints}"));
        if modules == 0 && joints == 0 && self.error.is_none() {
            self.error = Some(crate::i18n::t("No joints found: panels must touch — the edge of one on the face of the other."));
        }
    }

    /// Status bar hint: what to do next.
    pub(crate) fn hint(&self) -> String {
        if self.guiding.is_some() {
            return "Guide lines: click in the front view — a vertical line, with Shift — a horizontal one; drag a line to move it. A Z line — by number on the left. Esc exits.".into();
        }
        if self.fastening {
            return "Fasteners: select a cabinet or panels (Ctrl for several) and press “Apply to selection”. Esc exits.".into();
        }
        if let Some(t) = &self.fill {
            let what = match t.plane {
                "horizontal" => "a shelf",
                "vertical" => "a partition",
                "front" => "a front panel",
                "door" => "a door",
                _ => "drawers",
            };
            return format!("Click inside an opening in the front view to place {what}. Esc exits the tool.");
        }
        if let Some(m) = self.modify {
            return if self.targets().is_empty() {
                format!("{}: select parts by clicking (Ctrl for several). Esc exits.", m.title())
            } else {
                format!("{}: set the parameters on the left and press “Apply” or Enter. Esc exits.", m.title())
            };
        }
        match &self.session {
            None => "The Bazis model is read-only. The lock at the top makes it editable.".into(),
            Some(s) if s.project.materials.is_empty() => "Start with a material: “Project” panel → “Materials”.".into(),
            Some(_) if self.selection.is_empty() && self.furniture_sel.is_none() => {
                "Pick a tool on the left or click a part. In 3D: left drag rotates, right drag pans, wheel zooms.".into()
            }
            Some(_) => "Drag the selected part with the mouse to move it along an axis. Properties are on the right. Del deletes, right click opens the menu, Esc clears the selection.".into(),
        }
    }
}
