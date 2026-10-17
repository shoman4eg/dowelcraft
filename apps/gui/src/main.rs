//! Окно basis-craft: проект слева, вид спереди / детали / раскрой в центре.
//! Все изменения проекта идут через `Session::execute` — те же команды, что у CLI и будущего MCP.

use std::path::PathBuf;

use basis::design::{self, Part, Project, Session};
use basis::job::{self, Job, MaterialCut};
use basis::nest::Params;
use basis::panel::{self, Side};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};
use serde_json::{Value, json};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let screenshot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let tab = args.iter().position(|a| a == "--tab").and_then(|i| args.get(i + 1)).cloned();
    let file = args.iter().find(|a| !a.starts_with("--") && Some(a.as_str()) != tab.as_deref() && screenshot.as_deref() != Some(std::path::Path::new(a)));
    let mut app = App::default();
    if let Some(f) = file {
        app.open(PathBuf::from(f));
    }
    app.screenshot = screenshot;
    app.tab = match tab.as_deref() {
        Some("parts") => Tab::Parts,
        Some("cut") => Tab::Cut,
        _ => Tab::Front,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]).with_title("basis-craft"),
        ..Default::default()
    };
    eframe::run_native("basis-craft", options, Box::new(|_| Ok(Box::new(app))))
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Front,
    Parts,
    Cut,
}

/// Поля формы нового материала / кромки.
struct NewMaterial {
    name: String,
    thickness: f64,
    sheet: (f64, f64),
    coef: f64,
}

struct App {
    /// Свой проект; нет — открыт файл Базиса только для просмотра.
    session: Option<Session>,
    job: Job,
    path: Option<PathBuf>,
    tab: Tab,
    params: Params,
    cuts: Vec<MaterialCut>,
    card: usize,
    show_doors: bool,
    error: Option<String>,
    status: String,
    new_material: NewMaterial,
    new_edge: (String, f64, f64),
    screenshot: Option<PathBuf>,
    frames: u32,
}

impl Default for App {
    fn default() -> Self {
        let mut app = App {
            session: Some(Session::new(Project::new("Новый проект"))),
            job: Job::from_project(Project::new("Новый проект")),
            path: None,
            tab: Tab::Front,
            params: Params { sheet: (2800.0, 2070.0), kerf: 4.0, trim: 10.0 },
            cuts: Vec::new(),
            card: 0,
            show_doors: true,
            error: None,
            status: String::new(),
            new_material: NewMaterial { name: "ЛДСП 16".into(), thickness: 16.0, sheet: (2800.0, 2070.0), coef: 1.2 },
            new_edge: ("ПВХ 0,4".into(), 0.4, 19.0),
            screenshot: None,
            frames: 0,
        };
        app.refresh();
        app
    }
}

impl App {
    fn open(&mut self, path: PathBuf) {
        match Job::open(&path) {
            Ok(job) => {
                self.session = job.project.clone().map(Session::new);
                self.job = job;
                self.status = format!("Открыт {}", path.display());
                self.path = Some(path);
                self.error = None;
                self.card = 0;
                self.refresh();
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Пересобрать детали и раскрой после изменения.
    fn refresh(&mut self) {
        if let Some(s) = &self.session {
            self.job = Job::from_project(s.project.clone());
        }
        let (job, params) = (&self.job, self.params);
        self.cuts = job::cut(job, |m| Params { sheet: job.sheet(m, params.sheet), ..params });
    }

    fn exec(&mut self, cmd: &str, p: Value) {
        let Some(s) = &mut self.session else { return };
        match s.execute(cmd, &p) {
            Ok(_) => {
                self.error = None;
                self.refresh();
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn save(&mut self, ask: bool) {
        let Some(s) = &self.session else {
            self.error = Some("Модель Базиса открыта только для просмотра".into());
            return;
        };
        let path = match (&self.path, ask) {
            (Some(p), false) if p.extension().is_some_and(|e| e == "bcraft") => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("Проект basis-craft", &["bcraft"]).set_file_name(format!("{}.bcraft", s.project.name)).save_file(),
        };
        if let Some(p) = path {
            match std::fs::write(&p, s.project.save()) {
                Ok(()) => {
                    self.status = format!("Сохранено: {}", p.display());
                    self.path = Some(p);
                }
                Err(e) => self.error = Some(format!("{}: {e}", p.display())),
            }
        }
    }

    fn export(&mut self, what: &str) {
        let (ext, filter) = match what {
            "html" => ("html", "Карты раскроя"),
            "xml" => ("xml", "Проект БАЗИС-Раскроя"),
            "spec" | "smeta" => ("csv", "Таблица CSV"),
            _ => return,
        };
        let suffix = match what {
            "spec" => " спецификация",
            "smeta" => " смета",
            _ => "",
        };
        let Some(p) = rfd::FileDialog::new().add_filter(filter, &[ext]).set_file_name(format!("{}{suffix}.{ext}", self.job.name)).save_file() else {
            return;
        };
        let job = &self.job;
        let sheet = self.params.sheet;
        let bytes = match what {
            "html" => job::document(job, &self.cuts).into_bytes(),
            "xml" => basis::raskroy::project_xml(&job.name, &job.panels, |m| job.sheet(m, sheet)),
            "spec" => basis::report::spec_csv(&panel::spec(&job.panels)).into_bytes(),
            _ => basis::report::materials_csv(&panel::materials(&job.panels, |m| job.coef(m, 1.2))).into_bytes(),
        };
        match std::fs::write(&p, bytes) {
            Ok(()) => self.status = format!("Сохранено: {}", p.display()),
            Err(e) => self.error = Some(format!("{}: {e}", p.display())),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.top_bar(ui);
        egui::Panel::bottom("status").show(ui, |ui| match &self.error {
            Some(e) => {
                ui.colored_label(Color32::from_rgb(200, 40, 40), e);
            }
            None => {
                ui.label(&self.status);
            }
        });
        if self.session.is_some() {
            egui::Panel::left("project").default_size(360.0).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.project_panel(ui));
            });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Front, "Вид спереди");
                ui.selectable_value(&mut self.tab, Tab::Parts, "Детали");
                ui.selectable_value(&mut self.tab, Tab::Cut, "Раскрой");
            });
            ui.separator();
            match self.tab {
                Tab::Front => self.front_view(ui),
                Tab::Parts => self.parts_table(ui),
                Tab::Cut => self.cut_view(ui),
            }
        });
        self.take_screenshot(&ctx);
    }
}

impl App {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Файл", |ui| {
                    if ui.button("Новый проект").clicked() {
                        *self = App { params: self.params, ..App::default() };
                    }
                    if ui.button("Открыть…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("Модели", &["bcraft", "b3d", "fr3d"]).pick_file() {
                            self.open(p);
                        }
                    }
                    if ui.button("Сохранить").clicked() {
                        self.save(false);
                    }
                    if ui.button("Сохранить как…").clicked() {
                        self.save(true);
                    }
                });
                ui.menu_button("Экспорт", |ui| {
                    for (what, label) in [
                        ("html", "Карты раскроя и «Панели в производство» (HTML)…"),
                        ("xml", "Проект для БАЗИС-Раскроя (XML)…"),
                        ("spec", "Спецификация (CSV)…"),
                        ("smeta", "Смета материалов (CSV)…"),
                    ] {
                        if ui.button(label).clicked() {
                            self.export(what);
                        }
                    }
                });
                let (undo, redo) = self.session.as_ref().map_or((false, false), |s| (s.can_undo(), s.can_redo()));
                if ui.add_enabled(undo, egui::Button::new("Отменить")).clicked() {
                    self.exec("undo", json!({}));
                }
                if ui.add_enabled(redo, egui::Button::new("Вернуть")).clicked() {
                    self.exec("redo", json!({}));
                }
                ui.separator();
                ui.label(format!("{} — деталей: {}", self.job.name, self.job.panels.len()));
                if self.session.is_none() {
                    ui.weak("(модель Базиса, только просмотр)");
                }
            });
        });
    }

    fn project_panel(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.session else { return };
        let project = s.project.clone();
        let mut cmds: Vec<(&'static str, Value)> = Vec::new();

        ui.heading("Проект");
        let mut name = project.name.clone();
        if ui.text_edit_singleline(&mut name).lost_focus() && name != project.name {
            cmds.push(("project.rename", json!({ "name": name })));
        }

        egui::CollapsingHeader::new(format!("Материалы ({})", project.materials.len())).default_open(project.materials.is_empty()).show(ui, |ui| {
            for m in &project.materials {
                ui.label(format!("{} — {} мм, лист {}×{}, коэф. {}", m.name, m.thickness, m.sheet.0, m.sheet.1, m.coef));
            }
            let nm = &mut self.new_material;
            egui::Grid::new("new_mat").num_columns(2).show(ui, |ui| {
                ui.label("Название");
                ui.text_edit_singleline(&mut nm.name);
                ui.end_row();
                ui.label("Толщина, мм");
                ui.add(egui::DragValue::new(&mut nm.thickness).range(0.1..=100.0));
                ui.end_row();
                ui.label("Лист, мм");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut nm.sheet.0).range(100.0..=6000.0));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut nm.sheet.1).range(100.0..=6000.0));
                });
                ui.end_row();
                ui.label("Коэффициент");
                ui.add(egui::DragValue::new(&mut nm.coef).range(1.0..=3.0).speed(0.01));
                ui.end_row();
            });
            if ui.button("+ Добавить материал").clicked() {
                cmds.push(("material.add", json!({ "name": nm.name, "thickness": nm.thickness, "sheet": [nm.sheet.0, nm.sheet.1], "coef": nm.coef })));
            }
        });

        egui::CollapsingHeader::new(format!("Кромка ({})", project.edges.len())).show(ui, |ui| {
            for e in &project.edges {
                ui.label(format!("{} — {} мм × {} мм", e.name, e.thickness, e.width));
            }
            let (n, t, w) = &mut self.new_edge;
            ui.horizontal(|ui| {
                ui.text_edit_singleline(n);
                ui.add(egui::DragValue::new(t).range(0.1..=5.0).speed(0.1).suffix(" мм"));
                ui.add(egui::DragValue::new(w).range(1.0..=100.0).suffix(" мм"));
            });
            if ui.button("+ Добавить кромку").clicked() {
                cmds.push(("edge.add", json!({ "name": n, "thickness": t, "width": w })));
            }
        });

        ui.separator();
        ui.heading("Корпуса");
        for c in &project.cabinets {
            egui::CollapsingHeader::new(format!("{} ({}×{}×{})", c.name, c.width, c.height, c.depth)).id_salt(c.id).default_open(true).show(ui, |ui| {
                cabinet_editor(ui, &project, c, &mut cmds);
            });
        }
        ui.separator();
        ui.heading("Столешницы");
        for ct in &project.countertops {
            egui::CollapsingHeader::new(format!("{} ({}×{})", ct.name, ct.length, ct.depth)).id_salt(("ct", ct.id)).show(ui, |ui| {
                let id = ct.id;
                egui::Grid::new(("ctg", id)).num_columns(2).show(ui, |ui| {
                    for (key, label, value) in [("length", "Длина", ct.length), ("depth", "Глубина", ct.depth)] {
                        ui.label(label);
                        let mut v = value;
                        let r = ui.add(egui::DragValue::new(&mut v).range(10.0..=6000.0).suffix(" мм"));
                        if (r.drag_stopped() || (r.changed() && !r.dragged())) && v != value {
                            cmds.push(("countertop.set", json!({ "id": id, key: v })));
                        }
                        ui.end_row();
                    }
                    ui.label("Положение X, Y, Z");
                    ui.horizontal(|ui| {
                        let mut pos = ct.position;
                        let mut done = false;
                        for v in &mut pos {
                            let r = ui.add(egui::DragValue::new(v).range(-10000.0..=10000.0));
                            done |= r.drag_stopped() || (r.changed() && !r.dragged());
                        }
                        if done && pos != ct.position {
                            cmds.push(("countertop.set", json!({ "id": id, "position": pos })));
                        }
                    });
                    ui.end_row();
                });
                if ui.button("Удалить столешницу").clicked() {
                    cmds.push(("countertop.remove", json!({ "id": id })));
                }
            });
        }
        if let Some(m) = project.materials.first()
            && ui.button("+ Столешница над корпусами").clicked()
        {
            // По умолчанию — на напольные корпуса (стоят на полу), по всей их длине, с выносом 20 мм вперёд.
            let floor: Vec<&design::Cabinet> = project.cabinets.iter().filter(|c| c.position[1] <= 0.0).collect();
            let x0 = floor.iter().map(|c| c.position[0]).fold(f64::MAX, f64::min).min(0.0);
            let x1 = floor.iter().map(|c| c.position[0] + c.width).fold(600.0, f64::max);
            let top = floor.iter().map(|c| c.height).fold(720.0, f64::max);
            let depth = floor.iter().map(|c| c.depth).fold(560.0, f64::max) + 20.0;
            cmds.push(("countertop.add", json!({ "material": m.name, "length": x1 - x0, "depth": depth,
                "position": [x0, top, 0], "edge": project.edges.first().map(|e| e.name.clone()) })));
        }
        let first = project.materials.first().map(|m| m.name.clone());
        if ui.add_enabled(first.is_some(), egui::Button::new("+ Корпус")).on_disabled_hover_text("Сначала добавьте материал").clicked() {
            cmds.push((
                "cabinet.add",
                json!({ "width": 800, "height": 2000, "depth": 600, "material": first, "plinth": 80,
                        "edge": project.edges.first().map(|e| e.name.clone()), "doors": 2 }),
            ));
        }
        for (cmd, p) in cmds {
            self.exec(cmd, p);
        }
    }

    fn front_view(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.show_doors, "Показывать двери и фасады");
        let parts = match job::parts(&self.job) {
            Ok(p) => p,
            Err(e) => {
                ui.colored_label(Color32::RED, e);
                return;
            }
        };
        if parts.is_empty() {
            ui.label("Добавьте материал и корпус в панели слева.");
            return;
        }
        let rects = design::front_view(&parts, self.show_doors);
        let Some((lo, hi)) = design::bounds(&rects) else { return };
        let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::hover());
        let area = resp.rect.shrink(30.0);
        let k = ((area.width() as f64) / (hi[0] - lo[0])).min((area.height() as f64) / (hi[1] - lo[1]));
        let to = |x: f64, y: f64| Pos2::new(area.left() + ((x - lo[0]) * k) as f32, area.bottom() - ((y - lo[1]) * k) as f32);
        painter.rect_filled(resp.rect, 0.0, Color32::from_gray(245));
        let mut hovered: Option<&Part> = None;
        for fr in &rects {
            let r = Rect::from_two_pos(to(fr.x, fr.y), to(fr.x + fr.w, fr.y + fr.h));
            let fill = match fr.kind {
                design::Kind::Door | design::Kind::Drawer => Color32::from_rgba_unmultiplied(170, 200, 230, 150),
                design::Kind::Back => Color32::from_gray(225),
                design::Kind::Body => Color32::from_rgb(236, 220, 190),
            };
            painter.rect(r, 0.0, fill, Stroke::new(1.0, Color32::from_gray(60)), StrokeKind::Inside);
            if resp.hovered() && ui.ctx().pointer_hover_pos().is_some_and(|m| r.contains(m)) {
                hovered = Some(&parts[fr.part]);
            }
        }
        // Габаритные размеры корпусов.
        if let Some(c) = self.session.as_ref().and_then(|s| s.project.cabinets.first()) {
            let y = c.position[1] - 0.0;
            painter.text(to(c.position[0] + c.width / 2.0, y) + Vec2::new(0.0, 14.0), egui::Align2::CENTER_CENTER, format!("{}", c.width), egui::FontId::proportional(14.0), Color32::BLACK);
            painter.text(to(c.position[0], c.position[1] + c.height / 2.0) - Vec2::new(16.0, 0.0), egui::Align2::CENTER_CENTER, format!("{}", c.height), egui::FontId::proportional(14.0), Color32::BLACK);
        }
        if let Some(p) = hovered {
            let (l, w) = p.panel.cut_size();
            resp.on_hover_text(format!("{} {}: {}×{}×{} мм\n{}", p.panel.pos, p.panel.name, l, w, p.panel.thickness, panel::material_name(&p.panel.material)));
        }
    }

    fn parts_table(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("parts").striped(true).num_columns(10).show(ui, |ui| {
                for h in ["№", "Наименование", "Материал", "Длина", "Ширина", "Толщина", "L1", "L2", "W1", "W2"] {
                    ui.strong(h);
                }
                ui.end_row();
                for (i, p) in self.job.panels.iter().enumerate() {
                    let (l, w) = p.cut_size();
                    ui.label(self.job.number(i));
                    ui.label(&p.name);
                    ui.label(panel::material_name(&p.material));
                    ui.label(format!("{l:.1}"));
                    ui.label(format!("{w:.1}"));
                    ui.label(format!("{}", p.thickness));
                    for sd in [Side::L1, Side::L2, Side::W1, Side::W2] {
                        let t = p.edges.iter().filter(|e| p.side(e) == sd).map(|e| e.thickness).fold(0.0, f64::max);
                        ui.label(if t > 0.0 { format!("{t}") } else { String::new() });
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn cut_view(&mut self, ui: &mut egui::Ui) {
        let before = self.params;
        ui.horizontal(|ui| {
            ui.label("Лист по умолчанию");
            ui.add(egui::DragValue::new(&mut self.params.sheet.0).range(500.0..=6000.0));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut self.params.sheet.1).range(500.0..=6000.0));
            ui.label("Пропил");
            ui.add(egui::DragValue::new(&mut self.params.kerf).range(0.0..=10.0).speed(0.1));
            ui.label("Обрезка");
            ui.add(egui::DragValue::new(&mut self.params.trim).range(0.0..=50.0));
        });
        if self.params != before {
            self.refresh();
        }
        let sheets: Vec<(usize, usize)> = self.cuts.iter().enumerate().flat_map(|(m, c)| (0..c.layout.sheets.len()).map(move |k| (m, k))).collect();
        if sheets.is_empty() {
            ui.label("Нет деталей для раскроя.");
            return;
        }
        self.card = self.card.min(sheets.len() - 1);
        ui.horizontal_wrapped(|ui| {
            for (i, &(m, k)) in sheets.iter().enumerate() {
                let c = &self.cuts[m];
                let label = format!("{} — {}", panel::material_name(&c.material), k + 1);
                ui.selectable_value(&mut self.card, i, label);
            }
        });
        for c in &self.cuts {
            for &id in &c.layout.too_big {
                ui.colored_label(Color32::RED, format!("Не помещается на лист: № {} «{}»", self.job.number(id), self.job.panels[id].name));
            }
        }
        let (m, k) = sheets[self.card];
        let cut = &self.cuts[m];
        let sheet = &cut.layout.sheets[k];
        ui.label(format!("Деталей на листе: {}, использование {:.1}%", sheet.parts.len(), sheet.usage(&cut.params) * 100.0));
        let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::hover());
        let (sw, sh) = cut.params.sheet;
        let area = resp.rect.shrink(20.0);
        let kx = ((area.width() as f64) / sw).min((area.height() as f64) / sh) as f32;
        let to = |x: f64, y: f64| area.left_top() + Vec2::new(x as f32 * kx, y as f32 * kx);
        painter.rect(Rect::from_min_size(to(0.0, 0.0), Vec2::new(sw as f32 * kx, sh as f32 * kx)), 0.0, Color32::from_rgb(250, 225, 225), Stroke::new(1.0, Color32::BLACK), StrokeKind::Inside);
        for f in &sheet.free {
            if f.w.min(f.h) >= 200.0 && f.w.max(f.h) >= 300.0 {
                let r = Rect::from_min_size(to(f.x, f.y), Vec2::new(f.w as f32 * kx, f.h as f32 * kx));
                painter.rect(r, 0.0, Color32::from_rgb(215, 220, 250), Stroke::new(0.5, Color32::GRAY), StrokeKind::Inside);
                painter.text(r.center(), egui::Align2::CENTER_CENTER, format!("Обрезок {:.0}×{:.0}", f.w, f.h), egui::FontId::proportional(11.0), Color32::from_rgb(40, 40, 140));
            }
        }
        for d in &sheet.parts {
            let r = Rect::from_min_size(to(d.x, d.y), Vec2::new(d.w as f32 * kx, d.h as f32 * kx));
            painter.rect(r, 0.0, Color32::WHITE, Stroke::new(1.0, Color32::from_rgb(220, 0, 0)), StrokeKind::Inside);
            let l = job::label(&self.job, d.id);
            let text = format!("{}\n{:.0}×{:.0}", l.num, l.size.0, l.size.1);
            let size = (r.height().min(r.width()) / 4.0).clamp(8.0, 14.0);
            painter.text(r.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(size), Color32::BLACK);
        }
    }

    /// `--screenshot файл.png`: после нескольких кадров сохранить окно и выйти.
    fn take_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frames += 1;
        if self.frames == 5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = image {
            if let Err(e) = save_png(&path, &img) {
                eprintln!("снимок: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

fn save_png(path: &std::path::Path, img: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    let data: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    w.write_image_data(&data).map_err(|e| e.to_string())
}

/// Поля корпуса. Каждое изменение — команда `cabinet.set` и т.п.
fn cabinet_editor(ui: &mut egui::Ui, project: &Project, c: &design::Cabinet, cmds: &mut Vec<(&'static str, Value)>) {
    let id = c.id;
    let mut set = |key: &str, v: Value| cmds.push(("cabinet.set", json!({ "id": id, key: v })));
    egui::Grid::new(("cab", id)).num_columns(2).show(ui, |ui| {
        let mut name = c.name.clone();
        ui.label("Название");
        if ui.text_edit_singleline(&mut name).lost_focus() && name != c.name {
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
