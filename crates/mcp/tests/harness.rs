//! Minimal stdio JSON-RPC driver for the MCP server binary.
//!
//! Deliberately does not use rmcp's client: the `client` feature is disabled
//! (Global Constraints), and driving the wire format directly is also what we
//! want to be testing.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

pub struct Server {
    child: Child,
    reader: BufReader<ChildStdout>,
    next_id: i64,
}

impl Server {
    pub fn start() -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zoetrope-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn zoetrope-mcp");
        let reader = BufReader::new(child.stdout.take().expect("stdout piped"));
        Server {
            child,
            reader,
            next_id: 1,
        }
    }

    pub fn send(&mut self, msg: &Value) {
        let stdin = self.child.stdin.as_mut().expect("stdin piped");
        writeln!(stdin, "{}", serde_json::to_string(msg).expect("serialize"))
            .expect("write to server");
        stdin.flush().expect("flush");
    }

    pub fn recv(&mut self) -> Value {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).expect("read from server");
        assert!(n > 0, "server closed stdout before responding");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("server emitted non-JSON line {line:?}: {e}"))
    }

    /// Send a request with an auto-assigned id and read exactly one response.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        self.recv()
    }

    /// Perform the MCP handshake and return the `initialize` response.
    pub fn initialize(&mut self) -> Value {
        let resp = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "zoetrope-mcp-test", "version": "0" },
            }),
        );
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }));
        resp
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
