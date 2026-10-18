//! Оболочка окна как в cadcraft: меню под строкой иконок, стартовая страница (открыть, новый,
//! последние файлы), окно «Как пользоваться» и список последних файлов на диске.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};
use serde_json::json;

use crate::tools::Modify;
use crate::viewport::Mode;
use crate::{App, MenuAct, Pane};

/// Текст справки (docs/help.md) — вшит в программу.
const HELP: &str = include_str!("../../../docs/help.md");
const HELP_EN: &str = include_str!("../../../docs/help.en.md");
/// Сколько последних файлов помнить.
const RECENT_MAX: usize = 12;
/// Примеры, которые предлагает стартовая страница (первый найденный).
const SAMPLES: [&str; 3] = ["example/Кухня Эл.b3d", "example/Детская кровать.b3d", "example/Витрина высокая.b3d"];

fn recent_file() -> Option<PathBuf> {
    Some(crate::settings::config_dir()?.join("recent.json"))
}

/// Последние открытые и сохранённые файлы (новые сверху), только существующие.
pub fn recent() -> Vec<PathBuf> {
    let Some(text) = recent_file().and_then(|f| std::fs::read_to_string(f).ok()) else { return Vec::new() };
    serde_json::from_str::<Vec<PathBuf>>(&text).unwrap_or_default().into_iter().filter(|p| p.exists()).collect()
}

/// Запомнить файл в «Последних».
pub fn remember(path: &Path) {
    let Some(file) = recent_file() else { return };
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut list = recent();
    list.retain(|p| *p != path);
    list.insert(0, path);
    list.truncate(RECENT_MAX);
    let saved = file.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&file, serde_json::to_string_pretty(&list).unwrap_or_default()));
    if let Err(e) = saved {
        eprintln!("[последние файлы] {}: {e}", file.display());
    }
}

/// Строка справки: **жирное** и `код` внутри обычного текста.
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

/// Простая разметка docs/help.md: заголовки `#`/`##`, списки `-` и `1.`, абзацы.
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
    /// Меню под строкой иконок: всё, что есть в программе, текстом.
    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let editable = self.session.is_some();
        let selected = !self.targets().is_empty();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(crate::i18n::t("Файл"), |ui| {
                if ui.button(crate::i18n::t("Новый проект")).clicked() {
                    self.new_project();
                }
                if ui.button(crate::i18n::t("Открыть…")).clicked() {
                    self.open_dialog();
                }
                ui.menu_button(crate::i18n::t("Последние"), |ui| {
                    let list = recent();
                    if list.is_empty() {
                        ui.weak(crate::i18n::t("пока пусто"));
                    }
                    for p in list {
                        if ui.button(p.file_name().map_or_else(|| p.display().to_string(), |f| f.to_string_lossy().into_owned())).on_hover_text(p.display().to_string()).clicked() {
                            self.open(p);
                        }
                    }
                });
                ui.separator();
                if ui.add_enabled(editable, egui::Button::new(crate::i18n::t("Сохранить")).shortcut_text("Ctrl+S")).clicked() {
                    self.save(false);
                }
                if ui.add_enabled(editable, egui::Button::new(crate::i18n::t("Сохранить как…"))).clicked() {
                    self.save(true);
                }
                ui.menu_button(crate::i18n::t("Экспорт"), |ui| {
                    for (what, label) in crate::EXPORTS {
                        if ui.button(crate::i18n::t(label)).clicked() {
                            self.export(what);
                        }
                    }
                });
                if !editable && ui.button(crate::i18n::t("Сделать редактируемой")).clicked() {
                    self.make_editable();
                }
                ui.separator();
                if ui.button(crate::i18n::t("Выход")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(crate::i18n::t("Правка"), |ui| {
                let (undo, redo) = self.session.as_ref().map_or((false, false), |s| (s.can_undo(), s.can_redo()));
                if ui.add_enabled(undo, egui::Button::new(crate::i18n::t("Отменить")).shortcut_text("Ctrl+Z")).clicked() {
                    self.exec("undo", json!({}));
                }
                if ui.add_enabled(redo, egui::Button::new(crate::i18n::t("Вернуть")).shortcut_text("Ctrl+Y")).clicked() {
                    self.exec("redo", json!({}));
                }
                ui.separator();
                if ui.add_enabled(editable && !self.delete_targets().is_empty(), egui::Button::new(crate::i18n::t("Удалить")).shortcut_text("Del")).clicked() {
                    self.menu_apply(MenuAct::Delete);
                }
                if ui.add(egui::Button::new(crate::i18n::t("Снять выделение")).shortcut_text("Esc")).clicked() {
                    self.menu_apply(MenuAct::Deselect);
                }
                ui.separator();
                if ui.button(crate::i18n::t("Настройки…")).clicked() {
                    self.drafting = true;
                }
            });
            ui.menu_button(crate::i18n::t("Вид"), |ui| {
                for (label, pane) in [("3D", Pane::Model), ("Вид спереди", Pane::Front), ("Детали", Pane::Parts), ("Раскрой", Pane::Cut)] {
                    if ui.button(crate::i18n::t(label)).clicked() {
                        self.start = false;
                        self.show_pane(pane);
                    }
                }
                if ui.button(crate::i18n::t("Начало")).clicked() {
                    self.start = true;
                }
                ui.separator();
                for (label, yaw, pitch) in [("Спереди", 0.0, 0.0), ("Слева", 90.0, 0.0), ("Сверху", 0.0, 89.9), ("Аксонометрия", crate::viewport::Camera::AXON.0, crate::viewport::Camera::AXON.1)] {
                    if ui.button(crate::i18n::t(format!("Камера: {label}"))).clicked() {
                        self.viewport.set_view(yaw, pitch);
                        self.show_pane(Pane::Model);
                    }
                }
                if ui.button(crate::i18n::t("Показать всё")).clicked() {
                    self.viewport.refit();
                }
                ui.checkbox(&mut self.quad, crate::i18n::t("4 окна"));
                if ui.checkbox(&mut self.show_doors, crate::i18n::t("Двери и фасады")).changed() {
                    self.revision += 1;
                }
                ui.separator();
                for (mode, label) in Mode::ALL {
                    ui.radio_value(&mut self.viewport.mode, mode, crate::i18n::t(label));
                }
            });
            ui.menu_button(crate::i18n::t("Вставка"), |ui| {
                let can_add = editable && self.session.as_ref().is_some_and(|s| !s.project.materials.is_empty());
                for (kind, label) in [("cabinet", "Напольный корпус"), ("wall", "Навесной корпус"), ("corner", "Угловой модуль"), ("countertop", "Столешница"), ("panel", "Отдельная панель")] {
                    if ui.add_enabled(can_add, egui::Button::new(crate::i18n::t(label))).clicked() {
                        self.add_object(kind);
                    }
                }
                ui.separator();
                for (plane, label) in [("horizontal", "Полка в проём"), ("vertical", "Перегородка в проём"), ("front", "Фронтальная панель в проём"), ("door", "Дверь в проём"), ("drawers", "Ящики в проём")] {
                    if ui.add_enabled(can_add, egui::Button::new(crate::i18n::t(label))).clicked() {
                        self.toggle_fill(plane);
                    }
                }
                ui.separator();
                if ui.button(crate::i18n::t("Фурнитура из библиотеки…")).clicked() {
                    self.show_pane(Pane::Library);
                }
            });
            ui.menu_button(crate::i18n::t("Изменить"), |ui| {
                for (m, label) in [(Modify::Move, "Сдвинуть"), (Modify::Copy, "Копировать"), (Modify::Rotate, "Повернуть"), (Modify::Mirror, "Отразить"), (Modify::MirrorCopy, "Зеркальная копия")] {
                    if ui.add_enabled(editable && selected, egui::Button::new(crate::i18n::t(label))).clicked() {
                        self.fill = None;
                        self.modify = Some(m);
                        self.show_pane(Pane::Tools);
                    }
                }
                ui.separator();
                if ui.add_enabled(editable && selected, egui::Button::new(crate::i18n::t("Собрать в блок"))).clicked() {
                    self.menu_apply(MenuAct::Block);
                }
                if ui.add_enabled(editable && selected, egui::Button::new(crate::i18n::t("Разрушить"))).clicked() {
                    self.menu_apply(MenuAct::Explode);
                }
            });
            ui.menu_button(crate::i18n::t("Сервис"), |ui| {
                if ui.button(crate::i18n::t("Анализ модели")).clicked() {
                    self.analyze();
                }
                if ui.add_enabled(editable, egui::Button::new(crate::i18n::t("Выполнить скрипт Базиса…"))).clicked() {
                    self.pick_script();
                }
            });
            ui.menu_button(crate::i18n::t("Справка"), |ui| {
                if ui.button(crate::i18n::t("Как пользоваться программой")).clicked() {
                    self.help = true;
                }
                if ui.button(crate::i18n::t("О программе")).clicked() {
                    self.about = true;
                }
            });
        });
    }

    /// Строка документов под меню (как файловые вкладки cadcraft): «+» — новый проект, «Начало», текущий проект.
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
        if resp.on_hover_text(crate::i18n::t("Новый проект")).clicked() {
            self.new_project();
        }
        x += 26.0;
        // «Начало» и по вкладке на каждый открытый проект; «*» — есть несохранённые изменения.
        let mut tabs: Vec<(String, Option<usize>, f32)> = vec![(crate::i18n::t("Начало"), None, 110.0)];
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
                if cresp.on_hover_text(crate::i18n::t("Закрыть")).clicked() {
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

    /// Стартовая страница на всё окно, как в cadcraft: слева название и кнопки в рамке, справа — последние файлы.
    pub(crate) fn start_page(&mut self, ui: &mut egui::Ui) {
        let t = &crate::theme::T;
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let left = egui::Rect::from_min_size(r.min, egui::vec2(300.0, r.height()));
        p.rect_filled(left, 0.0, t.chrome_dark);
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(left.right(), r.top()), r.max), 0.0, t.chrome);
        p.text(egui::pos2(left.left() + 34.0, left.top() + 70.0), egui::Align2::LEFT_CENTER, "basis-craft", egui::FontId::proportional(28.0), t.text);
        p.text(egui::pos2(left.left() + 34.0, left.top() + 98.0), egui::Align2::LEFT_CENTER, crate::i18n::t(format!("Версия {}", env!("CARGO_PKG_VERSION"))), egui::FontId::proportional(12.0), t.text_faint);
        let sample = SAMPLES.iter().map(Path::new).find(|p| p.exists());
        let mut y = left.top() + 140.0;
        let mut action = None;
        let mut buttons = vec![("Открыть…", "open"), ("Новый проект", "new")];
        if sample.is_some() {
            buttons.push(("Открыть пример", "sample"));
        }
        buttons.push(("Как пользоваться", "help"));
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
        p.text(main.min, egui::Align2::LEFT_TOP, crate::i18n::t("Последние"), egui::FontId::proportional(22.0), t.text);
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
            p.text(main.center(), egui::Align2::CENTER_CENTER, crate::i18n::t("Здесь пока ничего нет."), egui::FontId::proportional(18.0), t.text);
            p.text(main.center() + egui::vec2(0.0, 26.0), egui::Align2::CENTER_CENTER, crate::i18n::t("Чтобы начать, создайте или откройте проект."), egui::FontId::proportional(14.0), t.text_dim);
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
                    self.open(s.to_path_buf());
                }
            }
            Some("help") => self.help = true,
            _ => {}
        }
    }

    /// После выбора на стартовой странице — к модели.
    fn leave_start(&mut self) {
        self.start = false;
        self.show_pane(Pane::Model);
    }

    /// Окна «Как пользоваться» и «О программе».
    pub(crate) fn help_windows(&mut self, ctx: &egui::Context) {
        let before = self.settings.grid;
        let mut open = self.drafting;
        let mut language = None;
        egui::Window::new(crate::i18n::t("Настройки")).id(egui::Id::new("settings_window")).open(&mut open).resizable(false).show(ctx, |ui| {
            ui.heading("Язык / Language");
            ui.horizontal(|ui| {
                for (code, label) in [("ru", "Русский"), ("en", "English")] {
                    if ui.radio(self.settings.language == code, label).clicked() {
                        language = Some(code);
                    }
                }
            });
            ui.separator();
            let g = &mut self.settings.grid;
            ui.heading(crate::i18n::t("Сетка"));
            ui.checkbox(&mut g.on, crate::i18n::t("Сетка включена (F7)"));
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("Шаг сетки"));
                ui.add(egui::DragValue::new(&mut g.step).speed(1.0).range(1.0..=1000.0).suffix(crate::i18n::t(" мм")));
                ui.label(crate::i18n::t("Основная линия каждые"));
                ui.add(egui::DragValue::new(&mut g.major).range(1..=100));
            });
            ui.weak(crate::i18n::t("При отдалении шаг сам укрупняется в 5 раз, чтобы линии не сливались."));
        });
        self.drafting = open;
        if let Some(code) = language {
            self.settings.language = code.into();
            crate::i18n::set_language(code);
            self.settings.save();
        }
        if self.settings.grid != before {
            self.settings.save();
        }
        egui::Window::new(crate::i18n::t("Как пользоваться basis-craft")).open(&mut self.help).default_size([620.0, 640.0]).vscroll(true).show(ctx, |ui| {
            markdown(ui, if crate::i18n::english() { HELP_EN } else { HELP });
        });
        egui::Window::new(crate::i18n::t("О программе")).open(&mut self.about).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.heading("basis-craft");
            ui.label(crate::i18n::t(format!("Версия {}", env!("CARGO_PKG_VERSION"))));
            ui.label(crate::i18n::t("Проектирование корпусной мебели, совместимое с Базис-Мебельщиком: .b3d, XML для БАЗИС-Раскроя, карты раскроя, спецификация."));
            ui.weak(crate::i18n::t("Иконки и оболочка — по образцу cadcraft (storytold), см. NOTICE.md."));
        });
    }
}
