//! MCP-сервер basis-craft: JSON-RPC 2.0 через stdio, одно сообщение на строку.
//! ИИ-агент проектирует мебель теми же командами, что окно и CLI (`Session::execute`).

use std::io::{BufRead, Write};
use std::path::PathBuf;

use basis::design::{self, Kind, Project, Session};
use basis::job::{self, Job};
use basis::nest::Params;
use basis::panel;
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
}

impl Default for Server {
    fn default() -> Self {
        let p = Project::new("Новый проект");
        Server { job: Job::from_project(p.clone()), session: Some(Session::new(p)), path: None }
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
        {"name": "project_new", "description": "Создать пустой проект мебели.",
         "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}},
        {"name": "project_open", "description": "Открыть свой проект .bcraft (можно править) или модель Базиса .b3d/.fr3d (только чтение).",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
        {"name": "project_import", "description": "Открыть модель Базиса .b3d как свой проект: плиты станут свободными деталями, которые можно править (panel.set, panel.remove) и дополнять корпусами.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
        {"name": "project_save", "description": "Сохранить проект в .bcraft. Без path — в файл, из которого открыт.",
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
        {"name": "export", "description": "Сохранить документ: cards_html (карты раскроя и «Панели в производство»), raskroy_xml (проект БАЗИС-Раскроя), spec_csv, smeta_csv.",
         "inputSchema": {"type": "object", "properties": {
             "kind": {"type": "string", "enum": ["cards_html", "raskroy_xml", "spec_csv", "smeta_csv"]},
             "path": {"type": "string"}}, "required": ["kind", "path"]}},
        {"name": "render_iso", "description": "3D-картинка (PNG, аксонометрия спереди-справа-сверху) проекта или модели Базиса.",
         "inputSchema": {"type": "object", "properties": {
             "width": {"type": "integer", "description": "ширина картинки, пикселей (по умолчанию 900)"},
             "doors": {"type": "boolean", "description": "рисовать двери и фасады (по умолчанию да)"}}}},
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
                "serverInfo": {"name": "basis-craft", "version": env!("CARGO_PKG_VERSION")},
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

    fn call(&mut self, name: &str, a: &Value) -> ToolResult {
        let str_arg = |k: &str| a.get(k).and_then(Value::as_str).ok_or(format!("нужен параметр «{k}»"));
        match name {
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
                let msg = format!("Импортировано деталей: {}, материалов: {}. Сохраните project_save с путём .bcraft.", p.panels().len(), p.materials.len());
                self.session = Some(Session::new(p));
                self.path = None;
                self.refresh();
                Ok(text(msg))
            }
            "project_save" => {
                let s = self.session.as_ref().ok_or("открыта модель Базиса — сохранять нечего")?;
                let path = match a.get("path").and_then(Value::as_str) {
                    Some(p) => PathBuf::from(p),
                    None => self.path.clone().filter(|p| p.extension().is_some_and(|e| e == "bcraft")).ok_or("укажите path")?,
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
                    "raskroy_xml" => basis::raskroy::project_xml(&job.name, &job.panels, |m| job.sheet(m, p.sheet)),
                    "spec_csv" => basis::report::spec_csv(&panel::spec(&job.panels)).into_bytes(),
                    "smeta_csv" => basis::report::materials_csv(&job.estimate(1.2)).into_bytes(),
                    k => return Err(format!("неизвестный вид документа «{k}»")),
                };
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(text(format!("Сохранено: {}", path.display())))
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
    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
