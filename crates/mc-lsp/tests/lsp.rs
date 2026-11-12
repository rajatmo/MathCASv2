//! The server driven through its binary with framed messages, as an editor drives it.
use mc_lsp::rpc::{read_message, write_message, Incoming};
use mc_lsp::text::path_to_uri;
use serde_json::{json, Value};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

/// How long to wait for any one message before failing (the server answers in milliseconds).
const PATIENCE: Duration = Duration::from_secs(30);

struct Client {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Value>,
    next_id: i64,
    /// notifications received while waiting for responses
    notifications: Vec<Value>,
}

impl Client {
    fn start() -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mc-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start mc-lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, incoming) = channel();
        // a reader thread, so that a silent server fails the test after PATIENCE instead of hanging it
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            while let Ok(Some(Incoming::Message(v))) = read_message(&mut r) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        });
        let mut c = Client { child, stdin, incoming, next_id: 1, notifications: vec![] };
        let init = c.request("initialize", json!({"processId": null, "rootUri": null, "capabilities": {}}));
        assert_eq!(init["capabilities"]["textDocumentSync"]["change"], 1);
        assert_eq!(init["capabilities"]["completionProvider"]["triggerCharacters"][0], ".");
        assert_eq!(init["capabilities"]["signatureHelpProvider"]["triggerCharacters"][0], "(");
        c.notify("initialized", json!({}));
        c
    }

    fn notify(&mut self, method: &str, params: Value) {
        write_message(&mut self.stdin, &json!({"jsonrpc": "2.0", "method": method, "params": params})).unwrap();
    }

    fn receive(&mut self) -> Value {
        self.incoming.recv_timeout(PATIENCE).expect("a message from the server")
    }

    /// The full response to a request.
    fn exchange(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        write_message(&mut self.stdin, &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap();
        loop {
            let m = self.receive();
            if m["id"] == id && m.get("method").is_none() {
                return m;
            }
            self.notifications.push(m);
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let r = self.exchange(method, params);
        assert!(r.get("error").is_none(), "{method}: {r}");
        r["result"].clone()
    }

    /// The diagnostics published for version `version` of `uri` (`None`: after closing it).
    fn diagnostics(&mut self, uri: &str, version: Option<i64>) -> Vec<Value> {
        let matches = |n: &Value| is_diagnostics_for(n, uri) && n["params"]["version"] == json!(version);
        if let Some(i) = self.notifications.iter().position(matches) {
            let n = self.notifications.remove(i);
            return n["params"]["diagnostics"].as_array().unwrap().clone();
        }
        loop {
            let m = self.receive();
            if matches(&m) {
                return m["params"]["diagnostics"].as_array().unwrap().clone();
            }
            self.notifications.push(m);
        }
    }

    fn open(&mut self, uri: &str, text: &str) -> Vec<Value> {
        self.notify("textDocument/didOpen", json!({"textDocument": {"uri": uri, "languageId": "mathcas", "version": 1, "text": text}}));
        self.diagnostics(uri, Some(1))
    }

    fn change(&mut self, uri: &str, version: i64, text: &str) -> Vec<Value> {
        self.notify("textDocument/didChange", json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}));
        self.diagnostics(uri, Some(version))
    }

    fn at(&mut self, method: &str, uri: &str, line: u32, character: u32) -> Value {
        self.request(method, json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}}))
    }

    /// `shutdown` then `exit`; the exit code.
    fn finish(mut self) -> i32 {
        assert_eq!(self.request("shutdown", Value::Null), Value::Null);
        self.notify("exit", Value::Null);
        drop(self.stdin);
        self.child.wait().unwrap().code().unwrap_or(-1)
    }
}

fn is_diagnostics_for(m: &Value, uri: &str) -> bool {
    m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri
}

fn range(r: &Value) -> [u64; 4] {
    let n = |v: &Value| v.as_u64().unwrap();
    [n(&r["start"]["line"]), n(&r["start"]["character"]), n(&r["end"]["line"]), n(&r["end"]["character"])]
}

/// A fresh folder for one test's files.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mc-lsp-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn uri(dir: &Path, file: &str) -> String {
    path_to_uri(&dir.join(file))
}

const GEO: &str = "# Geometry of circles.\nmodule Geo {\n  # the area of a circle of radius r\n  pub fn area(r) = pi*r^2\n  pub fn circumference(r) = 2*pi*r\n  fn helper(x) = x\n}\n";

#[test]
fn diagnostics_with_ranges() {
    let dir = folder("diagnostics");
    let mut c = Client::start();
    let main = uri(&dir, "main.mc");
    // a syntax error: strings do not multiply, on line 2
    let d = c.open(&main, "fn f(x) = x\ny = 2 \"a\"\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0]["severity"], 1);
    let r = range(&d[0]["range"]);
    assert_eq!(r[0], 1, "{d:?}");
    // fixed: no diagnostics (`2 x` is a product, round 35)
    assert!(c.change(&main, 2, "fn f(x) = x\ny = 2 x\n").is_empty());
    // a function juxtaposed with an operand: a warning with its fix as edits
    let d = c.change(&main, 3, "fn f(x) = x\ny = sin x\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0]["severity"], 2);
    assert_eq!(d[0]["data"]["fix"], "write sin(x)", "{d:?}");
    assert_eq!(d[0]["data"]["edits"].as_array().map(|a| a.len()), Some(2), "{d:?}");
    // a resolution error with its range: the private member, on line 2 at its name
    let d = c.change(&main, 4, "module M { fn hidden(x) = x }\nz = M.hidden(1)\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0]["message"].as_str().unwrap().contains("private"), "{d:?}");
    assert_eq!(range(&d[0]["range"]), [1, 4, 1, 12]);
    // an unknown module, columns counted in UTF-16 units after a non-ASCII name
    let d = c.change(&main, 5, "θ𝑥 = Nope.f(1)\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(range(&d[0]["range"]), [0, 6, 0, 12]);
    // an error inside an imported file is shown at the import
    std::fs::write(dir.join("Bad.mc"), "module Bad { pub fn f(x) = Other.g(x) }\n").unwrap();
    let d = c.change(&main, 6, "x = 1\nimport Bad\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(range(&d[0]["range"]), [1, 7, 1, 10]);
    assert!(d[0]["message"].as_str().unwrap().contains("Bad.mc:1:"), "{d:?}");
    // closing clears them
    c.notify("textDocument/didClose", json!({"textDocument": {"uri": main}}));
    assert!(c.diagnostics(&main, None).is_empty());
    assert_eq!(c.finish(), 0);
}

#[test]
fn definition_across_an_import() {
    let dir = folder("definition");
    std::fs::write(dir.join("Geo.mc"), GEO).unwrap();
    let mut c = Client::start();
    let main = uri(&dir, "main.mc");
    let src = "import Geo\nfn ring(a, b) = area(b) - area(a)\nlen = Geo.circumference(1) + ring(1, 2)\n";
    assert!(c.open(&main, src).is_empty());
    let geo = uri(&dir, "Geo.mc");

    // `area` (imported) on line 1
    let r = c.at("textDocument/definition", &main, 1, 17);
    assert_eq!(r[0]["uri"], geo.as_str(), "{r}");
    assert_eq!(range(&r[0]["range"]), [3, 9, 3, 13]);
    // `Geo.circumference` (qualified)
    let r = c.at("textDocument/definition", &main, 2, 10);
    assert_eq!(r[0]["uri"], geo.as_str());
    assert_eq!(range(&r[0]["range"]), [4, 9, 4, 22]);
    // the module name in the import
    let r = c.at("textDocument/definition", &main, 0, 8);
    assert_eq!(range(&r[0]["range"]), [1, 7, 1, 10]);
    // a definition in the same file
    let r = c.at("textDocument/definition", &main, 2, 30);
    assert_eq!(r[0]["uri"], main.as_str());
    assert_eq!(range(&r[0]["range"]), [1, 3, 1, 7]);
    // a parameter and a keyword have no definition site
    assert_eq!(c.at("textDocument/definition", &main, 1, 22), Value::Null);
    assert_eq!(c.at("textDocument/definition", &main, 0, 3), Value::Null);

    // an open, unsaved buffer of the module file wins over the file on disk
    let edited = format!("\n\n{GEO}");
    c.open(&geo, &edited);
    let r = c.at("textDocument/definition", &main, 1, 17);
    assert_eq!(range(&r[0]["range"]), [5, 9, 5, 13]);

    // while the text has a syntax error, definitions of the file itself are still found
    c.change(&main, 2, "fn ring(a, b) = a + b\nlen = ring(1, 2) +\n");
    let r = c.at("textDocument/definition", &main, 1, 8);
    assert_eq!(range(&r[0]["range"]), [0, 3, 0, 7]);
    assert_eq!(c.finish(), 0);
}

#[test]
fn hover_completion_and_symbols() {
    let dir = folder("hover");
    std::fs::write(dir.join("Geo.mc"), GEO).unwrap();
    let mut c = Client::start();
    let main = uri(&dir, "main.mc");
    let src = "import Geo.{area}\n# twice x\nfn twice(x) = 2*x\ny = twice(area(1)) + clamp(2, 0, 1) + factor(6)\nz = Geo.\n";
    c.open(&main, src);

    let text = |h: &Value| h["contents"]["value"].as_str().unwrap().to_string();
    // a definition in another file: its signature line, the comment above it, the file
    let h = c.at("textDocument/hover", &main, 3, 12);
    let t = text(&h);
    assert!(t.contains("pub fn area(r) = pi*r^2") && t.contains("the area of a circle") && t.contains("Geo.mc"), "{t}");
    assert_eq!(range(&h["range"]), [3, 10, 3, 14]);
    // a definition in this file
    let t = text(&c.at("textDocument/hover", &main, 3, 5));
    assert!(t.contains("fn twice(x) = 2*x") && t.contains("twice x"), "{t}");
    // a prelude function and a library function
    let t = text(&c.at("textDocument/hover", &main, 3, 23));
    assert!(t.contains("fn clamp(x, lo, hi)") && t.contains("prelude"), "{t}");
    // a documented library function: its usage lines from the help index and a link to its page
    let t = text(&c.at("textDocument/hover", &main, 3, 40));
    assert!(t.contains("factor(") && t.contains("](command:mathcas.openDocs?%5B%22factor%22%5D)"), "{t}");
    // nothing on a keyword
    assert_eq!(c.at("textDocument/hover", &main, 2, 1), Value::Null);

    // completion after `Geo.`: the public members only
    let labels = |r: &Value| -> Vec<String> {
        r["items"].as_array().unwrap().iter().map(|i| i["label"].as_str().unwrap().to_string()).collect()
    };
    let r = c.at("textDocument/completion", &main, 4, 8);
    let mut l = labels(&r);
    l.sort();
    assert_eq!(l, ["area", "circumference"]);
    let area = r["items"].as_array().unwrap().iter().find(|i| i["label"] == "area").unwrap();
    assert_eq!(area["detail"], "pub fn area(r) = pi*r^2");
    // after `M.` for a module of the same file, inside it all members, outside the public ones
    let local = "module M {\n  pub fn p(x) = x\n  fn r(x) = x\n  fn q(x) = M.\n}\nw = M.\n";
    c.change(&main, 2, local);
    let mut l = labels(&c.at("textDocument/completion", &main, 3, 14));
    l.sort();
    assert_eq!(l, ["p", "q", "r"]);
    assert_eq!(labels(&c.at("textDocument/completion", &main, 5, 6)), ["p"]);
    // elsewhere: keywords, library and prelude names, local definitions and imported names
    c.change(&main, 3, src);
    let l = labels(&c.at("textDocument/completion", &main, 3, 0));
    for want in ["twice", "y", "area", "Geo", "clamp", "range", "factor", "fn", "while", "true"] {
        assert!(l.iter().any(|x| x == want), "missing {want}");
    }
    assert!(!l.iter().any(|x| x == "circumference" || x == "helper"));
    // a name being typed: only the names matching it, documented ones with their one-line usage
    c.change(&main, 4, "y = n_int\n");
    let r = c.at("textDocument/completion", &main, 0, 9);
    let l = labels(&r);
    assert!(l.iter().any(|x| x == "n_integrate") && !l.iter().any(|x| x == "range"), "{l:?}");
    let item = r["items"].as_array().unwrap().iter().find(|i| i["label"] == "n_integrate").unwrap();
    assert!(item["detail"].as_str().unwrap().starts_with("n_integrate(f, [x, a, b]): "), "{item}");
    assert!(item["documentation"]["value"].as_str().unwrap().contains("command:mathcas.openDocs"), "{item}");
    c.change(&main, 5, src);

    // document symbols: functions, rules, values and modules with their members
    c.change(&main, 4, "fn f(x) = x\nrule g(a_) := a\nc = 1\nmodule M {\n  pub fn p(x) = x\n}\n");
    let s = c.request("textDocument/documentSymbol", json!({"textDocument": {"uri": main}}));
    let names: Vec<(&str, u64)> = s.as_array().unwrap().iter().map(|x| (x["name"].as_str().unwrap(), x["kind"].as_u64().unwrap())).collect();
    assert_eq!(names, [("f", 12), ("g", 12), ("c", 13), ("M", 2)]);
    assert_eq!(s[3]["children"][0]["name"], "p");
    assert_eq!(range(&s[3]["children"][0]["selectionRange"]), [4, 9, 4, 10]);
    assert_eq!(range(&s[3]["range"]), [3, 0, 5, 1]);
    assert_eq!(c.finish(), 0);
}

#[test]
fn signature_help() {
    let mut c = Client::start();
    let main = "untitled:sig";
    c.open(main, "y = n_integrate(sin(x), [x, 0, 1])\nz = table(\n");
    let sig = |c: &mut Client, line: u32, ch: u32| c.at("textDocument/signatureHelp", main, line, ch);
    // inside `sin(...)`: sin's usage, its only parameter
    let s = sig(&mut c, 0, 20);
    assert_eq!(s["signatures"][s["activeSignature"].as_u64().unwrap() as usize]["label"], "sin(x)", "{s}");
    assert_eq!(s["activeParameter"], 0);
    // after `sin(x), `: n_integrate's first usage, the second parameter `[x, a, b]` (UTF-16 offsets in the label)
    let s = sig(&mut c, 0, 24);
    let active = s["activeSignature"].as_u64().unwrap() as usize;
    let label = s["signatures"][active]["label"].as_str().unwrap().to_string();
    assert!(label.starts_with("n_integrate(f, [x, a, b]"), "{s}");
    assert_eq!(s["activeParameter"], 1);
    let p = &s["signatures"][active]["parameters"][1]["label"];
    let (a, b) = (p[0].as_u64().unwrap() as usize, p[1].as_u64().unwrap() as usize);
    assert_eq!(&label[a..b], "[x, a, b]");
    assert!(s["signatures"][active]["documentation"]["value"].as_str().unwrap().contains("numerical"), "{s}");
    // a call being typed (no closing bracket yet), and nothing outside calls
    assert!(sig(&mut c, 1, 10)["signatures"][0]["label"].as_str().unwrap().starts_with("table("));
    assert_eq!(sig(&mut c, 0, 2), Value::Null);
    assert_eq!(c.finish(), 0);
}

#[test]
fn notebooks_and_protocol_errors() {
    let mut c = Client::start();
    // a notebook: header, text and output blocks are not code
    let nb = "file:///notebook/a.mcn";
    let d = c.open(nb, "#! mathcas 2 notebook\n#% text\nWords, not code: 2 x\n#% cell\nx = 1\n#% output\n1 2 3\n#% cell\ny = x +\n");
    assert_eq!(d.len(), 1, "{d:?}");
    // at the end of the input, after `y = x +`
    assert_eq!(range(&d[0]["range"])[0], 9);
    // a document without a file: the standard modules still import
    let u = "untitled:Untitled-1";
    assert!(c.open(u, "import stats\nm = mean([1, 2])\n").is_empty());
    let t = c.at("textDocument/hover", u, 1, 5)["contents"]["value"].as_str().unwrap().to_string();
    assert!(t.contains("fn mean"), "{t}");
    // unknown methods and bad parameters are errors; the session goes on
    let r = c.exchange("textDocument/formatting", json!({"textDocument": {"uri": u}}));
    assert_eq!(r["error"]["code"], -32601);
    let r = c.exchange("textDocument/hover", json!({"textDocument": {"uri": u}}));
    assert_eq!(r["error"]["code"], -32602);
    // a request on a document that is not open has no answer
    assert_eq!(c.at("textDocument/hover", "file:///nowhere.mc", 0, 0), Value::Null);
    assert_eq!(c.finish(), 0);
}

#[test]
fn requests_before_initialize_and_exit_without_shutdown() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mc-lsp")).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    write_message(&mut stdin, &json!({"jsonrpc": "2.0", "id": 7, "method": "textDocument/hover", "params": {}})).unwrap();
    let Ok(Some(Incoming::Message(r))) = read_message(&mut out) else { panic!("no answer") };
    assert_eq!(r["id"], 7);
    assert_eq!(r["error"]["code"], -32002);
    write_message(&mut stdin, &json!({"jsonrpc": "2.0", "method": "exit"})).unwrap();
    drop(stdin);
    assert_eq!(child.wait().unwrap().code(), Some(1));
}
