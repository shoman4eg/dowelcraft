//! Bazis scripts (BAZIS-Script) on the boa engine (JavaScript in pure Rust).
//!
//! Bazis objects (`Model`, panels, contours, `Action`, `system`, `fs`, `NewForm` windows, the modular API
//! `objects3d`, `panelOperations`) are written in JavaScript in `prelude.js` on top of a single bridge `__h(command, JSON)`.
//! The bridge goes to the project session: the model is read from parts, changes go through `Session::execute` commands,
//! so a script cannot do anything the program cannot, and every change it makes is validated.
//! Scripts read and write files only in their own sandbox folder. Windows objects (COM) are unavailable.

use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use dowel::design::{self, Part, Session};
use boa_engine::{Context, JsResult, JsString, JsValue, NativeFunction, Source, js_string};
use serde_json::{Value, json};

/// Result of running a script.
#[derive(Debug, Default, Clone)]
pub struct Run {
    /// Script messages (`alert`, `MessageBox`, `system.log`, `console.log`).
    pub log: Vec<String>,
    /// Execution error; model changes are rolled back on error.
    pub error: Option<String>,
    /// Windows the script showed (`NewForm().Show()`, `Action.Properties`), described as JSON.
    pub forms: Vec<Value>,
    /// Number of model-changing commands executed.
    pub changes: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Folder inside which the script may read and write files.
    pub sandbox: PathBuf,
    /// Script folder — for `system.include` and relative read paths.
    pub script_dir: PathBuf,
    /// Selected parts: (top-level object index, tree panel index, index within the object).
    pub selection: Vec<(u32, u32, usize)>,
    /// Loop iteration limit (protection against hangs).
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

    /// Part key: top-level object, tree panel, index within the object.
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

    /// Path inside the sandbox: no ".." and no absolute paths leading out.
    fn sandboxed(&self, path: &str) -> Result<PathBuf, String> {
        let p = Path::new(path);
        let rel: PathBuf = p.components().filter(|c| matches!(c, Component::Normal(_))).collect();
        if p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(dowel::i18n::t(format!("path “{path}” leaves the script folder")));
        }
        // An absolute path inside the sandbox is allowed; otherwise the name is taken relative to the sandbox.
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
                let i = self.find(&a).ok_or_else(|| dowel::i18n::t("the part no longer exists"))?;
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
                let out = self.session.execute(a["cmd"].as_str().ok_or("exec: no cmd")?, &a["args"])?;
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
                    dowel::write(&p, data)
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
                let p = self.opts.script_dir.join(Path::new(&name).file_name().ok_or("include: empty name")?);
                read_script(&p)
                    .map(Value::String)
            }
            _ => Err(format!("bridge: unknown command “{cmd}”")),
        }
    }
}

/// Script text: UTF-8 (with or without BOM) or windows-1251 — as Bazis saves it.
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
    let host = ctx.get_data::<Rc<RefCell<Host>>>().cloned().expect("bridge is installed");
    let r = host.borrow_mut().call(&cmd, a);
    match r {
        Ok(v) => Ok(JsValue::from(JsString::from(v.to_string()))),
        Err(e) => Err(boa_engine::JsError::from_opaque(JsValue::from(JsString::from(e)))),
    }
}

/// Live script: the JavaScript context persists while the script window is open — button clicks
/// and field input call the script's handlers. Model changes accumulate and are applied in [`Runtime::finish`].
pub struct Runtime {
    ctx: Context,
    host: Rc<RefCell<Host>>,
    /// Startup or handler error; after it changes are not applied.
    pub error: Option<String>,
}

impl Runtime {
    /// Start the script: prelude, script text, deferred jobs.
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
            rt.error = Some(format!("prelude: {e}"));
            return rt;
        }
        rt.eval(source);
        rt
    }

    /// Evaluate code in the script context (a window handler etc.). The error is remembered.
    pub fn eval(&mut self, code: &str) {
        if self.error.is_some() {
            return;
        }
        let r = self.ctx.eval(Source::from_bytes(code)).and_then(|_| self.ctx.run_jobs());
        if let Err(e) = r {
            self.error = Some(e.to_string());
        }
    }

    /// Windows shown by the script, in their current state (JSON).
    pub fn forms(&mut self) -> Vec<Value> {
        let r = self.ctx.eval(Source::from_bytes("JSON.stringify(__forms())"));
        r.ok()
            .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
            .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
            .unwrap_or_default()
    }

    pub fn click(&mut self, id: i64) {
        self.eval(&format!("__click({id})"));
    }

    /// New value of an element field (Text, Checked, Value, ItemIndex) — and its OnChange.
    pub fn change(&mut self, id: i64, field: &str, value: &Value) {
        let f = serde_json::to_string(field).unwrap_or_default();
        self.eval(&format!("__change({id}, {f}, {value})"));
    }

    /// Script messages so far.
    pub fn log(&self) -> Vec<String> {
        self.host.borrow().run.log.clone()
    }

    /// Finish: `Action.OnFinish` handlers, then changes go to the session (one undo entry).
    pub fn finish(mut self, session: &mut Session) -> Run {
        self.eval("__finish()");
        let error = self.error.take();
        drop(self.ctx);
        let Ok(host) = Rc::try_unwrap(self.host).map(RefCell::into_inner) else {
            return Run { error: Some("internal error: the bridge was not released".into()), ..Run::default() };
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

/// Run a script without a window: start and finish at once. On error the model is unchanged.
pub fn run(session: &mut Session, source: &str, opts: Options) -> Run {
    Runtime::start(session, source, opts).finish(session)
}

#[cfg(test)]
mod tests;
