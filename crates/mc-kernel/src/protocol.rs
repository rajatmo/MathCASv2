//! The messages of the kernel protocol (specified in the crate documentation): [`Request`] read from the client,
//! [`Response`] written to it, one JSON object per line.
use crate::cell::{ErrorReport, Status};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The protocol version in `hello`. It changes only when a message changes incompatibly; new optional fields and
/// new message types keep it.
pub const PROTOCOL: u32 = 1;

/// A request from the client. Every request may carry an `id` (any JSON value; `null` when left out), which the
/// responses to it repeat. Unknown fields are ignored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Evaluate `src` in the session.
    Eval {
        /// the request id
        #[serde(default)]
        id: Value,
        /// the client's name for the cell, repeated in the responses
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cell: Option<String>,
        /// the source text
        src: String,
        /// the file the source belongs to: named in messages, imports are looked up next to it
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<String>,
        /// the 1-based line of `src`'s first line in `origin` (default 1), for error positions
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
    },
    /// Stop the evaluation with this id (running or still queued); without an id, the running one.
    Interrupt {
        /// the id of the `eval` request to stop
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<Value>,
    },
    /// The names that start with `prefix`.
    Complete {
        /// the request id
        #[serde(default)]
        id: Value,
        /// what the name starts with (may contain a `.` for module members)
        #[serde(default)]
        prefix: String,
    },
    /// The loaded modules.
    Modules {
        /// the request id
        #[serde(default)]
        id: Value,
    },
    /// A fresh session: every definition, module and import is forgotten; the prelude is evaluated again.
    Reset {
        /// the request id
        #[serde(default)]
        id: Value,
    },
    /// End the process after answering.
    Shutdown {
        /// the request id
        #[serde(default)]
        id: Value,
    },
}

impl Request {
    /// The id the responses carry.
    pub fn id(&self) -> Value {
        match self {
            Request::Eval { id, .. }
            | Request::Complete { id, .. }
            | Request::Modules { id }
            | Request::Reset { id }
            | Request::Shutdown { id } => id.clone(),
            Request::Interrupt { id } => id.clone().unwrap_or(Value::Null),
        }
    }
}

/// A loaded module in a `modules` response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleInfo {
    /// the module name
    pub name: String,
    /// where it was defined: a file path, `<repl>`, `<input>`, ...
    pub origin: String,
    /// the public members, sorted
    pub public: Vec<String>,
}

/// What a response says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Body {
    /// The first line the kernel writes.
    Hello {
        /// [`PROTOCOL`]
        protocol: u32,
        /// the kernel's version
        version: String,
        /// the engine's name and version
        engine: String,
    },
    /// A line of `print` output.
    Output {
        /// the line, without line end
        text: String,
    },
    /// The value of the last statement (not sent when it is `null` or a failure already sent).
    Value {
        /// the value in the concrete syntax
        printed: String,
    },
    /// A top-level statement whose value is a failure.
    Failure {
        /// the failure in the concrete syntax
        printed: String,
    },
    /// A read error, a limit, or a malformed request.
    Error(ErrorReport),
    /// The answer to `complete`.
    Completions {
        /// the matching names, sorted
        names: Vec<String>,
    },
    /// The answer to `modules`.
    Modules {
        /// the loaded modules, by name
        modules: Vec<ModuleInfo>,
    },
    /// The last response to every request except `interrupt`.
    Done {
        /// how the request ended (`ok` for everything but `eval` and malformed requests)
        status: Status,
        /// wall-clock milliseconds the request took
        millis: u64,
    },
}

/// A response: the id of the request it answers, the cell name of an `eval`, and the body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// the request's id (`null` for `hello` and for requests without one)
    pub id: Value,
    /// the `cell` of the `eval` request, if it had one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<String>,
    /// what the response says
    #[serde(flatten)]
    pub body: Body,
}

impl Response {
    /// The response as one line of JSON (without the line end).
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("responses serialise")
    }
}

/// A request line: the request, or the id found in the line (if any) and why the line is not a request.
pub fn parse_request(line: &str) -> Result<Request, (Value, String)> {
    serde_json::from_str(line).map_err(|e| {
        let id = serde_json::from_str::<Value>(line).ok().and_then(|v| v.get("id").cloned()).unwrap_or(Value::Null);
        (id, format!("bad request: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests() {
        let r = parse_request(r#"{"type":"eval","id":7,"src":"1+1","cell":"a","extra":true}"#).unwrap();
        assert_eq!(
            r,
            Request::Eval { id: json!(7), cell: Some("a".into()), src: "1+1".into(), origin: None, line: None }
        );
        assert_eq!(parse_request(r#"{"type":"interrupt"}"#).unwrap(), Request::Interrupt { id: None });
        assert_eq!(parse_request(r#"{"type":"reset"}"#).unwrap().id(), Value::Null);
        assert_eq!(parse_request(r#"{"type":"complete","id":"c","prefix":"ma"}"#).unwrap().id(), json!("c"));
        let (id, msg) = parse_request(r#"{"type":"evil","id":3}"#).unwrap_err();
        assert_eq!(id, json!(3));
        assert!(msg.starts_with("bad request: unknown variant `evil`"), "{msg}");
        let (id, _) = parse_request("not json").unwrap_err();
        assert_eq!(id, Value::Null);
        assert!(parse_request(r#"{"type":"eval","id":1}"#).unwrap_err().1.contains("missing field `src`"));
    }

    #[test]
    fn responses() {
        let r = Response { id: json!(1), cell: None, body: Body::Done { status: Status::Ok, millis: 3 } };
        assert_eq!(r.to_line(), r#"{"id":1,"type":"done","status":"ok","millis":3}"#);
        let r = Response { id: json!("x"), cell: Some("c1".into()), body: Body::Output { text: "hi".into() } };
        assert_eq!(r.to_line(), r#"{"id":"x","cell":"c1","type":"output","text":"hi"}"#);
        let r = Response { id: Value::Null, cell: None, body: Body::Error(ErrorReport::plain("limit: fuel")) };
        let line = r.to_line();
        assert_eq!(
            line,
            r#"{"id":null,"type":"error","message":"limit: fuel","line":null,"column":null,"origin":null,"rendered":"limit: fuel"}"#
        );
        assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), r);
    }
}
