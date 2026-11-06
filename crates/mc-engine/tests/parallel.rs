//! Parallel evaluation (decision sp-core.4): `parallel_table`, `parallel_map`, `parallel_sum` and `parallel: true`
//! give exactly the serial results, keep the order of effects, and stop on interrupts and limits.
use mc_bench::{Engine, EngineError, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn engine(threads: usize) -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() })
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// Definitions shared by the tests: a function whose calls take a while (a numerical integral) and one with a
/// result depending on a global.
const SETUP: &[&str] = &[
    "fn work(k) = n_integrate(sin(k*x)^2*exp(-x/k), [x, 0, 20])",
    "fn noisy(k) = { print(k); k^2 }",
];

#[test]
fn values_are_the_serial_ones() {
    for threads in [1, 3] {
        let mut e = engine(threads);
        for s in SETUP {
            eval(&mut e, s);
        }
        // the serial values first, then the parallel ones on a fresh engine (no memo shared)
        let serial = eval(&mut e, "table(work(k), k, range(1, 24))");
        let mut p = engine(threads);
        for s in SETUP {
            eval(&mut p, s);
        }
        assert_eq!(eval(&mut p, "parallel_table(work(k), k, range(1, 24))"), serial, "threads {threads}");
        let mut p = engine(threads);
        for s in SETUP {
            eval(&mut p, s);
        }
        assert_eq!(eval(&mut p, "parallel_map(work, range(1, 24))"), serial);
        assert_eq!(eval(&mut p, "map(work, range(1, 24), parallel: true)"), serial);
        assert_eq!(eval(&mut p, "table(work(k), k, range(1, 24), parallel: true)"), serial);
        assert_eq!(eval(&mut p, "parallel_sum(work(k), k, range(1, 24)) == sum(table(work(k), k, range(1, 24)))"), "true");
    }
}

#[test]
fn forms_and_options() {
    let mut e = engine(3);
    for (src, want) in [
        ("parallel_table(k^2, k, 5)", "[1, 4, 9, 16, 25]"),
        ("parallel_table(i*j, i, 2, j, 3)", "[[1, 2, 3], [2, 4, 6]]"),
        ("parallel_table(1, 3)", "[1, 1, 1]"),
        ("parallel_table(f(k), k, [a, b])", "[f(a), f(b)]"),
        ("parallel_table(k, k, range(0))", "[]"),
        ("parallel_map(f, [1, 2, 3])", "[f(1), f(2), f(3)]"),
        ("parallel_map(f, g(1, 2))", "g(f(1), f(2))"),
        ("parallel_sum(k, k, range(1, 100))", "5050"),
        ("parallel_sum(x^k, k, range(0, 2))", "1 + x + x^2"),
        ("table(k, k, 3, parallel: false)", "[1, 2, 3]"),
        ("map(f, [1, 2], parallel: false)", "[f(1), f(2)]"),
        ("map(f, [1, 2], parallel: true)", "[f(1), f(2)]"),
        // failures stay elements, as in table
        ("parallel_table(1/(k - 2), k, 3)", "[-1, failure(division_by_zero), 1]"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}

#[test]
fn effects_keep_their_order() {
    let mut e = engine(3);
    for s in SETUP {
        eval(&mut e, s);
    }
    let r = e.eval("parallel_table(noisy(k), k, 12)", &StopFlag::new()).expect("runs");
    assert_eq!(r.printed, "[1, 4, 9, 16, 25, 36, 49, 64, 81, 100, 121, 144]");
    let want: Vec<String> = (1..=12).map(|k| k.to_string()).collect();
    assert_eq!(r.output, want);
    // an assignment inside the elements happens in order: the last one wins
    assert_eq!(eval(&mut e, "{ parallel_map(k => (last = k), range(1, 50)); last }"), "50");
}

#[test]
fn limits_and_interrupts_stop_the_workers() {
    let mut e = engine(3);
    eval(&mut e, "fn spin(k) = { var n = 0; while true { n = n + k }; n }");
    let stop = StopFlag::new();
    let s2 = stop.clone();
    let t = std::thread::spawn(move || {
        // the interrupt arrives while every worker spins
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(300) {
            std::hint::spin_loop();
        }
        s2.raise();
    });
    let r = e.eval("parallel_table(spin(k), k, 6)", &stop);
    t.join().expect("the interrupting thread");
    assert!(matches!(r, Err(EngineError::Stopped)), "{r:?}");
    // the session goes on
    assert_eq!(eval(&mut e, "parallel_table(k, k, 3)"), "[1, 2, 3]");
}
