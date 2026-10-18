//! Window shell as in cadcraft: menu under the icon bar, start page (open, new,
//! recent files), the "How to use" window and the on-disk recent files list.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};
use serde_json::json;

use crate::tools::Modify;
use crate::viewport::Mode;
use crate::{App, MenuAct, Pane};

/// Help text (docs/help.md), embedded into the binary.
const HELP: &str = include_str!("../../../docs/help.md");
const HELP_EN: &str = include_str!("../../../docs/help.en.md");
const RECENT_MAX: usize = 12;
/// A sample offered by the start page: the first Bazis model in the local `example/` folder, if there is one.
fn sample() -> Option<PathBuf> {
    let mut models: Vec<PathBuf> = std::fs::read_dir("example").ok()?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "b3d")).collect();
    models.sort();
    models.into_iter().next()
}

fn recent_file() -> Option<PathBuf> {
    Some(crate::settings::config_dir()?.join("recent.json"))
}

/// Recently opened and saved files (newest first), only those that still exist.
pub fn recent() -> Vec<PathBuf> {
    let Some(text) = recent_file().and_then(|f| std::fs::read_to_string(f).ok()) else { return Vec::new() };
    serde_json::from_str::<Vec<PathBuf>>(&text).unwrap_or_default().into_iter().filter(|p| p.exists()).collect()
}

/// Remember a file in "Recent".
pub fn remember(path: &Path) {
    let Some(file) = recent_file() else { return };
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut list = recent();
    list.retain(|p| *p != path);
    list.insert(0, path);
    list.truncate(RECENT_MAX);
    let saved = file.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&file, serde_json::to_string_pretty(&list).unwrap_or_default()));
    if let Err(e) = saved {
        eprintln!("[recent files] {}: {e}", file.display());
    }
}

/// Help line: **bold** and `code` inside plain text.
fn rich_line(ui: &mut egui::Ui, text: &str) {
    let mut job = egui::text::LayoutJob::default();
    let body = ui.style().text_styles[&egui::TextStyle::Body].clone();
    let color = ui.visuals().text_color();
    let strong = ui.visuals().strong_text_color();
    let mut bold = false;
    for (k, part) in text.split("**").enumerate() {
        if k > 0 {
            bold = !bold;
        }
        for (j, piece) in part.split('`').enumerate() {
            let code = j % 2 == 1;
            let format = egui::TextFormat {
                font_id: if code { egui::FontId::monospace(body.size) } else { body.clone() },
                color: if bold { strong } else if code { Color32::from_rgb(0xf5, 0xd0, 0x8a) } else { color },
                ..Default::default()
            };
            job.append(piece, 0.0, format);
        }
    }
    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

/// Minimal docs/help.md markup: `#`/`##` headings, `-` and `1.` lists, paragraphs.
fn markdown(ui: &mut egui::Ui, text: &str) {
    for line in text.lines() {
        let t = line.trim_end();
        if let Some(h) = t.strip_prefix("## ") {
            ui.add_space(10.0);
            ui.label(RichText::new(h).heading().size(17.0));
        } else if let Some(h) = t.strip_prefix("# ") {
            ui.label(RichText::new(h).heading());
        } else if let Some(item) = t.strip_prefix("- ") {
            ui.horizontal_top(|ui| {
                ui.label("•");
                rich_line(ui, item);
            });
        } else if t.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
            let (n, item) = t.split_once(". ").unwrap_or_default();
            ui.horizontal_top(|ui| {
                ui.label(format!("{n}."));
                rich_line(ui, item);
            });
        } else if t.is_empty() {
            ui.add_space(4.0);
        } else {
            rich_line(ui, t);
        }
    }
}

impl App {
    /// Menu under the icon bar: everything the program has, as text.
    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let editable = self.session.is_some();
        let selected = !self.targets().is_empty();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(crate::i18n::t("File"), |ui| {
                if ui.button(crate::i18n::t("New project")).clicked() {
                    self.new_project();
                }
                if ui.button(crate::i18n::t("Open…")).clicked() {
                    self.open_dialog();
                }
                ui.menu_button(crate::i18n::t("Recent"), |ui| {
                    let list = recent();
                    if list.is_empty() {
                        ui.weak(crate::i18n::t("empty so far"));
                    }
                    for p in list {
                        if ui.button(p.file_name().map_or_else(|| p.display().to_string(), |f| f.to_string_lossy().into_owned())).on_hover_text(p.display().to_string()).clicked() {
                            self.open(p);
                        }
                    }
                });
                ui.separator();
                if ui.add_enabled(editable, egui::Button::new(crate::i18n::t("Save")).shortcut_text(self.settings.key_label("save"))).clicked() {
                    self.save(false);
                }
                if ui.add_enabled(editable, egui::Button::new(crate::i18n::t("Save as…"))).clicked() {
                    self.save(true);
                }
                ui.menu_button(crate::i18n::t("Export"), |ui| {
                    for (what, label) in crate::EXPORTS {
                        if ui.button(crate::i18n::t(label)).clicked() {
                            self.export(what);
                        }
                    }
                });
                if !editable && ui.button(crate::i18n::t("Make editable")).clicked() {
                    self.make_editable();
                }
                ui.separator();
                if ui.button(crate::i18n::t("Exit")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(crate::i18n::t("Edit"), |ui| {
                let (undo, redo) = self.session.as_ref().map_or((false, false), |s| (s.can_undo(), s.can_redo()));
                if ui.add_enabled(undo, egui::Button::new(crate::i18n::t("Undo")).shortcut_text(self.settings.key_label("undo"))).clicked() {
                    self.exec("undo", json!({}));
                }
                if ui.add_enabled(redo, egui::Button::new(crate::i18n::t("Redo")).shortcut_text(self.settings.key_label("redo"))).clicked() {
                    self.exec("redo", json!({}));
                }
                ui.separator();
                if ui.add_enabled(editable && !self.delete_targets().is_empty(), egui::Button::new(crate::i18n::t("Delete")).shortcut_text(self.settings.key_label("delete"))).clicked() {
                    self.menu_apply(MenuAct::Delete);
                }
                if ui.add(egui::Button::new(crate::i18n::t("Clear selection")).shortcut_text("Esc")).clicked() {
                    self.menu_apply(MenuAct::Deselect);
                }
                ui.separator();
                if ui.button(crate::i18n::t("Settings…")).clicked() {
                    self.drafting = true;
                }
            });
            ui.menu_button(crate::i18n::t("View"), |ui| {
                for (label, pane) in [("3D", Pane::Model), ("Front view", Pane::Front), ("Parts", Pane::Parts), ("Cutting", Pane::Cut)] {
                    if ui.button(crate::i18n::t(label)).clicked() {
                        self.start = false;
                        self.show_pane(pane);
                    }
                }
                if ui.button(crate::i18n::t("Start")).clicked() {
                    self.start = true;
                }
                ui.separator();
                for (label, yaw, pitch) in [("Front", 0.0, 0.0), ("Left", 90.0, 0.0), ("Top", 0.0, 89.9), ("Axonometric", crate::viewport::Camera::AXON.0, crate::viewport::Camera::AXON.1)] {
                    if ui.button(crate::i18n::t(format!("Camera: {label}"))).clicked() {
                        self.viewport.set_view(yaw, pitch);
                        self.show_pane(Pane::Model);
                    }
                }
                if ui.button(crate::i18n::t("Zoom to fit")).clicked() {
                    self.viewport.refit();
                }
                ui.checkbox(&mut self.quad, crate::i18n::t("4 views"));
                if ui.checkbox(&mut self.show_doors, crate::i18n::t("Doors and fronts")).changed() {
                    self.revision += 1;
                }
                ui.separator();
                for (mode, label) in Mode::ALL {
                    ui.radio_value(&mut self.viewport.mode, mode, crate::i18n::t(label));
                }
            });
            ui.menu_button(crate::i18n::t("Insert"), |ui| {
                let can_add = editable && self.session.as_ref().is_some_and(|s| !s.project.materials.is_empty());
                for (kind, label) in [("cabinet", "Floor cabinet"), ("wall", "Wall cabinet"), ("corner", "Corner unit"), ("countertop", "Countertop"), ("panel", "Free panel")] {
                    if ui.add_enabled(can_add, egui::Button::new(crate::i18n::t(label))).clicked() {
                        self.add_object(kind);
                    }
                }
                ui.separator();
                for (plane, label) in [("horizontal", "Shelf into an opening"), ("vertical", "Partition into an opening"), ("front", "Front panel into an opening"), ("door", "Door into an opening"), ("drawers", "Drawers into an opening")] {
                    if ui.add_enabled(can_add, egui::Button::new(crate::i18n::t(label))).clicked() {
                        self.toggle_fill(plane);
                    }
                }
                ui.separator();
                if ui.button(crate::i18n::t("Hardware from the library…")).clicked() {
                    self.show_pane(Pane::Library);
                }
            });
            ui.menu_button(crate::i18n::t("Modify"), |ui| {
                for (m, label) in [(Modify::Move, "Move"), (Modify::Copy, "Copy"), (Modify::Rotate, "Rotate"), (Modify::Mirror, "Mirror"), (Modify::MirrorCopy, "Mirror copy")] {
                    if ui.add_enabled(editable && selected, egui::Button::new(crate::i18n::t(label))).clicked() {
                        self.fill = None;
                        self.modify = Some(m);
                        self.show_pane(Pane::Tools);
                    }
                }
                ui.separator();
                if ui.add_enabled(editable && selected, egui::Button::new(crate::i18n::t("Group into block"))).clicked() {
                    self.menu_apply(MenuAct::Block);
                }
                if ui.add_enabled(editable && selected, egui::Button::new(crate::i18n::t("Explode"))).clicked() {
                    self.menu_apply(MenuAct::Explode);
                }
            });
            ui.menu_button(crate::i18n::t("Utilities"), |ui| {
                if ui.button(crate::i18n::t("Model analysis")).clicked() {
                    self.analyze();
                }
                if ui.add_enabled(editable, egui::Button::new(crate::i18n::t("Run a Bazis script…"))).clicked() {
                    self.pick_script();
                }
            });
            ui.menu_button(crate::i18n::t("Help"), |ui| {
                if ui.button(crate::i18n::t("How to use the program")).clicked() {
                    self.help = true;
                }
                if ui.button(crate::i18n::t("About")).clicked() {
                    self.about = true;
                }
            });
        });
    }

    /// Document strip under the menu (like cadcraft's file tabs): "+" for a new project, "Start", the current project.
    pub(crate) fn doc_tabs(&mut self, ui: &mut egui::Ui) {
        let t = &crate::theme::T;
        let r = ui.max_rect();
        let mut x = r.left() + 6.0;
        let plus = egui::Rect::from_min_size(egui::pos2(x, r.top() + 4.0), egui::vec2(18.0, 18.0));
        let resp = ui.interact(plus, ui.id().with("doc+"), egui::Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(plus, 2.0, t.control_hover);
        }
        ui.painter().text(plus.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(16.0), t.text);
        if resp.on_hover_text(crate::i18n::t("New project")).clicked() {
            self.new_project();
        }
        x += 26.0;
        // "Start" plus one tab per open project; "*" means unsaved changes.
        let mut tabs: Vec<(String, Option<usize>, f32)> = vec![(crate::i18n::t("Start"), None, 110.0)];
        for i in 0..self.docs.len() {
            let (title, dirty) = if i == self.active { (self.doc_title(), self.dirty) } else { (self.docs[i].title(), self.docs[i].dirty) };
            tabs.push((format!("{title}{}", if dirty { " *" } else { "" }), Some(i), 180.0));
        }
        let (mut switch, mut close) = (None, None);
        for (label, doc, w) in tabs {
            let tr = egui::Rect::from_min_size(egui::pos2(x, r.top()), egui::vec2(w, r.height()));
            let resp = ui.interact(tr, ui.id().with(("doc", doc)), egui::Sense::click());
            let active = match doc {
                None => self.start,
                Some(i) => !self.start && i == self.active,
            };
            let fill = if active { t.tab_active } else if resp.hovered() { t.chrome } else { t.chrome_dark };
            ui.painter().rect_filled(tr, 0.0, fill);
            let text_w = if doc.is_some() { w - 30.0 } else { w - 16.0 };
            let clip = ui.painter().with_clip_rect(egui::Rect::from_min_size(tr.min, egui::vec2(8.0 + text_w, tr.height())));
            clip.text(egui::pos2(tr.left() + 8.0, tr.center().y), egui::Align2::LEFT_CENTER, &label, egui::FontId::proportional(13.0), if active { t.text } else { t.text_dim });
            if let Some(i) = doc
                && (active || resp.hovered() || ui.rect_contains_pointer(tr))
            {
                let cr = egui::Rect::from_center_size(egui::pos2(tr.right() - 12.0, tr.center().y), egui::vec2(14.0, 14.0));
                let cresp = ui.interact(cr, ui.id().with(("docx", i)), egui::Sense::click());
                if cresp.hovered() {
                    ui.painter().rect_filled(cr, 2.0, t.control_hover);
                }
                ui.painter().text(cr.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(14.0), t.text);
                if cresp.on_hover_text(crate::i18n::t("Close")).clicked() {
                    close = Some(i);
                }
            }
            ui.painter().vline(tr.right(), tr.y_range(), egui::Stroke::new(1.0, t.border));
            if resp.on_hover_text(label.clone()).clicked() {
                match doc {
                    None => self.start = true,
                    Some(i) => switch = Some(i),
                }
            }
            x += w;
        }
        if let Some(i) = close {
            self.request_close(i);
        } else if let Some(i) = switch {
            self.switch_doc(i);
        }
    }

    /// Full-window start page, as in cadcraft: title and framed buttons on the left, recent files on the right.
    pub(crate) fn start_page(&mut self, ui: &mut egui::Ui) {
        let t = &crate::theme::T;
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let left = egui::Rect::from_min_size(r.min, egui::vec2(300.0, r.height()));
        p.rect_filled(left, 0.0, t.chrome_dark);
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(left.right(), r.top()), r.max), 0.0, t.chrome);
        p.text(egui::pos2(left.left() + 34.0, left.top() + 70.0), egui::Align2::LEFT_CENTER, "dowel-craft", egui::FontId::proportional(28.0), t.text);
        p.text(egui::pos2(left.left() + 34.0, left.top() + 98.0), egui::Align2::LEFT_CENTER, crate::i18n::t(format!("Version {}", env!("CARGO_PKG_VERSION"))), egui::FontId::proportional(12.0), t.text_faint);
        let sample = sample();
        let mut y = left.top() + 140.0;
        let mut action = None;
        let mut buttons = vec![("Open…", "open"), ("New project", "new")];
        if sample.is_some() {
            buttons.push(("Open a sample", "sample"));
        }
        buttons.push(("How to use", "help"));
        for (label, cmd) in buttons {
            let br = egui::Rect::from_min_size(egui::pos2(left.left() + 34.0, y), egui::vec2(220.0, 34.0));
            let resp = ui.interact(br, ui.id().with(("start", cmd)), egui::Sense::click());
            p.rect_stroke(br, 2.0, egui::Stroke::new(1.0, if resp.hovered() { t.text } else { t.text_dim }), egui::StrokeKind::Inside);
            p.text(egui::pos2(br.left() + 16.0, br.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t(label), egui::FontId::proportional(14.0), t.text);
            if resp.clicked() {
                action = Some(cmd);
            }
            y += 44.0;
        }
        let main = egui::Rect::from_min_max(egui::pos2(left.right() + 36.0, r.top() + 40.0), r.max);
        p.text(main.min, egui::Align2::LEFT_TOP, crate::i18n::t("Recent"), egui::FontId::proportional(22.0), t.text);
        let list = recent();
        let mut open = None;
        let mut y = main.top() + 44.0;
        for (i, path) in list.iter().enumerate() {
            let row = egui::Rect::from_min_size(egui::pos2(main.left(), y), egui::vec2((r.right() - main.left() - 36.0).clamp(0.0, 640.0), 42.0));
            let resp = ui.interact(row, ui.id().with(("recent", i)), egui::Sense::click());
            if resp.hovered() {
                p.rect_filled(row, 2.0, t.control_hover.gamma_multiply(0.5));
            }
            let name = path.file_name().map_or_else(|| path.display().to_string(), |f| f.to_string_lossy().into_owned());
            let dir = path.parent().map(|d| d.display().to_string()).unwrap_or_default();
            let clip = p.with_clip_rect(row.shrink(2.0));
            clip.text(egui::pos2(row.left() + 10.0, row.top() + 13.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(14.0), t.text);
            clip.text(egui::pos2(row.left() + 10.0, row.top() + 30.0), egui::Align2::LEFT_CENTER, dir, egui::FontId::proportional(12.0), t.text_faint);
            if resp.on_hover_text(path.display().to_string()).clicked() {
                open = Some(path.clone());
            }
            y += 46.0;
            if y > r.bottom() {
                break;
            }
        }
        if list.is_empty() {
            p.text(main.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("There is nothing here yet."), egui::FontId::proportional(18.0), t.text);
            p.text(main.center() + egui::vec2(0.0, 26.0), egui::Align2::CENTER_CENTER, crate::i18n::t("To get started, create or open a project."), egui::FontId::proportional(14.0), t.text_dim);
        }
        ui.allocate_rect(r, egui::Sense::hover());
        if let Some(path) = open {
            self.open(path);
        }
        match action {
            Some("open") => self.open_dialog(),
            Some("new") => {
                if !self.current_is_blank() {
                    self.new_project();
                }
                self.leave_start();
            }
            Some("sample") => {
                if let Some(s) = sample {
                    self.open(s);
                }
            }
            Some("help") => self.help = true,
            _ => {}
        }
    }

    /// After a choice on the start page, go to the model.
    fn leave_start(&mut self) {
        self.start = false;
        self.show_pane(Pane::Model);
    }

    /// The "How to use" and "About" windows.
    pub(crate) fn help_windows(&mut self, ctx: &egui::Context) {
        let before = self.settings.grid;
        let mut open = self.drafting;
        let mut language = None;
        egui::Window::new(crate::i18n::t("Settings")).id(egui::Id::new("settings_window")).open(&mut open).resizable(false).show(ctx, |ui| {
            ui.heading(crate::i18n::t("Language"));
            ui.horizontal(|ui| {
                // Each language by its own name, whatever the current one.
                for (code, label) in [("en", "English".to_string()), ("ru", dowel::i18n::ru("Russian"))] {
                    if ui.radio(self.settings.language == code, label).clicked() {
                        language = Some(code);
                    }
                }
            });
            ui.separator();
            let g = &mut self.settings.grid;
            ui.heading(crate::i18n::t("Grid"));
            ui.checkbox(&mut g.on, crate::i18n::t("Grid on"));
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("Grid spacing"));
                ui.add(egui::DragValue::new(&mut g.step).speed(1.0).range(1.0..=1000.0).suffix(crate::i18n::t(" mm")));
                ui.label(crate::i18n::t("Major line every"));
                ui.add(egui::DragValue::new(&mut g.major).range(1..=100));
            });
            ui.weak(crate::i18n::t("When zoomed out the spacing grows 5× so lines do not merge."));
            ui.separator();
            ui.heading(crate::i18n::t("Hotkeys"));
            // Waiting for a new shortcut: Esc cancels, Backspace unbinds the key.
            if let Some(id) = self.key_capture {
                let hit = ui.input_mut(|i| {
                    let found = i.events.iter().find_map(|e| match e {
                        egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                        _ => None,
                    });
                    i.events.retain(|e| !matches!(e, egui::Event::Key { .. }));
                    found
                });
                match hit {
                    Some((egui::Key::Escape, _)) => self.key_capture = None,
                    Some((egui::Key::Backspace, _)) => {
                        self.settings.keys.insert(id.into(), String::new());
                        self.key_capture = None;
                        self.settings.save();
                    }
                    Some((key, mods)) => {
                        self.settings.keys.insert(id.into(), crate::keys::format(mods, key));
                        self.key_capture = None;
                        self.settings.save();
                    }
                    None => {}
                }
            }
            egui::Grid::new("hotkeys").num_columns(3).striped(true).show(ui, |ui| {
                for (id, title, _) in crate::keys::ACTIONS {
                    ui.label(crate::i18n::t(title));
                    let label = self.settings.key_label(id);
                    let text = if self.key_capture == Some(id) {
                        crate::i18n::t("press a combination…")
                    } else if label.is_empty() {
                        crate::i18n::t("not set")
                    } else {
                        label.clone()
                    };
                    if ui.selectable_label(self.key_capture == Some(id), text).on_hover_text(crate::i18n::t("Click and press a new combination. Esc cancels, Backspace clears")).clicked() {
                        self.key_capture = Some(id);
                    }
                    let twin = crate::keys::ACTIONS.iter().find(|(o, ..)| o != id && !label.is_empty() && self.settings.key_label(o) == label);
                    match twin {
                        Some((_, other, _)) => ui.colored_label(egui::Color32::from_rgb(230, 90, 80), crate::i18n::t(format!("same as “{other}”"))),
                        None => ui.label(""),
                    };
                    ui.end_row();
                }
            });
            if ui.button(crate::i18n::t("Reset all keys")).clicked() {
                self.settings.keys.clear();
                self.settings.save();
            }
        });
        if !open {
            self.key_capture = None;
        }
        self.drafting = open;
        if let Some(code) = language {
            self.settings.language = code.into();
            crate::i18n::set_language(code);
            self.settings.save();
        }
        if self.settings.grid != before {
            self.settings.save();
        }
        egui::Window::new(crate::i18n::t("How to use dowel-craft")).open(&mut self.help).default_size([620.0, 640.0]).vscroll(true).show(ctx, |ui| {
            markdown(ui, if !crate::i18n::russian() { HELP_EN } else { HELP });
        });
        egui::Window::new(crate::i18n::t("About")).open(&mut self.about).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.heading("dowel-craft");
            ui.label(crate::i18n::t(format!("Version {}", env!("CARGO_PKG_VERSION"))));
            ui.label(crate::i18n::t("Cabinet furniture design compatible with Bazis-Mebelshchik: .b3d, XML for BAZIS-Cutting, cutting maps, specification."));
            ui.weak(crate::i18n::t("Icons and window shell follow cadcraft (storytold), see NOTICE.md."));
        });
    }
}
