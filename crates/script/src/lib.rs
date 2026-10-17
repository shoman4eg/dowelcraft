//! Скрипты Базиса (BAZIS-Script) на движке boa (JavaScript на чистом Rust).
//!
//! Объекты Базиса (`Model`, панели, контуры, `Action`, `system`, `fs`, окна `NewForm`, модульное API
//! `objects3d`, `panelOperations`) написаны на JavaScript в `prelude.js` поверх одного моста `__h(команда, JSON)`.
//! Мост — к сессии проекта: модель читается из деталей, изменения идут командами `Session::execute`,
//! поэтому скрипт не может сделать того, чего не умеет программа, и любое его изменение проверяется.
//! Файлы скрипт пишет и читает только в своей папке-песочнице. Windows-объекты (COM) недоступны.

use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use basis::design::{self, Part, Session};
use boa_engine::{Context, JsResult, JsString, JsValue, NativeFunction, Source, js_string};
use serde_json::{Value, json};

/// Итог выполнения скрипта.
#[derive(Debug, Default, Clone)]
pub struct Run {
    /// Сообщения скрипта (`alert`, `MessageBox`, `system.log`, `console.log`).
    pub log: Vec<String>,
    /// Ошибка выполнения; изменения модели при ошибке отменяются.
    pub error: Option<String>,
    /// Окна, которые скрипт показал (`NewForm().Show()`, `Action.Properties`) — описание в JSON.
    pub forms: Vec<Value>,
    /// Выполнено команд изменения модели.
    pub changes: usize,
}

/// Настройки запуска.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Папка, внутри которой скрипт может читать и писать файлы.
    pub sandbox: PathBuf,
    /// Папка скрипта — для `system.include` и относительных путей чтения.
    pub script_dir: PathBuf,
    /// Выделенные детали: (номер объекта верхнего уровня, номер панели дерева, номер внутри объекта).
    pub selection: Vec<(u32, u32, usize)>,
    /// Предел числа итераций циклов (защита от зависания).
    pub loop_limit: u64,
}

struct Host {
    session: Session,
    parts: Vec<Part>,
    revision: u64,
    run: Run,
    opts: Options,
}

impl Host {
    fn refresh(&mut self) {
        self.parts = design::build(&self.session.project).unwrap_or_default();
        self.revision += 1;
    }

    /// Ключ детали: объект верхнего уровня, панель дерева, номер внутри объекта.
    fn key(parts: &[Part], i: usize) -> Value {
        let p = &parts[i];
        let nth = parts[..i].iter().filter(|q| q.owner == p.owner && q.id == p.id).count();
        json!([p.owner, p.id, nth])
    }

    fn find(&self, k: &Value) -> Option<usize> {
        let (o, id, nth) = (k[0].as_u64()? as u32, k[1].as_u64()? as u32, k[2].as_u64()? as usize);
        self.parts.iter().enumerate().filter(|(_, p)| p.owner == o && p.id == id).nth(nth).map(|(i, _)| i)
    }

    fn part_json(&self, i: usize) -> Value {
        let p = &self.parts[i];
        let pl = &p.panel;
        json!({
            "key": Self::key(&self.parts, i), "owner": p.owner, "id": p.id, "kind": format!("{:?}", p.kind),
            "name": pl.name, "pos": pl.pos, "designation": pl.designation, "material": pl.material,
            "thickness": pl.thickness, "size": [pl.size.0, pl.size.1], "texture": format!("{:?}", pl.texture),
            "segs": pl.segs, "edges": pl.edges, "holes": pl.holes, "frame": pl.frame,
            "origin": p.origin, "box": p.size, "cut": pl.cut_size(),
        })
    }

    /// Путь внутри песочницы: без «..» и абсолютных путей наружу.
    fn sandboxed(&self, path: &str) -> Result<PathBuf, String> {
        let p = Path::new(path);
        let rel: PathBuf = p.components().filter(|c| matches!(c, Component::Normal(_))).collect();
        if p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(format!("путь «{path}» выходит из папки скрипта"));
        }
        // Абсолютный путь внутри песочницы допустим, иначе — берётся имя относительно песочницы.
        if p.is_absolute() && p.starts_with(&self.opts.sandbox) {
            return Ok(p.to_path_buf());
        }
        Ok(self.opts.sandbox.join(rel))
    }

    fn call(&mut self, cmd: &str, a: Value) -> Result<Value, String> {
        let text = |v: &Value| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
        match cmd {
            "parts" => Ok(Value::Array((0..self.parts.len()).map(|i| Self::key(&self.parts, i)).collect())),
            "part" => {
                let i = self.find(&a).ok_or("деталь больше не существует")?;
                Ok(self.part_json(i))
            }
            "revision" => Ok(json!(self.revision)),
            "project" => Ok(json!({ "name": self.session.project.name, "materials": self.session.project.materials, "edges": self.session.project.edges })),
            "selection" => {
                let keys: Vec<Value> = self
                    .opts
                    .selection
                    .iter()
                    .filter_map(|&(o, id, nth)| self.find(&json!([o, id, nth])).map(|i| Self::key(&self.parts, i)))
                    .collect();
                Ok(Value::Array(keys))
            }
            "unselect" => {
                self.opts.selection.clear();
                Ok(Value::Null)
            }
            "exec" => {
                let out = self.session.execute(a["cmd"].as_str().ok_or("exec: нет cmd")?, &a["args"])?;
                self.run.changes += 1;
                self.refresh();
                Ok(out)
            }
            "log" => {
                self.run.log.push(text(&a));
                Ok(Value::Null)
            }
            "form" => {
                self.run.forms.push(a);
                Ok(Value::Null)
            }
            "fs.read" => std::fs::read_to_string(self.sandboxed(&text(&a))?).map(Value::String).map_err(|e| e.to_string()),
            "fs.write" | "fs.append" => {
                let p = self.sandboxed(&text(&a["path"]))?;
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                let data = text(&a["data"]);
                let r = if cmd == "fs.write" {
                    std::fs::write(&p, data)
                } else {
                    use std::io::Write;
                    std::fs::OpenOptions::new().create(true).append(true).open(&p).and_then(|mut f| f.write_all(data.as_bytes()))
                };
                r.map(|_| Value::Null).map_err(|e| e.to_string())
            }
            "fs.exists" => Ok(json!(self.sandboxed(&text(&a)).map(|p| p.exists()).unwrap_or(false))),
            "fs.mkdir" => std::fs::create_dir_all(self.sandboxed(&text(&a))?).map(|_| Value::Null).map_err(|e| e.to_string()),
            "fs.remove" => std::fs::remove_file(self.sandboxed(&text(&a))?).map(|_| Value::Null).map_err(|e| e.to_string()),
            "fs.list" => {
                let rd = std::fs::read_dir(self.sandboxed(&text(&a))?).map_err(|e| e.to_string())?;
                Ok(Value::Array(rd.flatten().map(|e| json!(e.file_name().to_string_lossy())).collect()))
            }
            "fs.isdir" => Ok(json!(self.sandboxed(&text(&a)).map(|p| p.is_dir()).unwrap_or(false))),
            "include" => {
                let name = text(&a);
                let p = self.opts.script_dir.join(Path::new(&name).file_name().ok_or("include: пустое имя")?);
                read_script(&p)
                    .map(Value::String)
            }
            _ => Err(format!("мост: неизвестная команда «{cmd}»")),
        }
    }
}

/// Текст скрипта: UTF-8 (с BOM или без) или windows-1251 — как сохраняет Базис.
pub fn read_script(path: &Path) -> Result<String, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let b = b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&b);
    match std::str::from_utf8(b) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => Ok(encoding_rs::WINDOWS_1251.decode(b).0.into_owned()),
    }
}

const PRELUDE: &str = include_str!("prelude.js");

fn bridge(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let cmd = args.first().map(|v| v.to_string(ctx)).transpose()?.map(|s| s.to_std_string_escaped()).unwrap_or_default();
    let raw = args.get(1).map(|v| v.to_string(ctx)).transpose()?.map(|s| s.to_std_string_escaped()).unwrap_or_else(|| "null".into());
    let a: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let host = ctx.get_data::<Rc<RefCell<Host>>>().cloned().expect("мост установлен");
    let r = host.borrow_mut().call(&cmd, a);
    match r {
        Ok(v) => Ok(JsValue::from(JsString::from(v.to_string()))),
        Err(e) => Err(boa_engine::JsError::from_opaque(JsValue::from(JsString::from(e)))),
    }
}

/// Живой скрипт: контекст JavaScript сохраняется, пока открыто окно скрипта — щелчки по кнопкам
/// и ввод в поля вызывают обработчики скрипта. Изменения модели копятся и применяются в [`Runtime::finish`].
pub struct Runtime {
    ctx: Context,
    host: Rc<RefCell<Host>>,
    /// Ошибка запуска или обработчика; после неё изменения не применяются.
    pub error: Option<String>,
}

impl Runtime {
    /// Запустить скрипт: пролог, текст скрипта, отложенные задачи.
    pub fn start(session: &Session, source: &str, opts: Options) -> Runtime {
        let mut host = Host { session: Session::new(session.project.clone()), parts: Vec::new(), revision: 0, run: Run::default(), opts: opts.clone() };
        host.refresh();
        let host = Rc::new(RefCell::new(host));
        let mut ctx = Context::default();
        ctx.runtime_limits_mut().set_loop_iteration_limit(if opts.loop_limit == 0 { 50_000_000 } else { opts.loop_limit });
        ctx.runtime_limits_mut().set_recursion_limit(20_000);
        ctx.runtime_limits_mut().set_stack_size_limit(1 << 20);
        ctx.insert_data(host.clone());
        let mut rt = Runtime { ctx, host, error: None };
        if let Err(e) = rt.ctx.register_global_callable(js_string!("__h"), 2, NativeFunction::from_fn_ptr(bridge)) {
            rt.error = Some(e.to_string());
            return rt;
        }
        if let Err(e) = rt.ctx.eval(Source::from_bytes(PRELUDE)) {
            rt.error = Some(format!("пролог: {e}"));
            return rt;
        }
        rt.eval(source);
        rt
    }

    /// Выполнить код в контексте скрипта (обработчик окна и т.п.). Ошибка запоминается.
    pub fn eval(&mut self, code: &str) {
        if self.error.is_some() {
            return;
        }
        let r = self.ctx.eval(Source::from_bytes(code)).and_then(|_| self.ctx.run_jobs());
        if let Err(e) = r {
            self.error = Some(e.to_string());
        }
    }

    /// Окна, показанные скриптом, в их текущем состоянии (JSON).
    pub fn forms(&mut self) -> Vec<Value> {
        let r = self.ctx.eval(Source::from_bytes("JSON.stringify(__forms())"));
        r.ok()
            .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
            .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
            .unwrap_or_default()
    }

    /// Щелчок по элементу окна.
    pub fn click(&mut self, id: i64) {
        self.eval(&format!("__click({id})"));
    }

    /// Новое значение поля элемента (Text, Checked, Value, ItemIndex) — и его OnChange.
    pub fn change(&mut self, id: i64, field: &str, value: &Value) {
        let f = serde_json::to_string(field).unwrap_or_default();
        self.eval(&format!("__change({id}, {f}, {value})"));
    }

    /// Сообщения скрипта на данный момент.
    pub fn log(&self) -> Vec<String> {
        self.host.borrow().run.log.clone()
    }

    /// Закончить: обработчики `Action.OnFinish`, затем изменения — в сессию (одна запись отмены).
    pub fn finish(mut self, session: &mut Session) -> Run {
        self.eval("__finish()");
        let error = self.error.take();
        drop(self.ctx);
        let Ok(host) = Rc::try_unwrap(self.host).map(RefCell::into_inner) else {
            return Run { error: Some("внутренняя ошибка: мост не освобождён".into()), ..Run::default() };
        };
        let mut out = host.run;
        out.error = error;
        if out.error.is_none()
            && out.changes > 0
            && let Err(e) = session.apply_project(host.session.project)
        {
            out.error = Some(e);
        }
        out
    }
}

/// Выполнить скрипт без окна: запуск и сразу завершение. При ошибке модель не меняется.
pub fn run(session: &mut Session, source: &str, opts: Options) -> Run {
    Runtime::start(session, source, opts).finish(session)
}

#[cfg(test)]
mod tests;
