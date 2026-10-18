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
            Modify::Move => "Сдвинуть",
            Modify::Copy => "Копировать",
            Modify::Rotate => "Повернуть",
            Modify::Mirror => "Отразить",
            Modify::MirrorCopy => "Зеркальная копия",
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
            ui.colored_label(Color32::from_rgb(0xf5, 0x9e, 0x3b), crate::i18n::t("Модель Базиса открыта для просмотра. Нажмите замок вверху, чтобы её менять."));
        } else if !has_material {
            ui.colored_label(Color32::from_rgb(0xf5, 0x9e, 0x3b), crate::i18n::t("Сначала добавьте материал на вкладке «Проект» — без него модули не создаются."));
        }
        let fill = self.fill.as_ref().map(|f| f.plane);
        let mut pick = None;
        let can_add = editable && has_material;
        group(ui, "Модули", |ui| {
            for (icon, kind, tip) in [
                (Icon::Cabinet, "cabinet", "Напольный корпус — встанет справа от модулей"),
                (Icon::WallCabinet, "wall", "Навесной корпус — над напольными"),
                (Icon::Corner, "corner", "Угловой модуль (Г-образный)"),
                (Icon::Countertop, "countertop", "Столешница над напольными корпусами"),
                (Icon::PanelFree, "panel", "Отдельная панель — размеры и положение в «Свойствах»"),
            ] {
                if tool(ui, can_add, icon, tip, false).clicked() {
                    pick = Some(Pick::Add(kind));
                }
            }
        });
        group(ui, "Панели и фасады в проём", |ui| {
            for (icon, plane, tip) in [
                (Icon::PanelH, "horizontal", "Полка: щелчок в проём на виде спереди"),
                (Icon::PanelV, "vertical", "Перегородка: щелчок в проём на виде спереди"),
                (Icon::PanelF, "front", "Фронтальная панель (задняя стенка, фальшпанель) в проём"),
                (Icon::Door, "door", "Дверь в проём, с петлями и присадкой"),
                (Icon::Drawers, "drawers", "Ящики в проём, с направляющими"),
            ] {
                if tool(ui, can_add, icon, tip, fill == Some(plane)).clicked() {
                    pick = Some(Pick::Fill(plane));
                }
            }
        });
        group(ui, "Фурнитура", |ui| {
            if tool(ui, true, Icon::Furniture, "Библиотека фурнитуры (.fr3d): корзины, петли, направляющие", false).clicked() {
                pick = Some(Pick::Library);
            }
            if tool(ui, editable, Icon::Fastener, "Крепёж: выбрать вид (конфирмат, эксцентрик, шкант…) и поставить на выделенный корпус или панели", self.fastening).clicked() {
                pick = Some(Pick::Fasteners);
            }
        });
        group(ui, "Изменить выделенное", |ui| {
            for (icon, m, tip) in [
                (Icon::Move, Modify::Move, "Сдвинуть на dX, dY, dZ (или тащить мышью на виде спереди)"),
                (Icon::Copy, Modify::Copy, "Копировать со сдвигом, массивом"),
                (Icon::Rotate, Modify::Rotate, "Повернуть вокруг оси через центр выделенного"),
                (Icon::Mirror, Modify::Mirror, "Отразить"),
                (Icon::MirrorCopy, Modify::MirrorCopy, "Зеркальная копия"),
            ] {
                if tool(ui, editable && selected, icon, tip, self.modify == Some(m)).clicked() {
                    pick = Some(Pick::Modify(m));
                }
            }
            if tool(ui, editable && selected, Icon::Block, "Собрать в блок", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Block));
            }
            if tool(ui, editable && explodable, Icon::Explode, "Разрушить: корпус, угловой модуль, столешницу или блок — в отдельные панели", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Explode));
            }
            if tool(ui, editable && deletable, Icon::Erase, "Удалить (Del)", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Delete));
            }
        });
        group(ui, "Проверка и скрипты", |ui| {
            if tool(ui, true, Icon::Analyze, "Анализ модели: пересечения, крепёж мимо панели, деталь больше листа", false).clicked() {
                pick = Some(Pick::Analyze);
            }
            if tool(ui, editable, Icon::Script, "Выполнить скрипт Базиса (.js)", false).clicked() {
                pick = Some(Pick::Script);
            }
        });
        match pick {
            Some(Pick::Add(kind)) => self.add_object(kind),
            Some(Pick::Fill(plane)) => self.toggle_fill(plane),
            Some(Pick::Modify(m)) => {
                self.fill = None;
                self.fastening = false;
                self.modify = (self.modify != Some(m)).then_some(m);
            }
            Some(Pick::Menu(a)) => self.menu_apply(a),
            Some(Pick::Library) => self.show_pane(Pane::Library),
            Some(Pick::Fasteners) => {
                (self.fill, self.modify) = (None, None);
                self.fastening = !self.fastening;
            }
            Some(Pick::Analyze) => self.analyze(),
            Some(Pick::Script) => self.pick_script(),
            None => {}
        }
        // Parameters of the active tool.
        if self.fill.is_some() || self.modify.is_some() || self.fastening {
            ui.separator();
            if self.fill.is_some() {
                self.fill_params(ui);
            } else if self.modify.is_some() {
                self.modify_params(ui);
            } else {
                self.fastener_params(ui);
            }
        }
    }

    /// Toggle the "into opening" tool and show the front view, where it is clicked.
    pub(crate) fn toggle_fill(&mut self, plane: &'static str) {
        self.modify = None;
        self.fastening = false;
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
                ("cabinet.add", json!({ "name": "Навесной", "width": 600, "height": 720, "depth": 300, "material": material, "plinth": 0, "edge": edge, "doors": 1, "position": [x, y, 0] }))
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
            "horizontal" => "Полка в проём",
            "vertical" => "Перегородка в проём",
            "front" => "Фронтальная панель в проём",
            "door" => "Дверь в проём",
            _ => "Ящики в проём",
        };
        ui.strong(crate::i18n::t(title));
        crate::editors::props(ui, "fill_params", |ui| {
            crate::editors::caption(ui, "Материал");
            if let Some(Some(m)) = crate::editors::choose(ui, "fill_mat", Some(&t.material), &names, false) {
                t.material = m;
            }
            ui.end_row();
            if matches!(t.plane, "door" | "drawers") {
                crate::editors::caption(ui, "Установка");
                ui.horizontal_wrapped(|ui| {
                    for (m, label) in [("overlay", "накладная"), ("half", "полунакладная"), ("inset", "вкладная")] {
                        ui.selectable_value(&mut t.mount, m, crate::i18n::t(label));
                    }
                });
                ui.end_row();
                if t.plane == "door" {
                    crate::editors::caption(ui, "Створки");
                    ui.checkbox(&mut t.double, crate::i18n::t("двойная"));
                } else {
                    crate::editors::caption(ui, "Фасады, мм");
                    ui.add(egui::TextEdit::singleline(&mut t.heights).desired_width(120.0)).on_hover_text(crate::i18n::t("Высоты фасадов снизу вверх через пробел"));
                }
                ui.end_row();
                return;
            }
            let labels = match t.plane {
                "horizontal" => ["Отступ слева", "Отступ справа", "Отступ сзади", "Отступ спереди"],
                "vertical" => ["Отступ сзади", "Отступ спереди", "Отступ снизу", "Отступ сверху"],
                _ => ["Отступ слева", "Отступ справа", "Отступ снизу", "Отступ сверху"],
            };
            for (g, l) in t.gaps.iter_mut().zip(labels) {
                crate::editors::caption(ui, l);
                crate::expr::add(ui, g, |d| d.range(-100.0..=500.0).suffix(crate::i18n::t(" мм")));
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
            ui.weak(crate::i18n::t("Выделите детали щелчком — потом «Применить»."));
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
                        crate::expr::add(ui, &mut d[k], |d| d.speed(1.0).suffix(crate::i18n::t(" мм")));
                        ui.end_row();
                    }
                    if m == Modify::Copy {
                        crate::editors::caption(ui, "Копий");
                        crate::expr::add(ui, count, |d| d.range(1..=100));
                        ui.end_row();
                    }
                }
                Modify::Rotate | Modify::Mirror | Modify::MirrorCopy => {
                    crate::editors::caption(ui, if m == Modify::Rotate { "Ось" } else { "Поперёк оси" });
                    ui.horizontal(|ui| {
                        for a in ['x', 'y', 'z'] {
                            ui.selectable_value(axis, a, a.to_ascii_uppercase().to_string());
                        }
                    });
                    ui.end_row();
                    if m == Modify::Rotate {
                        crate::editors::caption(ui, "Угол");
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
        let apply = ui.button(crate::i18n::t("Применить (Enter)")).clicked() || (!ui.ctx().text_edit_focused() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
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
        ui.strong(crate::i18n::t("Крепёж"));
        let names = crate::editors::fastener_names(&project);
        crate::editors::props(ui, "fastener_params", |ui| {
            crate::editors::caption(ui, "Тип");
            if let Some(Some(v)) = crate::editors::choose(ui, "fast_tool_kind", Some(&self.fastener_kind), &names, false) {
                self.fastener_kind = v;
            }
            ui.end_row();
            crate::editors::caption(ui, "Чашка эксцентрика");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.fastener_outside, false, crate::i18n::t("внутри"));
                ui.selectable_value(&mut self.fastener_outside, true, crate::i18n::t("снаружи"));
            })
            .response
            .on_hover_text(crate::i18n::t("С какой стороны панели сверлится чашка ⌀15: внутри корпуса (у дна — сверху, у крыши — снизу) или снаружи. У отдельных панелей; «Поставить» меняет и уже стоящий крепёж"));
            ui.end_row();
        });
        ui.weak(crate::i18n::t("Выделите корпус (щелчок по любой его детали) или несколько отдельных панелей (Ctrl+щелчок) и нажмите «Поставить». Отверстия с координатами появятся на обеих деталях каждого стыка."));
        let ids = self.targets();
        let go = ui.add_enabled(!ids.is_empty(), egui::Button::new(crate::i18n::t("Поставить на выделенное"))).clicked();
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
        self.status = crate::i18n::t(format!("Крепёж «{kind}»: корпусов {modules}, стыков панелей {joints}"));
        if modules == 0 && joints == 0 && self.error.is_none() {
            self.error = Some(crate::i18n::t("Стыков не найдено: панели должны касаться — торец одной на пласти другой."));
        }
    }

    /// Status bar hint: what to do next.
    pub(crate) fn hint(&self) -> String {
        if self.fastening {
            return "Крепёж: выделите корпус или панели (Ctrl — несколько) и нажмите «Поставить на выделенное». Esc — выйти.".into();
        }
        if let Some(t) = &self.fill {
            let what = match t.plane {
                "horizontal" => "полку",
                "vertical" => "перегородку",
                "front" => "фронтальную панель",
                "door" => "дверь",
                _ => "ящики",
            };
            return format!("Щёлкните внутри проёма на виде спереди — поставить {what}. Esc — выйти из инструмента.");
        }
        if let Some(m) = self.modify {
            return if self.targets().is_empty() {
                format!("{}: выделите детали щелчком (Ctrl — несколько). Esc — выйти.", m.title())
            } else {
                format!("{}: задайте параметры слева и нажмите «Применить» или Enter. Esc — выйти.", m.title())
            };
        }
        match &self.session {
            None => "Модель Базиса открыта для просмотра. Замок вверху — сделать её редактируемой.".into(),
            Some(s) if s.project.materials.is_empty() => "Начните с материала: вкладка «Проект» → «Материалы».".into(),
            Some(_) if self.selection.is_empty() && self.furniture_sel.is_none() => {
                "Выберите инструмент слева или щёлкните деталь. ЛКМ в 3D — вращать, ПКМ — двигать вид, колесо — масштаб.".into()
            }
            Some(_) => "Тяните выделенную деталь мышью — она двигается по оси. Свойства — справа. Del — удалить, ПКМ — меню, Esc — снять выделение.".into(),
        }
    }
}
