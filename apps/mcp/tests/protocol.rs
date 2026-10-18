//! Сервер как его видит MCP-клиент: initialize, список инструментов, сценарий проектирования.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

#[test]
fn design_a_cabinet_over_mcp() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dowel-craft-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("сервер запускается");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut id = 0;
    let mut rpc = |method: &str, params: Value| -> Value {
        id += 1;
        writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], id);
        v
    };
    let call = |rpc: &mut dyn FnMut(&str, Value) -> Value, name: &str, args: Value| -> Value {
        rpc("tools/call", json!({"name": name, "arguments": args}))["result"].clone()
    };

    let init = rpc("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}));
    assert_eq!(init["result"]["serverInfo"]["name"], "dowel-craft");
    let tools = rpc("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"command") && names.contains(&"render_front"));

    let cmd = |rpc: &mut dyn FnMut(&str, Value) -> Value, c: &str, p: Value| {
        let r = call(rpc, "command", json!({"command": c, "params": p}));
        assert!(r.get("isError").is_none(), "{c}: {r}");
        r
    };
    cmd(&mut rpc, "material.add", json!({"name": "ЛДСП 16", "thickness": 16}));
    cmd(&mut rpc, "edge.add", json!({"name": "ПВХ", "thickness": 0.4}));
    cmd(&mut rpc, "cabinet.add", json!({"width": 600, "height": 720, "depth": 560, "material": "ЛДСП 16", "edge": "ПВХ", "doors": 1}));
    cmd(&mut rpc, "shelf.add", json!({"cabinet": 1, "height": 300}));

    let bad = call(&mut rpc, "command", json!({"command": "shelf.add", "params": {"cabinet": 1, "height": 5000}}));
    assert_eq!(bad["isError"], true, "ошибка модели возвращается как ошибка инструмента");

    let parts = call(&mut rpc, "parts", json!({}));
    let t = parts["content"][0]["text"].as_str().unwrap();
    assert!(t.contains("Полка") && t.contains("Дверь"), "{t}");

    let img = call(&mut rpc, "render_front", json!({"width": 300}));
    assert_eq!(img["content"][0]["type"], "image");
    assert!(img["content"][0]["data"].as_str().unwrap().starts_with("iVBOR"), "PNG в base64");

    let iso = call(&mut rpc, "render_iso", json!({"width": 300}));
    assert!(iso["content"][0]["data"].as_str().unwrap().starts_with("iVBOR"), "3D — тоже PNG");

    let cut = call(&mut rpc, "cut", json!({}));
    assert!(cut["content"][0]["text"].as_str().unwrap().contains("ЛДСП 16"));

    drop(stdin);
    child.wait().unwrap();
}
