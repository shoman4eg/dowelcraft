//! Link to an open window: MCP sends commands to the project open on screen. A local socket (a socket file on
//! macOS/Linux, a named pipe on Windows), one JSON line per request `{command, params}` and response `{ok}` or `{error}`.

use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use interprocess::local_socket::{Name, Stream, prelude::*};
use serde_json::{Value, json};

/// Window endpoint. Unix: a socket file in the session's private directory (`XDG_RUNTIME_DIR`), otherwise in home —
/// not in shared `/tmp`, where another user could grab the name first. Windows: a named pipe with the user's name.
pub fn socket_path() -> PathBuf {
    if cfg!(windows) {
        return PathBuf::from(format!("dowel-craft-{}.sock", std::env::var("USERNAME").unwrap_or_default()));
    }
    std::env::var_os("XDG_RUNTIME_DIR").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default().join(".dowel-craft.sock")
}

/// Socket name for `path`: the file itself on Unix; on Windows the pipe `\\.\pipe\<file name>`.
pub fn name(path: &Path) -> io::Result<Name<'_>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        path.file_name().unwrap_or(path.as_os_str()).to_ns_name::<GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::GenericFilePath;
        path.to_fs_name::<GenericFilePath>()
    }
}

/// Connection to a window.
pub struct Client {
    conn: BufReader<Stream>,
}

impl Client {
    pub fn connect(path: &Path) -> Result<Self, String> {
        let s = name(path).and_then(Stream::connect).map_err(|e| format!("the dowel-craft window is not running or not reachable ({}: {e})", path.display()))?;
        Ok(Client { conn: BufReader::new(s) })
    }

    /// Command to the window; a link error starts with the Russian "connection to window" prefix.
    pub fn call(&mut self, command: &str, params: &Value) -> Result<Value, String> {
        writeln!(self.conn.get_mut(), "{}", json!({ "command": command, "params": params })).map_err(|e| format!("window link: {e}"))?;
        let mut line = String::new();
        if self.conn.read_line(&mut line).map_err(|e| format!("window link: {e}"))? == 0 {
            return Err("window link: the window is closed".into());
        }
        let v: Value = serde_json::from_str(&line).map_err(|e| format!("window link: {e}"))?;
        match v.get("error").and_then(Value::as_str) {
            Some(e) => Err(e.to_string()),
            None => Ok(v.get("ok").cloned().unwrap_or(Value::Null)),
        }
    }
}
