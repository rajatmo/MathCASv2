//! The message loop: requests and notifications of the protocol, dispatched to the [`Workspace`].
//!
//! Handled: `initialize`, `initialized`, `shutdown`, `exit`; `textDocument/didOpen`, `didChange` (full text),
//! `didClose`, `didSave`; `textDocument/definition`, `hover`, `completion`, `signatureHelp`, `documentSymbol`. Sent:
//! `textDocument/publishDiagnostics` for every open document after each open, change or close (a change to one
//! file can change the diagnostics of the files importing it). Messages are handled one at a time, in order.
use crate::analysis::Workspace;
use crate::rpc::{self, code, Incoming};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// The server's state.
pub struct Server<W: Write> {
    out: W,
    workspace: Workspace,
    initialized: bool,
    shutdown: bool,
}

/// Why a handler did not produce a result.
struct Failure {
    code: i64,
    message: String,
}

fn invalid(message: &str) -> Failure {
    Failure { code: code::INVALID_PARAMS, message: message.to_string() }
}

/// `params.textDocument.uri`.
fn uri(params: &Value) -> Result<&str, Failure> {
    params["textDocument"]["uri"].as_str().ok_or_else(|| invalid("missing textDocument.uri"))
}

/// `params.position` as (line, character).
fn position(params: &Value) -> Result<(u32, u32), Failure> {
    let p = &params["position"];
    let n = |v: &Value| v.as_u64().map(|x| x.min(u32::MAX as u64) as u32);
    Ok((n(&p["line"]).ok_or_else(|| invalid("missing position"))?, n(&p["character"]).ok_or_else(|| invalid("missing position"))?))
}

impl<W: Write> Server<W> {
    /// A server writing its messages to `out`.
    pub fn new(out: W) -> Server<W> {
        Server { out, workspace: Workspace::new(), initialized: false, shutdown: false }
    }

    /// Reads and handles messages until `exit` or the end of the input; the process exit code (0 after
    /// `shutdown`, 1 otherwise, as the protocol asks).
    pub fn run(&mut self, input: &mut impl BufRead) -> io::Result<i32> {
        loop {
            let msg = match rpc::read_message(input)? {
                None => return Ok(if self.shutdown { 0 } else { 1 }),
                Some(Incoming::Malformed(e)) => {
                    self.reply_error(Value::Null, code::PARSE_ERROR, &format!("the message is not JSON: {e}"))?;
                    continue;
                }
                Some(Incoming::Message(m)) => m,
            };
            if let Some(exit) = self.handle(msg)? {
                return Ok(exit);
            }
        }
    }

    fn send(&mut self, v: Value) -> io::Result<()> {
        rpc::write_message(&mut self.out, &v)
    }

    fn reply_error(&mut self, id: Value, code: i64, message: &str) -> io::Result<()> {
        self.send(json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}))
    }

    /// Handles one message; `Some(code)` when the server should exit.
    pub fn handle(&mut self, msg: Value) -> io::Result<Option<i32>> {
        let Some(method) = msg["method"].as_str().map(str::to_string) else {
            // a response to a request of ours (none are sent), or garbage
            if msg.get("id").is_none() {
                self.reply_error(Value::Null, code::INVALID_REQUEST, "not a request or notification")?;
            }
            return Ok(None);
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = msg.get("id").cloned() else {
            return self.notification(&method, &params);
        };
        if !self.initialized && method != "initialize" {
            self.reply_error(id, code::SERVER_NOT_INITIALIZED, "the server is not initialized")?;
            return Ok(None);
        }
        // a defect in a handler answers that request with an error instead of ending the session
        let r = catch_unwind(AssertUnwindSafe(|| self.request(&method, &params)))
            .unwrap_or_else(|_| Err(Failure { code: code::INTERNAL_ERROR, message: format!("internal error in {method}") }));
        match r {
            Ok(result) => self.send(json!({"jsonrpc": "2.0", "id": id, "result": result}))?,
            Err(f) => self.reply_error(id, f.code, &f.message)?,
        }
        Ok(None)
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, Failure> {
        let ws = &self.workspace;
        match method {
            "initialize" => {
                self.initialized = true;
                Ok(json!({
                    "capabilities": {
                        "textDocumentSync": {"openClose": true, "change": 1, "save": false},
                        "definitionProvider": true,
                        "hoverProvider": true,
                        "completionProvider": {"triggerCharacters": ["."], "resolveProvider": false},
                        "signatureHelpProvider": {"triggerCharacters": ["(", ","], "retriggerCharacters": [","]},
                        "documentSymbolProvider": true,
                    },
                    "serverInfo": {"name": "mc-lsp", "version": env!("CARGO_PKG_VERSION")},
                }))
            }
            "shutdown" => {
                self.shutdown = true;
                Ok(Value::Null)
            }
            "textDocument/definition" => {
                let (l, c) = position(params)?;
                Ok(ws.definition(uri(params)?, l, c))
            }
            "textDocument/hover" => {
                let (l, c) = position(params)?;
                Ok(ws.hover(uri(params)?, l, c))
            }
            "textDocument/completion" => {
                let (l, c) = position(params)?;
                Ok(ws.completion(uri(params)?, l, c))
            }
            "textDocument/signatureHelp" => {
                let (l, c) = position(params)?;
                Ok(ws.signature_help(uri(params)?, l, c))
            }
            "textDocument/documentSymbol" => Ok(ws.symbols(uri(params)?)),
            _ => Err(Failure { code: code::METHOD_NOT_FOUND, message: format!("unknown method {method}") }),
        }
    }

    fn notification(&mut self, method: &str, params: &Value) -> io::Result<Option<i32>> {
        if method == "exit" {
            return Ok(Some(if self.shutdown { 0 } else { 1 }));
        }
        if !self.initialized {
            return Ok(None);
        }
        let uri = params["textDocument"]["uri"].as_str().unwrap_or_default().to_string();
        let version = params["textDocument"]["version"].as_i64().unwrap_or(0);
        match method {
            "textDocument/didOpen" => {
                let text = params["textDocument"]["text"].as_str().unwrap_or_default().to_string();
                self.workspace.open(&uri, version, text);
            }
            "textDocument/didChange" => {
                // full synchronisation: the last change holds the whole text
                let Some(text) = params["contentChanges"].as_array().and_then(|c| c.last()).and_then(|c| c["text"].as_str()) else {
                    return Ok(None);
                };
                self.workspace.open(&uri, version, text.to_string());
            }
            "textDocument/didClose" => {
                if self.workspace.close(&uri) {
                    self.send(json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                        "params": {"uri": uri, "diagnostics": []}}))?;
                }
            }
            // `initialized`, `didSave`, `$/cancelRequest`, `$/setTrace`, ...: nothing to do
            _ => return Ok(None),
        }
        self.publish_all()?;
        Ok(None)
    }

    /// Diagnostics for every open document.
    fn publish_all(&mut self) -> io::Result<()> {
        for uri in self.workspace.uris() {
            let diagnostics = catch_unwind(AssertUnwindSafe(|| self.workspace.diagnostics(&uri))).unwrap_or_default();
            let version = self.workspace.document(&uri).map(|d| d.version);
            self.send(json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                "params": {"uri": uri, "version": version, "diagnostics": diagnostics}}))?;
        }
        Ok(())
    }
}
