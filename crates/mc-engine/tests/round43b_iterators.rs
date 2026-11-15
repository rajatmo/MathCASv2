//! The iterating functions that read `break()` and `continue()`, called with every shape of arguments: none, one,
//! malformed ones, functions and bodies that give the loop signals (at the first element, in the middle, always,
//! nested inside another iteration), lists holding the signals as elements, and the parallel forms. The properties:
//! no call panics (a native panic is turned into `failure(internal, ...)`), no call outlives its time limit, and the
//! well-formed calls agree with the meaning of the signals (checked against the same loop written with `for`).
//!
//! The suite runs a short deterministic campaign; `MC_ITER_FUZZ_CASES=<n>` (and `MC_ITER_FUZZ_SEED`) runs a longer
//! one, printing `[k/N pct%]` progress.
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Event, Origin};
use std::time::{Duration, Instant};

/// The functions that read the loop signals (semantics 10), and `while`/`for`/`do` for comparison.
const ITERATORS: &[&str] = &[
    "map", "map_indexed", "map_thread", "array", "table", "sum", "product", "select", "select_first", "all_true", "any_true",
    "none_true", "scan", "fold", "fold_list", "nest", "nest_list", "nest_while", "nest_while_list", "fixed_point_list",
    "fixed_point", "parallel_map", "parallel_table", "count", "position", "apply", "sort_by", "gather_by", "group_by",
];

/// Functions of one or two arguments giving the signals in every place.
const FUNCTIONS: &[&str] = &[
    "(i => if i == 2 then break() else i)",
    "(i => if i == 2 then continue() else i)",
    "(i => break())",
    "(i => continue())",
    "(i => if i == 1 then break() else i)",
    "((a, b) => if b == 3 then break() else a + b)",
    "((a, b) => if b == 2 then continue() else a + b)",
    "(i => map(j => if j == 2 then break() else j, range(3)))",
    "(i => table(if j == i then continue() else j, j, 3))",
    "(i => if is_number(i) && i > 3 then break() else i > 1)",
    "(i => [break(), continue()])",
    "(i => failure(break))",
    "(i => i)",
    "(i => i + 1)",
    "add",
    "break",
    "continue",
    "break()",
    "continue()",
];

/// Other arguments: lists (some holding the signals as elements), counts, iterators, options, junk.
const ARGS: &[&str] = &[
    "range(5)", "[]", "[1, 2, 3]", "[1, break(), 3]", "[continue(), 2]", "[failure(break)]", "[[1, 2], [3, 4]]",
    "[[1, 2, 3], [4, 5, 6]]", "[[1, 2], [3]]", "0", "1", "3", "-2", "2.5", "10^6", "x", "i", "j", "\"s\"", "[i, 1, 4]",
    "[i, 4]", "[i, [3, 1, 2]]", "[i, 1, 0]", "[i, 1, x]", "i, 4", "j, 3", "parallel: true", "parallel: false", "true",
    "failure(break)", "failure(continue)", "inf", "null", "i => i > 2", "assoc(\"a\" -> 1)",
];

/// Bodies of `table`, `sum` and `product` (held, the iterator bound in them).
const BODIES: &[&str] = &[
    "if i == 2 then break() else i",
    "if i == 2 then continue() else i",
    "break()",
    "continue()",
    "if i > 2 then break() else table(if j == 2 then break() else j, j, 3)",
    "i",
    "sow(i)",
];

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

/// One random call of an iterating function.
fn random_call(rng: &mut Rng) -> String {
    let f = rng.pick(ITERATORS);
    let n = rng.below(5);
    let args: Vec<String> = (0..n)
        .map(|k| match rng.below(10) {
            0..=3 => rng.pick(FUNCTIONS).to_string(),
            4 | 5 if k == 0 => rng.pick(BODIES).to_string(),
            6 => random_call(rng),
            _ => rng.pick(ARGS).to_string(),
        })
        .collect();
    let call = format!("{f}({})", args.join(", "));
    // inside another iteration, or alone
    match rng.below(6) {
        0 => format!("map(k => {call}, range(2))"),
        1 => format!("table({call}, k, 2, parallel: true)"),
        2 => format!("reap({call})"),
        _ => call,
    }
}

/// The calls of every iterating function with no argument and with one argument of each kind.
fn degenerate_calls() -> Vec<String> {
    let mut out = vec![];
    for f in ITERATORS {
        out.push(format!("{f}()"));
        for a in FUNCTIONS.iter().chain(ARGS).chain(BODIES) {
            out.push(format!("{f}({a})"));
        }
    }
    out
}

fn engine(threads: usize) -> CoreEngine {
    let mut cfg = EngineConfig { threads, ..EngineConfig::default() };
    cfg.eval.fuel = 5_000_000;
    CoreEngine::with_config(cfg)
}

/// Evaluate every input in one session per 300 inputs; the inputs that gave an internal failure or a panic, and
/// those that took longer than the limit (with the stop flag raised at the limit by a watchdog thread).
fn campaign(inputs: &[String], threads: usize, progress: bool) -> Vec<String> {
    const LIMIT: Duration = Duration::from_secs(3);
    let mut bad = vec![];
    let mut e = engine(threads);
    for (k, src) in inputs.iter().enumerate() {
        if progress && (k * 100 / inputs.len()) != ((k + 1) * 100 / inputs.len()) {
            eprintln!("[{}/{} {}%] bad {}", k + 1, inputs.len(), (k + 1) * 100 / inputs.len(), bad.len());
        }
        if k % 300 == 299 {
            e = engine(threads);
        }
        let stop = StopFlag::new();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let watch = stop.clone();
        let dog = std::thread::spawn(move || {
            if done_rx.recv_timeout(LIMIT).is_err() {
                watch.raise();
            }
        });
        let t0 = Instant::now();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.eval_in(src, &Origin::input(), &stop)));
        let took = t0.elapsed();
        let _ = done_tx.send(());
        let _ = dog.join();
        let mut note = |what: String| bad.push(format!("{what}\n  input: {src}"));
        if took > LIMIT + Duration::from_secs(2) {
            note(format!("late stop after {took:?}"));
        }
        match r {
            Err(p) => {
                note(format!("ESCAPED: {}", mc_engine::guard::payload_text(&*p)));
                e = engine(threads);
            }
            Ok(Err(mc_engine::RunError::Engine(err))) if err.to_string().contains("internal") => note(err.to_string()),
            Ok(Ok(script)) => {
                for ev in &script.events {
                    if let Event::Failure(s) = ev {
                        if s.starts_with("failure(internal") {
                            note(s.clone());
                        }
                    }
                }
                if script.last.printed.starts_with("failure(internal") {
                    note(script.last.printed.clone());
                }
            }
            Ok(Err(_)) => {}
        }
    }
    bad
}

/// Every iterating function with no argument, one argument of each kind, and random calls (nested, parallel,
/// malformed): no panic, no internal failure, every call stops in time.
#[test]
fn iterating_functions_never_panic() {
    let cases = std::env::var("MC_ITER_FUZZ_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(600);
    let seed = std::env::var("MC_ITER_FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x17E2_2026u64);
    let mut inputs = degenerate_calls();
    let mut rng = Rng(seed.max(1));
    inputs.extend((0..cases).map(|_| random_call(&mut rng)));
    let long = cases > 600;
    let mut bad = campaign(&inputs, 1, long);
    // the parallel forms on several worker threads
    let par: Vec<String> = inputs.iter().filter(|s| s.contains("parallel")).cloned().collect();
    bad.extend(campaign(&par, 4, long));
    assert!(bad.is_empty(), "{} inputs:\n{}", bad.len(), bad.join("\n"));
}

/// `table()` and the other iterators without arguments stay unevaluated (no rule reads them), as every function
/// called with the wrong number of arguments does; with one argument they stay unevaluated too.
#[test]
fn iterators_without_arguments_stay_unevaluated() {
    let mut e = engine(1);
    let mut ev = |s: &str| e.eval_in(s, &Origin::input(), &StopFlag::new()).map(|r| r.last.printed).unwrap_or_else(|err| err.to_string());
    assert_eq!(ev("table()"), "table()");
    assert_eq!(ev("parallel_table()"), "parallel_table()");
    assert_eq!(ev("table(parallel: true)"), "table(parallel: true)");
    assert_eq!(ev("table(i)"), "table(i)");
    // a well-formed call next to the malformed ones: the meaning of the signals, checked against a `for` loop
    assert_eq!(ev("table(if i == 3 then break() else i^2, i, 6)"), "[1, 4]");
    assert_eq!(ev("{ var out = []; for i in range(6) { if i == 3 then break(); out = append(out, i^2) }; out }"), "[1, 4]");
    assert_eq!(ev("parallel_table(if i == 3 then continue() else i, i, 5)"), "[1, 2, 4, 5]");
}
