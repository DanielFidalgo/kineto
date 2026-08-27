//! Minimal stdio JSON-RPC driver for the MCP server binary.
//!
//! Deliberately does not use rmcp's client: the `client` feature is disabled
//! (Global Constraints), and driving the wire format directly is also what we
//! want to be testing.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

/// Bound on how long `recv` waits for a line on stdout. Generous enough that
/// a real render (Task 4 onward: tools that rasterize frames and shell out
/// to ffmpeg) never trips it, but short enough that a hung handler fails the
/// test in seconds rather than stalling until CI's job-level timeout.
const RECV_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Server {
    child: Child,
    stdout_rx: Receiver<String>,
    stderr: Arc<Mutex<String>>,
    next_id: i64,
}

impl Server {
    pub fn start() -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kineto-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn kineto-mcp");

        // Forward stdout lines through a channel from a dedicated thread, so
        // `recv` can wait on `Receiver::recv_timeout` instead of blocking
        // indefinitely on `BufRead::read_line`.
        let stdout = child.stdout.take().expect("stdout piped");
        let (stdout_tx, stdout_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break, // EOF or read error: stop forwarding
                    Ok(_) => {
                        if stdout_tx.send(line).is_err() {
                            break; // Server dropped, nobody to receive
                        }
                    }
                }
            }
        });

        // Capture stderr continuously (rather than discarding it) so a
        // server-side panic or error has somewhere to surface: both the
        // closed-stdout and the timeout failure below include it.
        let stderr_pipe = child.stderr.take().expect("stderr piped");
        let stderr = Arc::new(Mutex::new(String::new()));
        let stderr_writer = Arc::clone(&stderr);
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr_pipe);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => stderr_writer.lock().unwrap().push_str(&line),
                }
            }
        });

        Server {
            child,
            stdout_rx,
            stderr,
            next_id: 1,
        }
    }

    pub fn send(&mut self, msg: &Value) {
        let stdin = self.child.stdin.as_mut().expect("stdin piped");
        writeln!(stdin, "{}", serde_json::to_string(msg).expect("serialize"))
            .expect("write to server");
        stdin.flush().expect("flush");
    }

    /// Wait up to `RECV_TIMEOUT` for one response line. Panics with a
    /// diagnostic message — including any stderr captured so far — if the
    /// server closes stdout, or if a handler hangs and never responds.
    pub fn recv(&mut self) -> Value {
        match self.stdout_rx.recv_timeout(RECV_TIMEOUT) {
            Ok(line) => serde_json::from_str(&line).unwrap_or_else(|e| {
                panic!(
                    "server emitted non-JSON line {line:?}: {e}\n--- captured stderr ---\n{}",
                    self.stderr_snapshot()
                )
            }),
            Err(RecvTimeoutError::Disconnected) => panic!(
                "server closed stdout before responding\n--- captured stderr ---\n{}",
                self.stderr_snapshot()
            ),
            Err(RecvTimeoutError::Timeout) => panic!(
                "server did not respond within {RECV_TIMEOUT:?}\n--- captured stderr ---\n{}",
                self.stderr_snapshot()
            ),
        }
    }

    fn stderr_snapshot(&self) -> String {
        let captured = self.stderr.lock().unwrap().clone();
        if captured.is_empty() {
            "(empty)".to_string()
        } else {
            captured
        }
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
                "clientInfo": { "name": "kineto-mcp-test", "version": "0" },
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
