//! Связь с открытым окном: MCP шлёт команды в проект, открытый на экране. Unix-сокет, по строке JSON
//! на запрос `{command, params}` и ответ `{ok}` или `{error}`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Сокет окна: в личной папке сеанса (`XDG_RUNTIME_DIR`), иначе в домашней — не в общей `/tmp`,
/// где чужой пользователь мог бы занять имя первым.
pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default().join(".basis-craft.sock")
}

/// Подключение к окну.
pub struct Client {
    read: BufReader<UnixStream>,
    write: UnixStream,
}

impl Client {
    pub fn connect(path: &Path) -> Result<Self, String> {
        let s = UnixStream::connect(path).map_err(|e| format!("окно basis-craft не запущено или недоступно ({}: {e})", path.display()))?;
        let read = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
        Ok(Client { read, write: s })
    }

    /// Команда в окно; ошибка связи начинается с «связь с окном».
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
