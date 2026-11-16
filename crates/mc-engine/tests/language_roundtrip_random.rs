//! The language after the decisions before 1.0, tested adversarially.
//!
//! - Random expressions of every syntax form (operators of every precedence, strings with escapes, numbers of every
//!   kind, options, patterns, blocks, functions, parts) are read, evaluated and printed: the printed value read and
//!   evaluated again prints the same (a printed value is a normal form), and a held input prints as text that
//!   reads back to the same held term.
//! - Deep nesting, long lists, huge exponents and runaway recursion end with a value or a failure, never a panic,
//!   a stack overflow or an allocation past the memory limit.
//!
//! The suite runs short campaigns with fixed seeds; `MC_ROUNDTRIP_CASES=<n>` (and `MC_ROUNDTRIP_SEED`) runs a longer one,
//! printing `[k/N pct%]` progress.
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Origin};
use std::time::{Duration, Instant};

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
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

fn cases(default: usize) -> usize {
    std::env::var("MC_ROUNDTRIP_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn seed(default: u64) -> u64 {
    std::env::var("MC_ROUNDTRIP_SEED").ok().and_then(|s| s.parse::<u64>().ok()).map_or(default, |s| s.wrapping_mul(default) | 1)
}

/// Numbers of every kind the reader knows.
const NUMBERS: &[&str] = &[
    "0", "1", "7", "42", "123456789012345678901234567890", "0x1F", "0b101", "1/2", "22/7", "0.5", "1.25", "2.5e-8",
    "1.0e300", "3.0e-300", "1.25~40", "3.0~20", "2im", "3*im", "1.5*im", "1 + 2*im", "0.0", "1e16", "123.456",
    "9223372036854775807", "9223372036854775808", "-9223372036854775808", "2^64", "10^30", "2^-3", "inf", "pi", "e",
];

/// Strings with escapes and characters outside ASCII.
const STRINGS: &[&str] =
    &["\"\"", "\"a\"", "\"a\\\"b\"", "\"back\\\\slash\"", "\"tab\\tnew\\nline\"", "\"θ π ∞\"", "\"x_ + 1\"", "\"#% cell\""];

const SYMBOLS: &[&str] = &["x", "y", "z", "a", "b", "n", "f", "g"];

/// Random expression text over a fixed set of forms.
struct Gen {
    rng: Rng,
}

impl Gen {
    fn atom(&mut self) -> String {
        match self.rng.below(10) {
            0..=3 => self.rng.pick(NUMBERS).to_string(),
            4 => self.rng.pick(STRINGS).to_string(),
            5 => self.rng.pick(&["true", "false", "null", "[]", "missing()"]).to_string(),
            _ => self.rng.pick(SYMBOLS).to_string(),
        }
    }

    fn pattern(&mut self) -> String {
        let x = self.rng.pick(SYMBOLS);
        match self.rng.below(6) {
            0 => "_".into(),
            1 => format!("{x}_"),
            2 => format!("{x}__"),
            3 => format!("{x}___"),
            4 => format!("{x}_: numeric"),
            _ => format!("({x}_ when {x} > 0)"),
        }
    }

    /// A term of depth at most `d`; every operand is parenthesised when it is not an atom, so the text reads
    /// whatever the precedences are (the printer's own choice of parentheses is what is under test).
    fn term(&mut self, d: u32) -> String {
        if d == 0 || self.rng.chance(20) {
            return self.atom();
        }
        let d = d - 1;
        let sub = |g: &mut Gen| {
            let t = g.term(d);
            if t.chars().all(|c| c.is_alphanumeric() || c == '_') {
                t
            } else {
                format!("({t})")
            }
        };
        match self.rng.below(24) {
            0..=5 => {
                let op = self.rng.pick(&[
                    "+", "-", "*", "/", "^", "==", "!=", "<", "<=", ">", ">=", "and", "or", "->", ":>", "===", "!==", "|",
                ]);
                let (a, b) = (sub(self), sub(self));
                format!("{a} {op} {b}")
            }
            6 => {
                let op = self.rng.pick(&["-", "not "]);
                format!("{op}{}", sub(self))
            }
            7 | 8 => {
                let f = self.rng.pick(&[
                    "f", "g", "sin", "exp", "log", "sqrt", "abs", "max", "min", "plus", "times", "list", "length", "reverse",
                    "first", "string_join", "to_string", "expand", "factor", "together", "re", "im_part", "floor", "mod",
                    "gcd", "binomial", "is_number", "is_string", "hold", "join", "sort", "union", "total",
                ]);
                let n = self.rng.below(3) + usize::from(self.rng.chance(50));
                let args: Vec<String> = (0..n).map(|_| self.term(d)).collect();
                format!("{f}({})", args.join(", "))
            }
            9 => {
                let n = self.rng.below(4);
                let xs: Vec<String> = (0..n).map(|_| self.term(d)).collect();
                format!("[{}]", xs.join(", "))
            }
            10 => {
                let opt = self.rng.pick(&["method", "working_precision", "modulus", "assumptions", "heads", "parallel"]);
                let v = self.rng.pick(&["auto", "automatic", "none", "true", "false", "10", "x > 0", "least_squares", "[1, 2]"]);
                format!("f({}, {opt}: {v})", self.term(d))
            }
            11 => format!("quote({})", self.term(d)),
            12 => {
                let p = self.pattern();
                format!("is_match({}, {p})", self.term(d))
            }
            13 => {
                let x = self.rng.pick(SYMBOLS);
                format!("({x} => {})", self.term(d))
            }
            14 => format!("(if {} then {} else {})", sub(self), self.term(d), self.term(d)),
            15 => {
                let x = self.rng.pick(&["u", "v", "w"]);
                format!("{{ var {x} = {}; {x} }}", self.term(d))
            }
            16 => format!("assoc(\"k\" -> {}, \"j\" -> {})", self.term(d), self.term(d)),
            17 => format!("[1, 2, 3][{}]", self.rng.pick(&["1", "-1", "2", "1:2", "all", "4", "[1, 3]"])),
            18 => format!("{}'", self.rng.pick(&["f", "g", "sin"])),
            19 => format!("replace_all({}, {} -> {})", sub(self), self.rng.pick(SYMBOLS), self.term(d)),
            20 => format!("{} |> {}", sub(self), self.rng.pick(&["n", "expand", "simplify", "length", "head"])),
            21 => format!("string_join(\"s\", {})", self.rng.pick(STRINGS)),
            22 => format!("n({}, {})", sub(self), self.rng.pick(&["5", "30"])),
            _ => self.atom(),
        }
    }
}

fn engine() -> CoreEngine {
    let mut cfg = EngineConfig { threads: 1, ..EngineConfig::default() };
    cfg.eval.fuel = 2_000_000;
    CoreEngine::with_config(cfg)
}

/// The printed value of `src` in `e`, or the error text; the stop flag is raised after `limit`.
fn run(e: &mut CoreEngine, src: &str, limit: Duration) -> (String, Duration) {
    let stop = StopFlag::new();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let watch = stop.clone();
    let dog = std::thread::spawn(move || {
        if done_rx.recv_timeout(limit).is_err() {
            watch.raise();
        }
    });
    let t0 = Instant::now();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.eval_in(src, &Origin::input(), &stop)));
    let took = t0.elapsed();
    let _ = done_tx.send(());
    let _ = dog.join();
    let text = match r {
        Err(p) => format!("PANIC: {}", mc_engine::guard::payload_text(&*p)),
        Ok(Ok(s)) => s.last.printed,
        Ok(Err(err)) => format!("error: {err}"),
    };
    (text, took)
}

/// Values whose printed form need not be a normal form: effects, failures that name the input they met, and
/// machine reals printed in shortest form whose arithmetic may round differently when read again.
fn exempt(printed: &str) -> bool {
    printed.starts_with("error") || printed.contains("failure(") || printed.contains("random") || printed.contains("~")
}

/// `MC_ROUNDTRIP_PROBE=<file>`: every line of the file evaluated in one session, printed with its value and time (a tool
/// for looking at inputs by hand; does nothing in the suite).
#[test]
fn probe() {
    let Ok(path) = std::env::var("MC_ROUNDTRIP_PROBE") else { return };
    let text = std::fs::read_to_string(path).unwrap();
    // `MC_ROUNDTRIP_PROBE_MS`: the stop is raised after this many milliseconds (default 20 s)
    let limit = std::env::var("MC_ROUNDTRIP_PROBE_MS").ok().and_then(|s| s.parse().ok()).map_or(Duration::from_secs(20), Duration::from_millis);
    let mut e = engine();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (v, took) = run(&mut e, line, limit);
        println!("{line}\n   => {v}   [{} ms]", took.as_millis());
    }
}

/// A printed value read and evaluated again prints the same; a held input prints as text that reads back to the
/// same held term.
#[test]
fn printed_values_and_held_terms_read_back() {
    let n = cases(1_500);
    let mut g = Gen { rng: Rng(seed(0x44_1A06)) };
    let mut e = engine();
    let mut bad = vec![];
    let limit = Duration::from_secs(4);
    for k in 0..n {
        if n > 1_500 && (k * 100 / n) != ((k + 1) * 100 / n) {
            eprintln!("[{}/{n} {}%] bad {}", k + 1, (k + 1) * 100 / n, bad.len());
        }
        if k % 200 == 199 {
            e = engine();
        }
        let src = g.term(4);
        // balls print only their correct digits (04-syntax section 4), which read back as machine reals: an input
        // that can make one is checked for panics only
        let balls = src.contains('~') || src.contains("n(");
        // the held input
        let (held, _) = run(&mut e, &format!("quote({src})"), limit);
        if held.starts_with("PANIC") || held.contains("failure(internal") {
            bad.push(format!("{src}\n   held gives {held}"));
            continue;
        }
        if !held.starts_with("error") && !balls {
            let (again, _) = run(&mut e, &held, limit);
            if again != held {
                bad.push(format!("{src}\n   held prints {held}\n   which reads as {again}"));
            }
        }
        // the value
        let (v, _) = run(&mut e, &src, limit);
        if v.starts_with("PANIC") || v.contains("failure(internal") {
            bad.push(format!("{src}\n   gives {v}"));
            continue;
        }
        if exempt(&v) || balls || v.starts_with("graphics") {
            continue;
        }
        let (again, _) = run(&mut e, &v, limit);
        if again != v && !exempt(&again) {
            bad.push(format!("{src}\n   gives {v}\n   which evaluates to {again}"));
        }
        if bad.len() >= 12 {
            break;
        }
    }
    assert!(bad.is_empty(), "{} inputs:\n{}", bad.len(), bad.join("\n"));
}

/// Every statement in one fresh session (one worker thread unless `threads`), on a big stack: the last value.
fn ev_on(src: &str, threads: usize) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let mut e = CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
            let out = match e.eval_in(&s, &Origin::input(), &StopFlag::new()) {
                Ok(v) => v.last.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

fn ev(src: &str) -> String {
    ev_on(src, 1)
}

/// `xs[k] = v` with a failure `v`: a list holds it in its place (as `xs = [..., v, ...]` does), an association stays
/// as it was (an association built from rules propagates a failure), the failure is the value; a failure as the
/// key propagates. Before, the whole variable became the failure.
#[test]
fn a_part_assignment_reads_a_failure_as_data() {
    let g = "g = failure(boom)\n";
    assert_eq!(ev(&format!("{g}xs = [1, 2, 3]\nxs[2] = g\nxs")), "[1, failure(boom), 3]");
    // the same list written whole: the second route
    assert_eq!(ev(&format!("{g}[1, g, 3]")), "[1, failure(boom), 3]");
    assert_eq!(ev(&format!("{g}xs = [1, 2, 3]\nxs[2] = g")), "failure(boom)");
    assert_eq!(ev(&format!("{g}m = [[1, 2], [3, 4]]\nm[2, 1] = g\nm")), "[[1, 2], [failure(boom), 4]]");
    assert_eq!(ev(&format!("{g}xs = [1, 2, 3]\nxs[g] = 5\nxs")), "[1, 2, 3]");
    assert_eq!(ev(&format!("{g}r = assoc(\"a\" -> 1)\nr[\"b\"] = g\nr")), "assoc(\"a\" -> 1)");
    assert_eq!(ev(&format!("{g}r = assoc(\"a\" -> 1)\nr[\"b\"] = g\nr[\"c\"] = 3\nr")), "assoc(\"a\" -> 1, \"c\" -> 3)");
    // a `var` local
    assert_eq!(ev(&format!("{g}{{ var ys = [1, 2, 3]; ys[2] = g; ys }}")), "[1, failure(boom), 3]");
    assert_eq!(ev(&format!("{g}{{ var ys = [1, 2, 3]; check(ys[2] = g, 0); ys }}")), "[1, failure(boom), 3]");
    assert_eq!(ev(&format!("{g}{{ var a = assoc(\"a\" -> 1); check(a[\"b\"] = g, 0); a }}")), "assoc(\"a\" -> 1)");
}

/// The loop signals are control: an assignment stores nothing and ends its statement with the signal, so the loop
/// around it reads it; `match` passes it through. Checked against the same loop with the test before the
/// assignment.
#[test]
fn assignments_and_match_pass_loop_signals_on() {
    let plain = ev("{ var ys = [1, 2, 3]; for i in [1, 2, 3] { if i == 2 then break(); ys[i] = 0 }; ys }");
    assert_eq!(plain, "[0, 2, 3]");
    assert_eq!(ev("{ var ys = [1, 2, 3]; for i in [1, 2, 3] { ys[i] = if i == 2 then break() else 0 }; ys }"), plain);
    assert_eq!(ev("zs = [1, 2, 3]\nmap(i => { zs[i] = if i == 2 then break() else 0 }, [1, 2, 3])\nzs"), plain);
    assert_eq!(ev("{ var c = 5; for i in [1, 2, 3] { c = if i == 2 then break() else i }; c }"), "1");
    assert_eq!(ev("{ var s = 0; for i in range(5) { s = if i == 3 then continue() else s + i }; s }"), "12");
    assert_eq!(ev("map(i => match (if i == 2 then break() else i) { _ => 0 }, [1, 2, 3])"), "[0]");
    assert_eq!(ev("map(i => match (if i == 2 then continue() else i) { 1 => a, _ => b }, [1, 2, 3])"), "[a, b]");
}

/// `match` reads a failure as data: a `failure(...)` arm matches it; with no arm matching the failure is the value.
#[test]
fn match_reads_a_failure_as_data() {
    assert_eq!(ev("match failure(boom) { failure(t_) => t, _ => 0 }"), "boom");
    assert_eq!(ev("match failure(boom, 7) { failure(_, k_) => k + 1, _ => 0 }"), "8");
    assert_eq!(ev("match failure(boom) { failure(___) => 1, _ => 0 }"), "1");
    assert_eq!(ev("match failure(boom) { 1 => 2 }"), "failure(boom)");
    // the same arms on a list holding the failure (lists always held it as data)
    assert_eq!(ev("match [failure(boom)] { [failure(t_)] => t, _ => 0 }"), "boom");
    assert_eq!(ev("match 3 { 1 => 2 }"), "failure(no_match, 3)");
}

/// `map` over an association reads `break()` and `continue()` as over a list: the entries before the break, the
/// entry left out; another failure is the value. The second route: the same values as a list.
#[test]
fn map_over_an_association_reads_the_loop_signals() {
    let a = "assoc(\"a\" -> 1, \"b\" -> 2, \"c\" -> 3)";
    assert_eq!(ev(&format!("map(x => if x == 2 then break() else x, {a})")), "assoc(\"a\" -> 1)");
    assert_eq!(ev(&format!("map(x => if x == 2 then continue() else 10*x, {a})")), "assoc(\"a\" -> 10, \"c\" -> 30)");
    assert_eq!(ev("map(x => if x == 2 then continue() else 10*x, [1, 2, 3])"), "[10, 30]");
    assert_eq!(ev(&format!("map(x => x^2, {a})")), "assoc(\"a\" -> 1, \"b\" -> 4, \"c\" -> 9)");
    assert_eq!(ev(&format!("map(x => if x == 2 then failure(boom) else x, {a})")), "failure(boom)");
    assert_eq!(ev(&format!("reap(map(x => {{ sow(x); if x == 2 then break() else x }}, {a}))")), "[assoc(\"a\" -> 1), [[1, 2]]]");
    assert_eq!(ev(&format!("map(f, {a})")), "assoc(\"a\" -> f(1), \"b\" -> f(2), \"c\" -> f(3))");
}

/// A multi-dimensional `array` and `outer` build a structure of fixed shape: a loop signal from the function ends
/// the call and reaches the iteration around it (semantics 10), as with `sort_by`.
#[test]
fn fixed_shapes_end_on_a_loop_signal() {
    assert_eq!(ev("map(k => array((i, j) => if i == 2 then break() else i, [2, 2]), [1, 2])"), "[]");
    assert_eq!(ev("array((i, j) => if i == 2 then continue() else i, [2, 2])"), "failure(continue)");
    assert_eq!(ev("map(k => outer((a, b) => if a == 2 then break() else a*b, [1, 2], [3, 4]), [1, 2])"), "[]");
    assert_eq!(ev("map(k => sort_by([3, 1, 2], x => if x == 2 then break() else x), [1, 2])"), "[]");
    // without a signal: unchanged, and the effects in order
    assert_eq!(ev("array((i, j) => i*j, [2, 3])"), "[[1, 2, 3], [2, 4, 6]]");
    assert_eq!(ev("outer((a, b) => a*b, [1, 2], [3, 4])"), "[[3, 4], [6, 8]]");
    assert_eq!(ev("outer(f, [1, [2, 3]], [x, y])"), "[[f(1, x), f(1, y)], [[f(2, x), f(2, y)], [f(3, x), f(3, y)]]]");
    assert_eq!(ev("reap(outer((a, b) => sow(a*b), [1, 2], [3, 4]))"), "[[[3, 4], [6, 8]], [[3, 4, 6, 8]]]");
    // an ordinary failure stays an element, as in a table
    assert_eq!(ev("outer((a, b) => if a == 2 then failure(boom) else a*b, [1, 2], [3])"), "[[3], [failure(boom)]]");
}

/// The function plots call an evaluation monitor with the sample point (one argument per variable); a failure
/// from it ends the plot and is its value. The second route: a counter in a `var` agrees with the sown points, and
/// every point lies in the plot range.
#[test]
fn plots_call_the_evaluation_monitor_at_every_sample() {
    let n = ev("length(last(reap(plot(sin(x), [x, 0, 1], evaluation_monitor: (x => sow(x)))))[1])");
    let c = ev("{ var c = 0; plot(sin(x), [x, 0, 1], evaluation_monitor: (x => { c = c + 1 })); c }");
    assert_eq!(n, c);
    assert!(n.parse::<usize>().is_ok_and(|k| k >= 64), "{n}");
    assert_eq!(ev("all_true(last(reap(plot(x^2, [x, -2, 3], evaluation_monitor: (x => sow(x)))))[1], x => -2 <= x and x <= 3)"), "true");
    // two variables, one argument each
    assert_eq!(
        ev("all_true(last(reap(plot3d(x*y, [x, 0, 1], [y, 2, 3], evaluation_monitor: ((x, y) => sow([x, y])))))[1], p => p[2] >= 2)"),
        "true"
    );
    assert_eq!(ev("length(last(reap(contour_plot(x*y, [x, 0, 1], [y, 0, 1], evaluation_monitor: ((x, y) => sow(x)))))[1]) > 100"), "true");
    assert_eq!(ev("length(last(reap(parametric_plot([cos(t), sin(t)], [t, 0, 1], evaluation_monitor: (t => sow(t)))))[1]) > 64"), "true");
    // a throw ends the plot; a step monitor is unknown to a plot
    assert_eq!(ev("catch(plot(x, [x, 0, 1], evaluation_monitor: (x => if x > 1/2 then throw(\"half\"))))"), "\"half\"");
    assert_eq!(ev("plot(x, [x, 0, 1], step_monitor: (x => null))"), "failure(unknown_option, step_monitor)");
    assert!(ev("plot(x, [x, 0, 1], evaluation_monitor: (x => null))").starts_with("image("));
}

/// The Taylor method of `n_dsolve` ends its last step at the end of the range: no step passes it (before, `[t, 0,
/// 12]` stepped to 12.217 and back). The solution at the end agrees with the exact one.
#[test]
fn taylor_steps_do_not_pass_the_end() {
    for (ode, end, exact) in [
        ("[y''(t) + y(t) == 0, y(0) == 0, y'(0) == 1], y, [t, 0, 12], digits: 30", "12", "sin(12)"),
        ("[y'(t) == y(t), y(0) == 1], y, [t, 0, 12], digits: 30", "12", "exp(12)"),
        ("[y'(t) == -2*t*y(t), y(0) == 1], y, [t, 0, -5], digits: 25", "-5", "exp(-25)"),
    ] {
        let r = ev(&format!(
            "{{ let ts = last(reap(n_dsolve({ode}, step_monitor: (t => sow(t)))))[1]; all_true(ts, t => abs(t) <= abs({end}) + 10^-20) }}"
        ));
        assert_eq!(r, "true", "{ode}");
        let r = ev(&format!("{{ let s = n_dsolve({ode}); abs((y({end}) /. s) - {exact}) < 10^-15*max(1, abs(n({exact}))) }}"));
        assert_eq!(r, "true", "{ode}");
    }
}
