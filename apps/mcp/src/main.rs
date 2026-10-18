//! MCP-сервер dowel-craft: JSON-RPC 2.0 через stdio, одно сообщение на строку.
//! ИИ-агент проектирует мебель теми же командами, что окно и CLI (`Session::execute`).

use std::io::{BufRead, Write};
use std::path::PathBuf;

use dowel::design::{self, Kind, Project, Session};
use dowel::job::{self, Job};
use dowel::nest::Params;
use dowel::panel;
use serde_json::{Value, json};

const PROTOCOL: &str = "2025-06-18";

fn main() {
    let mut server = Server::default();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => server.handle(&msg),
            Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}})),
        };
        if let Some(r) = reply {
            // Ошибка записи — клиент ушёл, завершаемся.
            if writeln!(out, "{r}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}

struct Server {
    session: Option<Session>,
    /// Открытая модель Базиса (только чтение) или собранный из проекта набор деталей.
    job: Job,
    path: Option<PathBuf>,
    /// Подключено к открытому окну: команды идут в его вкладку, чтение — с её копии.
    #[cfg(unix)]
    live: Option<dowel::live::Client>,
}

impl Default for Server {
    fn default() -> Self {
        let p = Project::new("Новый проект");
        Server {
            job: Job::from_project(p.clone()),
            session: Some(Session::new(p)),
            path: None,
            #[cfg(unix)]
            live: None,
        }
    }
}

/// Ответ инструмента: содержимое MCP и признак ошибки.
type ToolResult = Result<Vec<Value>, String>;

fn text(s: impl Into<String>) -> Vec<Value> {
    vec![json!({"type": "text", "text": s.into()})]
}

fn tools() -> Value {
    let commands: String = design::COMMANDS.iter().map(|(n, h)| format!("\n- {n}: {h}")).collect();
    json!([
        {"name": "window_connect", "description": "Подключиться к открытому окну dowel-craft: дальше command, run_script и project_save меняют проект в активной вкладке окна (пользователь видит сразу, Ctrl+Z в окне отменяет), а parts, cut, render_*, export читают его. project_new/open/import недоступны до window_disconnect.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "window_disconnect", "description": "Отключиться от окна и снова работать со своей копией проекта.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "project_new", "description": "Создать пустой проект мебели.",
         "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}},
        {"name": "project_open", "description": "Открыть свой проект .dcraft (можно править) или модель Базиса .b3d/.fr3d (только чтение).",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
        {"name": "project_import", "description": "Открыть модель Базиса .b3d как свой проект: плиты станут свободными деталями, которые можно править (panel.set, panel.remove) и дополнять корпусами.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
        {"name": "project_save", "description": "Сохранить проект в .dcraft. Без path — в файл, из которого открыт.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}}},
        {"name": "command", "description": format!(
            "Выполнить команду над проектом. Размеры в мм. Оси корпуса: X вправо, Y вверх, Z к фасаду. \
             Изменение, после которого изделие не собирается, отклоняется с объяснением. Команды:{commands}"),
         "inputSchema": {"type": "object", "properties": {
             "command": {"type": "string"}, "params": {"type": "object"}}, "required": ["command"]}},
        {"name": "parts", "description": "Список деталей: №, наименование, материал, пильные длина×ширина×толщина, кромка.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "smeta", "description": "Расход материалов: плиты (м²), кромка (м) и фурнитура из моделей Базиса (шт), как смета Базиса.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "cut", "description": "Раскроить детали по листам. Итог по материалам: листов, % использования, не поместившиеся детали.",
         "inputSchema": {"type": "object", "properties": {
             "sheet": {"type": "array", "items": {"type": "number"}, "description": "лист по умолчанию [длина, ширина]"},
             "kerf": {"type": "number"}, "trim": {"type": "number"}}}},
        {"name": "export", "description": "Сохранить документ: cards_html (карты раскроя и «Панели в производство»), raskroy_xml (проект БАЗИС-Раскроя), spec_csv, smeta_csv, b3d (модель Базис-Мебельщика из своего проекта), drawings_html (сборочный и рабочие чертежи деталей с отверстиями).",
         "inputSchema": {"type": "object", "properties": {
             "kind": {"type": "string", "enum": ["cards_html", "raskroy_xml", "spec_csv", "smeta_csv", "b3d", "drawings_html"]},
             "path": {"type": "string"}}, "required": ["kind", "path"]}},
        {"name": "render_iso", "description": "3D-картинка (PNG, аксонометрия спереди-справа-сверху) проекта или модели Базиса.",
         "inputSchema": {"type": "object", "properties": {
             "width": {"type": "integer", "description": "ширина картинки, пикселей (по умолчанию 900)"},
             "doors": {"type": "boolean", "description": "рисовать двери и фасады (по умолчанию да)"}}}},
        {"name": "run_script", "description": "Выполнить скрипт Базиса (BAZIS-Script, JavaScript) над открытым проектом: Model, AddFrontPanel/AddHorizPanel/AddVertPanel, objects3d, panelOperations, fs (только в папке скрипта). Изменения проверяются и применяются одной записью отмены; при ошибке модель не меняется.",
         "inputSchema": {"type": "object", "properties": {
             "source": {"type": "string", "description": "текст скрипта"},
             "path": {"type": "string", "description": "или путь к файлу .js; его папка — песочница для файлов скрипта"}}}},
        {"name": "render_front", "description": "Картинка вида спереди (PNG) проекта или модели Базиса: корпус — бежевый, двери — голубые полупрозрачные, задняя стенка — серая.",
         "inputSchema": {"type": "object", "properties": {
             "width": {"type": "integer", "description": "ширина картинки, пикселей (по умолчанию 800)"},
             "doors": {"type": "boolean", "description": "рисовать двери (по умолчанию да)"}}}},
    ])
}

impl Server {
    fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        // Уведомления (без id) не требуют ответа.
        let id = id?;
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "dowel-craft", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Проектирование корпусной мебели и раскрой. Сначала добавьте материал (material.add) и кромку (edge.add), \
                                 затем корпус (cabinet.add), полки и перегородки. Смотрите результат через render_front, parts, cut."
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools()})),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                Ok(match self.call(name, &args) {
                    Ok(content) => json!({"content": content}),
                    Err(e) => json!({"content": text(e), "isError": true}),
                })
            }
            _ => Err(json!({"code": -32601, "message": format!("неизвестный метод {method}")})),
        };
        Some(match result {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
        })
    }

    fn refresh(&mut self) {
        if let Some(s) = &self.session {
            self.job = Job::from_project(s.project.clone());
        }
    }

    /// Работа с окном: команды и сохранение — туда, чтение — с его копии проекта.
    #[cfg(unix)]
    fn call_live(&mut self, name: &str, a: &Value) -> Option<ToolResult> {
        let live = self.live.as_mut()?;
        let r = match name {
            "project_new" | "project_open" | "project_import" => Err("подключено к окну: сначала window_disconnect (или откройте файл в окне)".into()),
            "command" => match a.get("command").and_then(Value::as_str) {
                Some(cmd) => live.call(cmd, a.get("params").unwrap_or(&json!({}))).map(|out| text(if out.is_null() { "ok".into() } else { serde_json::to_string_pretty(&out).unwrap_or_default() })),
                None => Err("нужен параметр «command»".into()),
            },
            "project_save" => live.call("save", &json!({ "path": a.get("path") })).map(|p| text(format!("Сохранено: {}", p.as_str().unwrap_or_default()))),
            "window_connect" | "window_disconnect" => return None,
            _ => {
                // Чтение и скрипт — над свежей копией проекта окна.
                match live.call("project.get", &json!({})).and_then(|v| Project::load(&v.to_string()).map(|p| (v, p))) {
                    Err(e) => Err(e),
                    Ok((base, p)) => {
                        self.session = Some(Session::new(p));
                        self.refresh();
                        let r = self.call_local(name, a);
                        // Итог скрипта — в окно, если там тот же проект, с которого начинали (`base`).
                        match (name, &r, &self.session, &mut self.live) {
                            ("run_script", Ok(_), Some(s), Some(live)) => live.call("project.replace", &json!({ "project": s.project.save(), "base": base })).and(r),
                            _ => r,
                        }
                    }
                }
            }
        };
        // Окно закрыли — дальше работаем со своей копией.
        if let Err(e) = &r
            && e.starts_with("связь с окном")
        {
            self.live = None;
            return Some(Err(format!("{e}. Отключено от окна, дальше — своя копия проекта.")));
        }
        Some(r)
    }

    fn call(&mut self, name: &str, a: &Value) -> ToolResult {
        #[cfg(unix)]
        if let Some(r) = self.call_live(name, a) {
            return r;
        }
        self.call_local(name, a)
    }

    fn call_local(&mut self, name: &str, a: &Value) -> ToolResult {
        let str_arg = |k: &str| a.get(k).and_then(Value::as_str).ok_or(format!("нужен параметр «{k}»"));
        match name {
            #[cfg(unix)]
            "window_connect" => {
                let mut c = dowel::live::Client::connect(&dowel::live::socket_path())?;
                let info = c.call("window.info", &json!({}))?;
                self.live = Some(c);
                Ok(text(format!(
                    "Подключено к окну: вкладка «{}»{}{}. Команды идут в неё.",
                    info["name"].as_str().unwrap_or_default(),
                    info["path"].as_str().map(|p| format!(", файл {p}")).unwrap_or_default(),
                    if info["editable"].as_bool() == Some(false) { " (модель Базиса только для просмотра — менять нельзя)" } else { "" }
                )))
            }
            #[cfg(unix)]
            "window_disconnect" => {
                self.live = None;
                Ok(text("Отключено от окна."))
            }
            "project_new" => {
                let p = Project::new(str_arg("name")?);
                self.session = Some(Session::new(p));
                self.path = None;
                self.refresh();
                Ok(text("Создан пустой проект. Добавьте материал, кромку и корпус командами."))
            }
            "project_open" => {
                let path = PathBuf::from(str_arg("path")?);
                let job = Job::open(&path)?;
                self.session = job.project.clone().map(Session::new);
                let msg = format!(
                    "Открыт {}: деталей {}{}",
                    path.display(),
                    job.panels.len(),
                    if self.session.is_some() { "" } else { " (модель Базиса, только чтение)" }
                );
                self.job = job;
                self.path = Some(path);
                Ok(text(msg))
            }
            "project_import" => {
                let path = PathBuf::from(str_arg("path")?);
                let job = Job::open(&path)?;
                let p = job.import();
                let msg = format!("Импортировано деталей: {}, материалов: {}. Сохраните project_save с путём .dcraft.", p.panels().len(), p.materials.len());
                self.session = Some(Session::new(p));
                self.path = None;
                self.refresh();
                Ok(text(msg))
            }
            "project_save" => {
                let s = self.session.as_ref().ok_or("открыта модель Базиса — сохранять нечего")?;
                let path = match a.get("path").and_then(Value::as_str) {
                    Some(p) => PathBuf::from(p),
                    None => self.path.clone().filter(|p| dowel::design::is_project_file(p)).ok_or("укажите path")?,
                };
                std::fs::write(&path, s.project.save()).map_err(|e| format!("{}: {e}", path.display()))?;
                self.path = Some(path.clone());
                Ok(text(format!("Сохранено: {}", path.display())))
            }
            "command" => {
                let s = self.session.as_mut().ok_or("открыта модель Базиса — она только для чтения; создайте проект project_new")?;
                let out = s.execute(str_arg("command")?, a.get("params").unwrap_or(&json!({})))?;
                self.refresh();
                Ok(text(if out.is_null() { "ok".into() } else { serde_json::to_string_pretty(&out).unwrap_or_default() }))
            }
            "parts" => {
                let mut t = String::from("№ | наименование | материал | длина × ширина × толщина | кромка L1/L2/W1/W2\n");
                for (i, p) in self.job.panels.iter().enumerate() {
                    let (l, w) = p.cut_size();
                    let e = job::label(&self.job, i).edges.map(|b| if b { "+" } else { "-" }).join("");
                    t += &format!("{} | {} | {} | {:.1} × {:.1} × {} | {}\n", self.job.number(i), p.name, panel::material_name(&p.material), l, w, p.thickness, e);
                }
                Ok(text(t))
            }
            "smeta" => {
                let rows = self.job.estimate(1.2);
                Ok(text(rows.iter().map(|r| format!("{} — {} {}\n", panel::material_name(&r.material), r.qty, r.unit)).collect::<String>()))
            }
            "cut" => {
                let p = self.params(a);
                let cuts = job::cut(&self.job, |m| self.job.params(m, p));
                let mut t = String::new();
                for c in &cuts {
                    let usage: Vec<String> = c.layout.sheets.iter().map(|s| format!("{:.0}%", s.usage(&c.params) * 100.0)).collect();
                    t += &format!("{} (лист {}×{}): листов {}, заполнение {}\n", panel::material_name(&c.material), c.params.sheet.0, c.params.sheet.1, c.layout.sheets.len(), usage.join(", "));
                    for &id in &c.layout.too_big {
                        t += &format!("  не помещается: № {} «{}»\n", self.job.number(id), self.job.panels[id].name);
                    }
                }
                Ok(text(if t.is_empty() { "Нет деталей.".into() } else { t }))
            }
            "export" => {
                let path = PathBuf::from(str_arg("path")?);
                let job = &self.job;
                let p = self.params(a);
                let bytes = match str_arg("kind")? {
                    "cards_html" => {
                        let cuts = job::cut(job, |m| job.params(m, p));
                        job::document(job, &cuts).into_bytes()
                    }
                    "raskroy_xml" => dowel::raskroy::project_xml(&job.name, &job.panels, |m| job.sheet(m, p.sheet)),
                    "spec_csv" => dowel::report::spec_csv(&panel::spec(&job.panels)).into_bytes(),
                    "smeta_csv" => dowel::report::materials_csv(&job.estimate(1.2)).into_bytes(),
                    "drawings_html" => dowel::report::document_html(&job.name, &dowel::drawing::drawings(&job::parts(job)?, &job.name), "").into_bytes(),
                    "b3d" => dowel::export::b3d(job.project.as_ref().ok_or("b3d пишется из своего проекта: откройте или импортируйте его")?)?,
                    k => return Err(format!("неизвестный вид документа «{k}»")),
                };
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(text(format!("Сохранено: {}", path.display())))
            }
            "run_script" => {
                let s = self.session.as_mut().ok_or("открыта модель Базиса — сначала project_import")?;
                let (source, dir) = match (a.get("source").and_then(Value::as_str), a.get("path").and_then(Value::as_str)) {
                    (Some(src), _) => (src.to_string(), std::env::temp_dir().join("dowel-craft-mcp-script")),
                    (None, Some(p)) => {
                        let p = PathBuf::from(p);
                        (script::read_script(&p)?, p.parent().map(PathBuf::from).unwrap_or_default())
                    }
                    _ => return Err("нужен source или path".into()),
                };
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let r = script::run(s, &source, script::Options { sandbox: dir.clone(), script_dir: dir, ..Default::default() });
                self.refresh();
                let mut t = r.log.join("\n");
                t += &format!("\nизменений: {}, окон: {}", r.changes, r.forms.len());
                match r.error {
                    Some(e) => Err(format!("{t}\nОшибка: {e} (модель не изменена)")),
                    None => Ok(text(t)),
                }
            }
            "render_front" => {
                let parts = job::parts(&self.job)?;
                let width = a.get("width").and_then(Value::as_u64).unwrap_or(800).clamp(100, 4000) as u32;
                let doors = a.get("doors").and_then(Value::as_bool).unwrap_or(true);
                let png = render_front(&parts, width, doors)?;
                Ok(vec![json!({"type": "image", "data": base64(&png), "mimeType": "image/png"})])
            }
            "render_iso" => {
                let parts = job::parts(&self.job)?;
                let width = a.get("width").and_then(Value::as_u64).unwrap_or(900).clamp(100, 4000) as u32;
                let doors = a.get("doors").and_then(Value::as_bool).unwrap_or(true);
                let png = render_iso(&parts, width, doors)?;
                Ok(vec![json!({"type": "image", "data": base64(&png), "mimeType": "image/png"})])
            }
            _ => Err(format!("неизвестный инструмент «{name}»")),
        }
    }

    fn params(&self, a: &Value) -> Params {
        let sheet = a.get("sheet").and_then(Value::as_array).and_then(|v| Some((v.first()?.as_f64()?, v.get(1)?.as_f64()?)));
        Params {
            sheet: sheet.unwrap_or((2800.0, 2070.0)),
            kerf: a.get("kerf").and_then(Value::as_f64).unwrap_or(4.0),
            trim: a.get("trim").and_then(Value::as_f64).unwrap_or(10.0),
        }
    }
}

fn render_front(parts: &[design::Part], width: u32, doors: bool) -> Result<Vec<u8>, String> {
    use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
    let rects = design::front_view(parts, doors);
    let (lo, hi) = design::bounds(&rects).ok_or("в проекте нет деталей")?;
    let pad = 20.0;
    let k = (width as f64 - 2.0 * pad) / (hi[0] - lo[0]);
    let height = ((hi[1] - lo[1]) * k + 2.0 * pad).ceil().clamp(50.0, 8000.0) as u32;
    let mut pm = Pixmap::new(width, height).ok_or("нельзя создать картинку")?;
    pm.fill(Color::from_rgba8(250, 250, 250, 255));
    let stroke = Stroke { width: 1.0, ..Default::default() };
    let mut edge = Paint::default();
    edge.set_color_rgba8(60, 60, 60, 255);
    for r in &rects {
        let x = (pad + (r.x - lo[0]) * k) as f32;
        let y = (height as f64 - pad - (r.y + r.h - lo[1]) * k) as f32;
        let (w, h) = ((r.w * k) as f32, (r.h * k) as f32);
        let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w.max(1.0), h.max(1.0)) else { continue };
        let path = PathBuilder::from_rect(rect);
        let mut fill = Paint::default();
        match r.kind {
            Kind::Door | Kind::Drawer => fill.set_color_rgba8(170, 200, 230, 150),
            Kind::Back => fill.set_color_rgba8(225, 225, 225, 255),
            Kind::Body => fill.set_color_rgba8(236, 220, 190, 255),
        }
        pm.fill_path(&path, &fill, FillRule::Winding, Transform::identity(), None);
        pm.stroke_path(&path, &edge, &stroke, Transform::identity(), None);
    }
    pm.encode_png().map_err(|e| e.to_string())
}

fn render_iso(parts: &[design::Part], width: u32, doors: bool) -> Result<Vec<u8>, String> {
    use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
    let faces = design::iso_view(parts, doors);
    if faces.is_empty() {
        return Err("нет деталей".into());
    }
    let (lo, hi) = faces.iter().flat_map(|f| f.pts).fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
    });
    let pad = 20.0;
    let k = (width as f64 - 2.0 * pad) / (hi[0] - lo[0]);
    let height = ((hi[1] - lo[1]) * k + 2.0 * pad).ceil().clamp(50.0, 8000.0) as u32;
    let mut pm = Pixmap::new(width, height).ok_or("нельзя создать картинку")?;
    pm.fill(Color::from_rgba8(250, 250, 250, 255));
    let mut edge = Paint::default();
    edge.set_color_rgba8(70, 70, 70, 255);
    let stroke = Stroke { width: 0.8, ..Default::default() };
    for f in &faces {
        let mut pb = PathBuilder::new();
        for (i, p) in f.pts.iter().enumerate() {
            let (x, y) = ((pad + (p[0] - lo[0]) * k) as f32, (height as f64 - pad - (p[1] - lo[1]) * k) as f32);
            if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
        }
        pb.close();
        let Some(path) = pb.finish() else { continue };
        let (r, g, b, a) = match f.kind {
            Kind::Door | Kind::Drawer => (170.0, 200.0, 230.0, 230),
            Kind::Back => (215.0, 215.0, 215.0, 255),
            Kind::Body => (236.0, 220.0, 190.0, 255),
        };
        let c = |v: f64| (v * f.light).min(255.0) as u8;
        let mut fill = Paint::default();
        fill.set_color_rgba8(c(r), c(g), c(b), a);
        pm.fill_path(&path, &fill, FillRule::Winding, Transform::identity(), None);
        pm.stroke_path(&path, &edge, &stroke, Transform::identity(), None);
    }
    pm.encode_png().map_err(|e| e.to_string())
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Подключение к окну: команды уходят в окно, чтение — с его проекта, project_new запрещён.
    /// Окно изображает поток, который отвечает как `apps/gui/src/live.rs`.
    #[cfg(unix)]
    #[test]
    fn window_mode_forwards_commands() {
        use std::io::{BufRead, BufReader};
        let dir = std::env::temp_dir().join(format!("dowel-craft-mcp-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("w.sock");
        let listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        let window = std::thread::spawn(move || {
            let mut s = Session::new(Project::new("Окно"));
            let (conn, _) = listener.accept().unwrap();
            let mut out = conn.try_clone().unwrap();
            let mut seen = Vec::new();
            for line in BufReader::new(conn).lines() {
                let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
                let cmd = v["command"].as_str().unwrap().to_string();
                let r = match cmd.as_str() {
                    "window.info" => Ok(json!({"name": "Окно", "path": null, "editable": true})),
                    _ => s.execute(&cmd, &v["params"]),
                };
                seen.push(cmd);
                let msg = match r { Ok(v) => json!({"ok": v}), Err(e) => json!({"error": e}) };
                writeln!(out, "{msg}").unwrap();
            }
            (seen, s.project.materials.len())
        });
        let mut server = Server { live: Some(dowel::live::Client::connect(&sock).unwrap()), ..Default::default() };
        server.call("command", &json!({"command": "material.add", "params": {"name": "ЛДСП 16", "thickness": 16}})).unwrap();
        assert!(server.call("project_new", &json!({"name": "x"})).is_err());
        // Чтение — с проекта окна: материал есть, деталей нет.
        let parts = server.call("parts", &json!({})).unwrap();
        assert!(parts[0]["text"].as_str().unwrap().starts_with("№"));
        assert_eq!(server.session.as_ref().unwrap().project.materials.len(), 1);
        server.call("window_disconnect", &json!({})).unwrap();
        let (seen, materials) = window.join().unwrap();
        assert_eq!(seen, ["material.add", "project.get"]);
        assert_eq!(materials, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
