//! Link to an open window: MCP sends commands to the project open on screen. Unix socket, one JSON line
//! per request `{command, params}` and response `{ok}` or `{error}`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Window socket: in the session's private directory (`XDG_RUNTIME_DIR`), otherwise in home — not in shared `/tmp`,
/// where another user could grab the name first.
pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default().join(".dowel-craft.sock")
}

/// Connection to a window.
pub struct Client {
    read: BufReader<UnixStream>,
    write: UnixStream,
}

impl Client {
    pub fn connect(path: &Path) -> Result<Self, String> {
        let s = UnixStream::connect(path).map_err(|e| format!("окно dowel-craft не запущено или недоступно ({}: {e})", path.display()))?;
        let read = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
        Ok(Client { read, write: s })
    }

    /// Command to the window; a link error starts with the Russian "connection to window" prefix.
    pub fn call(&mut self, command: &str, params: &Value) -> Result<Value, String> {
        writeln!(self.write, "{}", json!({ "command": command, "params": params })).map_err(|e| format!("связь с окном: {e}"))?;
        let mut line = String::new();
        if self.read.read_line(&mut line).map_err(|e| format!("связь с окном: {e}"))? == 0 {
            return Err("связь с окном: окно закрыто".into());
        }
        let v: Value = serde_json::from_str(&line).map_err(|e| format!("связь с окном: {e}"))?;
        match v.get("error").and_then(Value::as_str) {
            Some(e) => Err(e.to_string()),
            None => Ok(v.get("ok").cloned().unwrap_or(Value::Null)),
        }
    }
}
