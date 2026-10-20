//! Several open projects (like drawing tabs in cadcraft): each has its own project, file,
//! selection and unsaved-changes flag. The active one lives in `App` fields, the others in `App::docs`.

use std::path::PathBuf;

use dowel::design::{Project, Session};
use dowel::job::Job;

use crate::{App, Sel};

/// An inactive document.
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
        Doc { session: Some(Session::new(Project::new(&crate::i18n::t("New project")))), job: Job::from_project(Project::new(&crate::i18n::t("New project"))), path: None, selection: Vec::new(), furniture_sel: None, card: 0, dirty: false, hidden: Default::default() }
    }

    /// Name for the tab.
    pub fn title(&self) -> String {
        self.session.as_ref().map_or_else(|| self.job.name.clone(), |s| s.project.name.clone())
    }
}

/// What to ask before closing a tab with unsaved changes.
pub struct CloseAsk {
    pub index: usize,
    pub title: String,
}

impl App {
    /// Swap the active document (`App` fields) with `doc`.
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

    /// Make document `i` active (in `docs` the active one is a placeholder in its slot).
    pub(crate) fn switch_doc(&mut self, i: usize) {
        if i == self.active || i >= self.docs.len() {
            self.start = false;
            return;
        }
        let mut cur = std::mem::replace(&mut self.docs[i], Doc::empty());
        self.swap_doc(&mut cur);
        // `cur` now holds the former active one; put it back in its slot.
        self.docs[self.active] = cur;
        self.active = i;
        self.after_switch();
    }

    /// A new document in a tab.
    pub(crate) fn add_doc(&mut self, mut doc: Doc) {
        self.swap_doc(&mut doc);
        self.docs[self.active] = doc;
        self.docs.push(Doc::empty());
        self.active = self.docs.len() - 1;
        self.after_switch();
    }

    /// The current document is empty and unsaved, so a file being opened can replace it.
    pub(crate) fn current_is_blank(&self) -> bool {
        !self.dirty && self.path.is_none() && self.session.as_ref().is_some_and(|s| s.project.materials.is_empty() && s.project.objects.is_empty() && s.project.cabinets.is_empty() && s.project.corners.is_empty() && s.project.countertops.is_empty())
    }

    /// Close tab `i` (without asking); closing the last one leaves an empty project.
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

    /// Close, asking first if there are unsaved changes.
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
        (self.guiding, self.guide_drag) = (None, None);
        self.start = false;
        self.refresh();
        self.viewport.refit();
        for v in &mut self.views {
            v.refit();
        }
    }

    /// Closing the window with unsaved projects: the close is held back and each one is asked about in turn
    /// (Save / Don't save / Cancel); once none is left the window closes. Cancel keeps the program open.
    pub(crate) fn quit_check(&mut self, ctx: &eframe::egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.screenshot.is_none() {
            if !self.any_dirty() {
                return;
            }
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::CancelClose);
            self.quitting = true;
        }
        if !self.quitting || self.close_ask.is_some() {
            return;
        }
        let next = (0..self.docs.len()).find(|&i| if i == self.active { self.dirty } else { self.docs[i].dirty });
        match next {
            Some(i) => {
                self.switch_doc(i);
                self.request_close(i);
                ctx.request_repaint();
            }
            None => ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close),
        }
    }

    fn any_dirty(&self) -> bool {
        self.dirty || self.docs.iter().enumerate().any(|(i, d)| i != self.active && d.dirty)
    }

    /// The "Save changes?" window before closing a tab.
    pub(crate) fn close_window(&mut self, ctx: &eframe::egui::Context) {
        let Some(ask) = &self.close_ask else { return };
        let (index, title) = (ask.index, ask.title.clone());
        let mut choice = None;
        eframe::egui::Window::new(crate::i18n::t("Close project")).collapsible(false).resizable(false).anchor(eframe::egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(crate::i18n::t(format!("Save changes to “{title}”?")));
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::t("Save")).clicked() {
                    choice = Some(0);
                }
                if ui.button(crate::i18n::t("Don't save")).clicked() {
                    choice = Some(1);
                }
                if ui.button(crate::i18n::t("Cancel")).clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => {
                self.close_ask = None;
                self.switch_doc(index);
                self.save(false);
                if self.dirty {
                    // The save dialog was cancelled or saving failed: stay open.
                    self.quitting = false;
                } else {
                    self.close_doc(self.active);
                }
            }
            Some(1) => {
                self.close_ask = None;
                self.close_doc(index);
            }
            Some(_) => (self.close_ask, self.quitting) = (None, false),
            None => {}
        }
        // The next question, or the close itself, comes on the next frame.
        if self.quitting {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::App;

    /// "New" doesn't overwrite the open project: it stays in its tab with its name and changes.
    #[test]
    fn new_project_keeps_the_open_one() {
        let mut app = App::default();
        app.exec("project.rename", json!({ "name": "Kitchen" }));
        app.exec("material.add", json!({ "name": "Chipboard 16", "thickness": 16, "sheet": [2800, 2070], "coef": 1.2 }));
        assert!(app.dirty);
        app.new_project();
        assert_eq!((app.docs.len(), app.active), (2, 1));
        assert_eq!(app.doc_title(), "New project");
        assert!(!app.dirty);
        assert_eq!(app.docs[0].title(), "Kitchen");
        app.switch_doc(0);
        assert_eq!(app.doc_title(), "Kitchen");
        assert!(app.dirty);
        assert_eq!(app.session.as_ref().map(|s| s.project.materials.len()), Some(1));
        // Close the "Kitchen" tab; the new project remains.
        app.close_doc(0);
        assert_eq!((app.docs.len(), app.active), (1, 0));
        assert_eq!(app.doc_title(), "New project");
    }

    /// Two individual panels in a row don't overlap (previously the second one landed on top of the first).
    #[test]
    fn added_panels_do_not_overlap() {
        let mut app = App::default();
        app.exec("material.add", json!({ "name": "Chipboard 16", "thickness": 16, "sheet": [2800, 2070], "coef": 1.2 }));
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
                assert!(apart, "{} and {} overlap", a.panel.name, b.panel.name);
            }
        }
    }

    /// A part hidden with the eye icon stays hidden in its project when switching tabs.
    #[test]
    fn hidden_parts_belong_to_their_project() {
        let mut app = App::default();
        app.exec("material.add", json!({ "name": "Chipboard 16", "thickness": 16, "sheet": [2800, 2070], "coef": 1.2 }));
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

    /// Unsaved changes: ask first instead of closing.
    #[test]
    fn closing_dirty_project_asks() {
        let mut app = App::default();
        app.exec("project.rename", json!({ "name": "Wardrobe" }));
        app.request_close(0);
        assert!(app.close_ask.as_ref().is_some_and(|a| a.title == "Wardrobe"));
        assert_eq!(app.doc_title(), "Wardrobe");
    }

    /// Closing the window: an unsaved project in a background tab is asked about; Cancel keeps the window open.
    #[test]
    fn closing_window_asks_about_unsaved() {
        use eframe::egui;
        let ctx = egui::Context::default();
        let mut app = App::default();
        app.exec("project.rename", json!({ "name": "Table" }));
        app.new_project();
        let mut frame = eframe::Frame::_new_kittest();
        let mut run = |app: &mut App, close: bool| {
            let mut input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), ..Default::default() };
            input.viewports.entry(egui::ViewportId::ROOT).or_default().events = if close { vec![egui::ViewportEvent::Close] } else { vec![] };
            let mut out = ctx.run_ui(input, |ui| {
                eframe::App::logic(app, ui.ctx(), &mut frame);
                eframe::App::ui(app, ui, &mut frame);
            });
            out.textures_delta.clear();
            out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default()
        };
        let cmds = run(&mut app, true);
        assert!(cmds.contains(&egui::ViewportCommand::CancelClose));
        run(&mut app, false);
        assert!(app.close_ask.as_ref().is_some_and(|a| a.title == "Table"));
        assert_eq!(app.doc_title(), "Table");
        // "Don't save": the tab closes, nothing unsaved is left, so the window closes.
        let index = app.close_ask.as_ref().unwrap().index;
        app.close_ask = None;
        app.close_doc(index);
        let cmds = run(&mut app, false);
        assert!(cmds.contains(&egui::ViewportCommand::Close));
        // Nothing unsaved: closing is not held back.
        let cmds = run(&mut app, true);
        assert!(!cmds.contains(&egui::ViewportCommand::CancelClose));
    }
}

#[cfg(test)]
mod window_tests {
    use eframe::egui;

    use crate::App;

    /// Ctrl+1–4 switch the tab: 3D, front, parts, cutting; 1–4 without Ctrl switch the 3D camera.
    #[test]
    fn number_keys_switch_the_view() {
        let ctx = egui::Context::default();
        let mut app = App::default();
        let mut frame = eframe::Frame::_new_kittest();
        let key = |k| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND };
        for (k, pane) in [(egui::Key::Num3, crate::Pane::Parts), (egui::Key::Num4, crate::Pane::Cut), (egui::Key::Num2, crate::Pane::Front), (egui::Key::Num1, crate::Pane::Model)] {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), events: vec![key(k)], ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| eframe::App::ui(&mut app, ui, &mut frame));
            out.textures_delta.clear();
            assert_eq!(app.view, pane);
        }
        // Alt+4 is x-ray; a digit with Alt doesn't change the view.
        let alt = egui::Event::Key { key: egui::Key::Num4, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::ALT };
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), events: vec![alt], ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| eframe::App::ui(&mut app, ui, &mut frame));
        out.textures_delta.clear();
        assert_eq!(app.viewport.mode, crate::viewport::Mode::XRay);
        assert_eq!(app.view, crate::Pane::Model);
        // "3" without Ctrl is the top camera, and from the parts table it jumps straight to 3D.
        app.view = crate::Pane::Parts;
        let three = egui::Event::Key { key: egui::Key::Num3, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), events: vec![three], ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| eframe::App::ui(&mut app, ui, &mut frame));
        out.textures_delta.clear();
        assert_eq!(app.view, crate::Pane::Model);
        let mut top = crate::viewport::Viewport::default();
        top.set_view(0.0, 89.9);
        assert_eq!((app.viewport.camera.yaw, app.viewport.camera.pitch), (top.camera.yaw, top.camera.pitch));
    }

    /// Whole program window: hovering an icon in the palette must open its tooltip.
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
        // The "Floor cabinet" icon is the first in the left palette: find the point where the mouse gets its description.
        let mut found = None;
        'scan: for y in (60..400).step_by(6) {
            for x in (10..280).step_by(6) {
                let p = egui::pos2(x as f32, y as f32);
                run(&mut app, vec![egui::Event::PointerMoved(p)], time);
                run(&mut app, vec![], time);
                if crate::icons::take_hover(&ctx).is_some_and(|t| t.starts_with("Floor cabinet")) {
                    found = Some(p);
                    break 'scan;
                }
            }
        }
        let p = found.expect("icon not found");
        run(&mut app, vec![egui::Event::PointerMoved(p)], time);
        for _ in 0..15 {
            time += 0.1;
            run(&mut app, vec![], time);
        }
        let tooltip = ctx.memory(|m| m.areas().visible_layer_ids().into_iter().any(|l| l.order == egui::Order::Tooltip));
        assert!(tooltip, "tooltip did not open");
    }
}
