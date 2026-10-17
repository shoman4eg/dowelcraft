//! Палитра «Инструменты» (как Tool Sets в cadcraft): иконки по группам, под ними — параметры
//! включённого инструмента. Подсказка, что делать дальше, — в строке состояния (`App::hint`).

use basis::design;
use eframe::egui::{self, Color32};
use serde_json::{Value, json};

use crate::icons::{Icon, button, button_if};
use crate::{App, FillTool, MenuAct, Pane};

/// Размер иконки в палитре, точки.
const BIG: f32 = 36.0;

/// Инструмент изменения выделенного: параметры — под палитрой, «Применить» — Enter.
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

/// Что выбрано в палитре на этом кадре.
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

/// Группа палитры: заголовок и иконки в строку с переносом.
fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(egui::RichText::new(crate::i18n::t(title)).strong()).default_open(true).show(ui, |ui| {
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
                if button_if(ui, can_add, icon, BIG, tip, false).clicked() {
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
                if button_if(ui, can_add, icon, BIG, tip, fill == Some(plane)).clicked() {
                    pick = Some(Pick::Fill(plane));
                }
            }
        });
        group(ui, "Фурнитура", |ui| {
            if button(ui, Icon::Furniture, BIG, "Библиотека фурнитуры (.fr3d): корзины, петли, направляющие", false).clicked() {
                pick = Some(Pick::Library);
            }
            let tip = format!("Крепёж «{}» на все стыки выделенных панелей (вид крепежа — в «Свойствах» панели)", self.fastener_kind);
            if button_if(ui, editable && selected, Icon::Fastener, BIG, &tip, false).clicked() {
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
                if button_if(ui, editable && selected, icon, BIG, tip, self.modify == Some(m)).clicked() {
                    pick = Some(Pick::Modify(m));
                }
            }
            if button_if(ui, editable && selected, Icon::Block, BIG, "Собрать в блок", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Block));
            }
            if button_if(ui, editable && explodable, Icon::Explode, BIG, "Разрушить: корпус, угловой модуль, столешницу или блок — в отдельные панели", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Explode));
            }
            if button_if(ui, editable && deletable, Icon::Erase, BIG, "Удалить (Del)", false).clicked() {
                pick = Some(Pick::Menu(MenuAct::Delete));
            }
        });
        group(ui, "Проверка и скрипты", |ui| {
            if button(ui, Icon::Analyze, BIG, "Анализ модели: пересечения, крепёж мимо панели, деталь больше листа", false).clicked() {
                pick = Some(Pick::Analyze);
            }
            if button_if(ui, editable, Icon::Script, BIG, "Выполнить скрипт Базиса (.js)", false).clicked() {
                pick = Some(Pick::Script);
            }
        });
        match pick {
            Some(Pick::Add(kind)) => self.add_object(kind),
            Some(Pick::Fill(plane)) => self.toggle_fill(plane),
            Some(Pick::Modify(m)) => {
                self.fill = None;
                self.modify = (self.modify != Some(m)).then_some(m);
            }
            Some(Pick::Menu(a)) => self.menu_apply(a),
            Some(Pick::Library) => self.show_pane(Pane::Library),
            Some(Pick::Fasteners) => {
                let ids: Vec<u32> = self.session.as_ref().map(|s| self.targets().into_iter().filter(|id| s.project.panels().iter().any(|p| p.id == *id)).collect()).unwrap_or_default();
                if ids.is_empty() {
                    self.error = Some("Крепёж ставится между отдельными панелями: выделите их (детали корпуса — после «Разрушить»).".into());
                } else {
                    let kind = self.fastener_kind.clone();
                    self.exec("fastener.auto", json!({ "kind": kind, "ids": ids }));
                }
            }
            Some(Pick::Analyze) => self.analyze(),
            Some(Pick::Script) => self.pick_script(),
            None => {}
        }
        // Параметры включённого инструмента.
        if self.fill.is_some() || self.modify.is_some() {
            ui.separator();
            if self.fill.is_some() {
                self.fill_params(ui);
            } else {
                self.modify_params(ui);
            }
        }
    }

    /// Включить или выключить инструмент «в проём» и показать вид спереди, где им щёлкают.
    pub(crate) fn toggle_fill(&mut self, plane: &'static str) {
        self.modify = None;
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

    /// Новый модуль или панель с размерами по умолчанию; встаёт справа от модулей и сразу выделяется.
    pub(crate) fn add_object(&mut self, kind: &str) {
        let Some(s) = &self.session else { return };
        let project = &s.project;
        let Some(material) = project.materials.first().map(|m| m.name.clone()) else { return };
        let edge = project.edges.first().map(|e| e.name.clone());
        let floor: Vec<&design::Cabinet> = project.cabinets.iter().filter(|c| c.position[1] <= 0.0).collect();
        // Правый край напольных модулей и отдельных панелей.
        let right = floor.iter().map(|c| c.position[0] + c.width).chain(project.corners.iter().map(|c| c.position[0] + c.width_x)).fold(0.0, f64::max);
        let (cmd, args) = match kind {
            "cabinet" => ("cabinet.add", json!({ "width": 600, "height": 820, "depth": 560, "material": material, "plinth": 100, "edge": edge, "doors": 1, "position": [right, 0, 0] })),
            "wall" => {
                // Над столешницей (или напольными) на 600 мм; без них — на 1400 мм от пола.
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

    /// Выделить объект по номеру: первую его деталь.
    pub(crate) fn select_object(&mut self, id: u32) {
        let Ok(parts) = &self.parts else { return };
        if let Some(i) = parts.iter().position(|p| p.owner == id || p.id == id) {
            self.selection.clear();
            self.furniture_sel = None;
            self.select(Some(i));
        }
    }

    /// Параметры «в проём»: материал, отступы или вид фасада.
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

    /// Параметры сдвига, копии, поворота, отражения и кнопка «Применить» (или Enter).
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

    /// Подсказка в строке состояния: что делать дальше.
    pub(crate) fn hint(&self) -> String {
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
            Some(_) => "Свойства выделенного — справа. Del — удалить, ПКМ — меню, Esc — снять выделение.".into(),
        }
    }
}
