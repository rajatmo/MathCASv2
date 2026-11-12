//! The messages of the kernel protocol (specified in the crate documentation): [`Request`] read from the client,
//! [`Response`] written to it, one JSON object per line.
use crate::cell::{ErrorReport, Status};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The protocol version in `hello`. It changes only when a message changes incompatibly; new optional fields and
/// new message types keep it.
pub const PROTOCOL: u32 = 1;

/// What this kernel offers beyond the first version of protocol 1, announced in `hello` (the notebook additions,
/// 08-notebook 5.2 and 5.6). A client uses an addition only when its name is listed.
pub const CAPABILITIES: &[&str] = &[
    // `eval` with `values: "each"`: a `value` for every statement shown, `output` one per `print` call
    "values_each",
    // `eval` with `numbered: true`: `n` on responses, `first` and `next` on `done`
    "numbered",
    // `out(n)`, `out()`, `out(-k)` in numbered evaluations
    "history",
    // `display` responses (`kind: "image"`) for image values, in `values: "each"` mode
    "display",
    // `display` responses with `kind: "svg"` for image values naming SVG files
    "display_svg",
    // `message` responses for warnings of library code, in `values: "each"` mode
    "message",
    // the `names` request
    "names",
    // `shutdown` with `interrupt: true` interrupts the running evaluation and cancels the queued ones
    "shutdown_interrupt",
];

/// Which values an `eval` sends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Values {
    /// the value of the last statement (protocol 1 as first published)
    #[default]
    Last,
    /// the value of every statement whose value is shown (not `null`, not followed by `;`)
    Each,
}

fn is_false(b: &bool) -> bool {
    !*b
}

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
        /// which values to send (default: the last statement's)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        values: Option<Values>,
        /// number the statements with the session's line counter and keep their values for `out(n)`
        #[serde(default, skip_serializing_if = "is_false")]
        numbered: bool,
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
    /// The defined global names that start with `prefix`, with their kinds (for colouring).
    Names {
        /// the request id
        #[serde(default)]
        id: Value,
        /// what the names start with (default: all names)
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
        /// also interrupt the running evaluation and cancel the queued ones (quitting a busy kernel); without it
        /// the evaluations read before are answered first
        #[serde(default, skip_serializing_if = "is_false")]
        interrupt: bool,
    },
}

impl Request {
    /// The id the responses carry.
    pub fn id(&self) -> Value {
        match self {
            Request::Eval { id, .. }
            | Request::Complete { id, .. }
            | Request::Names { id, .. }
            | Request::Modules { id }
            | Request::Reset { id }
            | Request::Shutdown { id, .. } => id.clone(),
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

/// A name in a `names` response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameInfo {
    /// the name (`M.f` for a public member of module `M`)
    pub name: String,
    /// `function`, `value`, `module` or `symbol`
    pub kind: String,
}

/// What a `display` response shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayKind {
    /// a PNG file
    Image,
    /// an SVG file (`width`, `height`: its size in CSS pixels, rounded)
    Svg,
}

fn none<T>(n: &Option<T>) -> bool {
    n.is_none()
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
        /// the additions this kernel offers: [`CAPABILITIES`]
        #[serde(default)]
        capabilities: Vec<String>,
    },
    /// `print` output: a line (one call in `values: "each"` mode, where the text may contain line ends).
    Output {
        /// the text, without a final line end
        text: String,
        /// the number of the statement that printed (numbered evaluations)
        #[serde(default, skip_serializing_if = "none")]
        n: Option<u64>,
    },
    /// The value of the last statement (not sent when it is `null` or a failure already sent), or in
    /// `values: "each"` mode of each statement whose value is shown.
    Value {
        /// the value in the concrete syntax
        printed: String,
        /// the statement's number (numbered evaluations)
        #[serde(default, skip_serializing_if = "none")]
        n: Option<u64>,
    },
    /// A top-level statement whose value is a failure.
    Failure {
        /// the failure in the concrete syntax
        printed: String,
        /// the statement's number (numbered evaluations)
        #[serde(default, skip_serializing_if = "none")]
        n: Option<u64>,
    },
    /// A statement's value to show as a picture (`values: "each"` mode), sent instead of its `value`.
    Display {
        /// what it is
        kind: DisplayKind,
        /// the file
        path: String,
        /// its width in pixels
        width: u32,
        /// its height in pixels
        height: u32,
        /// the value in the concrete syntax (for files and text-only clients)
        printed: String,
        /// the statement's number (numbered evaluations)
        #[serde(default, skip_serializing_if = "none")]
        n: Option<u64>,
    },
    /// A warning of library code (`values: "each"` mode; otherwise it is an `output` line `tag: text`).
    Message {
        /// the warning
        text: String,
        /// who warns: a function's name
        tag: String,
        /// the number of the statement that warned (numbered evaluations)
        #[serde(default, skip_serializing_if = "none")]
        n: Option<u64>,
    },
    /// A read error, a limit, or a malformed request.
    Error(ErrorReport),
    /// The answer to `complete`.
    Completions {
        /// the matching names, sorted
        names: Vec<String>,
    },
    /// The answer to `names`.
    Names {
        /// the matching names with their kinds, sorted by name
        names: Vec<NameInfo>,
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
        /// numbered evaluations: the first number the request used (equal to `next` when it used none)
        #[serde(default, skip_serializing_if = "none")]
        first: Option<u64>,
        /// numbered evaluations: the line counter after the request (the number the next statement gets)
        #[serde(default, skip_serializing_if = "none")]
        next: Option<u64>,
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
        // invariant: a response holds strings, numbers and lists only, which always serialise
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

    fn eval(id: Value, src: &str, cell: Option<&str>, values: Option<Values>, numbered: bool) -> Request {
        let cell = cell.map(String::from);
        Request::Eval { id, cell, src: src.into(), origin: None, line: None, values, numbered }
    }

    #[test]
    fn requests() {
        let r = parse_request(r#"{"type":"eval","id":7,"src":"1+1","cell":"a","extra":true}"#).unwrap();
        assert_eq!(r, eval(json!(7), "1+1", Some("a"), None, false));
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
    fn notebook_requests() {
        let r = parse_request(r#"{"type":"eval","id":1,"src":"1","values":"each","numbered":true}"#).unwrap();
        assert_eq!(r, eval(json!(1), "1", None, Some(Values::Each), true));
        let r = parse_request(r#"{"type":"eval","id":1,"src":"1","values":"last"}"#).unwrap();
        assert_eq!(r, eval(json!(1), "1", None, Some(Values::Last), false));
        let (_, msg) = parse_request(r#"{"type":"eval","id":1,"src":"1","values":"all"}"#).unwrap_err();
        assert!(msg.contains("unknown variant `all`"), "{msg}");
        let r = parse_request(r#"{"type":"names","id":2}"#).unwrap();
        assert_eq!(r, Request::Names { id: json!(2), prefix: "".into() });
        let r = parse_request(r#"{"type":"shutdown","interrupt":true}"#).unwrap();
        assert_eq!(r, Request::Shutdown { id: Value::Null, interrupt: true });
        let r = parse_request(r#"{"type":"shutdown"}"#).unwrap();
        assert_eq!(r, Request::Shutdown { id: Value::Null, interrupt: false });
        // requests written back as sent
        let line = serde_json::to_string(&eval(json!(3), "x", None, Some(Values::Each), true)).unwrap();
        assert_eq!(line, r#"{"type":"eval","id":3,"src":"x","values":"each","numbered":true}"#);
    }

    #[test]
    fn responses() {
        let done = |status, millis| Body::Done { status, millis, first: None, next: None };
        let r = Response { id: json!(1), cell: None, body: done(Status::Ok, 3) };
        assert_eq!(r.to_line(), r#"{"id":1,"type":"done","status":"ok","millis":3}"#);
        let r = Response { id: json!("x"), cell: Some("c1".into()), body: Body::Output { text: "hi".into(), n: None } };
        assert_eq!(r.to_line(), r#"{"id":"x","cell":"c1","type":"output","text":"hi"}"#);
        let r = Response { id: Value::Null, cell: None, body: Body::Error(ErrorReport::plain("limit: fuel")) };
        let line = r.to_line();
        assert_eq!(
            line,
            r#"{"id":null,"type":"error","message":"limit: fuel","line":null,"column":null,"origin":null,"rendered":"limit: fuel"}"#
        );
        assert_eq!(serde_json::from_str::<Response>(&line).unwrap(), r);
    }

    #[test]
    fn notebook_responses() {
        let line = |body| Response { id: json!(1), cell: None, body }.to_line();
        let value = Body::Value { printed: "2".into(), n: Some(4) };
        assert_eq!(line(value), r#"{"id":1,"type":"value","printed":"2","n":4}"#);
        assert_eq!(
            line(Body::Done { status: Status::Ok, millis: 0, first: Some(2), next: Some(4) }),
            r#"{"id":1,"type":"done","status":"ok","millis":0,"first":2,"next":4}"#
        );
        let display = Body::Display {
            kind: DisplayKind::Image,
            path: "p.png".into(),
            width: 640,
            height: 400,
            printed: "image(\"p.png\")".into(),
            n: Some(1),
        };
        assert_eq!(
            line(display.clone()),
            r#"{"id":1,"type":"display","kind":"image","path":"p.png","width":640,"height":400,"printed":"image(\"p.png\")","n":1}"#
        );
        assert_eq!(serde_json::from_str::<Response>(&line(display.clone())).unwrap().body, display);
        assert_eq!(
            line(Body::Message { text: "slow".into(), tag: "n_integrate".into(), n: None }),
            r#"{"id":1,"type":"message","text":"slow","tag":"n_integrate"}"#
        );
        let names = Body::Names { names: vec![NameInfo { name: "f".into(), kind: "function".into() }] };
        assert_eq!(line(names), r#"{"id":1,"type":"names","names":[{"name":"f","kind":"function"}]}"#);
        let mut e = ErrorReport::plain("limit: fuel");
        e.n = Some(3);
        assert!(line(Body::Error(e)).ends_with(r#""rendered":"limit: fuel","n":3}"#));
        // hello without capabilities (an older kernel) still reads
        let old = r#"{"id":null,"type":"hello","protocol":1,"version":"0","engine":"e"}"#;
        let Body::Hello { capabilities, .. } = serde_json::from_str::<Response>(old).unwrap().body else { panic!() };
        assert!(capabilities.is_empty());
    }
}
