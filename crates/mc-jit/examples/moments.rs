//! Times the mean and variance of `random_normal(0, 1, n)` draws (package P's exit), compiled against the
//! interpreter, and checks that both give the same values:
//!
//! - `moments(v)` on a packed vector made beforehand (an indexed loop, two passes), and `vl(v)` (just `length`), whose
//!   time is the cost of calling a function with that argument (spent in the evaluator before the compiled code runs);
//! - `mc(n)`, which draws the vector itself (`table(random_normal(), n)`) and reduces it with `sum` and `table`, so the
//!   whole kernel runs compiled;
//! - the interpreter: the same functions at a size it finishes, and the list operations `sum(v)`, `v - m`,
//!   `(v - m)*(v - m)` on payloads at the full size.
//!
//! ```text
//! cargo run --release -p mc-jit --example moments -- [n] [reps] [interpreted size]   (defaults: 10^7, 5, 10^5)
//! ```
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Instant;

const KERNELS: &[&str] = &[
    "fn moments(v) = { let n = length(v); var s = 0.0; for k in range(n) { s = s + v[k] }; let m = s/n; \
     var q = 0.0; for k in range(n) { q = q + (v[k] - m)^2 }; [m, q/(n - 1)] }",
    "fn vl(v) = length(v)",
    "fn mc(n: Int) = { let v = table(random_normal(), n); let m = sum(v)/n; [m, sum(table((x - m)^2, x, v))/(n - 1)] }",
];

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// An engine with the kernels, compiled or not.
fn session(compiled: bool) -> CoreEngine {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    e.configure("memo", "off").expect("the memo setting");
    for k in KERNELS {
        assert_eq!(eval(&mut e, k), "null");
    }
    if compiled {
        for f in ["moments", "vl", "mc"] {
            assert_eq!(eval(&mut e, &format!("compile({f})")), f);
        }
    }
    e
}

/// The median time of `reps` runs of `src` (each after `seed(2026)` in its own statement), and its value.
fn time(e: &mut CoreEngine, src: &str, reps: usize, label: &str) -> (f64, String) {
    let mut times = vec![];
    let mut value = String::new();
    for k in 0..reps {
        eval(e, "seed(2026)");
        let t0 = Instant::now();
        value = eval(e, src);
        times.push(t0.elapsed().as_secs_f64());
        println!("[{}/{reps} {}%] {label}: {:.4} s  {value}", k + 1, 100 * (k + 1) / reps, times[k]);
    }
    times.sort_by(f64::total_cmp);
    (times[reps / 2], value)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(10_000_000);
    let reps: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(5).max(1);
    let small: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(100_000);
    let mut comp = session(true);
    let mut plain = session(false);
    for e in [&mut comp, &mut plain] {
        let make = format!("seed(2026); v = random_normal(0, 1, {n}); w = random_normal(0, 1, {small}); length(v)");
        assert_eq!(eval(e, &make), n.to_string());
    }
    println!("n = {n}");
    let (tm, vm) = time(&mut comp, "moments(v)", reps, "compiled moments(v)");
    let (tv, _) = time(&mut comp, "vl(v)", reps, "compiled vl(v): the call alone");
    let (tc, vc) = time(&mut comp, &format!("mc({n})"), reps, "compiled mc(n)");
    let lists = "{ let m = sum(v)/length(v); [m, sum((v - m)*(v - m))/(length(v) - 1)] }";
    let (tl, _) = time(&mut plain, lists, reps.min(3), "interpreted list operations");
    // the interpreter at a size it finishes, against the compiled code
    let (ti, vi) = time(&mut plain, "moments(w)", 1, "interpreted moments(w)");
    let (tcw, vcw) = time(&mut comp, "moments(w)", reps, "compiled moments(w)");
    assert_eq!(vcw, vi, "moments at {small}");
    let (tim, vim) = time(&mut plain, &format!("mc({small})"), 1, "interpreted mc(small)");
    let (tcm, vcm) = time(&mut comp, &format!("mc({small})"), reps, "compiled mc(small)");
    assert_eq!(vcm, vim, "mc at {small}");
    assert!(comp.jit().polls() > 0);
    println!("n = {n}: compiled moments(v) {tm:.4} s {vm} (of which the call {tv:.4} s); compiled mc(n) {tc:.4} s {vc}; interpreted list operations {tl:.4} s");
    println!("n = {small}: moments interpreted {ti:.4} s, compiled {tcw:.4} s; mc interpreted {tim:.4} s, compiled {tcm:.4} s (values equal)");
}
