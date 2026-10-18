//! Окно слушает сокет (`dowel::live`): команды MCP идут в открытую вкладку так же, как кнопки, —
//! с отменой (Ctrl+Z), пометкой «*» и перерисовкой. Запрос ждёт ответа от кадра окна.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use dowel::design::Project;
use eframe::egui;
use serde_json::{Value, json};

use crate::App;

/// Команда, её параметры, куда ответить и до какого времени (позже ответ уже не ждут — не выполнять).
pub type Request = (String, Value, Sender<Result<Value, String>>, Instant);

const WAIT: Duration = Duration::from_secs(60);

/// Слушать сокет; `None` — его уже слушает другое окно или не удалось открыть.
pub fn listen(path: &Path, ctx: egui::Context) -> Option<Receiver<Request>> {
    match UnixStream::connect(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        // Сокет остался от закрытого окна: файл есть, никто не слушает.
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            let _ = std::fs::remove_file(path);
        }
        _ => {
            eprintln!("[связь] {} занят другим окном dowel-craft", path.display());
            return None;
        }
    }
    let listener = match UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[связь] {}: {e}", path.display());
            return None;
        }
    };
    // Только владелец: через сокет пишутся файлы от его имени.
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        eprintln!("[связь] {}: {e}", path.display());
        return None;
    }
    eprintln!("[связь] жду команды MCP: {}", path.display());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let (tx, ctx) = (tx.clone(), ctx.clone());
            std::thread::spawn(move || serve(conn, &tx, &ctx));
        }
    });
    Some(rx)
}

fn serve(conn: UnixStream, tx: &Sender<Request>, ctx: &egui::Context) {
    let Ok(mut out) = conn.try_clone() else { return };
    for line in BufReader::new(conn).lines() {
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
                // Окно может быть занято диалогом выбора файла — не ждать вечно.
                rrx.recv_timeout(WAIT).unwrap_or_else(|_| Err("окно не ответило за минуту (открыт диалог?)".into()))
            }
            Err(e) => Err(format!("запрос: {e}")),
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
    /// Выполнить пришедшие запросы (раз в кадр).
    pub(crate) fn serve_live(&mut self) {
        let Some(rx) = &self.live else { return };
        let requests: Vec<Request> = rx.try_iter().collect();
        for (cmd, params, reply, deadline) in requests {
            // Окно было занято (диалог), клиенту уже ответили отказом — не выполнять задним числом.
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
            return Err("в окне открыта модель Базиса только для просмотра — нажмите «Сделать редактируемой»".into());
        };
        match cmd {
            "project.get" => serde_json::to_value(&s.project).map_err(|e| e.to_string()),
            "project.replace" => {
                let text = params.get("project").and_then(Value::as_str).ok_or("нужен параметр «project»")?;
                // Скрипт считался на копии: если с тех пор проект поменяли (или переключили вкладку) — не затирать.
                if params.get("base") != Some(&serde_json::to_value(&s.project).map_err(|e| e.to_string())?) {
                    return Err("проект в окне изменился, пока выполнялся скрипт (правка или другая вкладка) — запустите скрипт ещё раз".into());
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
                    Some(_) => return Err("проект сохраняется в файл .dcraft".into()),
                    None => self.path.clone().filter(|p| dowel::design::is_project_file(p)).ok_or("у проекта ещё нет файла: укажите path или сохраните в окне (Ctrl+S)")?,
                };
                self.save_to(path.clone())?;
                Ok(json!(path))
            }
            _ => self.exec_ret(cmd, params.clone()).ok_or_else(|| self.error.clone().unwrap_or_else(|| format!("команда {cmd} не выполнена"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MCP-клиент шлёт команды в окно: материал появляется в открытом проекте, отменяется Ctrl+Z окна.
    #[test]
    fn commands_reach_the_open_window() {
        let dir = std::env::temp_dir().join(format!("dowel-craft-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.sock");
        let mut app = App { live: listen(&path, egui::Context::default()), ..Default::default() };
        assert!(app.live.is_some());
        // Второе окно на тот же сокет не садится.
        assert!(listen(&path, egui::Context::default()).is_none());
        let client = std::thread::spawn({
            let path = path.clone();
            move || {
                let mut c = dowel::live::Client::connect(&path).unwrap();
                c.call("material.add", &json!({"name": "ЛДСП 16", "thickness": 16})).unwrap();
                let bad = c.call("material.add", &json!({"name": "ЛДСП 16", "thickness": 16})).unwrap_err();
                let p = c.call("project.get", &json!({})).unwrap();
                // Итог скрипта, посчитанный на старой копии, не затирает проект окна.
                let stale = c.call("project.replace", &json!({"project": p.to_string(), "base": {}})).unwrap_err();
                assert!(stale.contains("изменился"), "{stale}");
                assert!(c.call("save", &json!({"path": "/tmp/x.txt"})).is_err());
                (bad, p)
            }
        });
        // Кадры окна, пока клиент не закончит.
        while !client.is_finished() {
            app.serve_live();
            std::thread::sleep(Duration::from_millis(5));
        }
        let (bad, p) = client.join().unwrap();
        assert!(bad.contains("уже есть"), "{bad}");
        assert_eq!(p["materials"][0]["name"], "ЛДСП 16");
        assert!(app.dirty);
        assert_eq!(app.session.as_ref().unwrap().project.materials.len(), 1);
        app.exec("undo", json!({}));
        assert!(app.session.as_ref().unwrap().project.materials.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
