//! The protocol through the built `mc-kernel` binary, spawned with pipes: `hello`, evaluation with values,
//! streamed output, failures, errors with positions, interrupts of a running and a queued evaluation, `complete`,
//! `modules`, `reset`, bad requests, `shutdown` and the end of the input.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// No single response may take longer than this (a hang fails the test instead of blocking it).
const PATIENCE: Duration = Duration::from_secs(60);

/// An evaluation that runs until it is interrupted (it polls the stop flag on every iteration).
const FOREVER: &str = "{ var i = 0; while true { i = i + 1 } }";

struct Kernel {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Kernel {
    fn start() -> Kernel {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mc-kernel"))
            .args(["--threads", "2"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("mc-kernel starts");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        let mut k = Kernel { child, stdin, lines };
        let hello = k.recv();
        assert_eq!((hello["type"].as_str(), hello["protocol"].as_u64()), (Some("hello"), Some(1)), "{hello}");
        assert_eq!(hello["id"], Value::Null);
        assert!(hello["engine"].as_str().unwrap().starts_with("mathcas-v2"), "{hello}");
        k
    }

    fn send(&mut self, request: Value) {
        self.send_line(&request.to_string());
    }

    fn send_line(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        writeln!(stdin, "{line}").expect("write a request");
        stdin.flush().expect("flush");
    }

    fn recv(&mut self) -> Value {
        let line = self.lines.recv_timeout(PATIENCE).expect("a response in time");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    /// The responses up to and including `done`, which must answer `id`.
    fn until_done(&mut self, id: Value) -> Vec<Value> {
        let mut out = vec![];
        loop {
            let r = self.recv();
            assert_eq!(r["id"], id, "response to the wrong request: {r}");
            let done = r["type"] == "done";
            out.push(r);
            if done {
                return out;
            }
        }
    }

    /// Send `request` and return its responses, up to `done`.
    fn ask(&mut self, request: Value) -> Vec<Value> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        self.send(request);
        self.until_done(id)
    }

    fn eval(&mut self, id: u64, src: &str) -> Vec<Value> {
        self.ask(json!({"type": "eval", "id": id, "src": src}))
    }

    /// Wait for the process to end; returns its exit code.
    fn wait(mut self) -> Option<i32> {
        drop(self.stdin.take());
        self.child.wait().expect("mc-kernel ends").code()
    }
}

/// `(type, main field)` of each response, for compact comparisons.
fn summary(rs: &[Value]) -> Vec<(String, Value)> {
    rs.iter()
        .map(|r| {
            let t = r["type"].as_str().unwrap().to_string();
            let v = match t.as_str() {
                "output" => r["text"].clone(),
                "value" | "failure" => r["printed"].clone(),
                "error" => r["message"].clone(),
                "done" => r["status"].clone(),
                "completions" => r["names"].clone(),
                _ => r.clone(),
            };
            (t, v)
        })
        .collect()
}

fn s(t: &str, v: Value) -> (String, Value) {
    (t.to_string(), v)
}

#[test]
fn evaluation_output_failures_and_errors() {
    let mut k = Kernel::start();
    let rs = k.ask(json!({"type": "eval", "id": 1, "cell": "c1", "src": "print(\"hi\")\nx = 6\nprint(x)\nx * 7"}));
    assert_eq!(
        summary(&rs),
        [s("output", json!("hi")), s("output", json!("6")), s("value", json!("42")), s("done", json!("ok"))]
    );
    assert!(rs.iter().all(|r| r["cell"] == "c1"), "every response names the cell");
    assert!(rs.last().unwrap()["millis"].is_u64());
    // definitions persist; a null value is not sent
    assert_eq!(summary(&k.eval(2, "y = x + 1")), [s("done", json!("ok"))]);
    assert_eq!(summary(&k.eval(3, "y")), [s("value", json!("7")), s("done", json!("ok"))]);
    // failures are reported in order and the evaluation goes on
    let rs = k.eval(4, "print(1)\n1/0\nprint(2)\nfailure(oops)\n3");
    assert_eq!(
        summary(&rs),
        [
            s("output", json!("1")),
            s("failure", json!("failure(division_by_zero)")),
            s("output", json!("2")),
            s("failure", json!("failure(oops)")),
            s("value", json!("3")),
            s("done", json!("failure")),
        ]
    );
    // a failing last statement is sent once, as a failure
    assert_eq!(
        summary(&k.eval(5, "1/0")),
        [s("failure", json!("failure(division_by_zero)")), s("done", json!("failure"))]
    );
    // a read error: nothing runs, the position is given
    let rs = k.eval(6, "print(0)\nz = 1\n1 +");
    assert_eq!(rs.len(), 2);
    let e = &rs[0];
    assert_eq!(
        (e["type"].as_str(), e["line"].as_u64(), e["column"].as_u64()),
        (Some("error"), Some(3), Some(4)),
        "{e}"
    );
    assert_eq!(e["origin"], "<input>");
    assert!(e["message"].as_str().unwrap().starts_with("expected an expression"), "{e}");
    assert!(e["rendered"].as_str().unwrap().starts_with("<input>:3:4: expected"), "{e}");
    assert_eq!(rs[1]["status"], "error");
    assert_eq!(summary(&k.eval(7, "z")), [s("value", json!("z")), s("done", json!("ok"))], "nothing ran");
    // an origin and a first line: positions in the file
    let rs = k.ask(
        json!({"type": "eval", "id": 8, "src": "w = 1\n\n  w +* 2", "origin": "notes/nb.mcn", "line": 10, "cell": "k"}),
    );
    let e = &rs[0];
    assert_eq!(
        (e["line"].as_u64(), e["column"].as_u64(), e["origin"].as_str()),
        (Some(12), Some(6), Some("notes/nb.mcn"))
    );
    assert!(e["rendered"].as_str().unwrap().starts_with("notes/nb.mcn:12:6: "), "{e}");
    // without an origin, a named cell is named in messages
    let rs = k.ask(json!({"type": "eval", "id": 9, "src": "M.f", "cell": "c9"}));
    assert_eq!(rs[0]["origin"], "<cell c9>");
    assert_eq!((rs[0]["line"].as_u64(), rs[0]["column"].as_u64()), (Some(1), Some(1)));
    // ids may be strings; a request without id gets null
    assert_eq!(summary(&k.ask(json!({"type": "eval", "id": "s", "src": "2 + 2"})))[0], s("value", json!("4")));
    assert_eq!(summary(&k.ask(json!({"type": "eval", "src": "3"})))[0], s("value", json!("3")));
    assert_eq!(summary(&k.ask(json!({"type": "shutdown", "id": 99}))), [s("done", json!("ok"))]);
    assert_eq!(k.wait(), Some(0));
}

#[test]
fn interrupts_stop_running_and_queued_evaluations() {
    let mut k = Kernel::start();
    // output is streamed: the first statement's output arrives while the loop still runs
    k.send(json!({"type": "eval", "id": 1, "src": format!("print(\"started\")\n{FOREVER}")}));
    let first = k.recv();
    assert_eq!((first["type"].as_str(), first["text"].as_str()), (Some("output"), Some("started")), "{first}");
    k.send(json!({"type": "interrupt", "id": 1}));
    assert_eq!(summary(&k.until_done(json!(1))), [s("done", json!("interrupted"))]);
    // the session goes on, its definitions intact
    assert_eq!(summary(&k.eval(2, "a = 5; a + 1")), [s("value", json!("6")), s("done", json!("ok"))]);
    // an interrupt for another id, or for a finished one, does nothing; one without id stops what runs
    k.send(json!({"type": "eval", "id": 3, "src": format!("print(\"go\")\n{FOREVER}")}));
    assert_eq!(summary(&[k.recv()]), [s("output", json!("go"))], "3 runs");
    k.send(json!({"type": "eval", "id": 4, "src": "print(\"queued\")\n7"}));
    k.send(json!({"type": "eval", "id": 5, "src": "8"}));
    k.send(json!({"type": "interrupt", "id": 2}));
    k.send(json!({"type": "interrupt", "id": 4}));
    k.send(json!({"type": "interrupt"}));
    assert_eq!(summary(&k.until_done(json!(3))), [s("done", json!("interrupted"))]);
    // 4 was cancelled while queued: it did not run
    assert_eq!(summary(&k.until_done(json!(4))), [s("done", json!("interrupted"))]);
    assert_eq!(summary(&k.until_done(json!(5))), [s("value", json!("8")), s("done", json!("ok"))]);
    // a cancelled id can be used again
    assert_eq!(summary(&k.eval(4, "9")), [s("value", json!("9")), s("done", json!("ok"))]);
    // an interrupt inside long arithmetic
    k.send(json!({"type": "eval", "id": 6, "src": "x = 3^(10^9)\n1"}));
    k.send(json!({"type": "interrupt", "id": 6}));
    assert_eq!(summary(&k.until_done(json!(6))), [s("done", json!("interrupted"))]);
    assert_eq!(summary(&k.eval(7, "a")), [s("value", json!("5")), s("done", json!("ok"))]);
    assert_eq!(k.wait(), Some(0));
}

#[test]
fn completion_modules_and_reset() {
    let mut k = Kernel::start();
    let names = |k: &mut Kernel, id: u64, prefix: &str| {
        let rs = k.ask(json!({"type": "complete", "id": id, "prefix": prefix}));
        assert_eq!(rs[1]["status"], "ok");
        rs[0]["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_string()).collect::<Vec<_>>()
    };
    // library functions (mc-numtheory, mc-poly), constants without definitions, the prelude, built-ins
    let fac = names(&mut k, 1, "fac");
    assert!(fac.contains(&"factorial".to_string()) && fac.contains(&"factor".to_string()), "{fac:?}");
    assert!(fac.windows(2).all(|w| w[0] < w[1]), "sorted: {fac:?}");
    assert!(fac.iter().all(|n| n.starts_with("fac")), "{fac:?}");
    assert!(names(&mut k, 2, "euler_g").contains(&"euler_gamma".to_string()));
    assert_eq!(names(&mut k, 3, "uniq"), ["unique", "unique_by"], "library functions");
    assert!(names(&mut k, 4, "pri").contains(&"print".to_string()), "a built-in");
    assert!(names(&mut k, 5, "my_").is_empty());
    let all = names(&mut k, 6, "");
    assert!(all.len() > 100 && all.iter().all(|n| !n.contains('.')), "{} names", all.len());
    // session definitions and modules
    k.eval(
        7,
        "my_value = 1\nfn my_fun(x) = x\nmodule geo { pub fn area(r) = pi*r^2\n fn helper(x) = x }\nimport stats",
    );
    assert_eq!(names(&mut k, 8, "my_"), ["my_fun", "my_value"]);
    assert_eq!(names(&mut k, 9, "geo"), ["geo", "geo.area"], "private members are not offered");
    assert_eq!(names(&mut k, 10, "stats.m"), ["stats.mean", "stats.median"]);
    let rs = k.ask(json!({"type": "modules", "id": 11}));
    assert_eq!(rs[0]["type"], "modules");
    assert_eq!(
        rs[0]["modules"],
        json!([
            {"name": "geo", "origin": "<input>", "public": ["area"]},
            {"name": "stats", "origin": "lib/stats.mc", "public": ["geometric_mean", "mean", "median", "std_dev", "variance"]},
        ])
    );
    // reset: a fresh session with the prelude
    assert_eq!(summary(&k.ask(json!({"type": "reset", "id": 12}))), [s("done", json!("ok"))]);
    assert!(names(&mut k, 13, "my_").is_empty());
    assert_eq!(k.ask(json!({"type": "modules", "id": 14}))[0]["modules"], json!([]));
    assert_eq!(summary(&k.eval(15, "my_value")), [s("value", json!("my_value")), s("done", json!("ok"))]);
    assert_eq!(summary(&k.eval(16, "unique([1, 1, 2])")), [s("value", json!("[1, 2]")), s("done", json!("ok"))]);
    assert_eq!(k.wait(), Some(0));
}

#[test]
fn bad_requests_and_the_end_of_input() {
    let mut k = Kernel::start();
    k.send_line("this is not json");
    let rs = k.until_done(Value::Null);
    assert_eq!(rs[0]["type"], "error");
    assert!(rs[0]["message"].as_str().unwrap().starts_with("bad request: "), "{}", rs[0]);
    assert_eq!(rs[1]["status"], "error");
    let rs = k.ask(json!({"type": "frobnicate", "id": 3}));
    assert!(rs[0]["message"].as_str().unwrap().contains("unknown variant `frobnicate`"), "{}", rs[0]);
    let rs = k.ask(json!({"type": "eval", "id": 4}));
    assert!(rs[0]["message"].as_str().unwrap().contains("missing field `src`"), "{}", rs[0]);
    // blank lines are skipped, unknown fields ignored
    k.send_line("");
    assert_eq!(summary(&k.ask(json!({"type": "eval", "id": 5, "src": "1", "future": [1]})))[0], s("value", json!("1")));
    // requests read before the end of the input are still answered
    k.send(json!({"type": "eval", "id": 6, "src": "{ var i = 0; while i < 20000 { i = i + 1 }; i }"}));
    k.send(json!({"type": "eval", "id": 7, "src": "print(\"last\")\nnull"}));
    drop(k.stdin.take());
    assert_eq!(summary(&k.until_done(json!(6))), [s("value", json!("20000")), s("done", json!("ok"))]);
    assert_eq!(summary(&k.until_done(json!(7))), [s("output", json!("last")), s("done", json!("ok"))]);
    assert_eq!(k.wait(), Some(0));
}
