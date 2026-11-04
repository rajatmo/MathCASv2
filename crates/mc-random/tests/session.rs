//! The generator in a session: reproducibility, independence of the thread count, no caching, argument checks.
mod common;
use common::{numbers, Sys};

const DRAWS: &str = "[random_real(), random_real(), random_integer([1, 100]), random_normal(), random_real([2, 3], 3)]";

#[test]
fn seeded_sessions_reproduce() {
    let a = Sys::new().run(&format!("seed(11); {DRAWS}"));
    let b = Sys::new().run(&format!("seed(11); {DRAWS}"));
    assert_eq!(a, b);
    // the same session, reset
    let mut s = Sys::new();
    let first = s.run(&format!("seed(11); {DRAWS}"));
    let next = s.run(DRAWS);
    assert_ne!(first, next);
    assert_eq!(s.run(&format!("seed(11); {DRAWS}")), first);
    assert_eq!(first, a);
    // other seeds, big and negative ones too
    let mut outs = vec![a];
    for seed in ["12", "0", "-11", "2^64 + 11", "10^100"] {
        let o = s.run(&format!("seed({seed}); {DRAWS}"));
        assert!(!outs.contains(&o), "seed {seed}");
        outs.push(o);
    }
    assert_eq!(s.run(&format!("seed(10^100); {DRAWS}")), outs[5]);
    assert_eq!(s.run("seed(10^100); random_seed()"), format!("1{}", "0".repeat(100)));
}

#[test]
fn unseeded_sessions_differ_and_random_seed_reproduces() {
    let mut a = Sys::new();
    let mut b = Sys::new();
    assert_ne!(a.run(DRAWS), b.run(DRAWS));
    // a session's seed reproduces it from its start
    let mut c = Sys::new();
    let s = c.run("random_seed()");
    let first = c.run(DRAWS);
    assert_eq!(Sys::new().run(&format!("seed({s}); {DRAWS}")), first);
    // seed() takes a new seed from the operating system
    let t = c.run("seed(); random_seed()");
    assert_ne!(s, t);
    assert_ne!(c.run(DRAWS), first);
}

/// Draws in every evaluation context the evaluator may parallelise: list elements, arguments of pure calls, `map`,
/// `table`, function calls, nested groups, and work around them that can run speculatively.
const PROGRAMS: &[&str] = &[
    "[random_real(), random_real(), random_real(), random_real()]",
    "f(random_integer(9), random_integer(9), g(random_integer(9), random_integer(9)))",
    "map(x => x + random_integer(1000), range(200))",
    "map(x => if mod(x, 3) == 0 then random_real() else x^2, range(300))",
    "table(random_real([0, 10]), 100)",
    "table(i + random_integer(i), i, 100)",
    "table([i, j, random_integer(9)], i, 5, j, 5)",
    "fn r(k) = k + random_normal(); map(r, range(100))",
    "fn heavy(n) = if n < 2 then n else heavy(n - 1) + heavy(n - 2); \
     [heavy(15), random_real(), heavy(16), random_real(), heavy(14)]",
    "map(k => [heavy(10), random_integer([1, 6], 3), heavy(11)], range(30))",
    "map(xs => random_choice(xs), table(range(k), k, 50))",
    "[random_sample(range(20)), random_sample(range(20), 5), random_choice(range(20), 5)]",
    "map(k => random_integer([-2^80, 2^80]), range(40))",
    "{ var s = 0; for k in range(500) { s = s + random_integer(1) }; s }",
    // named functions with measured costs whose work is not memoised (a loop over a local): these groups do split
    // at four threads (checked with the evaluator's counters: about 100 splits and 150 holes per `map(w, ...)`),
    // the speculative elements reach the draw and leave holes, and the frontier redoes them in order
    "fn spin(k) = { var s = 0; for j in range(300) { s = s + mod(j*k, 7) }; s }; map(spin, range(64))",
    "fn w(k) = spin(k) + random_integer(10^6); map(w, range(64))",
    "map(w, range(64))",
    "fn w2(k) = if mod(k, 7) == 0 then random_real() else spin(k); map(w2, range(64))",
    "map(w2, range(64))",
    "[spin(1), random_real(), spin(2), random_real(), spin(3)]",
    "[spin(1), w(1), spin(2), w(2), spin(3), random_normal(0, 1, 3)]",
    "table(spin(i) + random_integer(9), i, 64)",
];

fn run_all(threads: usize, stress: bool) -> Vec<String> {
    let mut s = Sys::with_threads(threads, stress);
    s.run("seed(2026)");
    PROGRAMS.iter().map(|p| s.run(p)).collect()
}

#[test]
fn identical_at_one_and_four_threads() {
    let one = run_all(1, false);
    assert_eq!(one, run_all(4, false));
    assert_eq!(one, run_all(4, true));
    assert_eq!(one, run_all(1, true));
    // repeated runs at four threads, where the timing of speculation varies
    for _ in 0..3 {
        assert_eq!(one, run_all(4, false));
    }
    // the draws are those of a plain sequential loop over the same stream
    let mut s = Sys::new();
    let seq = s.run("seed(5); { var v = []; for k in range(100) { v = append(v, random_real()) }; v }");
    assert_eq!(seq, Sys::with_threads(4, false).run("seed(5); map(k => random_real(), range(100))"));
    assert_eq!(seq, Sys::with_threads(4, false).run("seed(5); table(random_real(), 100)"));
    assert_eq!(seq, Sys::with_threads(4, false).run("seed(5); random_real([0, 1], 100)"));
}

#[test]
fn draws_are_never_cached() {
    let mut s = Sys::new();
    let v = s.numbers("[random_real(), random_real()]");
    assert_ne!(v[0], v[1]);
    let v = s.numbers("fn r() = random_real(); [r(), r()]");
    assert_ne!(v[0], v[1]);
    // nor the calls around them
    let v = s.numbers("fn g(x) = x + random_real(); [g(1), g(1), g(1)]");
    assert!(v[0] != v[1] && v[1] != v[2]);
    let a = s.run("g(1)");
    assert_ne!(a, s.run("g(1)"));
    // random_seed reads the state: it follows seed
    assert_eq!(s.run("seed(3); random_seed()"), "3");
    assert_eq!(s.run("seed(4); random_seed()"), "4");
}

#[test]
fn no_draws_in_conditions() {
    let mut s = Sys::new();
    let out = s.run("rule pick(x_) := x when random_real() < 2; pick(1)");
    assert!(out.contains("effect_in_condition"), "{out}");
}

#[test]
fn values_and_shapes() {
    let mut s = Sys::new();
    s.run("seed(1)");
    assert_eq!(s.run("random_integer(0)"), "0");
    assert_eq!(s.run("random_integer([5, 5], 3)"), "[5, 5, 5]");
    assert_eq!(s.run("random_real([2, 2])"), "2.0");
    assert_eq!(s.run("random_real([0, 1], 0)"), "[]");
    assert_eq!(s.run("random_real([0, 1], [2, 0])"), "[[], []]");
    assert_eq!(s.run("random_normal(0, 1, [0, 3])"), "[]");
    assert_eq!(s.run("length(random_integer(9, [3, 4]))"), "3");
    assert_eq!(s.run("map(length, random_integer(9, [3, 4]))"), "[4, 4, 4]");
    assert_eq!(s.run("random_normal(5, 0)"), "5.0");
    assert_eq!(s.run("sort(random_sample(range(10)))"), "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]");
    assert_eq!(s.run("length(random_sample([a, b, c], 2))"), "2");
    assert_eq!(s.run("random_sample([a, b, c], 0)"), "[]");
    assert_eq!(s.run("random_choice([q])"), "q");
    assert_eq!(s.run("seed(7)"), "null");
    // ranges: exact bounds, closed expressions
    for x in s.numbers("random_real([-1/2, pi], 1000)") {
        assert!((-0.5..std::f64::consts::PI).contains(&x));
    }
    for x in s.numbers("random_real(3, 1000)") {
        assert!((0.0..3.0).contains(&x));
    }
    for x in s.numbers("random_integer([-3, 3], 1000)") {
        assert!((-3.0..=3.0).contains(&x) && x.fract() == 0.0);
    }
    // a range of 2^64 values and one past it
    assert_eq!(s.run("union(map(k => k >= -2^63 and k <= 2^63 - 1, random_integer([-2^63, 2^63 - 1], 50)))"), "[true]");
    assert_eq!(s.run("union(map(k => k >= 0 and k <= 2^64, random_integer(2^64, 50)))"), "[true]");
    assert_eq!(s.run("length(union(random_integer([10^40, 10^40 + 3], 200)))"), "4");
}

#[test]
fn bad_arguments() {
    let mut s = Sys::new();
    for (src, f) in [
        ("random_real([3, 1])", "random_real"),
        ("random_real(-1)", "random_real"),
        ("random_real([0, x])", "random_real"),
        ("random_real([0, 1], -1)", "random_real"),
        ("random_real([0, 1], 1.5)", "random_real"),
        ("random_real([0, 1], [])", "random_real"),
        ("random_real([0, 1], [10^5, 10^5])", "random_real"),
        ("random_integer(1.5)", "random_integer"),
        ("random_integer(-1)", "random_integer"),
        ("random_integer([2, 1])", "random_integer"),
        ("random_normal(0, -1)", "random_normal"),
        ("random_normal(x, 1)", "random_normal"),
        ("random_choice([])", "random_choice"),
        ("random_choice(x)", "random_choice"),
        ("random_sample([a, b], 3)", "random_sample"),
        ("seed(1.5)", "seed"),
        ("seed(x)", "seed"),
    ] {
        let out = s.run(src);
        assert!(out.starts_with(&format!("failure(argument, {f}, \"")), "{src}: {out}");
    }
    // argument counts not handled stay unevaluated
    assert_eq!(s.run("random_real(1, 2, 3)"), "random_real(1, 2, 3)");
    assert_eq!(s.run("random_normal(1)"), "random_normal(1)");
    assert_eq!(s.run("random_seed(1)"), "random_seed(1)");
    assert_eq!(s.run("seed(1, 2)"), "seed(1, 2)");
    // a failed check draws nothing
    let a = Sys::new().run("seed(9); random_real([3, 1]); random_real()");
    assert_eq!(a, Sys::new().run("seed(9); random_real()"));
    let _ = numbers("[]");
}
