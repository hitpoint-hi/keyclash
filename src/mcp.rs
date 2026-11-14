//! `keyclash --mcp`: a Model Context Protocol server on stdin and stdout.
//!
//! One JSON-RPC message per line. No network: the agent app starts this program and talks to it
//! through a pipe. Logs go to stderr only, because stdout carries the protocol.

use crate::agent::{Session, tool_specs};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// Protocol versions this server speaks, newest first.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Keyclash reports shortcuts discovered on this Mac: macOS settings, app menus, app settings, and the config files of AeroSpace, OmniWM, skhd, Karabiner and others. Some sources need permissions or a deep scan. \
Use it to find clashes, free keys, and to change keys. \
To change keys: find the shortcut id (conflicts, list_shortcuts or lookup), pick free keys (free_keys or free_nearby), call propose_change, show the diff and the conflicts to the person, \
and call apply_change only after they agree. Then do the after_apply step. undo_last puts the last change back. \
Keyclash cannot write macOS system hotkeys, Karabiner rules, or settings inside apps; for those, give the person the how_to_change steps.";

fn reply(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The server state: the scan starts at the first tool call, so `initialize` answers at once.
pub struct Server {
    session: Option<Session>,
    deep: bool,
    initialized: bool,
}

impl Server {
    pub fn new(deep: bool) -> Self {
        Server { session: None, deep, initialized: false }
    }

    #[cfg(test)]
    fn with_session(session: Session) -> Self {
        Server { deep: session.deep, session: Some(session), initialized: true }
    }

    /// Answer one message. None for a notification (no id), which gets no answer.
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        if !msg.is_object() || msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error(&Value::Null, -32600, "Invalid JSON-RPC request"));
        }
        let id = msg.get("id").cloned();
        if id.as_ref().is_some_and(|id| !id.is_string() && !id.is_number()) {
            return Some(error(&Value::Null, -32600, "Request id must be a string or number"));
        }
        let method = msg.get("method").and_then(Value::as_str);
        let Some(method) = method else {
            // A response from the client (we never ask it anything) or a bad message.
            if msg.get("result").is_some() || msg.get("error").is_some() { return None; }
            return Some(error(&id.unwrap_or(Value::Null), -32600, "Invalid request"));
        };
        let id = id?;
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        if !params.is_object() {
            return Some(error(&id, -32602, "Parameters must be an object"));
        }
        if !self.initialized && !matches!(method, "initialize" | "ping") {
            return Some(error(&id, -32000, "Initialize the server before calling tools"));
        }
        Some(match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(VERSIONS[0]);
                let version = if VERSIONS.contains(&asked) { asked } else { VERSIONS[0] };
                self.initialized = true;
                reply(&id, json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "keyclash", "title": "Keyclash", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => reply(&id, json!({})),
            "tools/list" => reply(&id, json!({ "tools": tool_specs() })),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(error(&id, -32602, "tools/call needs a tool name"));
                };
                if !crate::agent::TOOL_NAMES.contains(&name) {
                    return Some(error(&id, -32602, &format!("Unknown tool: {name}")));
                }
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let deep = self.deep;
                let session = self.session.get_or_insert_with(|| Session::new(deep));
                match session.call(name, &args) {
                    Ok(v) => reply(&id, json!({ "content": [{ "type": "text", "text": v.to_string() }], "structuredContent": v, "isError": false })),
                    // A tool error goes back as a result, so the model reads it and can try again.
                    Err(e) => reply(&id, json!({ "content": [{ "type": "text", "text": e }], "isError": true })),
                }
            }
            "resources/list" => reply(&id, json!({ "resources": [] })),
            "prompts/list" => reply(&id, json!({ "prompts": [] })),
            other => error(&id, -32601, &format!("Method not found: {other}")),
        })
    }
}

/// Read messages from stdin until it closes.
pub fn serve(deep: bool) {
    let mut server = Server::new(deep);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let answers: Vec<Value> = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => server.handle(&msg).into_iter().collect(),
            Err(e) => vec![error(&Value::Null, -32700, &format!("Parse error: {e}"))],
        };
        for a in answers {
            if writeln!(out, "{a}").and_then(|_| out.flush()).is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::scan::ScanResult;

    fn server() -> Server {
        let t = Trigger::combo(CTRL | OPT, "F");
        let list = vec![
            Shortcut::new("Rectangle", "Center Third", t.clone(), Scope::Global, Origin::BuiltInDefault).bundle(Some("com.knollsoft.Rectangle")),
            Shortcut::new("Raycast", "Open", t, Scope::Global, Origin::AppSettings).bundle(Some("com.raycast.macos")),
        ];
        Server::with_session(Session { scan: ScanResult { shortcuts: list, ..Default::default() }, deep: false })
    }

    #[test]
    fn handshake_and_tool_list() {
        let mut s = server();
        let r = s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } })).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(r["result"]["serverInfo"]["name"], "keyclash");
        let r = s.handle(&json!({ "jsonrpc": "2.0", "id": 9, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } })).unwrap();
        assert_eq!(r["result"]["protocolVersion"], VERSIONS[0]);
        assert!(s.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let r = s.handle(&json!({ "jsonrpc": "2.0", "id": "a", "method": "tools/list" })).unwrap();
        assert_eq!(r["id"], "a");
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), crate::agent::TOOL_NAMES.len());
        let r = s.handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" })).unwrap();
        assert_eq!(r["result"], json!({}));
    }

    #[test]
    fn tool_calls_and_errors() {
        let mut s = server();
        let r = s.handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "conflicts", "arguments": {} } })).unwrap();
        assert_eq!(r["result"]["isError"], false);
        let body: Value = serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["total"], 1);
        // A tool error is a result with isError, so the model can read it.
        let r = s.handle(&json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "lookup", "arguments": { "keys": "cmd banana" } } })).unwrap();
        assert_eq!(r["result"]["isError"], true);
        // An unknown tool or method is a protocol error.
        let r = s.handle(&json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "rm_rf" } })).unwrap();
        assert_eq!(r["error"]["code"], -32602);
        let r = s.handle(&json!({ "jsonrpc": "2.0", "id": 6, "method": "sampling/createMessage" })).unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }

    #[test]
    fn invalid_requests_and_uninitialized_calls_do_not_scan() {
        let mut s = Server::new(false);
        for msg in [json!(null), json!([]), json!({}), json!({"jsonrpc":"1.0", "id":1, "method":"ping"}), json!({"jsonrpc":"2.0", "id":[], "method":"ping"})] {
            assert_eq!(s.handle(&msg).unwrap()["error"]["code"], -32600);
        }
        assert_eq!(s.handle(&json!({"jsonrpc":"2.0", "id":1, "method":"tools/list"})).unwrap()["error"]["code"], -32000);
        assert!(s.session.is_none());
        assert!(s.handle(&json!({"jsonrpc":"2.0", "method":"notifications/cancelled", "params":{"requestId":1}})).is_none());
        assert_eq!(s.handle(&json!({"jsonrpc":"2.0", "id":2, "method":"ping"})).unwrap()["result"], json!({}));
    }
}
