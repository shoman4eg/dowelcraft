//! The window listens on a socket (`dowel::live`): MCP commands go to the open tab just like buttons —
//! with undo (Ctrl+Z), the "*" mark and redraw. A request waits for a reply from the window's frame.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use dowel::design::Project;
use eframe::egui;
use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};
use serde_json::{Value, json};

use crate::App;
use crate::i18n::t;

/// Command, its params, where to reply and the deadline (after it nobody waits for the reply — don't run it).
pub type Request = (String, Value, Sender<Result<Value, String>>, Instant);

const WAIT: Duration = Duration::from_secs(60);

/// Listen on the socket; `None` — another window already listens on it or it couldn't be opened.
pub fn listen(path: &Path, ctx: egui::Context) -> Option<Receiver<Request>> {
    let name = match dowel::live::name(path) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[link] {}: {e}", path.display());
            return None;
        }
    };
    match Stream::connect(name.borrow()) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        // Socket left over from a closed window: the file exists, nobody is listening.
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            let _ = std::fs::remove_file(path);
        }
        _ => {
            eprintln!("[link] {} is taken by another dowel-craft window", path.display());
            return None;
        }
    }
    let listener = match owner_only(ListenerOptions::new().name(name)).and_then(|o| o.create_sync()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[link] {}: {e}", path.display());
            return None;
        }
    };
    // Owner only: files are written through the socket on their behalf. (Windows: set on the pipe above.)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            eprintln!("[link] {}: {e}", path.display());
            return None;
        }
    }
    eprintln!("[link] waiting for MCP commands: {}", path.display());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let (tx, ctx) = (tx.clone(), ctx.clone());
            std::thread::spawn(move || serve(conn, &tx, &ctx));
        }
    });
    Some(rx)
}

/// Windows: only the pipe's owner (the user who started the window) may connect; others get "access denied".
#[cfg(windows)]
fn owner_only(o: ListenerOptions<'_>) -> std::io::Result<ListenerOptions<'_>> {
    use interprocess::os::windows::{local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor};
    Ok(o.security_descriptor(SecurityDescriptor::deserialize(widestring::u16cstr!("D:P(A;;GA;;;OW)"))?))
}

#[cfg(not(windows))]
fn owner_only(o: ListenerOptions<'_>) -> std::io::Result<ListenerOptions<'_>> {
    Ok(o)
}

fn serve(conn: Stream, tx: &Sender<Request>, ctx: &egui::Context) {
    let mut out = &conn;
    for line in BufReader::new(&conn).lines() {
        let Ok(line) = line else { return };
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(v) => {
                let cmd = v.get("command").and_then(Value::as_str).unwrap_or_default().to_string();
                let params = v.get("params").cloned().unwrap_or(json!({}));
                let (rtx, rrx) = channel();
                if tx.send((cmd, params, rtx, Instant::now() + WAIT)).is_err() {
                    return;
                }
                ctx.request_repaint();
                // The window may be busy with a file dialog — don't wait forever.
                rrx.recv_timeout(WAIT).unwrap_or_else(|_| Err(t("the window did not answer within a minute (is a dialog open?)")))
            }
            Err(e) => Err(t(format!("request: {e}"))),
        };
        let msg = match reply {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "error": e }),
        };
        if writeln!(out, "{msg}").is_err() {
            return;
        }
    }
}

impl App {
    /// Run pending requests (once per frame).
    pub(crate) fn serve_live(&mut self) {
        let Some(rx) = &self.live else { return };
        let requests: Vec<Request> = rx.try_iter().collect();
        for (cmd, params, reply, deadline) in requests {
            // The window was busy (dialog), the client was already refused — don't run it after the fact.
            if Instant::now() > deadline {
                continue;
            }
            let r = self.live_call(&cmd, &params);
            let _ = reply.send(r);
        }
    }

    fn live_call(&mut self, cmd: &str, params: &Value) -> Result<Value, String> {
        if cmd == "window.info" {
            return Ok(json!({ "name": self.doc_title(), "path": self.path, "dirty": self.dirty, "editable": self.session.is_some() }));
        }
        let Some(s) = &mut self.session else {
            return Err(t("the window shows a Bazis model, read-only — press “Make editable”"));
        };
        match cmd {
            "project.get" => serde_json::to_value(&s.project).map_err(|e| e.to_string()),
            "project.replace" => {
                let text = params.get("project").and_then(Value::as_str).ok_or_else(|| t("the “project” parameter is required"))?;
                // The script ran on a copy: if the project changed since (or the tab was switched) — don't overwrite.
                if params.get("base") != Some(&serde_json::to_value(&s.project).map_err(|e| e.to_string())?) {
                    return Err(t("the project in the window changed while the script ran (an edit or another tab) — run the script again"));
                }
                s.apply_project(Project::load(text)?)?;
                self.dirty = true;
                self.error = None;
                self.refresh();
                Ok(Value::Null)
            }
            "save" => {
                let path = match params.get("path").and_then(Value::as_str) {
                    Some(p) if dowel::design::is_project_file(Path::new(p)) => PathBuf::from(p),
                    Some(_) => return Err(t("a project is saved to a .dcraft file")),
                    None => self.path.clone().filter(|p| dowel::design::is_project_file(p)).ok_or_else(|| t("the project has no file yet: give a path or save it in the window (Ctrl+S)"))?,
                };
                self.save_to(path.clone())?;
                Ok(json!(path))
            }
            _ => self.exec_ret(cmd, params.clone()).ok_or_else(|| self.error.clone().unwrap_or_else(|| t(format!("command {cmd} failed")))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An MCP client sends commands to the window: the material shows up in the open project, undone by the window's Ctrl+Z.
    #[test]
    fn commands_reach_the_open_window() {
        let dir = std::env::temp_dir().join(format!("dowel-craft-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("test-{}.sock", std::process::id()));
        let mut app = App { live: listen(&path, egui::Context::default()), ..Default::default() };
        assert!(app.live.is_some());
        // A second window doesn't bind the same socket.
        assert!(listen(&path, egui::Context::default()).is_none());
        let client = std::thread::spawn({
            let path = path.clone();
            move || {
                let mut c = dowel::live::Client::connect(&path).unwrap();
                c.call("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
                let bad = c.call("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap_err();
                let p = c.call("project.get", &json!({})).unwrap();
                // A script result computed on a stale copy doesn't overwrite the window's project.
                let stale = c.call("project.replace", &json!({"project": p.to_string(), "base": {}})).unwrap_err();
                assert!(stale.contains("changed"), "{stale}");
                assert!(c.call("save", &json!({"path": "/tmp/x.txt"})).is_err());
                (bad, p)
            }
        });
        // Window frames until the client finishes.
        while !client.is_finished() {
            app.serve_live();
            std::thread::sleep(Duration::from_millis(5));
        }
        let (bad, p) = client.join().unwrap();
        assert!(bad.contains("already exists"), "{bad}");
        assert_eq!(p["materials"][0]["name"], "Chipboard 16");
        assert!(app.dirty);
        assert_eq!(app.session.as_ref().unwrap().project.materials.len(), 1);
        app.exec("undo", json!({}));
        assert!(app.session.as_ref().unwrap().project.materials.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
