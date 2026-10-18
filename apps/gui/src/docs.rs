//! Несколько открытых проектов (как вкладки чертежей в cadcraft): у каждого свой проект, файл,
//! выделение и признак несохранённых изменений. Активный живёт в полях `App`, остальные — в `App::docs`.

use std::path::PathBuf;

use basis::design::{Project, Session};
use basis::job::Job;

use crate::{App, Sel};

/// Неактивный документ.
pub struct Doc {
    pub session: Option<Session>,
    pub job: Job,
    pub path: Option<PathBuf>,
    pub selection: Vec<Sel>,
    pub furniture_sel: Option<u32>,
    pub card: usize,
    pub dirty: bool,
    pub hidden: std::collections::HashSet<Sel>,
}

impl Doc {
    pub fn empty() -> Doc {
        Doc { session: Some(Session::new(Project::new("Новый проект"))), job: Job::from_project(Project::new("Новый проект")), path: None, selection: Vec::new(), furniture_sel: None, card: 0, dirty: false, hidden: Default::default() }
    }

    /// Имя для вкладки.
    pub fn title(&self) -> String {
        self.session.as_ref().map_or_else(|| self.job.name.clone(), |s| s.project.name.clone())
    }
}

/// Что спросить перед закрытием вкладки с несохранёнными изменениями.
pub struct CloseAsk {
    pub index: usize,
    pub title: String,
}

impl App {
    /// Поменять местами активный документ (поля `App`) и `doc`.
    pub(crate) fn swap_doc(&mut self, doc: &mut Doc) {
        std::mem::swap(&mut self.session, &mut doc.session);
        std::mem::swap(&mut self.job, &mut doc.job);
        std::mem::swap(&mut self.path, &mut doc.path);
        std::mem::swap(&mut self.selection, &mut doc.selection);
        std::mem::swap(&mut self.furniture_sel, &mut doc.furniture_sel);
        std::mem::swap(&mut self.card, &mut doc.card);
        std::mem::swap(&mut self.dirty, &mut doc.dirty);
        std::mem::swap(&mut self.hidden, &mut doc.hidden);
    }

    /// Сделать активным документ `i` (в `docs` активный лежит пустышкой на своём месте).
    pub(crate) fn switch_doc(&mut self, i: usize) {
        if i == self.active || i >= self.docs.len() {
            self.start = false;
            return;
        }
        let mut cur = std::mem::replace(&mut self.docs[i], Doc::empty());
        self.swap_doc(&mut cur);
        // В `cur` теперь бывший активный — на его место.
        self.docs[self.active] = cur;
        self.active = i;
        self.after_switch();
    }

    /// Новый документ во вкладке.
    pub(crate) fn add_doc(&mut self, mut doc: Doc) {
        self.swap_doc(&mut doc);
        self.docs[self.active] = doc;
        self.docs.push(Doc::empty());
        self.active = self.docs.len() - 1;
        self.after_switch();
    }

    /// Текущий документ пуст и не сохранён — его можно заменить открываемым файлом.
    pub(crate) fn current_is_blank(&self) -> bool {
        !self.dirty && self.path.is_none() && self.session.as_ref().is_some_and(|s| s.project.materials.is_empty() && s.project.objects.is_empty() && s.project.cabinets.is_empty() && s.project.corners.is_empty() && s.project.countertops.is_empty())
    }

    /// Закрыть вкладку `i` (без вопросов); последняя закрытая — пустой проект.
    pub(crate) fn close_doc(&mut self, i: usize) {
        if i >= self.docs.len() {
            return;
        }
        if self.docs.len() == 1 {
            let mut blank = Doc::empty();
            self.swap_doc(&mut blank);
            self.after_switch();
            self.start = true;
            return;
        }
        if i == self.active {
            let next = if i + 1 < self.docs.len() { i + 1 } else { i - 1 };
            self.switch_doc(next);
        }
        self.docs.remove(i);
        if self.active > i {
            self.active -= 1;
        }
    }

    /// Закрыть с вопросом, если есть несохранённые изменения.
    pub(crate) fn request_close(&mut self, i: usize) {
        let (dirty, title) = if i == self.active { (self.dirty, self.doc_title()) } else { (self.docs[i].dirty, self.docs[i].title()) };
        if dirty {
            self.close_ask = Some(CloseAsk { index: i, title });
        } else {
            self.close_doc(i);
        }
    }

    pub(crate) fn doc_title(&self) -> String {
        self.session.as_ref().map_or_else(|| self.job.name.clone(), |s| s.project.name.clone())
    }

    pub(crate) fn after_switch(&mut self) {
        self.drag = None;
        self.handle = None;
        self.problems = None;
        self.error = None;
        self.fill = None;
        self.modify = None;
        self.start = false;
        self.refresh();
        self.viewport.refit();
        for v in &mut self.views {
            v.refit();
        }
    }

    /// Окно «Сохранить изменения?» перед закрытием вкладки.
    pub(crate) fn close_window(&mut self, ctx: &eframe::egui::Context) {
        let Some(ask) = &self.close_ask else { return };
        let (index, title) = (ask.index, ask.title.clone());
        let mut choice = None;
        eframe::egui::Window::new(crate::i18n::t("Закрыть проект")).collapsible(false).resizable(false).anchor(eframe::egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(crate::i18n::t(format!("Сохранить изменения в «{title}»?")));
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::t("Сохранить")).clicked() {
                    choice = Some(0);
                }
                if ui.button(crate::i18n::t("Не сохранять")).clicked() {
                    choice = Some(1);
                }
                if ui.button(crate::i18n::t("Отмена")).clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => {
                self.close_ask = None;
                self.switch_doc(index);
                self.save(false);
                if !self.dirty {
                    self.close_doc(self.active);
                }
            }
            Some(1) => {
                self.close_ask = None;
                self.close_doc(index);
            }
            Some(_) => self.close_ask = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::App;

    /// «Новый» не затирает открытый проект: он остаётся во вкладке со своим именем и изменениями.
    #[test]
    fn new_project_keeps_the_open_one() {
        let mut app = App::default();
        app.exec("project.rename", json!({ "name": "Кухня" }));
        app.exec("material.add", json!({ "name": "ЛДСП 16", "thickness": 16, "sheet": [2800, 2070], "coef": 1.2 }));
        assert!(app.dirty);
        app.new_project();
        assert_eq!((app.docs.len(), app.active), (2, 1));
        assert_eq!(app.doc_title(), "Новый проект");
        assert!(!app.dirty);
        assert_eq!(app.docs[0].title(), "Кухня");
        app.switch_doc(0);
        assert_eq!(app.doc_title(), "Кухня");
        assert!(app.dirty);
        assert_eq!(app.session.as_ref().map(|s| s.project.materials.len()), Some(1));
        // Закрыть вкладку «Кухня» — останется новый проект.
        app.close_doc(0);
        assert_eq!((app.docs.len(), app.active), (1, 0));
        assert_eq!(app.doc_title(), "Новый проект");
    }

    /// Две отдельные панели подряд не ложатся друг на друга (раньше вторая вставала поверх первой).
    #[test]
    fn added_panels_do_not_overlap() {
        let mut app = App::default();
        app.exec("material.add", json!({ "name": "ЛДСП 16", "thickness": 16, "sheet": [2800, 2070], "coef": 1.2 }));
        app.add_object("panel");
        app.add_object("panel");
        app.add_object("cabinet");
        let parts = app.parts.as_ref().unwrap();
        for (i, a) in parts.iter().enumerate() {
            for b in &parts[i + 1..] {
                if a.owner == b.owner {
                    continue;
                }
                let apart = (0..3).any(|k| a.origin[k] + a.size[k] <= b.origin[k] + 1e-6 || b.origin[k] + b.size[k] <= a.origin[k] + 1e-6);
                assert!(apart, "{} и {} пересекаются", a.panel.name, b.panel.name);
            }
        }
    }

    /// Скрытая глазом деталь остаётся скрытой у своего проекта при переключении вкладок.
    #[test]
    fn hidden_parts_belong_to_their_project() {
        let mut app = App::default();
        app.exec("material.add", json!({ "name": "ЛДСП 16", "thickness": 16, "sheet": [2800, 2070], "coef": 1.2 }));
        app.add_object("panel");
        app.add_object("panel");
        let first = crate::Sel::of(app.parts.as_ref().unwrap(), 0);
        app.hidden.insert(first);
        assert_eq!(app.hidden_parts().into_iter().collect::<Vec<_>>(), vec![0]);
        app.new_project();
        assert!(app.hidden_parts().is_empty());
        app.switch_doc(0);
        assert_eq!(app.hidden_parts().into_iter().collect::<Vec<_>>(), vec![0]);
    }

    /// Несохранённые изменения — сначала вопрос, а не закрытие.
    #[test]
    fn closing_dirty_project_asks() {
        let mut app = App::default();
        app.exec("project.rename", json!({ "name": "Шкаф" }));
        app.request_close(0);
        assert!(app.close_ask.as_ref().is_some_and(|a| a.title == "Шкаф"));
        assert_eq!(app.doc_title(), "Шкаф");
    }
}

#[cfg(test)]
mod window_tests {
    use eframe::egui;

    use crate::App;

    /// Клавиши 1–4 переключают вид: 3D, спереди, детали, раскрой.
    #[test]
    fn number_keys_switch_the_view() {
        let ctx = egui::Context::default();
        let mut app = App::default();
        let mut frame = eframe::Frame::_new_kittest();
        let key = |k| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        for (k, pane) in [(egui::Key::Num3, crate::Pane::Parts), (egui::Key::Num4, crate::Pane::Cut), (egui::Key::Num2, crate::Pane::Front), (egui::Key::Num1, crate::Pane::Model)] {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), events: vec![key(k)], ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| eframe::App::ui(&mut app, ui, &mut frame));
            out.textures_delta.clear();
            assert_eq!(app.view, pane);
        }
        // Alt+4 — прозрачность; цифра с Alt вид не меняет.
        let alt = egui::Event::Key { key: egui::Key::Num4, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::ALT };
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), events: vec![alt], ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| eframe::App::ui(&mut app, ui, &mut frame));
        out.textures_delta.clear();
        assert_eq!(app.viewport.mode, crate::viewport::Mode::XRay);
        assert_eq!(app.view, crate::Pane::Model);
    }

    /// Всё окно программы: навести мышь на иконку палитры — должна открыться подсказка.
    #[test]
    fn tool_icons_show_tooltips_in_the_app() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let mut app = App::default();
        let mut frame = eframe::Frame::_new_kittest();
        let mut time = 0.0;
        let mut run = |app: &mut App, events: Vec<egui::Event>, time: f64| {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), events, time: Some(time), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| eframe::App::ui(app, ui, &mut frame));
            out.textures_delta.clear();
        };
        for _ in 0..3 {
            run(&mut app, vec![], time);
        }
        // Иконка «Напольный корпус» — первая в палитре слева: ищем точку, где мышь даёт её описание.
        let mut found = None;
        'scan: for y in (60..400).step_by(6) {
            for x in (10..280).step_by(6) {
                let p = egui::pos2(x as f32, y as f32);
                run(&mut app, vec![egui::Event::PointerMoved(p)], time);
                run(&mut app, vec![], time);
                if crate::icons::take_hover(&ctx).is_some_and(|t| t.starts_with("Напольный корпус")) {
                    found = Some(p);
                    break 'scan;
                }
            }
        }
        let p = found.expect("иконка не найдена");
        run(&mut app, vec![egui::Event::PointerMoved(p)], time);
        for _ in 0..15 {
            time += 0.1;
            run(&mut app, vec![], time);
        }
        let tooltip = ctx.memory(|m| m.areas().visible_layer_ids().into_iter().any(|l| l.order == egui::Order::Tooltip));
        assert!(tooltip, "подсказка не открылась");
    }
}
