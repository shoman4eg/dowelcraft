//! Окно basis-craft: проект слева, вид спереди / детали / раскрой в центре.
//! Все изменения проекта идут через `Session::execute` — те же команды, что у CLI и будущего MCP.

use std::path::PathBuf;

mod editors;
mod expr;
mod viewport;

use craft_ui::docking::{Action, DockArea, DockStyle, Layout, Node, Permissions};
use craft_ui::layout::{SplitAxis, SplitSize};
use editors::{Cmds, text_field};

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
    // Режим 3D для снимков: solid | hidden | wire | xray.
    let mode = args.iter().position(|a| a == "--mode").and_then(|i| args.get(i + 1)).cloned();
    let file = args.iter().enumerate().find(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--"))).map(|(_, a)| a);
    let mut app = App::default();
    if let Some(f) = file {
        app.open(PathBuf::from(f));
    }
    app.screenshot = screenshot;
    app.viewport.mode = match mode.as_deref() {
        Some("hidden") => viewport::Mode::Hidden,
        Some("wire") => viewport::Mode::Wire,
        Some("xray") => viewport::Mode::XRay,
        _ => viewport::Mode::Solid,
    };
    let pane = match tab.as_deref() {
        Some("parts") => Pane::Parts,
        Some("cut") => Pane::Cut,
        Some("front") => Pane::Front,
        Some("library") => Pane::Library,
        _ => Pane::Model,
    };
    let _ = app.layout.apply(Action::Activate { panel: pane });
    app.quad = args.iter().any(|a| a == "--quad");
    // Для снимков: выделить деталь по номеру.
    if let Some(i) = args.iter().position(|a| a == "--select").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()) {
        app.select(Some(i));
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]).with_title("basis-craft"),
        ..Default::default()
    };
    eframe::run_native("basis-craft", options, Box::new(|_| Ok(Box::new(app))))
}

/// Панели окна (раскладка — craft-ui docking).
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum Pane {
    Model,
    Front,
    Parts,
    Cut,
    Structure,
    Project,
    Properties,
    Library,
}

impl Pane {
    fn label(self) -> &'static str {
        match self {
            Pane::Model => "3D",
            Pane::Front => "Вид спереди",
            Pane::Parts => "Детали",
            Pane::Cut => "Раскрой",
            Pane::Structure => "Структура модели",
            Pane::Project => "Проект",
            Pane::Properties => "Свойства",
            Pane::Library => "Фурнитура",
        }
    }
}

/// Раскладка по умолчанию: слева структура и проект, в центре 3D и вкладки, справа свойства.
fn default_layout() -> Layout<Pane> {
    let tabs = |panels: Vec<Pane>| Box::new(Node::Tabs { panels, active: 0 });
    Layout {
        root: Some(Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedFirst(300.0),
            first: tabs(vec![Pane::Structure, Pane::Project, Pane::Library]),
            second: Box::new(Node::Split {
                axis: SplitAxis::Horizontal,
                size: SplitSize::FixedSecond(380.0),
                first: tabs(vec![Pane::Model, Pane::Front, Pane::Parts, Pane::Cut]),
                second: tabs(vec![Pane::Properties]),
            }),
        }),
        floating: Vec::new(),
    }
}

/// Панели не закрываются (иначе их не вернуть), но двигаются и отрываются.
fn permissions(_: &Pane) -> Permissions {
    Permissions { closable: false, ..Permissions::default() }
}

/// Скрипт Базиса с открытыми окнами.
struct ScriptWin {
    name: String,
    rt: Option<script::Runtime>,
    /// Итог, когда скрипт завершён (сообщения, ошибка).
    done: Option<script::Run>,
}

/// Событие окна скрипта: щелчок или новое значение поля.
enum ScriptEvent {
    Click(i64),
    Change(i64, &'static str, Value),
}

/// Элемент окна скрипта по его описанию (JSON из `__forms()`).
fn script_control(ui: &mut egui::Ui, c: &Value, ev: &mut Vec<ScriptEvent>) {
    let id = c["__id"].as_i64().unwrap_or(-1);
    let caption = c["Caption"].as_str().unwrap_or("");
    let kids = |ui: &mut egui::Ui, ev: &mut Vec<ScriptEvent>| {
        for k in c["Children"].as_array().into_iter().flatten() {
            script_control(ui, k, ev);
        }
    };
    if c["Visible"] == json!(false) {
        return;
    }
    match c["Kind"].as_str().unwrap_or("") {
        "Form" | "GroupBox" | "Group" | "Panel" | "Tab" | "TabSheet" => {
            if caption.is_empty() || c["Kind"] == "Form" {
                kids(ui, ev);
            } else {
                ui.group(|ui| {
                    ui.strong(caption);
                    kids(ui, ev);
                });
            }
        }
        "Label" => {
            ui.label(if caption.is_empty() { c["Text"].as_str().unwrap_or("") } else { caption });
        }
        "Button" | "ImageButton" => {
            if ui.button(if caption.is_empty() { "Кнопка" } else { caption }).clicked() {
                ev.push(ScriptEvent::Click(id));
            }
        }
        "CheckBox" | "Bool" | "RadioButton" => {
            let key = if c["Kind"] == "Bool" { "Value" } else { "Checked" };
            let mut v = c[key].as_bool().unwrap_or(false);
            if ui.checkbox(&mut v, caption).changed() {
                ev.push(ScriptEvent::Change(id, key, json!(v)));
                ev.push(ScriptEvent::Click(id));
            }
        }
        "Number" => {
            ui.horizontal(|ui| {
                ui.label(caption);
                let key = if c.get("Value").is_some() { "Value" } else { "Text" };
                let mut v = c[key].as_f64().or_else(|| c[key].as_str().and_then(|s| s.replace(',', ".").parse().ok())).unwrap_or(0.0);
                if ui.add(crate::expr::drag(&mut v).speed(1.0)).changed() {
                    ev.push(ScriptEvent::Change(id, "Value", json!(v)));
                }
            });
        }
        "ComboBox" | "Combo" | "ListBox" => {
            let items: Vec<String> = c["Items"].as_array().into_iter().flatten().map(|v| v.as_str().map_or(v.to_string(), str::to_string)).collect();
            let mut i = c["ItemIndex"].as_i64().unwrap_or(-1);
            ui.horizontal(|ui| {
                ui.label(caption);
                let shown = usize::try_from(i).ok().and_then(|k| items.get(k)).cloned().unwrap_or_default();
                egui::ComboBox::from_id_salt(("script", id)).selected_text(shown).show_ui(ui, |ui| {
                    for (k, it) in items.iter().enumerate() {
                        ui.selectable_value(&mut i, k as i64, it);
                    }
                });
            });
            if Some(i) != c["ItemIndex"].as_i64() {
                ev.push(ScriptEvent::Change(id, "ItemIndex", json!(i)));
            }
        }
        _ => {
            // Текстовые поля и всё прочее с текстом.
            ui.horizontal(|ui| {
                if !caption.is_empty() {
                    ui.label(caption);
                }
                let key = if c["Kind"] == "String" { "Value" } else { "Text" };
                let mut t = c[key].as_str().map_or_else(|| c[key].to_string(), str::to_string);
                if ui.text_edit_singleline(&mut t).changed() {
                    ev.push(ScriptEvent::Change(id, key, json!(t)));
                }
            });
        }
    }
}

/// Библиотека моделей фурнитуры в окне.
struct Library {
    dir: String,
    models: Vec<basis::library::ModelInfo>,
    errors: Vec<String>,
    search: String,
    thumbs: std::collections::HashMap<PathBuf, egui::TextureHandle>,
}

impl Default for Library {
    fn default() -> Self {
        // По умолчанию — папка с примерами проекта, если есть.
        let dir = if std::path::Path::new("example").exists() { "example" } else { "." };
        Library { dir: dir.into(), models: Vec::new(), errors: Vec::new(), search: String::new(), thumbs: Default::default() }
    }
}

/// PNG → картинка egui.
fn decode_png(data: &[u8]) -> Option<egui::ColorImage> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        _ => return None,
    };
    Some(egui::ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &rgba))
}

/// Что делает щелчок по схеме контура панели.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ContourTool {
    /// Нанести или снять кромку на торце.
    Edge,
    /// Скруглить угол в вершине.
    Round,
    /// Срезать угол фаской.
    Chamfer,
    /// Круглый вырез в точке.
    Hole,
}

/// Режим установки панели в проём (как «~» в Базисе).
struct FillTool {
    /// horizontal | vertical | front — панель; door — дверь; drawers — ящики.
    plane: &'static str,
    material: String,
    /// Отступы от границ в осях контура (см. команду `panel.fill`).
    gaps: [f64; 4],
    /// Двери и ящики: накладная, полунакладная, вкладная.
    mount: &'static str,
    double: bool,
    /// Высоты фасадов ящиков снизу вверх, через пробел.
    heights: String,
}

/// Выделение: объект верхнего уровня, панель дерева (0 — деталь шаблона) и номер детали внутри объекта.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Sel {
    owner: u32,
    id: u32,
    nth: usize,
}

impl Sel {
    fn of(parts: &[Part], i: usize) -> Sel {
        let p = &parts[i];
        let nth = parts[..i].iter().filter(|q| q.owner == p.owner && q.id == p.id).count();
        Sel { owner: p.owner, id: p.id, nth }
    }

    /// Индекс выделенной детали в текущем списке деталей.
    fn find(&self, parts: &[Part]) -> Option<usize> {
        parts.iter().enumerate().filter(|(_, p)| p.owner == self.owner && p.id == self.id).nth(self.nth).map(|(i, _)| i)
    }
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
    layout: Layout<Pane>,
    /// Выделенные детали; последняя — основная (её свойства показываются).
    selection: Vec<Sel>,
    /// Параметры действий с выделенным: сдвиг/копия (dx, dy, dz), ось, угол, число копий.
    actions: ([f64; 3], char, f64, u32),
    /// Перетаскивание на виде спереди: сдвиг в мм.
    drag: Option<[f64; 2]>,
    /// Нажат Ctrl (на этом кадре): щелчок добавляет к выделению.
    ctx_ctrl: bool,
    params: Params,
    cuts: Vec<MaterialCut>,
    card: usize,
    show_doors: bool,
    error: Option<String>,
    status: String,
    new_material: NewMaterial,
    new_edge: (String, f64, f64),
    /// Новая панель: плоскость и размеры контура.
    new_panel: (&'static str, f64, f64),
    /// Режим «Панель в проём» на виде спереди: плоскость, материал, отступы.
    fill: Option<FillTool>,
    /// Облицовка: кромка, «Подрезать», припуск.
    edging: (Option<String>, bool, f64),
    /// Библиотека моделей фурнитуры: папка, найденные модели, превью (текстуры по пути), строка поиска.
    library: Library,
    /// «Анализ модели»: найденные проблемы (окно открыто, пока Some).
    problems: Option<Vec<design::analysis::Problem>>,
    /// Выполняемый скрипт Базиса и его окна.
    script: Option<ScriptWin>,
    /// Выделенная фурнитура (у неё нет деталей — выделяется отдельно).
    furniture_sel: Option<u32>,
    /// Крепёж для установки между панелями.
    fastener_kind: String,
    /// Действие щелчка по схеме контура и его размер (радиус, катет фаски, радиус выреза), мм.
    contour_tool: (ContourTool, f64),
    screenshot: Option<PathBuf>,
    frames: u32,
    /// Детали изделия с положением (или ошибка сборки) — пересобираются в `refresh`.
    parts: Result<Vec<Part>, String>,
    /// Растёт при каждом изменении деталей или их видимости: окно модели перестраивает сетку.
    revision: u64,
    viewport: viewport::Viewport,
    /// «4 окна» (как «Проекции» в Базисе): спереди, слева, сверху и основное окно — аксонометрия.
    quad: bool,
    views: [viewport::Viewport; 3],
}

impl Default for App {
    fn default() -> Self {
        let mut app = App {
            session: Some(Session::new(Project::new("Новый проект"))),
            job: Job::from_project(Project::new("Новый проект")),
            path: None,
            layout: default_layout(),
            selection: Vec::new(),
            actions: ([0.0; 3], 'x', 90.0, 1),
            drag: None,
            ctx_ctrl: false,
            params: Params { sheet: (2800.0, 2070.0), kerf: 4.0, trim: 10.0 },
            cuts: Vec::new(),
            card: 0,
            show_doors: true,
            error: None,
            status: String::new(),
            new_material: NewMaterial { name: "ЛДСП 16".into(), thickness: 16.0, sheet: (2800.0, 2070.0), coef: 1.2 },
            new_edge: ("ПВХ 0,4".into(), 0.4, 19.0),
            new_panel: ("vertical", 560.0, 720.0),
            fill: None,
            edging: (None, true, 0.0),
            contour_tool: (ContourTool::Edge, 50.0),
            fastener_kind: "Конфирмат 7х50".into(),
            library: Library::default(),
            script: None,
            problems: None,
            furniture_sel: None,
            screenshot: None,
            frames: 0,
            parts: Ok(Vec::new()),
            revision: 0,
            viewport: viewport::Viewport::default(),
            quad: false,
            views: [viewport::Viewport::with_view(0.0, 0.0, "Спереди"), viewport::Viewport::with_view(90.0, 0.0, "Слева"), viewport::Viewport::with_view(0.0, 89.9, "Сверху")],
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
        self.cuts = job::cut(job, |m| job.params(m, params));
        self.parts = job::parts(&self.job);
        self.revision += 1;
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
            "b3d" => ("b3d", "Модель Базис-Мебельщика"),
            "drawings" => ("html", "Чертежи"),
            _ => return,
        };
        let suffix = match what {
            "spec" => " спецификация",
            "drawings" => " чертежи",
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
            "drawings" => match job::parts(job) {
                Ok(parts) => basis::report::document_html(&job.name, &basis::drawing::drawings(&parts, &job.name), "").into_bytes(),
                Err(e) => return self.error = Some(e),
            },
            "b3d" => match job.project.as_ref().map(basis::export::b3d) {
                Some(Ok(b)) => b,
                Some(Err(e)) => return self.error = Some(e),
                None => return self.error = Some("Модель Базиса открыта для просмотра: «Сделать редактируемой», затем экспорт".into()),
            },
            _ => basis::report::materials_csv(&job.estimate(1.2)).into_bytes(),
        };
        match std::fs::write(&p, bytes) {
            Ok(()) => self.status = format!("Сохранено: {}", p.display()),
            Err(e) => self.error = Some(format!("{}: {e}", p.display())),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.ctx_ctrl = ui.input(|i| i.modifiers.command);
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
        egui::CentralPanel::default().show(ui, |ui| {
            let layout = self.layout.clone();
            let style = DockStyle::from_ui(ui);
            let out = DockArea::new(egui::Id::new("dock")).show(ui, &layout, &style, |p| p.label().to_string(), permissions, |ui, pane| match pane {
                Pane::Model => self.iso_view(ui, frame),
                Pane::Front => self.front_view(ui),
                Pane::Parts => self.parts_table(ui),
                Pane::Cut => self.cut_view(ui),
                Pane::Structure => egui::ScrollArea::vertical().id_salt("structure").show(ui, |ui| self.structure_panel(ui)).inner,
                Pane::Project => egui::ScrollArea::vertical().id_salt("project").show(ui, |ui| self.project_panel(ui)).inner,
                Pane::Properties => egui::ScrollArea::vertical().id_salt("props").show(ui, |ui| self.properties_panel(ui)).inner,
                Pane::Library => self.library_panel(ui),
            });
            for a in out.actions {
                if let Err(e) = self.layout.apply_with_permissions(a, permissions) {
                    self.status = format!("Раскладка: {e:?}");
                }
            }
        });
        self.script_window(&ctx);
        self.problems_window(&ctx);
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
                    if ui.button("Открыть…").clicked()
                        && let Some(p) = rfd::FileDialog::new().add_filter("Модели", &["bcraft", "b3d", "fr3d"]).pick_file()
                    {
                        self.open(p);
                    }
                    if ui.button("Сохранить").clicked() {
                        self.save(false);
                    }
                    if ui.button("Сохранить как…").clicked() {
                        self.save(true);
                    }
                });
                if ui.button("Анализ модели").clicked() {
                    let project = self.session.as_ref().map(|s| s.project.clone()).unwrap_or_else(|| self.job.import());
                    match design::analysis::analyze(&project) {
                        Ok(p) => self.problems = Some(p),
                        Err(e) => self.error = Some(e),
                    }
                }
                ui.menu_button("Скрипты", |ui| {
                    if ui.add_enabled(self.session.is_some(), egui::Button::new("Выполнить скрипт Базиса…")).on_disabled_hover_text("Сначала «Сделать редактируемой»").clicked()
                        && let Some(p) = rfd::FileDialog::new().add_filter("BAZIS-Script", &["js"]).pick_file()
                    {
                        self.start_script(&p);
                    }
                });
                ui.menu_button("Экспорт", |ui| {
                    for (what, label) in [
                        ("html", "Карты раскроя и «Панели в производство» (HTML)…"),
                        ("xml", "Проект для БАЗИС-Раскроя (XML)…"),
                        ("spec", "Спецификация (CSV)…"),
                        ("smeta", "Смета материалов (CSV)…"),
                        ("b3d", "Модель Базис-Мебельщика (.b3d)…"),
                        ("drawings", "Чертежи: сборочный и детали (HTML для печати)…"),
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
                    if ui.button("Сделать редактируемой").on_hover_text("Импорт в свой проект: детали можно менять и дополнять корпусами").clicked() {
                        let p = self.job.import();
                        self.status = format!("Импортировано деталей: {}", p.panels().len());
                        self.session = Some(Session::new(p));
                        self.path = None;
                        self.refresh();
                    }
                }
            });
        });
    }

    fn project_panel(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.session else { return };
        let project = s.project.clone();
        let mut cmds: Cmds = Vec::new();

        ui.heading("Проект");
        if let Some(name) = text_field(ui, "project_name", &project.name) {
            cmds.push(("project.rename", json!({ "name": name })));
        }

        egui::CollapsingHeader::new(format!("Материалы ({})", project.materials.len())).default_open(project.materials.is_empty()).show(ui, |ui| {
            for m in &project.materials {
                ui.label(format!("{} — {} мм, лист {}×{}, коэф. {}", m.name, m.thickness, m.sheet.0, m.sheet.1, m.coef));
                ui.horizontal(|ui| {
                    // Пусто — общие параметры раскроя со вкладки «Раскрой».
                    for (key, label, value, general) in [("trim", "обрезка", m.trim, self.params.trim), ("kerf", "пропил", m.kerf, self.params.kerf)] {
                        let mut v = value.unwrap_or(general);
                        ui.label(label);
                        let r = ui.add(crate::expr::drag(&mut v).range(0.0..=50.0).speed(0.1).suffix(" мм"));
                        if (r.drag_stopped() || (r.changed() && !r.dragged())) && Some(v) != value {
                            cmds.push(("material.set", json!({ "name": m.name, key: v })));
                        }
                    }
                });
            }
            let nm = &mut self.new_material;
            egui::Grid::new("new_mat").num_columns(2).show(ui, |ui| {
                ui.label("Название");
                ui.text_edit_singleline(&mut nm.name);
                ui.end_row();
                ui.label("Толщина, мм");
                ui.add(crate::expr::drag(&mut nm.thickness).range(0.1..=100.0));
                ui.end_row();
                ui.label("Лист, мм");
                ui.horizontal(|ui| {
                    ui.add(crate::expr::drag(&mut nm.sheet.0).range(100.0..=6000.0));
                    ui.label("×");
                    ui.add(crate::expr::drag(&mut nm.sheet.1).range(100.0..=6000.0));
                });
                ui.end_row();
                ui.label("Коэффициент");
                ui.add(crate::expr::drag(&mut nm.coef).range(1.0..=3.0).speed(0.01));
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
                ui.add(crate::expr::drag(t).range(0.1..=5.0).speed(0.1).suffix(" мм"));
                ui.add(crate::expr::drag(w).range(1.0..=100.0).suffix(" мм"));
            });
            if ui.button("+ Добавить кромку").clicked() {
                cmds.push(("edge.add", json!({ "name": n, "thickness": t, "width": w })));
            }
        });

        ui.separator();
        ui.heading("Добавить");
        let first = project.materials.first().map(|m| m.name.clone());
        let edge = project.edges.first().map(|e| e.name.clone());
        if first.is_none() {
            ui.weak("Сначала добавьте материал.");
        }
        // Новый модуль встаёт справа от последнего напольного, чтобы не попасть внутрь соседа.
        let right = project.cabinets.iter().filter(|c| c.position[1] <= 0.0).map(|c| c.position[0] + c.width)
            .chain(project.corners.iter().map(|c| c.position[0] + c.width_x))
            .fold(0.0, f64::max);
        ui.add_enabled_ui(first.is_some(), |ui| {
            if ui.button("+ Корпус").clicked() {
                cmds.push(("cabinet.add", json!({ "width": 800, "height": 2000, "depth": 600, "material": first, "plinth": 80, "edge": edge, "doors": 2, "position": [right, 0, 0] })));
            }
            if ui.button("+ Угловой модуль").clicked() {
                cmds.push(("corner.add", json!({ "width_x": 1000, "width_z": 1000, "height": 820, "depth": 560, "material": first, "plinth": 100, "doors": true, "edge": edge, "position": [right, 0, 0] })));
            }
            if ui.button("+ Столешница над корпусами").clicked() {
                // По умолчанию — на напольные корпуса, по всей их длине, с выносом 20 мм вперёд.
                let floor: Vec<&design::Cabinet> = project.cabinets.iter().filter(|c| c.position[1] <= 0.0).collect();
                let x0 = floor.iter().map(|c| c.position[0]).fold(f64::MAX, f64::min).min(0.0);
                let x1 = floor.iter().map(|c| c.position[0] + c.width).fold(600.0, f64::max);
                let top = floor.iter().map(|c| c.height).fold(720.0, f64::max);
                let depth = floor.iter().map(|c| c.depth).fold(560.0, f64::max) + 20.0;
                cmds.push(("countertop.add", json!({ "material": first, "length": x1 - x0, "depth": depth, "position": [x0, top, 0], "edge": edge })));
            }
            ui.label("Панель (размеры контура, мм):");
            let (plane, a, b) = &mut self.new_panel;
            ui.horizontal(|ui| {
                for (p, label) in [("vertical", "Вертикальная"), ("horizontal", "Горизонтальная"), ("front", "Фронтальная")] {
                    ui.selectable_value(plane, p, label);
                }
            });
            ui.horizontal(|ui| {
                ui.add(crate::expr::drag(a).range(10.0..=3000.0).suffix(" мм"));
                ui.label("×");
                ui.add(crate::expr::drag(b).range(10.0..=3000.0).suffix(" мм"));
            });
            if ui.button("+ Панель").on_hover_text("Встанет справа от модулей; положение — в свойствах").clicked() {
                cmds.push(("panel.add", json!({ "material": first, "plane": plane, "size": [*a, *b], "position": [right, 0, 0] })));
            }
        });
        for (cmd, p) in cmds {
            self.exec(cmd, p);
        }
    }

    /// Выделить деталь по индексу (или снять выделение).
    /// Выделить деталь (или снять выделение). С Ctrl — добавить к выделению или убрать из него.
    fn select(&mut self, part: Option<usize>) {
        let add = self.ctx_ctrl;
        let Ok(parts) = &self.parts else { return };
        let sel = part.filter(|&i| i < parts.len()).map(|i| Sel::of(parts, i));
        match (sel, add) {
            (Some(s), true) => {
                if let Some(k) = self.selection.iter().position(|x| *x == s) {
                    self.selection.remove(k);
                } else {
                    self.selection.push(s);
                }
            }
            (Some(s), false) => self.selection = vec![s],
            (None, true) => {}
            (None, false) => self.selection.clear(),
        }
    }

    /// Основная выделенная деталь.
    fn selected(&self) -> Option<usize> {
        self.selection.last().and_then(|s| s.find(self.parts.as_ref().ok()?))
    }

    /// Все выделенные детали.
    fn selected_all(&self) -> Vec<usize> {
        let Ok(parts) = &self.parts else { return Vec::new() };
        self.selection.iter().filter_map(|s| s.find(parts)).collect()
    }

    /// Объекты для действий: панель дерева — сама, деталь шаблона — весь шаблон.
    fn targets(&self) -> Vec<u32> {
        let Ok(parts) = &self.parts else { return Vec::new() };
        let mut ids: Vec<u32> = Vec::new();
        for i in self.selected_all() {
            let id = if parts[i].id != 0 { parts[i].id } else { parts[i].owner };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// Габарит выделенного: (мин, макс).
    fn selection_bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let parts = self.parts.as_ref().ok()?;
        self.selected_all().iter().map(|&i| &parts[i]).fold(None, |acc, p| {
            let (lo, hi) = acc.unwrap_or(([f64::MAX; 3], [f64::MIN; 3]));
            Some((std::array::from_fn(|k| lo[k].min(p.origin[k])), std::array::from_fn(|k| hi[k].max(p.origin[k] + p.size[k]))))
        })
    }

    /// Окно «Анализ модели»: щелчок по проблеме выделяет её детали.
    fn problems_window(&mut self, ctx: &egui::Context) {
        let Some(list) = &self.problems else { return };
        let mut open = true;
        let mut pick: Option<Vec<usize>> = None;
        egui::Window::new(format!("Анализ модели — проблем: {}", list.len())).open(&mut open).default_width(460.0).show(ctx, |ui| {
            if list.is_empty() {
                ui.label("Проблем не найдено: пересечений нет, крепёж и фурнитура на месте, детали помещаются на лист.");
            }
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                for p in list {
                    let r = ui.add(egui::Label::new(format!("{}: {}", p.kind, p.message)).sense(egui::Sense::click()));
                    if r.clicked() && !p.parts.is_empty() {
                        pick = Some(p.parts.clone());
                    }
                }
            });
        });
        if let (Some(ids), Ok(parts)) = (pick, &self.parts) {
            // Анализ мог строиться по импортированной копии — выделяем по номерам деталей текущей модели.
            self.selection = ids.into_iter().filter(|&i| i < parts.len()).map(|i| Sel::of(parts, i)).collect();
        }
        if !open {
            self.problems = None;
        }
    }

    /// Запуск скрипта: без окон — сразу завершается; с окнами — живёт, пока открыто его окно.
    fn start_script(&mut self, path: &std::path::Path) {
        let Some(s) = &self.session else { return };
        let source = match script::read_script(path) {
            Ok(t) => t,
            Err(e) => return self.error = Some(e),
        };
        let dir = path.parent().map(PathBuf::from).unwrap_or_default();
        let selection = self.selection.iter().map(|x| (x.owner, x.id, x.nth)).collect();
        let opts = script::Options { sandbox: dir.clone(), script_dir: dir, selection, ..Default::default() };
        let mut rt = script::Runtime::start(s, &source, opts);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let win = if rt.error.is_none() && !rt.forms().is_empty() {
            ScriptWin { name, rt: Some(rt), done: None }
        } else {
            let Some(s) = &mut self.session else { return };
            let r = rt.finish(s);
            ScriptWin { name, rt: None, done: Some(r) }
        };
        self.script = Some(win);
        self.refresh();
    }

    /// Окно скрипта: его формы, сообщения, «Применить» / «Отменить».
    fn script_window(&mut self, ctx: &egui::Context) {
        let Some(win) = &mut self.script else { return };
        let mut open = true;
        let (mut apply, mut cancel) = (false, false);
        let mut events = Vec::new();
        egui::Window::new(format!("Скрипт: {}", win.name)).open(&mut open).default_width(420.0).show(ctx, |ui| {
            if let Some(rt) = &mut win.rt {
                for f in rt.forms() {
                    ui.heading(f["Caption"].as_str().unwrap_or("Окно скрипта"));
                    script_control(ui, &f, &mut events);
                    ui.separator();
                }
                for l in rt.log() {
                    ui.label(l);
                }
                if let Some(e) = &rt.error {
                    ui.colored_label(Color32::from_rgb(200, 40, 40), e);
                }
                ui.horizontal(|ui| {
                    apply = ui.button("Применить").on_hover_text("Изменения скрипта — в модель (одна отмена)").clicked();
                    cancel = ui.button("Отменить").clicked();
                });
            } else if let Some(r) = &win.done {
                for l in &r.log {
                    ui.label(l);
                }
                match &r.error {
                    Some(e) => ui.colored_label(Color32::from_rgb(200, 40, 40), format!("Ошибка: {e}\nМодель не изменена.")),
                    None => ui.label(format!("Готово. Изменений: {}.", r.changes)),
                };
            }
        });
        if let Some(rt) = &mut win.rt {
            for e in events {
                match e {
                    ScriptEvent::Click(id) => rt.click(id),
                    ScriptEvent::Change(id, field, v) => rt.change(id, field, &v),
                }
            }
            // Скрипт закрыл все свои окна — завершаем, как «Применить».
            if rt.error.is_none() && rt.forms().is_empty() {
                apply = true;
            }
        }
        if apply && let (Some(rt), Some(s)) = (win.rt.take(), &mut self.session) {
            win.done = Some(rt.finish(s));
            self.refresh();
            return;
        }
        if cancel || !open {
            self.script = None;
        }
    }

    /// «Фурнитура»: модели .fr3d из папки — превью, поиск, «Поставить».
    fn library_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Папка");
            ui.text_edit_singleline(&mut self.library.dir);
            if ui.button("…").clicked()
                && let Some(d) = rfd::FileDialog::new().pick_folder()
            {
                self.library.dir = d.display().to_string();
                self.library.models.clear();
            }
            if ui.button("Обновить").clicked() || (self.library.models.is_empty() && self.library.errors.is_empty()) {
                let (m, e) = basis::library::scan(std::path::Path::new(&self.library.dir));
                (self.library.models, self.library.errors) = (m, e);
                if self.library.models.is_empty() && self.library.errors.is_empty() {
                    self.library.errors.push(format!("в «{}» нет моделей .fr3d", self.library.dir));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Поиск");
            ui.text_edit_singleline(&mut self.library.search);
        });
        for e in &self.library.errors {
            ui.colored_label(Color32::from_rgb(200, 40, 40), e);
        }
        let editable = self.session.is_some();
        let right = self.session.as_ref().map_or(0.0, |s| {
            s.project.cabinets.iter().map(|c| c.position[0] + c.width).chain(s.project.corners.iter().map(|c| c.position[0] + c.width_x)).fold(0.0, f64::max)
        });
        let q = self.library.search.to_lowercase();
        let mut place = None;
        egui::ScrollArea::vertical().id_salt("lib").show(ui, |ui| {
            for m in self.library.models.iter().filter(|m| q.is_empty() || m.name.to_lowercase().contains(&q) || m.kind.to_lowercase().contains(&q)) {
                ui.horizontal(|ui| {
                    let tex = self.library.thumbs.entry(m.path.clone()).or_insert_with(|| {
                        let img = m.thumbnail.as_deref().and_then(decode_png).unwrap_or_else(|| egui::ColorImage::new([1, 1], vec![Color32::from_gray(230)]));
                        ui.ctx().load_texture(m.path.display().to_string(), img, Default::default())
                    });
                    ui.add(egui::Image::new((tex.id(), Vec2::splat(64.0))));
                    ui.vertical(|ui| {
                        ui.label(&m.name);
                        let size: Vec<String> = (0..3).map(|k| format!("{:.0}", m.hi[k] - m.lo[k])).collect();
                        ui.weak(format!("{} · {} мм · отверстий {}", if m.kind.is_empty() { "модель" } else { &m.kind }, size.join("×"), m.holes.len()));
                        if ui.add_enabled(editable, egui::Button::new("Поставить")).on_hover_text("Встанет справа от модулей; дальше — сдвиг в «Свойствах»").clicked() {
                            place = Some(m.path.clone());
                        }
                    });
                });
                ui.separator();
            }
        });
        if let Some(p) = place {
            self.exec("furniture.add", json!({ "path": p, "position": [right, 0, 0] }));
        }
    }

    /// Свойства выделенной фурнитуры: положение, отверстия, удаление.
    fn furniture_properties(&mut self, ui: &mut egui::Ui, id: u32) {
        let Some(s) = &self.session else { return };
        let Some((f, fr)) = design::furniture(&s.project).into_iter().find(|(f, _)| f.id == id) else {
            self.furniture_sel = None;
            return;
        };
        let lost = design::furniture_lost_holes(&s.project).into_iter().find(|(n, _)| *n == f.name).map_or(0, |x| x.1);
        let mut cmds: Cmds = Vec::new();
        ui.heading(format!("Фурнитура «{}»", f.name));
        egui::Grid::new("furn").num_columns(2).show(ui, |ui| {
            ui.label("Вид");
            ui.label(if f.kind.is_empty() { "—" } else { &f.kind });
            ui.end_row();
            ui.label("Файл");
            ui.label(&f.source);
            ui.end_row();
            ui.label("Габарит");
            ui.label(format!("{:.0} × {:.0} × {:.0} мм", f.hi[0] - f.lo[0], f.hi[1] - f.lo[1], f.hi[2] - f.lo[2]));
            ui.end_row();
            ui.label("Отверстий");
            ui.label(format!("{} (не попали в панели: {lost})", f.holes.len()));
            ui.end_row();
            ui.label("Начало осей X, Y, Z");
            if let Some(o) = editors::position_editor(ui, fr.origin) {
                let d: [f64; 3] = std::array::from_fn(|k| o[k] - fr.origin[k]);
                cmds.push(("object.move", json!({ "ids": [id], "d": d })));
            }
            ui.end_row();
        });
        if lost > 0 {
            ui.colored_label(Color32::from_rgb(200, 40, 40), "Часть отверстий висит в воздухе: прижмите модель к панели.");
        }
        let (_, axis, angle, _) = &mut self.actions;
        ui.horizontal(|ui| {
            for a in ['x', 'y', 'z'] {
                ui.selectable_value(axis, a, a.to_ascii_uppercase().to_string());
            }
            ui.add(crate::expr::drag(angle).range(-360.0..=360.0).suffix("°"));
            if ui.button("Повернуть").clicked() {
                cmds.push(("object.rotate", json!({ "ids": [id], "axis": axis.to_string(), "angle": *angle, "center": fr.origin })));
            }
        });
        if ui.button("Удалить фурнитуру").clicked() {
            cmds.push(("object.remove", json!({ "ids": [id] })));
            self.furniture_sel = None;
        }
        for (cmd, v) in cmds {
            self.exec(cmd, v);
        }
    }

    /// «Крепёж»: между двумя выделенными панелями, на все стыки выделенных; крепёж этой панели.
    fn fastener_panel(&mut self, ui: &mut egui::Ui, project: &Project, panel: u32, cmds: &mut Cmds) {
        let panels: Vec<u32> = self.targets().into_iter().filter(|id| project.panels().iter().any(|p| p.id == *id)).collect();
        egui::CollapsingHeader::new("Крепёж").default_open(panels.len() >= 2).show(ui, |ui| {
            let names = editors::fastener_names(project);
            if let Some(Some(v)) = editors::choose(ui, "fast_kind", Some(&self.fastener_kind), &names, false) {
                self.fastener_kind = v;
            }
            ui.horizontal(|ui| {
                let two = panels.len() == 2;
                if ui.add_enabled(two, egui::Button::new("Между выделенными")).on_disabled_hover_text("Выделите две панели (Ctrl+щелчок): торец одной на пласти другой").clicked() {
                    cmds.push(("fastener.add", json!({ "kind": self.fastener_kind, "a": panels[0], "b": panels[1] })));
                }
                if ui.button("На все стыки выделенных").clicked() {
                    cmds.push(("fastener.auto", json!({ "kind": self.fastener_kind, "ids": panels })));
                }
            });
            for f in project.fasteners.iter().filter(|f| f.a == panel || f.b == panel) {
                ui.horizontal(|ui| {
                    ui.label(format!("{} — панели {} и {}", f.kind, f.a, f.b));
                    if ui.small_button("удалить").clicked() {
                        cmds.push(("fastener.remove", json!({ "id": f.id })));
                    }
                });
            }
            for (_, e) in design::hanging_fasteners(project) {
                ui.colored_label(Color32::from_rgb(200, 40, 40), e);
            }
        });
    }

    /// «Действия с выделенным» (как гл. 14 руководства Базиса).
    fn actions_panel(&mut self, ui: &mut egui::Ui) {
        let ids = self.targets();
        if ids.is_empty() || self.session.is_none() {
            return;
        }
        let Some((lo, hi)) = self.selection_bounds() else { return };
        let center: [f64; 3] = std::array::from_fn(|k| ((lo[k] + hi[k]) / 2.0).round());
        let mut cmds: Cmds = Vec::new();
        egui::CollapsingHeader::new(format!("Действия с выделенным ({})", ids.len())).default_open(true).show(ui, |ui| {
            let (d, axis, angle, count) = &mut self.actions;
            ui.horizontal(|ui| {
                ui.label("dX, dY, dZ");
                for v in d.iter_mut() {
                    ui.add(crate::expr::drag(v).speed(1.0).suffix(" мм"));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Сдвинуть").clicked() {
                    cmds.push(("object.move", json!({ "ids": ids, "d": *d })));
                }
                ui.add(crate::expr::drag(count).range(1..=100).prefix("копий "));
                if ui.button("Копировать").on_hover_text("Копии со сдвигом dX, dY, dZ, 2·d, … (массив)").clicked() {
                    cmds.push(("object.copy", json!({ "ids": ids, "d": *d, "count": *count })));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Ось");
                for a in ['x', 'y', 'z'] {
                    ui.selectable_value(axis, a, a.to_ascii_uppercase().to_string());
                }
                ui.add(crate::expr::drag(angle).range(-360.0..=360.0).suffix("°"));
                if ui.button("Повернуть").on_hover_text("Вокруг центра выделенного").clicked() {
                    cmds.push(("object.rotate", json!({ "ids": ids, "axis": axis.to_string(), "angle": *angle, "center": center })));
                }
            });
            let k = match axis { 'x' => 0, 'y' => 1, _ => 2 };
            ui.horizontal(|ui| {
                if ui.button("Отразить").on_hover_text("Плоскостью через центр выделенного поперёк выбранной оси").clicked() {
                    cmds.push(("object.mirror", json!({ "ids": ids, "axis": axis.to_string(), "at": center[k] })));
                }
                if ui.button("Зеркальная копия").on_hover_text("Копия, отражённая плоскостью по правому (верхнему, переднему) краю выделенного").clicked() {
                    cmds.push(("object.mirror", json!({ "ids": ids, "axis": axis.to_string(), "at": hi[k], "copy": true })));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Собрать в блок").clicked() {
                    cmds.push(("block.create", json!({ "ids": ids })));
                }
                if ui.button("Удалить").clicked() {
                    cmds.push(("object.remove", json!({ "ids": ids })));
                }
            });
        });
        for (cmd, v) in cmds {
            self.exec(cmd, v);
            if cmd == "object.remove" {
                self.selection.clear();
            }
        }
        ui.separator();
    }

    /// «Структура модели»: шаблоны (корпуса, угловые модули, столешницы) и дерево панелей и блоков.
    fn structure_panel(&mut self, ui: &mut egui::Ui) {
        let Ok(parts) = &self.parts else { return };
        let sel = self.selected();
        let all = self.selected_all();
        let mut pick = None;
        let mut row = |ui: &mut egui::Ui, i: usize, text: String| {
            if ui.selectable_label(all.contains(&i), text).clicked() {
                pick = Some(i);
            }
        };
        // Группы по объекту верхнего уровня, в порядке деталей.
        let mut owners: Vec<u32> = Vec::new();
        for p in parts {
            if !owners.contains(&p.owner) {
                owners.push(p.owner);
            }
        }
        let project = self.session.as_ref().map(|s| &s.project);
        let title = |owner: u32| -> String {
            let Some(pr) = project else { return "Модель Базиса".into() };
            pr.cabinets.iter().find(|c| c.id == owner).map(|c| format!("Корпус: {}", c.name))
                .or_else(|| pr.corners.iter().find(|c| c.id == owner).map(|c| format!("Угловой модуль: {}", c.name)))
                .or_else(|| pr.countertops.iter().find(|c| c.id == owner).map(|c| format!("Столешница: {}", c.name)))
                .or_else(|| pr.objects.iter().find(|o| o.id() == owner).map(|o| match o {
                    design::Object::Block(b) => format!("Блок: {}", b.name),
                    design::Object::Panel(p) => format!("Панель: {}", p.name),
                    design::Object::Furniture(f) => format!("Фурнитура: {}", f.name),
                }))
                .unwrap_or_else(|| format!("Объект {owner}"))
        };
        for owner in owners {
            let mine: Vec<usize> = (0..parts.len()).filter(|&i| parts[i].owner == owner).collect();
            if mine.len() == 1 && parts[mine[0]].id == owner {
                row(ui, mine[0], title(owner));
                continue;
            }
            let open = sel.is_some_and(|i| parts[i].owner == owner);
            egui::CollapsingHeader::new(format!("{} ({})", title(owner), mine.len())).id_salt(("own", owner)).open(open.then_some(true)).show(ui, |ui| {
                for i in mine {
                    let p = &parts[i].panel;
                    row(ui, i, format!("{} {}", p.pos, p.name));
                }
            });
        }
        let furniture: Vec<(u32, String)> = project.map(|p| design::furniture(p).into_iter().map(|(f, _)| (f.id, f.name)).collect()).unwrap_or_default();
        let mut fsel = None;
        if !furniture.is_empty() {
            ui.separator();
            ui.strong("Фурнитура");
            for (id, name) in furniture {
                if ui.selectable_label(self.furniture_sel == Some(id), name).clicked() {
                    fsel = Some(id);
                }
            }
        }
        if let Some(i) = pick {
            self.select(Some(i));
            self.furniture_sel = None;
        }
        if fsel.is_some() {
            self.furniture_sel = fsel;
            self.selection.clear();
        }
    }

    /// «Свойства» выделенной детали: её размеры и редактор объекта, которому она принадлежит.
    fn properties_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(id) = self.furniture_sel {
            self.furniture_properties(ui, id);
            return;
        }
        if self.selected().is_some() {
            self.actions_panel(ui);
        }
        let (Ok(parts), Some(i)) = (&self.parts, self.selected()) else {
            ui.weak("Выделите деталь щелчком в 3D, на виде спереди или в структуре модели. Ctrl+щелчок — добавить к выделению.");
            return;
        };
        let part = parts[i].clone();
        let p = &part.panel;
        let (l, w) = p.cut_size();
        egui::Grid::new("part_info").num_columns(2).show(ui, |ui| {
            ui.label("Деталь");
            ui.strong(format!("{} {}", p.pos, p.name));
            ui.end_row();
            ui.label("Материал");
            ui.label(panel::material_name(&p.material));
            ui.end_row();
            ui.label("Пильный размер");
            ui.label(format!("{l} × {w} × {} мм", p.thickness));
            ui.end_row();
            ui.label("Габарит в проекте");
            ui.label(format!("{} × {} × {} мм", part.size[0], part.size[1], part.size[2]));
            ui.end_row();
            ui.label("Начало габарита");
            ui.label(format!("X {} Y {} Z {}", part.origin[0], part.origin[1], part.origin[2]));
            ui.end_row();
            ui.label("Кромка");
            let edges: Vec<String> = [Side::L1, Side::L2, Side::W1, Side::W2]
                .iter()
                .filter_map(|&sd| {
                    let t = p.edges.iter().filter(|e| p.side(e) == sd).map(|e| e.thickness).fold(0.0, f64::max);
                    (t > 0.0).then(|| format!("{sd:?} {t}"))
                })
                .collect();
            ui.label(if edges.is_empty() { "нет".to_string() } else { edges.join(", ") });
            ui.end_row();
            ui.label("Отверстий");
            ui.label(p.holes.len().to_string());
            ui.end_row();
        });
        ui.separator();
        let Some(s) = &self.session else {
            ui.weak("Модель Базиса открыта для просмотра. «Сделать редактируемой» — в верхней строке.");
            return;
        };
        let project = s.project.clone();
        let mut cmds: Cmds = Vec::new();
        if let Some(c) = project.cabinets.iter().find(|c| c.id == part.owner) {
            ui.heading(format!("Корпус «{}»", c.name));
            editors::cabinet_editor(ui, &project, c, &mut cmds);
            if ui.button("Разрушить в панели").on_hover_text("Детали станут отдельными панелями блока; параметры корпуса больше не действуют").clicked() {
                cmds.push(("cabinet.explode", json!({ "id": c.id })));
            }
        } else if let Some(c) = project.corners.iter().find(|c| c.id == part.owner) {
            ui.heading(format!("Угловой модуль «{}»", c.name));
            editors::corner_editor(ui, &project, c, &mut cmds);
        } else if let Some(c) = project.countertops.iter().find(|c| c.id == part.owner) {
            ui.heading(format!("Столешница «{}»", c.name));
            editors::countertop_editor(ui, &project, c, &mut cmds);
        } else if let Some(fp) = project.panels().into_iter().find(|f| f.id == part.id) {
            self.fastener_panel(ui, &project, fp.id, &mut cmds);
            ui.heading("Панель");
            editors::panel_editor(ui, &project, fp, (p.size.0, p.size.1, p.thickness), &mut cmds);
            ui.separator();
            ui.heading("Облицовка кромки");
            let names: Vec<String> = project.edges.iter().map(|e| e.name.clone()).collect();
            if names.is_empty() {
                ui.weak("Добавьте кромку на вкладке «Проект».");
            }
            let (edge, clip, allowance) = &mut self.edging;
            if edge.is_none() {
                *edge = names.first().cloned();
            }
            ui.horizontal(|ui| {
                if let Some(Some(v)) = editors::choose(ui, "edging", edge.as_deref(), &names, false) {
                    *edge = Some(v);
                }
                ui.checkbox(clip, "Подрезать").on_hover_text("Заготовка меньше на толщину кромки: размер детали — чистовой");
                ui.add(crate::expr::drag(allowance).range(0.0..=10.0).speed(0.1).prefix("припуск ").suffix(" мм"));
            });
            ui.separator();
            ui.heading("Контур");
            let (tool, size) = &mut self.contour_tool;
            ui.horizontal(|ui| {
                for (t, label) in [(ContourTool::Edge, "Кромка"), (ContourTool::Round, "Скругление"), (ContourTool::Chamfer, "Фаска"), (ContourTool::Hole, "Вырез")] {
                    ui.selectable_value(tool, t, label);
                }
                if *tool != ContourTool::Edge {
                    let label = match tool { ContourTool::Round => "R ", ContourTool::Chamfer => "катет ", _ => "радиус " };
                    ui.add(crate::expr::drag(size).range(1.0..=2000.0).prefix(label).suffix(" мм"));
                }
            });
            ui.weak(match tool {
                ContourTool::Edge => "Щелчок по торцу — нанести или снять выбранную кромку.",
                ContourTool::Round | ContourTool::Chamfer => "Щелчок по вершине — скруглить или срезать угол.",
                ContourTool::Hole => "Щелчок внутри панели — круглый вырез с центром в этой точке.",
            });
            if fp.segs.len() == 4 {
                ui.horizontal(|ui| {
                    ui.label("Размеры:");
                    let (mut a, mut b) = (p.size.0, p.size.1);
                    let ra = ui.add(crate::expr::drag(&mut a).range(1.0..=6000.0).suffix(" мм"));
                    ui.label("×");
                    let rb = ui.add(crate::expr::drag(&mut b).range(1.0..=6000.0).suffix(" мм"));
                    let done = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
                    if (done(&ra) || done(&rb)) && (a, b) != p.size {
                        cmds.push(("panel.resize", json!({ "id": fp.id, "size": [a, b] })));
                    }
                });
            }
            let all: Vec<usize> = (0..fp.segs.len()).collect();
            ui.horizontal(|ui| {
                if ui.button("На все торцы").clicked() {
                    cmds.push(("panel.edge", json!({ "id": fp.id, "segments": all, "edge": edge, "clip": clip, "allowance": allowance })));
                }
                if ui.button("Снять со всех").clicked() {
                    cmds.push(("panel.edge", json!({ "id": fp.id, "segments": all, "edge": null })));
                }
            });
            let (tool, size) = self.contour_tool;
            if let Some(c) = editors::contour_view(ui, &fp.segs, &fp.edges, 260.0) {
                match (tool, c.segment, c.vertex) {
                    (ContourTool::Edge, Some(i), _) => {
                        let same = fp.edges.iter().any(|e| e.segment == i && edge.as_deref().is_some_and(|n| panel::material_name(&e.material) == n));
                        let edge = if same { None } else { edge.clone() };
                        cmds.push(("panel.edge", json!({ "id": fp.id, "segments": [i], "edge": edge, "clip": clip, "allowance": allowance })));
                    }
                    (ContourTool::Round, _, Some(v)) => cmds.push(("panel.corner", json!({ "id": fp.id, "at": v, "radius": size }))),
                    (ContourTool::Chamfer, _, Some(v)) => cmds.push(("panel.corner", json!({ "id": fp.id, "at": v, "chamfer": size }))),
                    (ContourTool::Hole, _, _) => cmds.push(("panel.cutout", json!({ "id": fp.id, "center": c.point, "radius": size }))),
                    _ => {}
                }
            }
        }
        for (cmd, v) in cmds {
            self.exec(cmd, v);
        }
    }

    /// Панель режима «в проём»: включение, плоскость, материал, отступы.
    fn fill_toolbar(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.session else { return };
        let names: Vec<String> = s.project.materials.iter().map(|m| m.name.clone()).collect();
        ui.horizontal(|ui| {
            let mut on = self.fill.is_some();
            if ui.toggle_value(&mut on, "Панель в проём").on_hover_text("Щелчок внутри проёма ставит панель между ближайшими деталями, как «~» в Базисе").changed() {
                self.fill = match (on, names.first()) {
                    (true, Some(m)) => Some(FillTool { plane: "horizontal", material: m.clone(), gaps: [0.0; 4], mount: "overlay", double: false, heights: "140 280 280".into() }),
                    _ => None,
                };
            }
            let Some(t) = &mut self.fill else { return };
            for (p, label) in [("horizontal", "Горизонтальная"), ("vertical", "Вертикальная"), ("front", "Фронтальная"), ("door", "Дверь"), ("drawers", "Ящики")] {
                ui.selectable_value(&mut t.plane, p, label);
            }
            if let Some(Some(m)) = editors::choose(ui, "fill_mat", Some(&t.material), &names, false) {
                t.material = m;
            }
            if matches!(t.plane, "door" | "drawers") {
                for (m, label) in [("overlay", "накладная"), ("half", "полунакладная"), ("inset", "вкладная")] {
                    ui.selectable_value(&mut t.mount, m, label);
                }
                if t.plane == "door" {
                    ui.checkbox(&mut t.double, "двойная");
                } else {
                    ui.label("фасады:");
                    ui.add(egui::TextEdit::singleline(&mut t.heights).desired_width(110.0)).on_hover_text("Высоты фасадов снизу вверх через пробел, мм");
                }
                return;
            }
            let labels = match t.plane {
                "horizontal" => ["слева", "справа", "сзади", "спереди"],
                "vertical" => ["сзади", "спереди", "снизу", "сверху"],
                _ => ["слева", "справа", "снизу", "сверху"],
            };
            ui.label("отступы:");
            for (g, l) in t.gaps.iter_mut().zip(labels) {
                ui.add(crate::expr::drag(g).range(-100.0..=500.0).prefix(format!("{l} ")).suffix(" мм"));
            }
        });
    }

    fn front_view(&mut self, ui: &mut egui::Ui) {
        if ui.checkbox(&mut self.show_doors, "Показывать двери и фасады").changed() {
            self.revision += 1;
        }
        self.fill_toolbar(ui);
        let parts = match &self.parts {
            Ok(p) => p.clone(),
            Err(e) => {
                ui.colored_label(Color32::RED, e);
                return;
            }
        };
        if parts.is_empty() {
            ui.label("Пусто. Добавьте материал и модуль или панель на вкладке «Проект».");
            return;
        }
        let rects = design::front_view(&parts, self.show_doors);
        let Some((lo, hi)) = design::bounds(&rects) else { return };
        let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let area = resp.rect.shrink(30.0);
        let k = ((area.width() as f64) / (hi[0] - lo[0]).max(1.0)).min((area.height() as f64) / (hi[1] - lo[1]).max(1.0));
        let to = |x: f64, y: f64| Pos2::new(area.left() + ((x - lo[0]) * k) as f32, area.bottom() - ((y - lo[1]) * k) as f32);
        painter.rect_filled(resp.rect, 0.0, Color32::from_gray(245));
        let selected = self.selected_all();
        // Перетаскивание выделенного: сдвиг в мм с прилипанием краёв к краям других деталей (порог 10 пикселей).
        let moving = self.drag.map(|d| {
            let (lo, hi) = self.selection_bounds().unwrap_or(([0.0; 3], [0.0; 3]));
            let snap = |axis: usize, d: f64| -> f64 {
                let tol = 10.0 / k;
                let edges = parts.iter().enumerate().filter(|(i, _)| !selected.contains(i)).flat_map(|(_, p)| [p.origin[axis], p.origin[axis] + p.size[axis]]);
                let best = edges.flat_map(|e| [e - (lo[axis] + d), e - (hi[axis] + d)]).filter(|c| c.abs() < tol).min_by(|a, b| a.abs().total_cmp(&b.abs()));
                (d + best.unwrap_or(0.0)).round()
            };
            [snap(0, d[0]), snap(1, d[1])]
        });
        // Ближайшая к зрителю деталь под курсором — последняя нарисованная.
        let mut hovered: Option<usize> = None;
        for fr in &rects {
            let mv = moving.filter(|_| selected.contains(&fr.part)).unwrap_or([0.0; 2]);
            let r = Rect::from_two_pos(to(fr.x + mv[0], fr.y + mv[1]), to(fr.x + fr.w + mv[0], fr.y + fr.h + mv[1]));
            let fill = match fr.kind {
                _ if selected.contains(&fr.part) => Color32::from_rgb(255, 170, 70),
                design::Kind::Door | design::Kind::Drawer => Color32::from_rgba_unmultiplied(170, 200, 230, 150),
                design::Kind::Back => Color32::from_gray(225),
                design::Kind::Body => Color32::from_rgb(236, 220, 190),
            };
            painter.rect(r, 0.0, fill, Stroke::new(1.0, Color32::from_gray(60)), StrokeKind::Inside);
            if ui.ctx().pointer_hover_pos().is_some_and(|m| resp.rect.contains(m) && r.contains(m)) {
                hovered = Some(fr.part);
            }
        }
        // Габаритные размеры корпусов.
        if let Some(s) = &self.session {
            for c in &s.project.cabinets {
                painter.text(to(c.position[0] + c.width / 2.0, c.position[1]) + Vec2::new(0.0, 14.0), egui::Align2::CENTER_CENTER, format!("{}", c.width), egui::FontId::proportional(13.0), Color32::BLACK);
                painter.text(to(c.position[0], c.position[1] + c.height / 2.0) - Vec2::new(16.0, 0.0), egui::Align2::CENTER_CENTER, format!("{}", c.height), egui::FontId::proportional(13.0), Color32::BLACK);
            }
        }
        // Режим «в проём»: фантом будущей панели под курсором, щелчок — поставить.
        let mm_at = |p: Pos2| [((p.x - area.left()) as f64 / k + lo[0]).round(), ((area.bottom() - p.y) as f64 / k + lo[1]).round()];
        if let (Some(t), Some(m)) = (&self.fill, ui.ctx().pointer_hover_pos().filter(|m| resp.rect.contains(*m))) {
            let at = mm_at(m);
            if matches!(t.plane, "door" | "drawers") {
                painter.text(m + Vec2::new(12.0, 12.0), egui::Align2::LEFT_TOP, if t.plane == "door" { "щелчок — дверь в этот проём" } else { "щелчок — ящики в этот проём" }, egui::FontId::proportional(12.0), Color32::from_rgb(160, 70, 0));
                if resp.clicked() {
                    let heights: Vec<f64> = t.heights.split_whitespace().filter_map(|v| v.replace(',', ".").parse().ok()).collect();
                    let (cmd, args) = if t.plane == "door" {
                        ("door.add", json!({ "at": at, "material": t.material, "mount": t.mount, "double": t.double }))
                    } else {
                        ("drawers.add", json!({ "at": at, "material": t.material, "mount": t.mount, "heights": heights }))
                    };
                    self.exec(cmd, args);
                }
                return;
            }
            let plane: Option<design::Plane> = serde_json::from_value(json!(t.plane)).ok();
            let thick = self.session.as_ref().and_then(|s| s.project.material(&t.material).ok()).map_or(16.0, |m| m.thickness);
            match plane.map(|pl| (pl, design::fill::fill(&parts, pl, at, thick, t.gaps))) {
                Some((pl, Ok(f))) => {
                    let (w, h) = match pl {
                        design::Plane::Horizontal => (f.size[0], thick),
                        design::Plane::Vertical => (thick, f.size[1]),
                        design::Plane::Front => (f.size[0], f.size[1]),
                    };
                    let r = Rect::from_two_pos(to(f.position[0], f.position[1]), to(f.position[0] + w, f.position[1] + h));
                    painter.rect(r, 0.0, Color32::from_rgba_unmultiplied(255, 170, 70, 90), Stroke::new(1.5, Color32::from_rgb(220, 110, 0)), StrokeKind::Inside);
                    painter.text(r.center_top() - Vec2::new(0.0, 4.0), egui::Align2::CENTER_BOTTOM, format!("{} × {}", f.size[0], f.size[1]), egui::FontId::proportional(13.0), Color32::from_rgb(160, 70, 0));
                    if resp.clicked() {
                        let cmd = json!({ "material": t.material, "plane": t.plane, "at": at, "gaps": t.gaps });
                        self.exec("panel.fill", cmd);
                        return;
                    }
                }
                Some((_, Err(e))) => {
                    painter.text(m + Vec2::new(12.0, 12.0), egui::Align2::LEFT_TOP, e, egui::FontId::proportional(12.0), Color32::from_rgb(200, 40, 40));
                }
                None => {}
            }
            return;
        }
        if resp.clicked() {
            self.select(hovered);
        }
        if resp.drag_started() && hovered.is_some_and(|h| selected.contains(&h)) && self.session.is_some() {
            self.drag = Some([0.0; 2]);
        }
        if let Some(d) = &mut self.drag {
            let delta = resp.drag_delta();
            d[0] += delta.x as f64 / k;
            d[1] -= delta.y as f64 / k;
            if let Some(m) = moving {
                painter.text(resp.rect.left_top() + Vec2::new(8.0, 8.0), egui::Align2::LEFT_TOP, format!("сдвиг: X {} Y {}", m[0], m[1]), egui::FontId::proportional(14.0), Color32::from_rgb(160, 70, 0));
            }
            if resp.drag_stopped() {
                self.drag = None;
                if let Some(m) = moving.filter(|m| *m != [0.0, 0.0]) {
                    let ids = self.targets();
                    self.exec("object.move", json!({ "ids": ids, "d": [m[0], m[1], 0.0] }));
                }
            }
            return;
        }
        if let Some(i) = hovered {
            let p = &parts[i];
            let (l, w) = p.panel.cut_size();
            resp.on_hover_text(format!("{} {}: {}×{}×{} мм\n{}", p.panel.pos, p.panel.name, l, w, p.panel.thickness, panel::material_name(&p.panel.material)));
        }
    }

    fn iso_view(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.show_doors, "Показывать двери и фасады").changed() {
                self.revision += 1;
            }
            ui.toggle_value(&mut self.quad, "4 окна").on_hover_text("Спереди, слева, сверху и аксонометрия одновременно");
            ui.separator();
            self.viewport.toolbar(ui);
        });
        // Общие режим, рамки фурнитуры и «Показать всё» — для всех окон.
        let boxes: Vec<(basis::geom::Frame, [f64; 3], [f64; 3], bool)> = self.session.as_ref().map(|s| design::furniture(&s.project).into_iter().map(|(f, fr)| (fr, f.lo, f.hi, self.furniture_sel == Some(f.id))).collect()).unwrap_or_default();
        let refit = self.viewport.needs_fit();
        for v in &mut self.views {
            v.mode = self.viewport.mode;
            v.furniture = boxes.clone();
            if refit {
                v.refit();
            }
        }
        self.viewport.furniture = boxes;
        let parts = match &self.parts {
            Ok(p) => p,
            Err(e) => {
                ui.colored_label(Color32::RED, e);
                return;
            }
        };
        let doors = self.show_doors;
        let selected = self.selected_all();
        let skip = |p: &Part| !doors && matches!(p.kind, design::Kind::Door | design::Kind::Drawer);
        let mut clicks = Vec::new();
        if self.quad {
            // Как в Базисе: спереди | слева, сверху | аксонометрия.
            let r = ui.available_rect_before_wrap();
            let (w, h) = (r.width() / 2.0, r.height() / 2.0);
            let cell = |i: usize, j: usize| egui::Rect::from_min_size(r.min + Vec2::new(i as f32 * w, j as f32 * h), Vec2::new(w, h)).shrink(1.0);
            let cells = [cell(0, 0), cell(1, 0), cell(0, 1)];
            for (v, c) in self.views.iter_mut().zip(cells) {
                clicks.push(ui.scope_builder(egui::UiBuilder::new().max_rect(c), |ui| v.show(ui, frame, parts, self.revision, &selected, skip)).inner);
            }
            clicks.push(ui.scope_builder(egui::UiBuilder::new().max_rect(cell(1, 1)), |ui| self.viewport.show(ui, frame, parts, self.revision, &selected, skip)).inner);
            ui.allocate_rect(r, egui::Sense::hover());
        } else {
            clicks.push(self.viewport.show(ui, frame, parts, self.revision, &selected, skip));
        }
        for c in clicks {
            match c {
                viewport::Click::Part(i) => self.select(Some(i)),
                viewport::Click::Empty => self.select(None),
                viewport::Click::None => {}
            }
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
            ui.add(crate::expr::drag(&mut self.params.sheet.0).range(500.0..=6000.0));
            ui.label("×");
            ui.add(crate::expr::drag(&mut self.params.sheet.1).range(500.0..=6000.0));
            ui.label("Пропил");
            ui.add(crate::expr::drag(&mut self.params.kerf).range(0.0..=10.0).speed(0.1));
            ui.label("Обрезка");
            ui.add(crate::expr::drag(&mut self.params.trim).range(0.0..=50.0));
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
