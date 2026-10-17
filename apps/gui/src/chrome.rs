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
            ui.menu_button("Файл", |ui| {
                if ui.button("Новый проект").clicked() {
                    self.new_project();
                }
                if ui.button("Открыть…").clicked() {
                    self.open_dialog();
                }
                ui.menu_button("Последние", |ui| {
                    let list = recent();
                    if list.is_empty() {
                        ui.weak("пока пусто");
                    }
                    for p in list {
                        if ui.button(p.file_name().map_or_else(|| p.display().to_string(), |f| f.to_string_lossy().into_owned())).on_hover_text(p.display().to_string()).clicked() {
                            self.open(p);
                        }
                    }
                });
                ui.separator();
                if ui.add_enabled(editable, egui::Button::new("Сохранить").shortcut_text("Ctrl+S")).clicked() {
                    self.save(false);
                }
                if ui.add_enabled(editable, egui::Button::new("Сохранить как…")).clicked() {
                    self.save(true);
                }
                ui.menu_button("Экспорт", |ui| {
                    for (what, label) in crate::EXPORTS {
                        if ui.button(label).clicked() {
                            self.export(what);
                        }
                    }
                });
                if !editable && ui.button("Сделать редактируемой").clicked() {
                    self.make_editable();
                }
                ui.separator();
                if ui.button("Выход").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Правка", |ui| {
                let (undo, redo) = self.session.as_ref().map_or((false, false), |s| (s.can_undo(), s.can_redo()));
                if ui.add_enabled(undo, egui::Button::new("Отменить").shortcut_text("Ctrl+Z")).clicked() {
                    self.exec("undo", json!({}));
                }
                if ui.add_enabled(redo, egui::Button::new("Вернуть").shortcut_text("Ctrl+Y")).clicked() {
                    self.exec("redo", json!({}));
                }
                ui.separator();
                if ui.add_enabled(editable && !self.delete_targets().is_empty(), egui::Button::new("Удалить").shortcut_text("Del")).clicked() {
                    self.menu_apply(MenuAct::Delete);
                }
                if ui.add(egui::Button::new("Снять выделение").shortcut_text("Esc")).clicked() {
                    self.menu_apply(MenuAct::Deselect);
                }
            });
            ui.menu_button("Вид", |ui| {
                for (label, pane) in [("3D", Pane::Model), ("Вид спереди", Pane::Front), ("Детали", Pane::Parts), ("Раскрой", Pane::Cut), ("Начало", Pane::Start)] {
                    if ui.button(label).clicked() {
                        self.show_pane(pane);
                    }
                }
                ui.separator();
                for (label, yaw, pitch) in [("Спереди", 0.0, 0.0), ("Слева", 90.0, 0.0), ("Сверху", 0.0, 89.9), ("Аксонометрия", crate::viewport::Camera::AXON.0, crate::viewport::Camera::AXON.1)] {
                    if ui.button(format!("Камера: {label}")).clicked() {
                        self.viewport.set_view(yaw, pitch);
                        self.show_pane(Pane::Model);
                    }
                }
                if ui.button("Показать всё").clicked() {
                    self.viewport.refit();
                }
                ui.checkbox(&mut self.quad, "4 окна");
                if ui.checkbox(&mut self.show_doors, "Двери и фасады").changed() {
                    self.revision += 1;
                }
                ui.separator();
                for (mode, label) in Mode::ALL {
                    ui.radio_value(&mut self.viewport.mode, mode, label);
                }
            });
            ui.menu_button("Вставка", |ui| {
                let can_add = editable && self.session.as_ref().is_some_and(|s| !s.project.materials.is_empty());
                for (kind, label) in [("cabinet", "Напольный корпус"), ("wall", "Навесной корпус"), ("corner", "Угловой модуль"), ("countertop", "Столешница"), ("panel", "Отдельная панель")] {
                    if ui.add_enabled(can_add, egui::Button::new(label)).clicked() {
                        self.add_object(kind);
                    }
                }
                ui.separator();
                for (plane, label) in [("horizontal", "Полка в проём"), ("vertical", "Перегородка в проём"), ("front", "Фронтальная панель в проём"), ("door", "Дверь в проём"), ("drawers", "Ящики в проём")] {
                    if ui.add_enabled(can_add, egui::Button::new(label)).clicked() {
                        self.toggle_fill(plane);
                    }
                }
                ui.separator();
                if ui.button("Фурнитура из библиотеки…").clicked() {
                    self.show_pane(Pane::Library);
                }
            });
            ui.menu_button("Изменить", |ui| {
                for (m, label) in [(Modify::Move, "Сдвинуть"), (Modify::Copy, "Копировать"), (Modify::Rotate, "Повернуть"), (Modify::Mirror, "Отразить"), (Modify::MirrorCopy, "Зеркальная копия")] {
                    if ui.add_enabled(editable && selected, egui::Button::new(label)).clicked() {
                        self.fill = None;
                        self.modify = Some(m);
                        self.show_pane(Pane::Tools);
                    }
                }
                ui.separator();
                if ui.add_enabled(editable && selected, egui::Button::new("Собрать в блок")).clicked() {
                    self.menu_apply(MenuAct::Block);
                }
                if ui.add_enabled(editable && selected, egui::Button::new("Разрушить")).clicked() {
                    self.menu_apply(MenuAct::Explode);
                }
            });
            ui.menu_button("Сервис", |ui| {
                if ui.button("Анализ модели").clicked() {
                    self.analyze();
                }
                if ui.add_enabled(editable, egui::Button::new("Выполнить скрипт Базиса…")).clicked() {
                    self.pick_script();
                }
            });
            ui.menu_button("Справка", |ui| {
                if ui.button("Как пользоваться программой").clicked() {
                    self.help = true;
                }
                if ui.button("О программе").clicked() {
                    self.about = true;
                }
            });
        });
    }

    /// Стартовая страница: открыть, новый, пример; справа — последние файлы.
    pub(crate) fn start_page(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full, 0.0, Color32::from_rgb(0x2b, 0x30, 0x3a));
        let left = egui::Rect::from_min_size(full.min, egui::vec2(300.0_f32.min(full.width() * 0.4), full.height()));
        ui.painter().rect_filled(left, 0.0, Color32::from_rgb(0x23, 0x27, 0x30));
        let right = egui::Rect::from_min_max(egui::pos2(left.max.x, full.min.y), full.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(left.shrink2(egui::vec2(28.0, 32.0))), |ui| {
            ui.label(RichText::new("basis-craft").size(30.0).color(Color32::from_rgb(0xdd, 0xe2, 0xea)));
            ui.weak(format!("Версия {} · проектирование корпусной мебели", env!("CARGO_PKG_VERSION")));
            ui.add_space(24.0);
            let width = ui.available_width();
            let wide = |text: &str| egui::Button::new(RichText::new(text).size(15.0)).min_size(egui::vec2(width, 34.0));
            if ui.add(wide("Открыть…")).on_hover_text(".bcraft, .b3d, .fr3d").clicked() {
                self.open_dialog();
                self.leave_start();
            }
            ui.add_space(6.0);
            if ui.add(wide("Новый проект")).clicked() {
                self.new_project();
                self.leave_start();
            }
            ui.add_space(6.0);
            if let Some(sample) = SAMPLES.iter().map(Path::new).find(|p| p.exists())
                && ui.add(wide("Открыть пример")).on_hover_text(sample.display().to_string()).clicked()
            {
                self.open(sample.to_path_buf());
                self.leave_start();
            }
            ui.add_space(6.0);
            if ui.add(wide("Как пользоваться")).clicked() {
                self.help = true;
            }
        });
        ui.scope_builder(egui::UiBuilder::new().max_rect(right.shrink2(egui::vec2(36.0, 32.0))), |ui| {
            ui.label(RichText::new("Последние").size(22.0).color(Color32::from_rgb(0xdd, 0xe2, 0xea)));
            ui.add_space(12.0);
            let list = recent();
            if list.is_empty() {
                ui.weak("Здесь появятся файлы, которые вы открывали и сохраняли.");
            }
            let mut open = None;
            egui::ScrollArea::vertical().id_salt("recent").show(ui, |ui| {
                for p in list {
                    let name = p.file_name().map_or_else(|| p.display().to_string(), |f| f.to_string_lossy().into_owned());
                    let r = ui.add(egui::Button::new(RichText::new(name).size(15.0)).frame(false));
                    ui.weak(p.parent().map(|d| d.display().to_string()).unwrap_or_default());
                    ui.add_space(6.0);
                    if r.clicked() {
                        open = Some(p);
                    }
                }
            });
            if let Some(p) = open {
                self.open(p);
                self.leave_start();
            }
        });
        ui.allocate_rect(full, egui::Sense::hover());
    }

    /// После выбора на стартовой странице — к модели.
    fn leave_start(&mut self) {
        self.show_pane(Pane::Model);
    }

    /// Окна «Как пользоваться» и «О программе».
    pub(crate) fn help_windows(&mut self, ctx: &egui::Context) {
        egui::Window::new("Как пользоваться basis-craft").open(&mut self.help).default_size([620.0, 640.0]).vscroll(true).show(ctx, |ui| {
            markdown(ui, HELP);
        });
        egui::Window::new("О программе").open(&mut self.about).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.heading("basis-craft");
            ui.label(format!("Версия {}", env!("CARGO_PKG_VERSION")));
            ui.label("Проектирование корпусной мебели, совместимое с Базис-Мебельщиком: .b3d, XML для БАЗИС-Раскроя, карты раскроя, спецификация.");
            ui.weak("Иконки и оболочка — по образцу cadcraft (storytold), см. NOTICE.md.");
        });
    }
}
