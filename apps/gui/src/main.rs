//! dowel-craft window: project on the left, front view / parts / cutting in the center.
//! All project changes go through `Session::execute`, the same commands as the CLI and the future MCP.

// Windows release builds have no black console window (debug keeps it for `--screenshot` output and errors).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

mod chrome;
mod docs;
mod editors;
mod expr;
mod i18n;
mod icons;
mod keys;
mod library;
mod live;
mod settings;
mod structure;
mod textures;
mod theme;
mod tools;
mod viewport;

use craft_ui::docking::{Action, DockArea, DockStyle, Layout, Node, Permissions, StackEntry};
use craft_ui::layout::{SplitAxis, SplitSize};
use editors::{Cmds, text_field};

use dowel::design::{self, Part, Project, Session};
use dowel::job::{self, Job, MaterialCut};
use dowel::nest::Params;
use dowel::panel::{self, Side};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};
use dowel::i18n::bazis;
use serde_json::{Value, json};

fn main() -> eframe::Result {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Language of commands and the MCP server: `--lang ru` or DOWEL_LANG=ru (the window keeps its own setting).
    dowel::i18n::from_env();
    while let Some(i) = args.iter().position(|a| a == "--lang" || a.starts_with("--lang=")) {
        let (lang, n) = match args[i].strip_prefix("--lang=") {
            Some(l) => (l.to_string(), 1),
            None => (args.get(i + 1).filter(|l| !l.starts_with('-')).cloned().unwrap_or_default(), 2),
        };
        dowel::i18n::set_language(&lang);
        args.drain(i..(i + n).min(args.len()));
    }
    // One program: a command or `mcp` runs without a window; anything else (nothing, a file) opens the window.
    match args.first().map(String::as_str) {
        Some("mcp") => {
            dowel_craft_mcp::serve();
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("dowel-craft {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(c) if dowel_craft_cli::is_command(c) => std::process::exit(if dowel_craft_cli::main(&args) == std::process::ExitCode::SUCCESS { 0 } else { 1 }),
        _ => {}
    }
    let screenshot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let tab = args.iter().position(|a| a == "--tab").and_then(|i| args.get(i + 1)).cloned();
    // 3D mode for screenshots: solid | hidden | wire | xray.
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
        Some("textured") => viewport::Mode::Textured,
        _ => viewport::Mode::Solid,
    };
    let pane = match tab.as_deref() {
        Some("parts") => Pane::Parts,
        Some("cut") => Pane::Cut,
        Some("front") => Pane::Front,
        Some("library") => Pane::Library,
        Some("project") => Pane::Project,
        _ => Pane::Model,
    };
    // No file means a full-window start page, as in cadcraft.
    app.start = (file.is_none() && tab.is_none()) || tab.as_deref() == Some("start");
    app.show_pane(pane);
    app.quad = args.iter().any(|a| a == "--quad");
    // For screenshots: the texture picker for a new material.
    if let Some(i) = args.iter().position(|a| a == "--pick-texture") {
        app.textures.open_for = Some(textures::Target::New);
        if let Some(q) = args.get(i + 1).filter(|q| !q.starts_with("--")) {
            app.textures.set_search(q);
        }
    }
    // For screenshots: how many frames to wait (so background previews have time to load).
    app.shot_frames = args.iter().position(|a| a == "--frames").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(5);
    // For screenshots: enable the "into opening" tool (horizontal | vertical | front | door | drawers) or a modify tool (move).
    match args.iter().position(|a| a == "--tool").and_then(|i| args.get(i + 1)).map(String::as_str) {
        Some("move") => app.modify = Some(tools::Modify::Move),
        Some("fasten") => app.fastening = true,
        Some(t @ ("horizontal" | "vertical" | "front" | "door" | "drawers")) => {
            let plane = ["horizontal", "vertical", "front", "door", "drawers"].into_iter().find(|p| *p == t).unwrap_or("horizontal");
            app.toggle_fill(plane);
        }
        _ => {}
    }
    // For screenshots: select a part by number.
    if let Some(i) = args.iter().position(|a| a == "--select").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()) {
        app.select(Some(i));
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]).with_title("dowel-craft")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).expect("bundled icon is a valid PNG")),
        ..Default::default()
    };
    eframe::run_native(
        "dowel-craft",
        options,
        Box::new(move |cc| {
            // cadcraft theme; icon tooltips appear almost immediately.
            theme::apply(&cc.egui_ctx);
            // MCP commands go to the open tab (not for screenshots).
            if app.screenshot.is_none() {
                app.live = live::listen(&dowel::live::socket_path(), cc.egui_ctx.clone());
            }
            Ok(Box::new(app))
        }),
    )
}

/// Window panes (layout via craft-ui docking).
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum Pane {
    Tools,
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
            Pane::Tools => "Tools",
            Pane::Model => "3D",
            Pane::Front => "Front view",
            Pane::Parts => "Parts",
            Pane::Cut => "Cutting",
            Pane::Structure => "Model structure",
            Pane::Project => "Project",
            Pane::Properties => "Properties",
            Pane::Library => "Hardware",
        }
    }
}

/// Default layout (as in cadcraft): on the left a stack of the "Tools", "Project", "Fasteners" palettes;
/// in the center Start, 3D and the tabs; on the right "Model structure" stacked above "Properties". Palettes
/// collapse with a button in the header, and can be dragged by the header elsewhere or out as a window.
fn default_layout() -> Layout<Pane> {
    let tabs = |panels: Vec<Pane>| Box::new(Node::Tabs { panels, active: 0 });
    let stack = |entries: Vec<(Pane, bool, Option<f32>)>| Box::new(Node::Stack { entries: entries.into_iter().map(|(panel, open, height)| StackEntry { panel, open, height }).collect() });
    Layout {
        root: Some(Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedFirst(280.0),
            first: stack(vec![(Pane::Tools, true, Some(600.0)), (Pane::Project, true, Some(260.0)), (Pane::Library, false, None)]),
            second: Box::new(Node::Split {
                axis: SplitAxis::Horizontal,
                size: SplitSize::FixedSecond(380.0),
                first: tabs(vec![Pane::Model]),
                second: stack(vec![(Pane::Structure, true, Some(300.0)), (Pane::Properties, true, None)]),
            }),
        }),
        floating: Vec::new(),
    }
}

/// Panes can't be closed (there'd be no way to bring them back), but they move and detach.
fn permissions(p: &Pane) -> Permissions {
    // The center canvas has no tab strip and is fixed, as in cadcraft; the view is chosen in the top bar.
    if *p == Pane::Model { Permissions::PROTECTED } else { Permissions { closable: false, ..Permissions::default() } }
}

/// A Bazis script with open windows.
struct ScriptWin {
    name: String,
    rt: Option<script::Runtime>,
    /// Result once the script has finished (messages, error).
    done: Option<script::Run>,
}

/// Script window event: a click or a new field value.
enum ScriptEvent {
    Click(i64),
    Change(i64, &'static str, Value),
}

/// Script window control from its description (JSON from `__forms()`).
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
            if ui.button(if caption.is_empty() { "Button" } else { caption }).clicked() {
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
                if crate::expr::add(ui, &mut v, |d| d.speed(1.0)).changed() {
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
            // Text fields and everything else with text.
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

/// Fastener model library in the window.
struct Library {
    /// Model folders; one tree per folder. Remembered in settings.
    dirs: Vec<String>,
    models: Vec<dowel::library::ModelInfo>,
    errors: Vec<String>,
    search: String,
    thumbs: library::Thumbs,
    /// Folders already scanned (otherwise scan when the tab is shown).
    scanned: bool,
    /// Background folder scan in progress: the result arrives here (the window doesn't freeze on hundreds of models).
    pending: Option<std::sync::mpsc::Receiver<(Vec<dowel::library::ModelInfo>, Vec<String>)>>,
}

impl Library {
    fn new(dirs: Vec<String>) -> Self {
        let mut lib = Library { dirs, models: Vec::new(), errors: Vec::new(), search: String::new(), thumbs: Default::default(), scanned: false, pending: None };
        // Right at startup, in the background, so everything is ready when the tab opens.
        lib.scan();
        lib
    }

    /// Rescan all folders on a background thread.
    fn scan(&mut self) {
        let dirs = self.dirs.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut models, mut errors) = (Vec::new(), Vec::new());
            for d in &dirs {
                let (m, e) = dowel::library::scan(std::path::Path::new(d));
                if m.is_empty() && e.is_empty() {
                    errors.push(format!("no .fr3d models in “{d}”"));
                }
                models.extend(m);
                errors.extend(e);
            }
            let _ = tx.send((models, errors));
        });
        self.pending = Some(rx);
        self.scanned = true;
    }

    /// Collect the background scan result if ready; true means still running.
    fn poll(&mut self) -> bool {
        let Some(rx) = &self.pending else { return false };
        match rx.try_recv() {
            Ok((models, errors)) => {
                (self.models, self.errors) = (models, errors);
                self.thumbs.clear();
                self.pending = None;
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                false
            }
        }
    }
}

/// PNG → egui image.
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

/// What a click on the panel contour diagram does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ContourTool {
    /// Apply or remove edge banding on an edge.
    Edge,
    /// Round the corner at a vertex.
    Round,
    /// Chamfer the corner.
    Chamfer,
    /// Round cutout at a point.
    Hole,
    /// Groove along a straight edge.
    Groove,
}

/// Mode for placing a panel into an opening (like "~" in Bazis).
struct FillTool {
    /// horizontal | vertical | front: a panel; door: a door; drawers: drawers.
    plane: &'static str,
    material: String,
    /// Gaps from the boundaries in contour axes (see the `panel.fill` command).
    gaps: [f64; 4],
    /// Doors and drawers: overlay, half-overlay, inset.
    mount: &'static str,
    double: bool,
    /// Drawer front heights from bottom to top, space-separated.
    heights: String,
}

/// Selection: top-level object, tree panel (0 means a template part) and part number within the object.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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

    /// Index of the selected part in the current parts list.
    fn find(&self, parts: &[Part]) -> Option<usize> {
        parts.iter().enumerate().filter(|(_, p)| p.owner == self.owner && p.id == self.id).nth(self.nth).map(|(i, _)| i)
    }
}

/// Number for display: to tenths of a mm, without computation noise (2341.9999999999927 → 2342) and "-0".
fn num(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    let r = if r == 0.0 { 0.0 } else { r };
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r:.1}") }
}

/// Export kinds: (what, menu item).
const EXPORTS: [(&str, &str); 8] = [
    ("html", "Cutting maps and production list (HTML)…"),
    ("xml", "BAZIS-Cutting project (XML)…"),
    ("spec", "Specification (CSV)…"),
    ("smeta", "Material estimate (CSV)…"),
    ("b3d", "Bazis-Mebelshchik model (.b3d)…"),
    ("drawings", "Drawings: assembly and parts (printable HTML)…"),
    ("drawings_pdf", "Drawings (PDF)…"),
    ("cards_pdf", "Cutting maps (PDF)…"),
];

/// Side panel: margin from the edges and scrolling.
fn side(ui: &mut egui::Ui, id: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 8 }).show(ui, body);
    });
}

/// A free name: `base`, or if taken, "base (2)", "base (3)"…
fn free_name(base: &str, taken: &[&str]) -> String {
    let base = base.trim();
    let stem = base.rsplit_once(" (").filter(|(_, n)| n.trim_end_matches(')').parse::<u32>().is_ok()).map_or(base, |(s, _)| s);
    std::iter::once(base.to_string()).chain((2..).map(|k| format!("{stem} ({k})"))).find(|n| !taken.contains(&n.as_str())).unwrap_or_default()
}

/// Right-click menu item (and the Delete key).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuAct {
    Delete,
    Block,
    Explode,
    Deselect,
}

/// Fields of the new material / edge banding form.
struct NewMaterial {
    name: String,
    thickness: f64,
    sheet: (f64, f64),
    coef: f64,
    /// Texture image (chosen in the texture library).
    texture: Option<String>,
}

struct App {
    /// Our own project; none means a Bazis file is open read-only.
    session: Option<Session>,
    job: Job,
    path: Option<PathBuf>,
    layout: Layout<Pane>,
    /// Selected parts; the last one is the primary (its properties are shown).
    selection: Vec<Sel>,
    /// Parameters of actions on the selection: move/copy (dx, dy, dz), axis, angle, number of copies.
    actions: ([f64; 3], char, f64, u32),
    /// Dragging in the front view: offset in mm.
    drag: Option<[f64; 2]>,
    /// Resize handle in the front view: (which edge X: −1/0/1, Y: −1/0/1) and offset in mm.
    handle: Option<([i8; 2], [f64; 2])>,
    /// Ctrl is held (this frame): a click adds to the selection.
    ctx_ctrl: bool,
    params: Params,
    cuts: Vec<MaterialCut>,
    card: usize,
    show_doors: bool,
    error: Option<String>,
    status: String,
    new_material: NewMaterial,
    new_edge: (String, f64, f64),
    /// The "New material" and "New edge banding" forms are open (via the button).
    adding: (bool, bool),
    /// New panel: plane and contour dimensions.
    new_panel: (&'static str, f64, f64),
    /// "Panel into opening" mode in the front view: plane, material, gaps.
    fill: Option<FillTool>,
    /// All open projects; the active slot (`active`) holds a placeholder, the active one itself lives in the window fields.
    docs: Vec<docs::Doc>,
    active: usize,
    /// The active project has unsaved changes.
    dirty: bool,
    /// Ask "Save changes?" before closing a tab.
    close_ask: Option<docs::CloseAsk>,
    /// The "Settings" window is open (language, grid).
    drafting: bool,
    /// The start page is shown (instead of the work panes), like the "Start" tab in cadcraft.
    start: bool,
    /// Parts hidden with the eye icon in "Model structure" (not drawn in 3D or the front view).
    hidden: std::collections::HashSet<Sel>,
    /// Canvas view: 3D, front view, parts table or cutting (switched in the top bar).
    view: Pane,
    /// The "Fasteners" tool is active (parameters below the palette).
    fastening: bool,
    /// The "How to use" and "About" windows are open.
    help: bool,
    about: bool,
    /// The active selection-modifying tool (the "Tools" palette).
    modify: Option<tools::Modify>,
    /// Edging: edge banding, "Trim", allowance.
    edging: (Option<String>, bool, f64),
    /// Fastener model library: folder, found models, previews (textures by path), search string.
    library: Library,
    /// "Model analysis": problems found (the window is open while Some).
    problems: Option<Vec<design::analysis::Problem>>,
    /// The running Bazis script and its windows.
    script: Option<ScriptWin>,
    /// Texture folder (like "File paths → Textures" in Bazis).
    textures: textures::TextureLib,
    /// Selected furniture (it has no parts, so it is selected separately).
    furniture_sel: Option<u32>,
    /// Settings kept between launches (fastener folders, language).
    settings: settings::Settings,
    /// Loaded fastener meshes: file → triangles.
    meshes: std::collections::HashMap<String, std::sync::Arc<Vec<dowel::panel::Tri>>>,
    /// Fastener to place between panels.
    fastener_kind: String,
    /// Cam lock (Minifix) cup outside the cabinet (inside by default).
    fastener_outside: bool,
    /// Contour diagram click action and its size (radius, chamfer leg, cutout radius), mm.
    contour_tool: (ContourTool, f64),
    /// New groove: width, depth, offset from the edge (mm), on the front face.
    groove: (f64, f64, f64, bool),
    screenshot: Option<PathBuf>,
    frames: u32,
    /// Frame on which to capture the window (`--frames`).
    shot_frames: u32,
    /// MCP requests to the open tab (`live`).
    live: Option<std::sync::mpsc::Receiver<live::Request>>,
    /// Product parts with positions (or a build error), rebuilt in `refresh`.
    parts: Result<Vec<Part>, String>,
    /// Bumped on every change to parts or their visibility: the model view rebuilds its mesh.
    revision: u64,
    viewport: viewport::Viewport,
    /// "4 views" (like "Projections" in Bazis): front, left, top, and the main view is axonometric.
    quad: bool,
    /// In "4 views" one view is maximized to the whole area: 0 front, 1 left, 2 top, 3 is 3D.
    quad_max: Option<usize>,
    /// Waiting for a key press in settings: the new shortcut for this action.
    key_capture: Option<&'static str>,
    views: [viewport::Viewport; 3],
}

impl Default for App {
    fn default() -> Self {
        let settings = settings::Settings::load();
        i18n::set_language(&settings.language);
        let textures = textures::TextureLib::new(settings.texture_dirs.clone());
        let mut app = App {
            session: Some(Session::new(Project::new("New project"))),
            job: Job::from_project(Project::new("New project")),
            path: None,
            layout: default_layout(),
            selection: Vec::new(),
            actions: ([0.0; 3], 'x', 90.0, 1),
            drag: None,
            handle: None,
            ctx_ctrl: false,
            params: Params { sheet: (2800.0, 2070.0), kerf: 4.0, trim: 10.0 },
            cuts: Vec::new(),
            card: 0,
            show_doors: true,
            error: None,
            status: String::new(),
            new_material: NewMaterial { name: crate::i18n::t("Chipboard 16"), thickness: 16.0, sheet: (2800.0, 2070.0), coef: 1.2, texture: None },
            new_edge: (crate::i18n::t("PVC 0.4"), 0.4, 19.0),
            adding: (false, false),
            new_panel: ("vertical", 560.0, 720.0),
            fill: None,
            modify: None,
            start: false,
            drafting: false,
            docs: vec![docs::Doc::empty()],
            active: 0,
            dirty: false,
            close_ask: None,
            help: false,
            fastening: false,
            view: Pane::Model,
            hidden: Default::default(),
            about: false,
            edging: (None, true, 0.0),
            contour_tool: (ContourTool::Edge, 50.0),
            groove: (4.0, 8.0, 16.0, false),
            fastener_kind: dowel::fasteners::library().first().map(|d| d.name.clone()).unwrap_or_default(),
            fastener_outside: false,
            library: Library::new(settings.library_dirs.clone()),
            script: None,
            problems: None,
            furniture_sel: None,
            meshes: Default::default(),
            settings,
            textures,
            screenshot: None,
            frames: 0,
            shot_frames: 5,
            live: None,
            parts: Ok(Vec::new()),
            revision: 0,
            viewport: viewport::Viewport::default(),
            quad: false,
            quad_max: None,
            key_capture: None,
            views: [viewport::Viewport::with_view(0.0, 0.0, "Front"), viewport::Viewport::with_view(90.0, 0.0, "Left"), viewport::Viewport::with_view(0.0, 89.9, "Top")],
        };
        app.refresh();
        app
    }
}

impl App {
    fn open(&mut self, path: PathBuf) {
        match Job::open(&path) {
            Ok(job) => {
                // An empty unsaved project is replaced by the file, otherwise the file opens in a new tab.
                if !self.current_is_blank() {
                    self.add_doc(docs::Doc::empty());
                }
                self.session = job.project.clone().map(Session::new);
                self.job = job;
                self.dirty = false;
                self.selection.clear();
                self.furniture_sel = None;
                self.status = format!("Opened {}", path.display());
                chrome::remember(&path);
                self.start = false;
                self.show_pane(Pane::Model);
                self.path = Some(path);
                self.error = None;
                self.card = 0;
                self.refresh();
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Rebuild parts and the cutting layout after a change.
    fn refresh(&mut self) {
        if let Some(s) = &self.session {
            self.job = Job::from_project(s.project.clone());
        }
        let (job, params) = (&self.job, self.params);
        self.cuts = job::cut(job, |m| job.params(m, params));
        self.parts = job::parts(&self.job);
        self.revision += 1;
    }

    /// Command on the project; each is logged to the terminal (stderr), errors also to the status bar.
    fn exec(&mut self, cmd: &str, p: Value) {
        self.exec_ret(cmd, p);
    }

    /// Same, returning the command's response (e.g. `{id}` of the new object); `None` on error.
    fn exec_ret(&mut self, cmd: &str, p: Value) -> Option<Value> {
        let Some(s) = &mut self.session else {
            eprintln!("[command] {cmd}: no own project (a Bazis file is open read-only)");
            return None;
        };
        match s.execute(cmd, &p) {
            Ok(r) => {
                eprintln!("[command] {cmd} {p} → {}", if r.is_null() { "ok".into() } else { r.to_string() });
                if !matches!(cmd, "parts" | "check" | "project.get") {
                    self.dirty = true;
                }
                self.error = None;
                self.refresh();
                Some(r)
            }
            Err(e) => {
                eprintln!("[error] {cmd} {p}: {e}");
                self.error = Some(e);
                None
            }
        }
    }

    /// Texture library window: the chosen image goes into the material or the new-material form.
    fn texture_window(&mut self, ctx: &egui::Context) {
        let current = match &self.textures.open_for {
            Some(textures::Target::New) => self.new_material.texture.clone(),
            Some(textures::Target::Material(name)) => self.session.as_ref().and_then(|s| s.project.materials.iter().find(|m| &m.name == name)).and_then(|m| m.texture.clone()),
            None => return,
        };
        let picked = self.textures.window(ctx, current.as_deref());
        if self.textures.dirs() != self.settings.texture_dirs.as_slice() {
            self.settings.texture_dirs = self.textures.dirs().to_vec();
            self.settings.save();
        }
        let Some((target, path)) = picked else { return };
        let path = path.map(|p| p.display().to_string());
        match target {
            textures::Target::New => self.new_material.texture = path,
            textures::Target::Material(name) => self.exec("material.set", json!({ "name": name, "texture": path })),
        }
    }

    /// Make a tab visible (active in its group).
    fn show_pane(&mut self, pane: Pane) {
        if matches!(pane, Pane::Model | Pane::Front | Pane::Parts | Pane::Cut) {
            self.view = pane;
        } else {
            let _ = self.layout.apply(Action::Activate { panel: pane });
        }
    }

    /// "Model analysis": a window with the problems.
    fn analyze(&mut self) {
        let project = self.session.as_ref().map(|s| s.project.clone()).unwrap_or_else(|| self.job.import());
        match design::analysis::analyze(&project) {
            Ok(p) => self.problems = Some(p),
            Err(e) => self.error = Some(e),
        }
    }

    /// Pick and run a Bazis script.
    fn pick_script(&mut self) {
        if self.session.is_some()
            && let Some(p) = rfd::FileDialog::new().add_filter("BAZIS-Script", &["js"]).pick_file()
        {
            self.start_script(&p);
        }
    }

    /// New empty project (cutting parameters are kept).
    /// The new project opens in a new tab; open ones are left untouched.
    fn new_project(&mut self) {
        self.add_doc(docs::Doc::empty());
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Models", &["dcraft", "b3d", "fr3d"]).pick_file() {
            self.open(p);
        }
    }

    /// Bazis model (read-only) → our own project.
    fn make_editable(&mut self) {
        let p = self.job.import();
        self.status = format!("Parts imported: {}", p.panels().len());
        self.session = Some(Session::new(p));
        self.path = None;
        self.refresh();
    }

    fn save(&mut self, ask: bool) {
        let Some(s) = &self.session else {
            self.error = Some("The Bazis model is open read-only".into());
            return;
        };
        let path = match (&self.path, ask) {
            (Some(p), false) if dowel::design::is_project_file(p) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("dowel-craft project", &["dcraft"]).set_file_name(format!("{}.dcraft", s.project.name)).save_file(),
        };
        if let Some(p) = path
            && let Err(e) = self.save_to(p)
        {
            self.error = Some(e);
        }
    }

    /// Write our project to a file; it becomes the tab's file.
    fn save_to(&mut self, p: PathBuf) -> Result<(), String> {
        let s = self.session.as_ref().ok_or("The Bazis model is open read-only")?;
        std::fs::write(&p, s.project.save()).map_err(|e| format!("{}: {e}", p.display()))?;
        self.status = format!("Saved: {}", p.display());
        self.dirty = false;
        chrome::remember(&p);
        self.path = Some(p);
        Ok(())
    }

    fn export(&mut self, what: &str) {
        let (ext, filter) = match what {
            "html" => ("html", "Cutting maps"),
            "xml" => ("xml", "BAZIS-Cutting project"),
            "spec" | "smeta" => ("csv", "CSV table"),
            "b3d" => ("b3d", "Bazis-Mebelshchik model"),
            "drawings" => ("html", "Drawings"),
            "drawings_pdf" | "cards_pdf" => ("pdf", "PDF"),
            _ => return,
        };
        let suffix = match what {
            "spec" => " specification",
            "drawings" | "drawings_pdf" => " drawings",
            "cards_pdf" => " cutting maps",
            "smeta" => " estimate",
            _ => "",
        };
        let Some(p) = rfd::FileDialog::new().add_filter(filter, &[ext]).set_file_name(format!("{}{}.{ext}", self.job.name, crate::i18n::t(suffix))).save_file() else {
            return;
        };
        let job = &self.job;
        let sheet = self.params.sheet;
        let bytes = match what {
            "html" => job::document(job, &self.cuts).into_bytes(),
            "xml" => dowel::raskroy::project_xml(&job.name, &job.panels, |m| job.sheet(m, sheet)),
            "spec" => dowel::report::spec_csv(&panel::spec(&job.panels)).into_bytes(),
            "drawings_pdf" => match job::parts(job).and_then(|parts| dowel::pdf::from_svg_pages(&dowel::drawing::drawings(&parts, &job.name))) {
                Ok(b) => b,
                Err(e) => return self.error = Some(e),
            },
            "cards_pdf" => match dowel::pdf::from_svg_pages(&job::cards(job, &self.cuts)) {
                Ok(b) => b,
                Err(e) => return self.error = Some(e),
            },
            "drawings" => match job::parts(job) {
                Ok(parts) => dowel::report::document_html(&job.name, &dowel::drawing::drawings(&parts, &job.name), "").into_bytes(),
                Err(e) => return self.error = Some(e),
            },
            "b3d" => match job.project.as_ref().map(dowel::export::b3d) {
                Some(Ok(b)) => b,
                Some(Err(e)) => return self.error = Some(e),
                None => return self.error = Some("The Bazis model is read-only: use “Make editable”, then export".into()),
            },
            _ => dowel::report::materials_csv(&job.estimate(1.2)).into_bytes(),
        };
        match std::fs::write(&p, bytes) {
            Ok(()) => self.status = format!("Saved: {}", p.display()),
            Err(e) => self.error = Some(format!("{}: {e}", p.display())),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.ctx_ctrl = ui.input(|i| i.modifiers.command);
        self.serve_live();
        let ctx = ui.ctx().clone();
        self.top_bar(ui);
        // Esc leaves the tool, and with no tool active, clears the selection.
        if !ctx.text_edit_focused() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.fill.is_some() || self.modify.is_some() || self.fastening {
                (self.fill, self.modify, self.fastening) = (None, None, false);
            } else {
                self.menu_apply(MenuAct::Deselect);
            }
        }
        // Hotkeys (configured in "Edit → Settings"). They don't fire while a new one is being assigned.
        // Without Ctrl/Alt only outside text fields; longer shortcuts first so "1" doesn't eat Alt+1.
        if self.key_capture.is_none() {
            let editing = ctx.text_edit_focused();
            let mut list: Vec<(&str, egui::KeyboardShortcut)> = keys::ACTIONS.iter().filter_map(|(id, ..)| Some((*id, self.settings.shortcut(id)?))).collect();
            list.sort_by_key(|(_, sc)| std::cmp::Reverse(keys::weight(sc)));
            let fired: Vec<&str> = list.into_iter().filter(|(_, sc)| (!editing || sc.modifiers.command || sc.modifiers.alt) && ui.input_mut(|i| i.consume_shortcut(sc))).map(|(id, _)| id).collect();
            for id in fired {
                self.hotkey(id);
            }
        }
        let hover = icons::take_hover(&ctx);
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                match &self.error {
                    Some(e) => {
                        ui.colored_label(Color32::from_rgb(220, 70, 70), i18n::t(e));
                    }
                    None => match &hover {
                        // Mouse over an icon: what it does.
                        Some(tip) => {
                            ui.label(egui::RichText::new(format!("🛈 {tip}")).color(Color32::from_rgb(0xf5, 0xd0, 0x8a)));
                        }
                        None => {
                            ui.label(egui::RichText::new(i18n::t(self.hint())).color(Color32::from_rgb(0x9f, 0xc8, 0xf5)));
                        }
                    },
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Like cadcraft's status bar toggles: grid (F7) and drafting settings.
                    if icons::button(ui, icons::Icon::Gear, 20.0, "Settings: language, grid", self.drafting).clicked() {
                        self.drafting = !self.drafting;
                    }
                    if icons::button(ui, icons::Icon::Grid, 20.0, "Grid (F7)", self.settings.grid.on).clicked() {
                        self.settings.grid.on = !self.settings.grid.on;
                        self.settings.save();
                    }
                    if !self.status.is_empty() {
                        ui.weak(i18n::t(&self.status));
                    }
                });
            });
        });
        if self.start {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.start_page(ui));
        } else {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::T.chrome_dark)).show(ui, |ui| {
            let layout = self.layout.clone();
            // Palette colors as in cadcraft: headers lighter than the background, the active tab bluish.
            let mut style = DockStyle::from_ui(ui);
            style.min_pane = 96.0;
            let t = &theme::T;
            (style.background, style.tab_background, style.active_background) = (t.panel, t.chrome, t.tab_active);
            (style.text, style.inactive_text, style.accent) = (t.text, t.text_dim, t.accent);
            style.border = Stroke::new(1.0, t.border);
            let out = DockArea::new(egui::Id::new("dock")).show(ui, &layout, &style, |p| i18n::t(p.label()), permissions, |ui, pane| match pane {
                Pane::Tools => side(ui, "tools", |ui| self.tools_panel(ui)),
                // Canvas: the view chosen in the top bar.
                Pane::Model | Pane::Front | Pane::Parts | Pane::Cut => match self.view {
                    Pane::Front => self.front_view(ui),
                    Pane::Parts => egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| self.parts_table(ui)).inner,
                    Pane::Cut => egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| self.cut_view(ui)).inner,
                    _ => self.iso_view(ui, frame),
                },
                Pane::Structure => side(ui, "structure", |ui| self.structure_panel(ui)),
                Pane::Project => side(ui, "project", |ui| self.project_panel(ui)),
                Pane::Properties => side(ui, "props", |ui| self.properties_panel(ui)),
                Pane::Library => egui::Frame::new().inner_margin(8).show(ui, |ui| self.library_panel(ui)).inner,
            });
            for a in out.actions {
                if let Err(e) = self.layout.apply_with_permissions(a, permissions) {
                    self.status = format!("Layout: {e:?}");
                }
            }
        });
        }
        // Grid settings go to all model views.
        let g = self.settings.grid;
        for v in std::iter::once(&mut self.viewport).chain(self.views.iter_mut()) {
            v.grid = viewport::Grid { on: g.on, step: g.step, major: g.major };
        }
        self.help_windows(&ctx);
        self.close_window(&ctx);
        self.texture_window(&ctx);
        self.script_window(&ctx);
        self.problems_window(&ctx);
        self.take_screenshot(&ctx);
    }
}

impl App {
    /// Top icon bar (as in cadcraft): file, undo, export, check; project name.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        use icons::{Icon, labeled, labeled_if};
        egui::Panel::top("toolbar").exact_size(34.0).frame(egui::Frame::NONE.fill(theme::T.chrome).inner_margin(egui::Margin::symmetric(10, 0))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if labeled(ui, Icon::New, "New", "New project", false).clicked() {
                    self.new_project();
                }
                if labeled(ui, Icon::Open, "Open", "Open… (.dcraft, .b3d, .fr3d)", false).clicked() {
                    self.open_dialog();
                }
                if labeled(ui, Icon::Save, "Save", "Save (Ctrl+S)", false).clicked() {
                    self.save(false);
                }
                if labeled(ui, Icon::SaveAs, "Save as", "Save as…", false).clicked() {
                    self.save(true);
                }
                ui.separator();
                let (undo, redo) = self.session.as_ref().map_or((false, false), |s| (s.can_undo(), s.can_redo()));
                if labeled_if(ui, undo, Icon::Undo, "Undo", "Undo (Ctrl+Z)", false).clicked() {
                    self.exec("undo", json!({}));
                }
                if labeled_if(ui, redo, Icon::Redo, "Redo", "Redo (Ctrl+Y)", false).clicked() {
                    self.exec("redo", json!({}));
                }
                ui.separator();
                let export = labeled(ui, Icon::Export, "Export", "Export: cutting, BAZIS-Cutting, specification, estimate, .b3d, drawings", false);
                egui::Popup::menu(&export).show(|ui| {
                    for (what, label) in EXPORTS {
                        if ui.button(crate::i18n::t(label)).clicked() {
                            self.export(what);
                        }
                    }
                });
                if labeled(ui, Icon::Analyze, "Analyze", "Model analysis", false).clicked() {
                    self.analyze();
                }
                if labeled_if(ui, self.session.is_some(), Icon::Script, "Script", "Run a Bazis script (.js)", false).clicked() {
                    self.pick_script();
                }
                ui.separator();
                // Canvas view: like tabs, but in the icon bar.
                for (pane, icon, text, tip) in [
                    (Pane::Model, Icon::ViewIso, "3D", "Model in 3D (Ctrl+1); the view bar has “4 windows”: front, left, top and 3D"),
                    (Pane::Front, Icon::ViewFront, "Front", "Front view (Ctrl+2): put shelves, doors and drawers into an opening, drag with the mouse"),
                    (Pane::Parts, Icon::Table, "Parts", "Parts table (Ctrl+3): sizes, material, edge banding"),
                    (Pane::Cut, Icon::CutSheet, "Cutting", "Sheet cutting maps (Ctrl+4)"),
                ] {
                    if icons::labeled(ui, icon, text, tip, !self.start && self.view == pane).clicked() {
                        self.start = false;
                        self.view = pane;
                    }
                }
                if self.session.is_none() && labeled(ui, Icon::Unlock, "Edit model", "Make editable: import the Bazis model into your own project", false).clicked() {
                    self.make_editable();
                }
                ui.separator();
                let title = format!("{} — parts: {}", self.job.name, self.job.panels.len());
                ui.label(egui::RichText::new(crate::i18n::t(title)).strong());
                if self.session.is_none() {
                    ui.weak(crate::i18n::t("(Bazis model, read-only)"));
                }
            });
        });
        egui::Panel::top("menu").frame(egui::Frame::NONE.fill(theme::T.chrome).inner_margin(egui::Margin::symmetric(6, 2))).show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("doc_tabs").exact_size(26.0).frame(egui::Frame::NONE.fill(theme::T.chrome_dark)).show(ui, |ui| self.doc_tabs(ui));
    }

    /// Hotkey action (`keys::ACTIONS`).
    fn hotkey(&mut self, id: &str) {
        let (undo, redo) = self.session.as_ref().map_or((false, false), |s| (s.can_undo(), s.can_redo()));
        match id {
            "save" => self.save(false),
            "undo" if undo => self.exec("undo", json!({})),
            "redo" if redo => self.exec("redo", json!({})),
            "delete" => self.menu_apply(MenuAct::Delete),
            "grid" => {
                self.settings.grid.on = !self.settings.grid.on;
                self.settings.save();
            }
            "view.3d" | "view.front" | "view.parts" | "view.cut" => {
                self.start = false;
                self.view = match id {
                    "view.3d" => Pane::Model,
                    "view.front" => Pane::Front,
                    "view.parts" => Pane::Parts,
                    _ => Pane::Cut,
                };
            }
            "cam.front" | "cam.left" | "cam.top" | "cam.iso" | "cam.fit" => {
                self.start = false;
                self.view = Pane::Model;
                // In "4 views" three views are already front, left, top: the camera applies to the one big view.
                if id != "cam.fit" {
                    self.quad = false;
                }
                match id {
                    "cam.front" => self.viewport.set_view(0.0, 0.0),
                    "cam.left" => self.viewport.set_view(90.0, 0.0),
                    "cam.top" => self.viewport.set_view(0.0, 89.9),
                    "cam.iso" => self.viewport.set_view(viewport::Camera::AXON.0, viewport::Camera::AXON.1),
                    _ => self.viewport.refit(),
                }
            }
            _ => {
                let modes = ["mode.solid", "mode.hidden", "mode.wire", "mode.xray", "mode.textured"];
                if let Some(n) = modes.iter().position(|m| *m == id) {
                    self.viewport.mode = viewport::Mode::ALL[n].0;
                }
            }
        }
    }

    fn project_panel(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.session else { return };
        let project = s.project.clone();
        let mut cmds: Cmds = Vec::new();

        if let Some(name) = text_field(ui, "project_name", &project.name) {
            cmds.push(("project.rename", json!({ "name": name })));
        }

        egui::CollapsingHeader::new(crate::i18n::t(format!("Materials ({})", project.materials.len()))).default_open(true).show(ui, |ui| {
            for m in &project.materials {
                // Each material in its own frame so it doesn't blend with the new-material form.
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.strong(&m.name);
                        if ui.small_button(crate::i18n::t("Delete")).on_hover_text(crate::i18n::t("Remove the material from the project (if it is not used anywhere)")).clicked() {
                            cmds.push(("material.remove", json!({ "name": m.name })));
                        }
                    });
                    ui.label(crate::i18n::t(format!("{} mm, sheet {}×{}, factor {}", m.thickness, m.sheet.0, m.sheet.1, m.coef)));
                    ui.horizontal_wrapped(|ui| {
                        let shown = m.texture.as_deref().map_or_else(|| crate::i18n::t("none"), |t| textures::label(std::path::Path::new(t)));
                        ui.label(crate::i18n::t(format!("texture: {shown}")));
                        if ui.small_button(crate::i18n::t("Choose…")).on_hover_text(crate::i18n::t("Texture library: folders, previews, search")).clicked() {
                            self.textures.open_for = Some(textures::Target::Material(m.name.clone()));
                        }
                        if ui.small_button(crate::i18n::t("by article")).on_hover_text(crate::i18n::t("Find d(<article>)1.jpg in the texture folders (Baltlaminat)")).clicked() {
                            match self.textures.dirs().iter().find_map(|d| dowel::library::find_texture(std::path::Path::new(d), &m.article)) {
                                Some(p) => cmds.push(("material.set", json!({ "name": m.name, "texture": p.display().to_string() }))),
                                None => self.status = format!("No texture for article “{}”", m.article),
                            }
                        }
                        let mut tile = m.texture_size.unwrap_or([1000.0, 1000.0]);
                        ui.label(crate::i18n::t("tile")).on_hover_text(crate::i18n::t("Texture image size on the part, mm: along the grain × across"));
                        let r1 = crate::expr::add(ui, &mut tile[0], |d| d.range(10.0..=5000.0).suffix(crate::i18n::t(" mm")));
                        let r2 = crate::expr::add(ui, &mut tile[1], |d| d.range(10.0..=5000.0).suffix(crate::i18n::t(" mm"))).on_hover_text(crate::i18n::t("Texture tile: along the grain × across"));
                        let done = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
                        if (done(&r1) || done(&r2)) && Some(tile) != m.texture_size {
                            cmds.push(("material.set", json!({ "name": m.name, "texture_size": tile })));
                        }
                    });
                    ui.horizontal(|ui| {
                        // Empty means the general cutting parameters from the "Cutting" tab.
                        for (key, label, value, general) in [("trim", "trim", m.trim, self.params.trim), ("kerf", "kerf", m.kerf, self.params.kerf)] {
                            let mut v = value.unwrap_or(general);
                            ui.label(crate::i18n::t(label));
                            let r = crate::expr::add(ui, &mut v, |d| d.range(0.0..=50.0).speed(0.1).suffix(crate::i18n::t(" mm")));
                            if (r.drag_stopped() || (r.changed() && !r.dragged())) && Some(v) != value {
                                cmds.push(("material.set", json!({ "name": m.name, key: v })));
                            }
                        }
                    });
                });
            }
            let nm = &mut self.new_material;
            let name_id = egui::Id::new("new_material_name");
            let names: Vec<&str> = project.materials.iter().map(|m| m.name.as_str()).collect();
            // A taken name not currently being edited is immediately replaced with a free one ("Chipboard 16 (2)").
            if names.contains(&nm.name.trim()) && !ui.memory(|m| m.has_focus(name_id)) {
                nm.name = free_name(&nm.name, &names);
            }
            let taken = names.contains(&nm.name.trim());
            ui.add_space(6.0);
            if !self.adding.0 {
                if ui.button(crate::i18n::t("➕ New material")).clicked() {
                    self.adding.0 = true;
                }
                return;
            }
            egui::Frame::group(ui.style()).fill(Color32::from_rgb(0x1f, 0x2d, 0x40)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.strong(crate::i18n::t("➕ New material"));
                editors::props(ui, "new_mat", |ui| {
                    editors::caption(ui, "Name");
                    ui.add(egui::TextEdit::singleline(&mut nm.name).id(name_id).desired_width(ui.available_width()));
                    ui.end_row();
                    if taken {
                        editors::caption(ui, "");
                        ui.colored_label(Color32::from_rgb(0xf5, 0x9e, 0x3b), crate::i18n::t("already exists — enter another name"));
                        ui.end_row();
                    }
                    editors::caption(ui, "Thickness, mm");
                    crate::expr::add(ui, &mut nm.thickness, |d| d.range(0.1..=100.0));
                    ui.end_row();
                    editors::caption(ui, "Sheet, mm");
                    ui.horizontal(|ui| {
                        crate::expr::add(ui, &mut nm.sheet.0, |d| d.range(100.0..=6000.0));
                        ui.label("×");
                        crate::expr::add(ui, &mut nm.sheet.1, |d| d.range(100.0..=6000.0));
                    });
                    ui.end_row();
                    editors::caption(ui, "Texture");
                    ui.horizontal(|ui| {
                        let shown = nm.texture.as_deref().map_or_else(|| crate::i18n::t("none"), |t| textures::label(std::path::Path::new(t)));
                        ui.label(shown);
                        if ui.small_button(crate::i18n::t("Choose…")).clicked() {
                            self.textures.open_for = Some(textures::Target::New);
                        }
                    });
                    ui.end_row();
                    editors::caption(ui, "Factor");
                    crate::expr::add(ui, &mut nm.coef, |d| d.range(1.0..=3.0).speed(0.01));
                    ui.end_row();
                });
                let ok = !taken && !nm.name.trim().is_empty();
                ui.horizontal(|ui| {
                    if ui.add_enabled(ok, egui::Button::new(crate::i18n::t("Add material"))).clicked() {
                        cmds.push(("material.add", json!({ "name": nm.name.trim(), "thickness": nm.thickness, "sheet": [nm.sheet.0, nm.sheet.1], "coef": nm.coef, "texture": nm.texture })));
                    }
                    if ui.button(crate::i18n::t("Cancel")).clicked() {
                        self.adding.0 = false;
                    }
                });
            });
        });

        egui::CollapsingHeader::new(crate::i18n::t(format!("Edge banding ({})", project.edges.len()))).show(ui, |ui| {
            for e in &project.edges {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.strong(&e.name);
                        if ui.small_button(crate::i18n::t("Delete")).on_hover_text(crate::i18n::t("Remove the edge banding from the project (if no part uses it)")).clicked() {
                            cmds.push(("edge.remove", json!({ "name": e.name })));
                        }
                    });
                    ui.label(crate::i18n::t(format!("{} mm × {} mm", e.thickness, e.width)));
                });
            }
            let (n, t, w) = &mut self.new_edge;
            let name_id = egui::Id::new("new_edge_name");
            let names: Vec<&str> = project.edges.iter().map(|e| e.name.as_str()).collect();
            if names.contains(&n.trim()) && !ui.memory(|m| m.has_focus(name_id)) {
                *n = free_name(n, &names);
            }
            let taken = names.contains(&n.trim());
            ui.add_space(6.0);
            if !self.adding.1 {
                if ui.button(crate::i18n::t("➕ New edge banding")).clicked() {
                    self.adding.1 = true;
                }
                return;
            }
            egui::Frame::group(ui.style()).fill(Color32::from_rgb(0x1f, 0x2d, 0x40)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.strong(crate::i18n::t("➕ New edge banding"));
                editors::props(ui, "new_edge", |ui| {
                    editors::caption(ui, "Name");
                    ui.add(egui::TextEdit::singleline(n).id(name_id).desired_width(ui.available_width()));
                    ui.end_row();
                    if taken {
                        editors::caption(ui, "");
                        ui.colored_label(Color32::from_rgb(0xf5, 0x9e, 0x3b), crate::i18n::t("already exists — please enter another name"));
                        ui.end_row();
                    }
                    editors::caption(ui, "Thickness");
                    crate::expr::add(ui, t, |d| d.range(0.1..=5.0).speed(0.1).suffix(crate::i18n::t(" mm")));
                    ui.end_row();
                    editors::caption(ui, "Width");
                    crate::expr::add(ui, w, |d| d.range(1.0..=100.0).suffix(crate::i18n::t(" mm")));
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!taken && !n.trim().is_empty(), egui::Button::new(crate::i18n::t("Add edge banding"))).clicked() {
                        cmds.push(("edge.add", json!({ "name": n.trim(), "thickness": t, "width": w })));
                    }
                    if ui.button(crate::i18n::t("Cancel")).clicked() {
                        self.adding.1 = false;
                    }
                });
            });
        });

        for (cmd, p) in cmds {
            let added = self.exec_ret(cmd, p).is_some();
            // After adding via the form, use the next free name so the second add doesn't hit "already exists".
            let Some(s) = self.session.as_ref().filter(|_| added) else { continue };
            if cmd == "material.add" {
                self.adding.0 = false;
                let taken: Vec<&str> = s.project.materials.iter().map(|m| m.name.as_str()).collect();
                self.new_material.name = free_name(&self.new_material.name, &taken);
            } else if cmd == "edge.add" {
                self.adding.1 = false;
                let taken: Vec<&str> = s.project.edges.iter().map(|e| e.name.as_str()).collect();
                self.new_edge.0 = free_name(&self.new_edge.0, &taken);
            }
        }
    }

    /// Select a part (or clear the selection). With Ctrl, add it to or remove it from the selection.
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
            (Some(s), false) => {
                self.selection = vec![s];
                self.furniture_sel = None;
            }
            (None, true) => {}
            (None, false) => {
                self.selection.clear();
                self.furniture_sel = None;
            }
        }
    }

    /// The primary selected part.
    fn selected(&self) -> Option<usize> {
        self.selection.last().and_then(|s| s.find(self.parts.as_ref().ok()?))
    }

    fn selected_all(&self) -> Vec<usize> {
        let Ok(parts) = &self.parts else { return Vec::new() };
        self.selection.iter().filter_map(|s| s.find(parts)).collect()
    }

    /// Objects to act on: a tree panel itself, a template part means the whole template.
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

    /// Bounding box of the selection: (min, max).
    fn selection_bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let parts = self.parts.as_ref().ok()?;
        self.selected_all().iter().map(|&i| &parts[i]).fold(None, |acc, p| {
            let (lo, hi) = acc.unwrap_or(([f64::MAX; 3], [f64::MIN; 3]));
            Some((std::array::from_fn(|k| lo[k].min(p.origin[k])), std::array::from_fn(|k| hi[k].max(p.origin[k] + p.size[k]))))
        })
    }

    /// Fastener model mesh by source file (read once; if it fails to read, empty and drawn as a box).
    fn furniture_mesh(&mut self, source: &str) -> std::sync::Arc<Vec<dowel::panel::Tri>> {
        if let Some(m) = self.meshes.get(source) {
            return m.clone();
        }
        let mesh = match dowel::library::read(std::path::Path::new(source)) {
            Ok(m) => m.mesh,
            Err(e) => {
                eprintln!("[hardware] {e}");
                Vec::new()
            }
        };
        let m = std::sync::Arc::new(mesh);
        self.meshes.insert(source.to_string(), m.clone());
        m
    }

    /// The "Explode" command for an object: block, cabinet, corner module or countertop.
    fn explode_cmd(&self, id: u32) -> Option<&'static str> {
        let pr = &self.session.as_ref()?.project;
        if pr.cabinets.iter().any(|c| c.id == id) {
            Some("cabinet.explode")
        } else if pr.corners.iter().any(|c| c.id == id) {
            Some("corner.explode")
        } else if pr.countertops.iter().any(|c| c.id == id) {
            Some("countertop.explode")
        } else {
            matches!(pr.objects.iter().find(|o| o.id() == id), Some(design::Object::Block(_))).then_some("block.explode")
        }
    }

    /// Objects that Delete removes: selected parts or the selected furniture.
    fn delete_targets(&self) -> Vec<u32> {
        let ids = self.targets();
        if ids.is_empty() { self.furniture_sel.into_iter().collect() } else { ids }
    }

    /// Right-click menu items; returns the chosen one.
    fn menu_items(&self, ui: &mut egui::Ui) -> Option<MenuAct> {
        let editable = self.session.is_some();
        let del = self.delete_targets();
        let ids = self.targets();
        let mut act = None;
        if del.is_empty() {
            ui.label(crate::i18n::t("Nothing selected"));
            return None;
        }
        if ui.add_enabled(editable && !del.is_empty(), egui::Button::new(crate::i18n::t(format!("Delete ({})", del.len()))).shortcut_text("Del")).clicked() {
            act = Some(MenuAct::Delete);
        }
        if ui.add_enabled(editable && !ids.is_empty(), egui::Button::new(crate::i18n::t("Group into block"))).clicked() {
            act = Some(MenuAct::Block);
        }
        let explodable = ids.iter().any(|&id| self.explode_cmd(id).is_some());
        if ui.add_enabled(editable && explodable, egui::Button::new(crate::i18n::t("Explode"))).on_hover_text(crate::i18n::t("Block — contents to the project root; cabinet, corner unit, countertop — into separate panels")).clicked() {
            act = Some(MenuAct::Explode);
        }
        ui.separator();
        if ui.button(crate::i18n::t("Clear selection")).clicked() {
            act = Some(MenuAct::Deselect);
        }
        if act.is_some() {
            ui.close();
        }
        act
    }

    fn menu_apply(&mut self, act: MenuAct) {
        if self.session.is_none() && act != MenuAct::Deselect {
            return;
        }
        match act {
            MenuAct::Delete => {
                let ids = self.delete_targets();
                if ids.is_empty() {
                    return;
                }
                self.exec("object.remove", json!({ "ids": ids }));
                self.selection.clear();
                self.furniture_sel = None;
            }
            MenuAct::Block => {
                let ids = self.targets();
                self.exec("block.create", json!({ "ids": ids }));
            }
            MenuAct::Explode => {
                for id in self.targets() {
                    if let Some(cmd) = self.explode_cmd(id) {
                        self.exec(cmd, json!({ "id": id }));
                    }
                }
                self.selection.clear();
            }
            MenuAct::Deselect => {
                self.selection.clear();
                self.furniture_sel = None;
            }
        }
    }

    /// Right-click on part `hit`: if it isn't selected, select only it (as in Bazis).
    fn menu_pick(&mut self, hit: Option<usize>) {
        if let Some(i) = hit
            && !self.selected_all().contains(&i)
        {
            self.selection.clear();
            self.select(Some(i));
        }
    }

    /// The "Model analysis" window: clicking a problem selects its parts.
    fn problems_window(&mut self, ctx: &egui::Context) {
        let Some(list) = &self.problems else { return };
        let mut open = true;
        let mut pick: Option<Vec<usize>> = None;
        egui::Window::new(crate::i18n::t(format!("Model analysis — problems: {}", list.len()))).open(&mut open).default_width(460.0).show(ctx, |ui| {
            if list.is_empty() {
                ui.label(crate::i18n::t("No problems: no intersections, fasteners and hardware in place, parts fit the sheet."));
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
            // Analysis may have run on an imported copy, so select by part numbers of the current model.
            self.selection = ids.into_iter().filter(|&i| i < parts.len()).map(|i| Sel::of(parts, i)).collect();
        }
        if !open {
            self.problems = None;
        }
    }

    /// Run a script: without windows it finishes immediately; with windows it lives while its window is open.
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

    /// Script window: its forms, messages, "Apply" / "Cancel".
    fn script_window(&mut self, ctx: &egui::Context) {
        let Some(win) = &mut self.script else { return };
        let mut open = true;
        let (mut apply, mut cancel) = (false, false);
        let mut events = Vec::new();
        egui::Window::new(crate::i18n::t(format!("Script: {}", win.name))).open(&mut open).default_width(420.0).show(ctx, |ui| {
            if let Some(rt) = &mut win.rt {
                for f in rt.forms() {
                    ui.heading(f["Caption"].as_str().unwrap_or("Script window"));
                    script_control(ui, &f, &mut events);
                    ui.separator();
                }
                for l in rt.log() {
                    ui.label(crate::i18n::t(l));
                }
                if let Some(e) = &rt.error {
                    ui.colored_label(Color32::from_rgb(200, 40, 40), e);
                }
                ui.horizontal(|ui| {
                    apply = ui.button(crate::i18n::t("Apply")).on_hover_text(crate::i18n::t("Script changes into the model (one undo step)")).clicked();
                    cancel = ui.button(crate::i18n::t("Undo")).clicked();
                });
            } else if let Some(r) = &win.done {
                for l in &r.log {
                    ui.label(crate::i18n::t(l));
                }
                match &r.error {
                    Some(e) => ui.colored_label(Color32::from_rgb(200, 40, 40), format!("{}\n{}", crate::i18n::t(format!("Error: {e}")), crate::i18n::t("The model was not changed."))),
                    None => ui.label(crate::i18n::t(format!("Done. Changes: {}.", r.changes))),
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
            // The script closed all its windows: finish, as with "Apply".
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

    /// "Fasteners": .fr3d models from a folder, previews, search, "Place".
    fn library_panel(&mut self, ui: &mut egui::Ui) {
        if !self.library.scanned {
            self.library.scan();
        }
        let loading = self.library.poll();
        let mut changed = false;
        egui::CollapsingHeader::new(crate::i18n::t(format!("Hardware folders ({})", self.library.dirs.len()))).default_open(self.library.dirs.is_empty()).show(ui, |ui| {
            let mut remove = None;
            for (i, d) in self.library.dirs.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("✕").on_hover_text(crate::i18n::t("Remove the folder from the library (files are not deleted)")).clicked() {
                        remove = Some(i);
                    }
                    ui.add(egui::Label::new(d.as_str()).truncate()).on_hover_text(d.as_str());
                });
            }
            if let Some(i) = remove {
                self.library.dirs.remove(i);
                changed = true;
            }
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::t("+ Add folder…")).clicked()
                    && let Some(d) = rfd::FileDialog::new().pick_folder()
                {
                    let d = d.display().to_string();
                    if !self.library.dirs.contains(&d) {
                        self.library.dirs.push(d);
                        changed = true;
                    }
                }
                if ui.button(crate::i18n::t("Refresh")).on_hover_text(crate::i18n::t("Rescan the folders (after adding files)")).clicked() {
                    self.library.scan();
                }
            });
        });
        if changed {
            self.settings.library_dirs = self.library.dirs.clone();
            self.settings.save();
            self.library.scan();
        }
        ui.horizontal(|ui| {
            ui.label(crate::i18n::t("Search"));
            ui.text_edit_singleline(&mut self.library.search);
        });
        for e in &self.library.errors {
            ui.colored_label(Color32::from_rgb(200, 40, 40), e);
        }
        let editable = self.session.is_some();
        let right = self.free_x();
        if loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(crate::i18n::t("Reading hardware folders…"));
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        let roots: Vec<PathBuf> = self.library.dirs.iter().map(PathBuf::from).collect();
        let place = library::show(ui, &roots, &self.library.models, &mut self.library.thumbs, editable, &self.library.search);
        if let Some(p) = place {
            self.exec("furniture.add", json!({ "path": p, "position": [right, 0, 0] }));
        }
    }

    /// Properties of the selected furniture: position, holes, deletion.
    fn furniture_properties(&mut self, ui: &mut egui::Ui, id: u32) {
        let Some(s) = &self.session else { return };
        let Some((f, fr)) = design::furniture(&s.project).into_iter().find(|(f, _)| f.id == id) else {
            self.furniture_sel = None;
            return;
        };
        let lost = design::furniture_lost_holes(&s.project).into_iter().find(|(n, _)| *n == f.name).map_or(0, |x| x.1);
        let mut cmds: Cmds = Vec::new();
        ui.heading(crate::i18n::t(format!("Hardware “{}”", f.name)));
        editors::props(ui, "furn", |ui| {
            editors::caption(ui, "Type");
            editors::value_box(ui, if f.kind.is_empty() { "—".to_string() } else { f.kind.clone() });
            ui.end_row();
            editors::caption(ui, "File");
            editors::value_box(ui, f.source.clone());
            ui.end_row();
            editors::caption(ui, "Size");
            editors::value_box(ui, format!("{:.0} × {:.0} × {:.0} mm", f.hi[0] - f.lo[0], f.hi[1] - f.lo[1], f.hi[2] - f.lo[2]));
            ui.end_row();
            editors::caption(ui, "Holes");
            editors::value_box(ui, format!("{} (missed panels: {lost})", f.holes.len()));
            ui.end_row();
            editors::caption(ui, "Origin X, Y, Z");
            if let Some(o) = editors::position_editor(ui, fr.origin) {
                let d: [f64; 3] = std::array::from_fn(|k| o[k] - fr.origin[k]);
                cmds.push(("object.move", json!({ "ids": [id], "d": d })));
            }
            ui.end_row();
        });
        if lost > 0 {
            ui.colored_label(Color32::from_rgb(200, 40, 40), crate::i18n::t("Some holes hang in the air: push the model against a panel."));
        }
        let (_, axis, angle, _) = &mut self.actions;
        ui.horizontal(|ui| {
            for a in ['x', 'y', 'z'] {
                ui.selectable_value(axis, a, a.to_ascii_uppercase().to_string());
            }
            crate::expr::add(ui, angle, |d| d.range(-360.0..=360.0).suffix("°"));
            if ui.button(crate::i18n::t("Rotate")).clicked() {
                cmds.push(("object.rotate", json!({ "ids": [id], "axis": axis.to_string(), "angle": *angle, "center": fr.origin })));
            }
        });
        if ui.button(crate::i18n::t("Delete hardware")).clicked() {
            cmds.push(("object.remove", json!({ "ids": [id] })));
            self.furniture_sel = None;
        }
        for (cmd, v) in cmds {
            self.exec(cmd, v);
        }
    }

    /// "Fasteners": between two selected panels, on all joints of the selected ones; this panel's fasteners.
    fn fastener_panel(&mut self, ui: &mut egui::Ui, project: &Project, panel: u32, cmds: &mut Cmds) {
        let panels: Vec<u32> = self.targets().into_iter().filter(|id| project.panels().iter().any(|p| p.id == *id)).collect();
        egui::CollapsingHeader::new(crate::i18n::t("Fasteners")).default_open(panels.len() >= 2).show(ui, |ui| {
            let names = editors::fastener_names(project);
            if let Some(Some(v)) = editors::choose(ui, "fast_kind", Some(&self.fastener_kind), &names, false) {
                self.fastener_kind = v;
            }
            ui.horizontal(|ui| {
                let two = panels.len() == 2;
                if ui.add_enabled(two, egui::Button::new(crate::i18n::t("Between selected"))).on_disabled_hover_text(crate::i18n::t("Select two panels (Ctrl+click): the edge of one on the face of the other")).clicked() {
                    cmds.push(("fastener.add", json!({ "kind": self.fastener_kind, "a": panels[0], "b": panels[1] })));
                }
                if ui.button(crate::i18n::t("On all joints of the selection")).clicked() {
                    cmds.push(("fastener.auto", json!({ "kind": self.fastener_kind, "ids": panels })));
                }
            });
            for f in project.fasteners.iter().filter(|f| f.a == panel || f.b == panel) {
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::t(format!("{} — panels {} and {}", f.kind, f.a, f.b)));
                    if ui.small_button(crate::i18n::t("delete")).clicked() {
                        cmds.push(("fastener.remove", json!({ "id": f.id })));
                    }
                });
            }
            for (_, e) in design::hanging_fasteners(project) {
                ui.colored_label(Color32::from_rgb(200, 40, 40), e);
            }
        });
    }

    /// "Properties" of the selected part: its dimensions and the editor of the object it belongs to.
    fn properties_panel(&mut self, ui: &mut egui::Ui) {
        // Header as in cadcraft: title and what is selected.
        let n = self.selected_all().len();
        let what = match (self.furniture_sel, self.selected(), &self.parts) {
            (Some(_), _, _) => crate::i18n::t("Hardware"),
            (None, Some(i), Ok(parts)) if n == 1 => format!("{} {}", parts[i].panel.pos, parts[i].panel.name).trim().to_string(),
            (None, Some(_), _) => crate::i18n::t(format!("Parts selected: {n}")),
            _ => crate::i18n::t("Nothing selected"),
        };
        editors::value_box(ui, what);
        ui.add_space(6.0);
        if let Some(id) = self.furniture_sel {
            self.furniture_properties(ui, id);
            return;
        }
        let (Ok(parts), Some(i)) = (&self.parts, self.selected()) else {
            ui.weak(crate::i18n::t("Select a part by clicking it in 3D, in the front view or in the model structure. Ctrl+click adds to the selection."));
            return;
        };
        let part = parts[i].clone();
        let p = &part.panel;
        let (l, w) = p.cut_size();
        editors::props(ui, "part_info", |ui| {
            editors::caption(ui, "Part");
            editors::value_box(ui, format!("{} {}", p.pos, p.name).trim().to_string());
            ui.end_row();
            editors::caption(ui, "Material");
            editors::value_box(ui, panel::material_name(&p.material).to_string());
            ui.end_row();
            editors::caption(ui, "Cut size");
            editors::value_box(ui, format!("{} × {} × {} mm", num(l), num(w), num(p.thickness)));
            ui.end_row();
            editors::caption(ui, "Size in project");
            editors::value_box(ui, format!("{} × {} × {} mm", num(part.size[0]), num(part.size[1]), num(part.size[2])));
            ui.end_row();
            editors::caption(ui, "Bounding box origin");
            editors::value_box(ui, format!("X {} Y {} Z {}", num(part.origin[0]), num(part.origin[1]), num(part.origin[2])));
            ui.end_row();
            editors::caption(ui, "Edge banding");
            let edges: Vec<String> = [Side::L1, Side::L2, Side::W1, Side::W2]
                .iter()
                .filter_map(|&sd| {
                    let t = p.edges.iter().filter(|e| p.side(e) == sd).map(|e| e.thickness).fold(0.0, f64::max);
                    (t > 0.0).then(|| format!("{sd:?} {}", num(t)))
                })
                .collect();
            editors::value_box(ui, if edges.is_empty() { i18n::t("none") } else { edges.join(", ") });
            ui.end_row();
            editors::caption(ui, "Holes");
            editors::value_box(ui, p.holes.len().to_string());
            ui.end_row();
        });
        ui.separator();
        let Some(s) = &self.session else {
            ui.weak(crate::i18n::t("The Bazis model is read-only. “Make editable” is in the top bar."));
            return;
        };
        let project = s.project.clone();
        let mut cmds: Cmds = Vec::new();
        if let Some(c) = project.cabinets.iter().find(|c| c.id == part.owner) {
            ui.heading(crate::i18n::t(format!("Cabinet “{}”", c.name)));
            // Formulas in cabinet fields: W/2, H-100, D-T… (English letters; their Russian equivalents are aliases).
            let t = project.material(&c.material).map_or(16.0, |m| m.thickness);
            let vars = vec![(["W", bazis("alias.W")], c.width, "width"), (["H", bazis("alias.H")], c.height, "height"), (["D", bazis("alias.D")], c.depth, "depth"), (["P", bazis("alias.P")], c.plinth, "plinth"), (["T", bazis("alias.T")], t, "material thickness")];
            expr::with_vars(vars, || editors::cabinet_editor(ui, &project, c, &mut cmds));
            if ui.button(crate::i18n::t("Explode into panels")).on_hover_text(crate::i18n::t("Parts become separate panels of a block; cabinet parameters stop applying")).clicked() {
                cmds.push(("cabinet.explode", json!({ "id": c.id })));
            }
        } else if let Some(c) = project.corners.iter().find(|c| c.id == part.owner) {
            ui.heading(crate::i18n::t(format!("Corner unit “{}”", c.name)));
            let t = project.material(&c.material).map_or(16.0, |m| m.thickness);
            let vars = vec![(["W", bazis("alias.W")], c.width_x, "wing X"), (["WZ", bazis("alias.WZ")], c.width_z, "wing Z"), (["H", bazis("alias.H")], c.height, "height"), (["D", bazis("alias.D")], c.depth, "depth"), (["P", bazis("alias.P")], c.plinth, "plinth"), (["T", bazis("alias.T")], t, "material thickness")];
            expr::with_vars(vars, || editors::corner_editor(ui, &project, c, &mut cmds));
        } else if let Some(c) = project.countertops.iter().find(|c| c.id == part.owner) {
            ui.heading(crate::i18n::t(format!("Countertop “{}”", c.name)));
            editors::countertop_editor(ui, &project, c, &mut cmds);
        } else if let Some(fp) = project.panels().into_iter().find(|f| f.id == part.id) {
            self.fastener_panel(ui, &project, fp.id, &mut cmds);
            ui.heading(crate::i18n::t("Panel"));
            let vars = vec![(["L", bazis("alias.L")], p.size.0, "length"), (["W", bazis("alias.W")], p.size.1, "width"), (["T", bazis("alias.T")], p.thickness, "thickness")];
            expr::with_vars(vars, || editors::panel_editor(ui, &project, fp, (p.size.0, p.size.1, p.thickness), &mut cmds));
            ui.separator();
            ui.heading(crate::i18n::t("Edge facing"));
            let names: Vec<String> = project.edges.iter().map(|e| e.name.clone()).collect();
            if names.is_empty() {
                ui.weak(crate::i18n::t("Add edge banding in the “Project” panel."));
            }
            let (edge, clip, allowance) = &mut self.edging;
            if edge.is_none() {
                *edge = names.first().cloned();
            }
            ui.horizontal(|ui| {
                if let Some(Some(v)) = editors::choose(ui, "edging", edge.as_deref(), &names, false) {
                    *edge = Some(v);
                }
                ui.checkbox(clip, crate::i18n::t("Trim")).on_hover_text(crate::i18n::t("Blank reduced by the banding thickness: the part size is finished size"));
                crate::expr::add(ui, allowance, |d| d.range(0.0..=10.0).speed(0.1).prefix("allowance").suffix(crate::i18n::t(" mm")));
            });
            ui.separator();
            ui.heading(crate::i18n::t("Contour"));
            let (tool, size) = &mut self.contour_tool;
            ui.horizontal(|ui| {
                for (t, label) in [(ContourTool::Edge, "Edge banding"), (ContourTool::Round, "Fillet"), (ContourTool::Chamfer, "Chamfer"), (ContourTool::Hole, "Cut-out"), (ContourTool::Groove, "Groove")] {
                    ui.selectable_value(tool, t, crate::i18n::t(label));
                }
                if !matches!(*tool, ContourTool::Edge | ContourTool::Groove) {
                    let label = match tool { ContourTool::Round => "R ", ContourTool::Chamfer => "leg", _ => "radius" };
                    crate::expr::add(ui, size, |d| d.range(1.0..=2000.0).prefix(label).suffix(crate::i18n::t(" mm")));
                }
            });
            if *tool == ContourTool::Groove {
                let (width, depth, offset, front) = &mut self.groove;
                ui.horizontal(|ui| {
                    crate::expr::add(ui, width, |d| d.range(1.0..=50.0).prefix(crate::i18n::t("width ")).suffix(crate::i18n::t(" mm")));
                    crate::expr::add(ui, depth, |d| d.range(1.0..=50.0).prefix(crate::i18n::t("depth ")).suffix(crate::i18n::t(" mm")));
                    crate::expr::add(ui, offset, |d| d.range(0.0..=2000.0).prefix(crate::i18n::t("from the edge ")).suffix(crate::i18n::t(" mm")));
                    ui.checkbox(front, crate::i18n::t("front face"));
                });
            }
            ui.weak(crate::i18n::t(match tool {
                ContourTool::Groove => "Click a straight edge: the groove runs along it, the whole length.",
                ContourTool::Edge => "Click an edge to apply or remove the chosen banding.",
                ContourTool::Round | ContourTool::Chamfer => "Click a vertex to fillet or chamfer the corner.",
                ContourTool::Hole => "Click inside the panel for a round cut-out centred there.",
            }));
            if fp.segs.len() == 4 {
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::t("Size:"));
                    let (mut a, mut b) = (p.size.0, p.size.1);
                    let ra = crate::expr::add(ui, &mut a, |d| d.range(1.0..=6000.0).suffix(crate::i18n::t(" mm")));
                    ui.label("×");
                    let rb = crate::expr::add(ui, &mut b, |d| d.range(1.0..=6000.0).suffix(crate::i18n::t(" mm")));
                    let done = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
                    if (done(&ra) || done(&rb)) && (a, b) != p.size {
                        cmds.push(("panel.resize", json!({ "id": fp.id, "size": [a, b] })));
                    }
                });
            }
            let all: Vec<usize> = (0..fp.segs.len()).collect();
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::t("All edges")).clicked() {
                    cmds.push(("panel.edge", json!({ "id": fp.id, "segments": all, "edge": edge, "clip": clip, "allowance": allowance })));
                }
                if ui.button(crate::i18n::t("Remove from all")).clicked() {
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
                    (ContourTool::Groove, Some(i), _) => {
                        let (width, depth, offset, front) = self.groove;
                        cmds.push(("panel.groove", json!({ "id": fp.id, "segment": i, "width": width, "depth": depth, "offset": offset, "face": if front { "front" } else { "back" } })));
                    }
                    _ => {}
                }
            }
            for (i, g) in fp.grooves.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} {}×{}", g.name, num(g.across[1] - g.across[0]), num(g.z[1] - g.z[0])));
                    if ui.small_button(crate::i18n::t("delete")).clicked() {
                        cmds.push(("panel.groove_remove", json!({ "id": fp.id, "index": i })));
                    }
                });
            }
        }
        for (cmd, v) in cmds {
            self.exec(cmd, v);
        }
    }

    /// The "into opening" mode panel: toggle, plane, material, gaps.
    fn front_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            self.doors_toggle(ui);
        });
        let parts = match &self.parts {
            Ok(p) => p.clone(),
            Err(e) => {
                ui.colored_label(Color32::RED, e);
                return;
            }
        };
        if parts.is_empty() {
            ui.label(crate::i18n::t("Empty. Add a material, then a module or a panel."));
            return;
        }
        let hidden = self.hidden_parts();
        let rects: Vec<_> = design::front_view(&parts, self.show_doors).into_iter().filter(|r| !hidden.contains(&r.part)).collect();
        let Some((lo, hi)) = design::bounds(&rects) else { return };
        let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let area = resp.rect.shrink(30.0);
        let k = ((area.width() as f64) / (hi[0] - lo[0]).max(1.0)).min((area.height() as f64) / (hi[1] - lo[1]).max(1.0));
        let to = |x: f64, y: f64| Pos2::new(area.left() + ((x - lo[0]) * k) as f32, area.bottom() - ((y - lo[1]) * k) as f32);
        // Canvas as in cadcraft: dark background, full-screen grid (every tenth line brighter), axes through the origin.
        painter.rect_filled(resp.rect, 0.0, theme::T.canvas);
        {
            let (minor, major) = (Color32::from_rgb(0x29, 0x30, 0x3a), Color32::from_rgb(0x31, 0x39, 0x45));
            let g = self.settings.grid;
            let step = viewport::adaptive(f64::from(g.step), k, 8.0);
            let every = f64::from(g.major.max(1));
            let mm = |p: Pos2| [(p.x - area.left()) as f64 / k + lo[0], (area.bottom() - p.y) as f64 / k + lo[1]];
            let (a, b) = (mm(resp.rect.left_bottom()), mm(resp.rect.right_top()));
            let is_major = |v: f64| ((v / (step * every)) - (v / (step * every)).round()).abs() < 1e-6;
            let mut x = (a[0] / step).ceil() * step;
            while g.on && x <= b[0] {
                let px = to(x, 0.0).x;
                painter.line_segment([Pos2::new(px, resp.rect.top()), Pos2::new(px, resp.rect.bottom())], Stroke::new(1.0, if is_major(x) { major } else { minor }));
                x += step;
            }
            let mut y = (a[1] / step).ceil() * step;
            while g.on && y <= b[1] {
                let py = to(0.0, y).y;
                painter.line_segment([Pos2::new(resp.rect.left(), py), Pos2::new(resp.rect.right(), py)], Stroke::new(1.0, if is_major(y) { major } else { minor }));
                y += step;
            }
            let o = to(0.0, 0.0);
            if resp.rect.y_range().contains(o.y) {
                painter.line_segment([Pos2::new(resp.rect.left(), o.y), Pos2::new(resp.rect.right(), o.y)], Stroke::new(1.0, Color32::from_rgb(0x56, 0x28, 0x29)));
            }
            if resp.rect.x_range().contains(o.x) {
                painter.line_segment([Pos2::new(o.x, resp.rect.top()), Pos2::new(o.x, resp.rect.bottom())], Stroke::new(1.0, Color32::from_rgb(0x28, 0x4a, 0x2e)));
            }
        }
        let selected = self.selected_all();
        // Dragging the selection: offset in mm with edges snapping to other parts' edges (10 pixel threshold).
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
        // The part nearest to the viewer under the cursor is the last one drawn.
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
        // Overall dimensions of cabinets.
        if let Some(s) = &self.session {
            for c in &s.project.cabinets {
                painter.text(to(c.position[0] + c.width / 2.0, c.position[1]) + Vec2::new(0.0, 14.0), egui::Align2::CENTER_CENTER, format!("{}", c.width), egui::FontId::proportional(13.0), Color32::from_rgb(0xdd, 0xe2, 0xea));
                painter.text(to(c.position[0], c.position[1] + c.height / 2.0) - Vec2::new(16.0, 0.0), egui::Align2::CENTER_CENTER, format!("{}", c.height), egui::FontId::proportional(13.0), Color32::from_rgb(0xdd, 0xe2, 0xea));
            }
        }
        // "Into opening" mode: a ghost of the future panel under the cursor; a click places it.
        let mm_at = |p: Pos2| [((p.x - area.left()) as f64 / k + lo[0]).round(), ((area.bottom() - p.y) as f64 / k + lo[1]).round()];
        if let (Some(t), Some(m)) = (&self.fill, ui.ctx().pointer_hover_pos().filter(|m| resp.rect.contains(*m))) {
            let at = mm_at(m);
            if matches!(t.plane, "door" | "drawers") {
                painter.text(m + Vec2::new(12.0, 12.0), egui::Align2::LEFT_TOP, if t.plane == "door" { "click — a door in this opening" } else { "click — drawers in this opening" }, egui::FontId::proportional(12.0), Color32::from_rgb(160, 70, 0));
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
                    painter.text(r.center_top() - Vec2::new(0.0, 4.0), egui::Align2::CENTER_BOTTOM, format!("{} × {}", num(f.size[0]), num(f.size[1])), egui::FontId::proportional(13.0), Color32::from_rgb(160, 70, 0));
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
        // Resize handles: on a single selected rectangular tree panel, along the axes other than thickness.
        let target = (selected.len() == 1).then(|| selected[0]).filter(|&i| parts[i].id != 0 && parts[i].panel.is_rect() && self.session.is_some());
        if let Some(i) = target {
            let p = &parts[i];
            let f = p.panel.frame.unwrap_or(dowel::geom::Frame::IDENTITY);
            let locked = |k: usize| f.z[k].abs() > 0.5;
            let (lo, hi) = ([p.origin[0], p.origin[1]], [p.origin[0] + p.size[0], p.origin[1] + p.size[1]]);
            let tol = 10.0 / k;
            let others: Vec<&Part> = parts.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, q)| q).collect();
            let snap = |axis: usize, v: f64| -> f64 {
                let best = others.iter().flat_map(|q| [q.origin[axis], q.origin[axis] + q.size[axis]]).map(|e| e - v).filter(|d| d.abs() < tol).min_by(|a, b| a.abs().total_cmp(&b.abs()));
                (v + best.unwrap_or(0.0)).round()
            };
            // New bounding box while dragging a handle.
            let moved = |h: [i8; 2], d: [f64; 2]| -> ([f64; 2], [f64; 2]) {
                let (mut a, mut b) = (lo, hi);
                for ax in 0..2 {
                    match h[ax] {
                        -1 => a[ax] = snap(ax, lo[ax] + d[ax]).min(b[ax] - 1.0),
                        1 => b[ax] = snap(ax, hi[ax] + d[ax]).max(a[ax] + 1.0),
                        _ => {}
                    }
                }
                (a, b)
            };
            // When the part is dragged, the handles move with it.
            let mv = moving.unwrap_or([0.0; 2]);
            let (pa, pb) = match self.handle { Some((h, d)) => moved(h, d), None => ([lo[0] + mv[0], lo[1] + mv[1]], [hi[0] + mv[0], hi[1] + mv[1]]) };
            let r = Rect::from_two_pos(to(pa[0], pa[1]), to(pb[0], pb[1]));
            if self.handle.is_some() {
                painter.rect(r, 0.0, Color32::from_rgba_unmultiplied(255, 170, 70, 60), Stroke::new(1.5, Color32::from_rgb(220, 110, 0)), StrokeKind::Inside);
                painter.text(r.center_top() - Vec2::new(0.0, 4.0), egui::Align2::CENTER_BOTTOM, format!("{} × {}", pb[0] - pa[0], pb[1] - pa[1]), egui::FontId::proportional(13.0), Color32::from_rgb(160, 70, 0));
            }
            let mut grabbed = None;
            for hx in [-1i8, 0, 1] {
                for hy in [-1i8, 0, 1] {
                    if (hx, hy) == (0, 0) || (hx != 0 && locked(0)) || (hy != 0 && locked(1)) {
                        continue;
                    }
                    let x = match hx { -1 => r.left(), 1 => r.right(), _ => r.center().x };
                    let y = match hy { -1 => r.bottom(), 1 => r.top(), _ => r.center().y };
                    let hr = Rect::from_center_size(Pos2::new(x, y), Vec2::splat(9.0));
                    painter.rect(hr, 1.0, Color32::WHITE, Stroke::new(1.2, Color32::from_rgb(220, 110, 0)), StrokeKind::Inside);
                    // Where it was pressed, not where the mouse is now: dragging starts once it has already moved a few points.
                    if resp.drag_started() && ui.input(|i| i.pointer.press_origin()).is_some_and(|m| hr.expand(4.0).contains(m)) {
                        grabbed = Some([hx, hy]);
                    }
                }
            }
            if let Some(h) = grabbed {
                self.handle = Some((h, [0.0; 2]));
            }
            if let Some((h, d)) = &mut self.handle {
                let delta = resp.drag_delta();
                d[0] += delta.x as f64 / k;
                d[1] -= delta.y as f64 / k;
                if resp.drag_stopped() {
                    let (h, d) = (*h, *d);
                    self.handle = None;
                    let (a, b) = moved(h, d);
                    if (a, b) != (lo, hi) {
                        let id = p.id;
                        let lo3 = [a[0], a[1], p.origin[2]];
                        let hi3 = [b[0], b[1], p.origin[2] + p.size[2]];
                        self.exec("panel.box", json!({ "id": id, "lo": lo3, "hi": hi3 }));
                    }
                }
                return;
            }
        }
        if resp.clicked() {
            self.select(hovered);
        }
        if resp.secondary_clicked() {
            self.menu_pick(hovered);
        }
        let mut act = None;
        resp.context_menu(|ui| act = self.menu_items(ui));
        if let Some(a) = act {
            self.menu_apply(a);
            return;
        }
        if resp.drag_started_by(egui::PointerButton::Primary) && hovered.is_some_and(|h| selected.contains(&h)) && self.session.is_some() {
            self.drag = Some([0.0; 2]);
        }
        if let Some(d) = &mut self.drag {
            let delta = resp.drag_delta();
            d[0] += delta.x as f64 / k;
            d[1] -= delta.y as f64 / k;
            if let Some(m) = moving {
                painter.text(resp.rect.left_top() + Vec2::new(8.0, 8.0), egui::Align2::LEFT_TOP, format!("offset: X {} Y {}", m[0], m[1]), egui::FontId::proportional(14.0), Color32::from_rgb(160, 70, 0));
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
            resp.on_hover_text(format!("{} {}: {}×{}×{}{}\n{}", p.panel.pos, p.panel.name, num(l), num(w), num(p.panel.thickness), crate::i18n::t(" mm"), panel::material_name(&p.panel.material)));
        }
    }

    /// The "Show doors and fronts" icon.
    fn doors_toggle(&mut self, ui: &mut egui::Ui) {
        if icons::button(ui, icons::Icon::Doors, 26.0, "Doors and fronts: show or hide (hide to look inside)", self.show_doors).clicked() {
            self.show_doors = !self.show_doors;
            self.revision += 1;
        }
    }

    fn iso_view(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(8.0);
            self.viewport.toolbar(ui);
            ui.separator();
            if icons::button(ui, icons::Icon::Quad, 26.0, "4 views: front, left, top and axonometric", self.quad).clicked() {
                self.quad = !self.quad;
            }
            self.doors_toggle(ui);
        });
        // Shared mode, furniture boxes and "Show all" for all views.
        let list = self.session.as_ref().map(|s| design::furniture(&s.project)).unwrap_or_default();
        let boxes: Vec<viewport::FurnView> = list
            .into_iter()
            .map(|(f, frame)| {
                let mesh = self.furniture_mesh(&f.source);
                viewport::FurnView { id: f.id, frame, lo: f.lo, hi: f.hi, selected: self.furniture_sel == Some(f.id), mesh }
            })
            .collect();
        let refit = self.viewport.needs_fit();
        // Material textures: full name "Name\rArticle" → image and tile.
        let textures: Vec<(String, String, [f64; 2])> = self.session.as_ref().map(|s| {
            s.project.materials.iter().filter_map(|m| {
                let full = if m.article.is_empty() { m.name.clone() } else { format!("{}\r{}", m.name, m.article) };
                Some((full, m.texture.clone()?, m.texture_size.unwrap_or([1000.0, 1000.0])))
            }).collect()
        }).unwrap_or_default();
        self.viewport.textures = textures.clone();
        for v in &mut self.views {
            v.mode = self.viewport.mode;
            v.furniture = boxes.clone();
            v.textures = textures.clone();
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
        // Parts hidden with the eye icon, by address in the list (the closure receives the part itself).
        let hidden: std::collections::HashSet<*const Part> = self.hidden_parts().into_iter().map(|i| &parts[i] as *const Part).collect();
        let skip = |p: &Part| hidden.contains(&(p as *const Part)) || (!doors && matches!(p.kind, design::Kind::Door | design::Kind::Drawer));
        let mut clicks = Vec::new();
        if self.quad {
            // As in Bazis: front | left, top | axonometric.
            let r = ui.available_rect_before_wrap();
            let (w, h) = (r.width() / 2.0, r.height() / 2.0);
            let cell = |i: usize, j: usize| egui::Rect::from_min_size(r.min + Vec2::new(i as f32 * w, j as f32 * h), Vec2::new(w, h)).shrink(1.0);
            let cells: [Option<egui::Rect>; 4] = match self.quad_max {
                Some(k) => std::array::from_fn(|i| (i == k).then(|| r.shrink(1.0))),
                None => [cell(0, 0), cell(1, 0), cell(0, 1), cell(1, 1)].map(Some),
            };
            for (v, c) in self.views.iter_mut().zip(cells) {
                if let Some(c) = c {
                    clicks.push(ui.scope_builder(egui::UiBuilder::new().max_rect(c), |ui| v.show(ui, frame, parts, self.revision, &selected, skip)).inner);
                }
            }
            if let Some(c) = cells[3] {
                clicks.push(ui.scope_builder(egui::UiBuilder::new().max_rect(c), |ui| self.viewport.show(ui, frame, parts, self.revision, &selected, skip)).inner);
            }
            // Button in the view corner: maximize to the whole area / restore the four views.
            for (i, c) in cells.iter().enumerate() {
                let Some(c) = c else { continue };
                let b = egui::Rect::from_min_size(c.right_top() + Vec2::new(-28.0, 4.0), Vec2::splat(24.0));
                let (text, tip) = if self.quad_max.is_some() { ("🗗", "Back to four windows") } else { ("🗖", "Maximize this window") };
                if ui.put(b, egui::Button::new(text)).on_hover_text(crate::i18n::t(tip)).clicked() {
                    self.quad_max = if self.quad_max.is_some() { None } else { Some(i) };
                }
            }
            ui.allocate_rect(r, egui::Sense::hover());
        } else {
            clicks.push(self.viewport.show(ui, frame, parts, self.revision, &selected, skip));
        }
        let mut act = None;
        for (c, resp) in clicks {
            match c {
                viewport::Click::Part(i) => self.select(Some(i)),
                viewport::Click::Empty => self.select(None),
                viewport::Click::Menu(hit) => self.menu_pick(hit),
                viewport::Click::Resize(lo, hi) => {
                    if let Some(id) = self.parts.as_ref().ok().and_then(|p| self.selected_all().first().map(|&i| p[i].id)) {
                        self.exec("panel.box", json!({ "id": id, "lo": lo, "hi": hi }));
                    }
                }
                viewport::Click::Move(d) => {
                    let ids = self.targets();
                    if !ids.is_empty() {
                        self.exec("object.move", json!({ "ids": ids, "d": d }));
                    }
                }
                viewport::Click::Furniture(id, right) => {
                    // Right-click on an unselected one selects it; left-click selects (with Ctrl, also just one).
                    if !right || self.furniture_sel != Some(id) {
                        self.selection.clear();
                        self.furniture_sel = Some(id);
                    }
                }
                viewport::Click::None => {}
            }
            if let Some(r) = resp {
                r.context_menu(|ui| act = self.menu_items(ui).or(act));
            }
        }
        if let Some(a) = act {
            self.menu_apply(a);
        }
    }

    fn parts_table(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("parts").striped(true).num_columns(10).show(ui, |ui| {
                for h in ["№", "Item name", "Material", "Length", "Width", "Thickness", "L1", "L2", "W1", "W2"] {
                    ui.strong(h);
                }
                ui.end_row();
                for (i, p) in self.job.panels.iter().enumerate() {
                    let (l, w) = p.cut_size();
                    ui.label(self.job.number(i));
                    ui.label(&p.name);
                    ui.label(panel::material_name(&p.material));
                    ui.label(num(l));
                    ui.label(num(w));
                    ui.label(num(p.thickness));
                    for sd in [Side::L1, Side::L2, Side::W1, Side::W2] {
                        let t = p.edges.iter().filter(|e| p.side(e) == sd).map(|e| e.thickness).fold(0.0, f64::max);
                        ui.label(if t > 0.0 { num(t) } else { String::new() });
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn cut_view(&mut self, ui: &mut egui::Ui) {
        let before = self.params;
        // In our own project, sheet, kerf and trim belong to each material; the general numbers are only needed
        // for a Bazis model whose materials have no sheet.
        if self.session.is_some() {
            ui.weak(crate::i18n::t("Sheet, kerf and trim are set per material: “Project” panel → “Materials”."));
        } else {
        ui.horizontal(|ui| {
            ui.label(crate::i18n::t("Sheet for the Bazis model materials"));
            crate::expr::add(ui, &mut self.params.sheet.0, |d| d.range(500.0..=6000.0));
            ui.label("×");
            crate::expr::add(ui, &mut self.params.sheet.1, |d| d.range(500.0..=6000.0));
            ui.label(crate::i18n::t("Kerf"));
            crate::expr::add(ui, &mut self.params.kerf, |d| d.range(0.0..=10.0).speed(0.1));
            ui.label(crate::i18n::t("Sheet trim"));
            crate::expr::add(ui, &mut self.params.trim, |d| d.range(0.0..=50.0));
        });
        }
        if self.params != before {
            self.refresh();
        }
        let sheets: Vec<(usize, usize)> = self.cuts.iter().enumerate().flat_map(|(m, c)| (0..c.layout.sheets.len()).map(move |k| (m, k))).collect();
        if sheets.is_empty() {
            ui.label(crate::i18n::t("No parts to cut."));
            return;
        }
        self.card = self.card.min(sheets.len() - 1);
        ui.horizontal_wrapped(|ui| {
            for (i, &(m, k)) in sheets.iter().enumerate() {
                let c = &self.cuts[m];
                let label = format!("{} — {}", panel::material_name(&c.material), k + 1);
                ui.selectable_value(&mut self.card, i, crate::i18n::t(label));
            }
        });
        for c in &self.cuts {
            for &id in &c.layout.too_big {
                ui.colored_label(Color32::RED, crate::i18n::t(format!("Does not fit the sheet: No. {} “{}”", self.job.number(id), self.job.panels[id].name)));
            }
        }
        let (m, k) = sheets[self.card];
        let cut = &self.cuts[m];
        let sheet = &cut.layout.sheets[k];
        ui.label(crate::i18n::t(format!("Parts on sheet: {}, usage {:.1}%", sheet.parts.len(), sheet.usage(&cut.params) * 100.0)));
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
                painter.text(r.center(), egui::Align2::CENTER_CENTER, crate::i18n::t(format!("Offcut {:.0}×{:.0}", f.w, f.h)), egui::FontId::proportional(11.0), Color32::from_rgb(40, 40, 140));
            }
        }
        for d in &sheet.parts {
            let r = Rect::from_min_size(to(d.x, d.y), Vec2::new(d.w as f32 * kx, d.h as f32 * kx));
            painter.rect(r, 0.0, Color32::WHITE, Stroke::new(1.0, Color32::from_rgb(220, 0, 0)), StrokeKind::Inside);
            let l = job::label(&self.job, d.id);
            let text = format!("{} {}\n{:.0}×{:.0}", l.num, l.name, l.size.0, l.size.1);
            let size = (r.height().min(r.width()) / 4.0).clamp(8.0, 14.0);
            // A long name must not spill onto neighbouring parts.
            painter.with_clip_rect(r).text(r.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(size), Color32::BLACK);
        }
    }

    /// `--screenshot file.png`: after a few frames save the window and exit.
    fn take_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frames += 1;
        if self.frames == self.shot_frames {
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
                eprintln!("screenshot: {e}");
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

#[cfg(test)]
mod tests {
    use super::free_name;

    #[test]
    fn free_name_skips_taken() {
        assert_eq!(free_name("MDF 16", &["Chipboard 16"]), "MDF 16");
        assert_eq!(free_name("Chipboard 16", &["Chipboard 16"]), "Chipboard 16 (2)");
        assert_eq!(free_name("Chipboard 16 (2)", &["Chipboard 16", "Chipboard 16 (2)"]), "Chipboard 16 (3)");
        assert_eq!(free_name(" Chipboard 16 ", &["Chipboard 16", "Chipboard 16 (2)"]), "Chipboard 16 (3)");
    }
}
