//! A scripted kernel for tests and key scripts. It follows the protocol's shape without an engine: each line of a
//! cell is a statement; `print(x)` prints, `warn(x)` sends a message, `1/0` and `fail...` fail, `loop` runs until
//! interrupted, a trailing `;` suppresses the value, `x = ...`, `let`, `fn` define (no value), a statement ending in
//! an operator or with unbalanced brackets is a read error (nothing runs), `a + b` and `a * b` of integers compute,
//! `image("path")` is shown as a picture (a `display` response, when numbered), anything else is its own value.
use crate::eval::{KernelLink, KernelMsg, KernelRequest};
use std::collections::VecDeque;

/// The fake kernel.
#[derive(Clone, Debug)]
pub struct FakeKernel {
    /// whether it numbers statements and sends every value (a kernel with the protocol additions)
    pub numbered: bool,
    /// every request received
    pub log: Vec<KernelRequest>,
    counter: u32,
    started: bool,
    /// an evaluation that runs until interrupted: (request id, first number)
    looping: Option<(u64, u32)>,
    out: VecDeque<KernelMsg>,
}

impl FakeKernel {
    /// A kernel with the protocol additions (`numbered`) or without them.
    pub fn new(numbered: bool) -> FakeKernel {
        FakeKernel { numbered, log: vec![], counter: 1, started: false, looping: None, out: VecDeque::new() }
    }

    /// Whether the process is running.
    pub fn is_started(&self) -> bool {
        self.started
    }

    /// End the process unexpectedly (as a crash would).
    pub fn crash(&mut self) {
        self.started = false;
        self.looping = None;
        self.out.push_back(KernelMsg::Exited { why: "exit code 1".into() });
    }

    fn n(&self, n: u32) -> Option<u32> {
        self.numbered.then_some(n)
    }

    fn eval(&mut self, id: u64, src: &str) {
        let statements: Vec<&str> =
            src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        let first = self.counter;
        let bad = statements.iter().position(|s| {
            let s = s.trim_end_matches(';').trim_end();
            let depth = s.chars().fold(0i32, |d, c| d + i32::from("([{".contains(c)) - i32::from(")]}".contains(c)));
            depth != 0 || s.ends_with(['+', '-', '*', '/', '=', '^'])
        });
        if let Some(k) = bad {
            let rendered = format!("<cell>:{}:1: expected an expression\n  {}\n  ^", k + 1, statements[k]);
            self.out.push_back(KernelMsg::Error { id, rendered, n: self.n(first) });
            self.counter += u32::from(self.numbered);
            self.done(id, false, first);
            return;
        }
        let mut last = None;
        for s in statements {
            let n = self.counter;
            self.counter += 1;
            let suppressed = s.ends_with(';');
            let s = s.trim_end_matches(';').trim();
            if s == "loop" {
                self.looping = Some((id, first));
                return;
            }
            if let Some(arg) = s.strip_prefix("print(").and_then(|r| r.strip_suffix(')')) {
                self.out.push_back(KernelMsg::Output { id, text: arg.trim_matches('"').to_string(), n: self.n(n) });
                continue;
            }
            if let Some(arg) = s.strip_prefix("warn(").and_then(|r| r.strip_suffix(')')) {
                self.out.push_back(KernelMsg::Message { id, text: arg.trim_matches('"').to_string(), n: self.n(n) });
                continue;
            }
            if s == "1/0" || s.starts_with("fail") {
                let printed = "failure(division_by_zero)".to_string();
                self.out.push_back(KernelMsg::Failure { id, printed, n: self.n(n) });
                last = None;
                continue;
            }
            let definition = s.starts_with("let ") || s.starts_with("fn ") || s.contains(" = ");
            if suppressed || definition {
                last = None;
                continue;
            }
            let value = compute(s).unwrap_or_else(|| s.to_string());
            if self.numbered && mc_nbfile::images::image_path(&value).is_some() {
                // a picture: a `display` response
                self.out.push_back(KernelMsg::Display { id, printed: value, n: Some(n) });
                continue;
            }
            if self.numbered {
                self.out.push_back(KernelMsg::Value { id, printed: value, n: Some(n) });
            } else {
                last = Some(value);
            }
        }
        if let Some(printed) = last {
            self.out.push_back(KernelMsg::Value { id, printed, n: None });
        }
        if !self.numbered {
            self.counter = first;
        }
        self.done(id, false, first);
    }

    fn done(&mut self, id: u64, interrupted: bool, first: u32) {
        let (first, next) = if self.numbered { (Some(first), Some(self.counter)) } else { (None, None) };
        self.out.push_back(KernelMsg::Done { id, interrupted, first, next, millis: 0 });
    }
}

/// `a + b`, `a - b` or `a * b` of integers.
fn compute(s: &str) -> Option<String> {
    for op in ['+', '*', '-'] {
        if let Some((a, b)) = s.split_once(op) {
            let (a, b) = (a.trim().parse::<i64>().ok()?, b.trim().parse::<i64>().ok()?);
            let v = match op {
                '+' => a.checked_add(b)?,
                '*' => a.checked_mul(b)?,
                _ => a.checked_sub(b)?,
            };
            return Some(v.to_string());
        }
    }
    None
}

impl KernelLink for FakeKernel {
    fn send(&mut self, request: KernelRequest) {
        self.log.push(request.clone());
        match request {
            KernelRequest::Eval { id, src, .. } => {
                if !self.started {
                    self.started = true;
                    self.counter = 1;
                    self.out.push_back(KernelMsg::Hello { engine: "fake 0".into() });
                }
                self.eval(id, &src);
            }
            KernelRequest::Interrupt { id } => {
                if let Some((run, first)) = self.looping {
                    if run == id {
                        self.looping = None;
                        self.done(id, true, first);
                    }
                }
            }
            KernelRequest::Shutdown => {
                self.started = false;
                self.looping = None;
            }
        }
    }

    fn poll(&mut self) -> Vec<KernelMsg> {
        self.out.drain(..).collect()
    }
}
