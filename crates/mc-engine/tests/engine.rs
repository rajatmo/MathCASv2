//! The engine end to end: statements, results and failures, the stop flag, limits, `bind_last`, fingerprints, and
//! the same results under collection stress and at 1 and 4 threads.
use mc_api::Store;
use mc_bench::{Engine, EngineError, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// Inputs and their printed results, evaluated in order in one session.
const SESSION: &[(&str, &str)] = &[
    ("1 + 2*3", "7"),
    ("2^100 - 1", "1267650600228229401496703205375"),
    ("1/3 + 1/6", "1/2"),
    ("-6/4", "-3/2"),
    ("x + 2x", "3*x"),
    ("x*x^2*y", "y*x^3"),
    ("8^(1/2)", "2*2^(1/2)"),
    ("rule fib(0) = 0\nrule fib(1) = 1\nrule fib(k_: Int) := fib(k - 1) + fib(k - 2)\nfib(20)", "6765"),
    ("fn fact(0) = 1", "null"),
    ("fn fact(n: Int) = n*fact(n - 1)", "null"),
    ("fact(25)", "15511210043330985984000000"),
    ("map(fact, [1, 2, 3, 4])", "[1, 2, 6, 24]"),
    ("1.25~20 + 1", "2.2500000000000000000"),
    ("1/0", "failure(division_by_zero)"),
    ("1 + 1/0", "failure(division_by_zero)"),
    ("[1 < 2, 2 <= 1, 1/2 == 0.5, x == y, 3 != 3]", "[true, false, true, x == y, false]"),
    ("1 < 2 and not(3 < 2)", "true"),
    ("a = 2\nb = a^10\nb + 1", "1025"),
    // negative integer powers of machine reals (real division is a product with the -1st power)
    ("[1.0/4.0, 2.0^-3, 3.0/7.0, 2.0^-1, (-2.0)^-3, 1/4.0]", "[0.25, 0.125, 0.42857142857142855, 0.5, -0.125, 0.25]"),
    ("u = 6.0\nv = 8.0\nu/v", "0.75"),
    ("[(2/3)^-2, 2^-3, 2.0~20^-2]", "[9/4, 1/8, 0.25000000000000000000]"),
    ("", "null"),
];

#[test]
fn a_session_evaluates_statement_by_statement() {
    let mut e = CoreEngine::new();
    assert!(e.name().starts_with("mathcas-v2 "));
    for (src, want) in SESSION {
        assert_eq!(eval(&mut e, src), *want, "{src}");
    }
}

#[test]
fn results_are_the_same_under_collection_stress_and_at_1_and_4_threads() {
    let reference: Vec<String> = {
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        SESSION.iter().map(|(src, _)| eval(&mut e, src)).collect()
    };
    for threads in [1, 4] {
        let cfg = EngineConfig { threads, heap: mc_core::Config::stress(), ..EngineConfig::default() };
        let mut e = CoreEngine::with_config(cfg);
        let got: Vec<String> = SESSION.iter().map(|(src, _)| eval(&mut e, src)).collect();
        assert_eq!(got, reference, "{threads} threads under stress");
    }
}

#[test]
fn syntax_errors_evaluate_nothing() {
    let mut e = CoreEngine::new();
    let stop = StopFlag::new();
    match e.eval("z = 1\n(1 +\n 2 2)", &stop) {
        Err(EngineError::Syntax { message, offset }) => {
            assert_eq!(message, "expected `)` to close the parenthesis, found a number");
            assert_eq!(offset, 14);
        }
        other => panic!("{other:?}"),
    }
    // the first statement did not run
    assert_eq!(eval(&mut e, "z"), "z");
}

#[test]
fn bind_last_defines_the_last_result() {
    let mut e = CoreEngine::new();
    assert!(matches!(e.bind_last("result"), Err(EngineError::Other(_))));
    assert_eq!(eval(&mut e, "2^10"), "1024");
    e.bind_last("result").unwrap();
    assert_eq!(eval(&mut e, "result + 1"), "1025");
    // a failed evaluation leaves the last result in place
    assert!(eval(&mut e, "(").starts_with("error"));
    e.bind_last("again").unwrap();
    assert_eq!(eval(&mut e, "again"), "1025");
}

#[test]
fn fingerprints_are_structural() {
    let fp = |prelude: &str, src: &str| {
        let mut e = CoreEngine::new();
        eval(&mut e, prelude);
        e.eval(src, &StopFlag::new()).unwrap().fingerprint.unwrap()
    };
    let a = fp("1", "[x^2 + 1/3, f(2^80), \"s\", 1.5]");
    // other terms and symbols created first, the same value built another way
    let b = fp("[zz, q(1), 7^30, w]", "[1/3 + x*x, f(2^40*2^40), \"s\", 3/2*1.0]");
    assert_eq!(a, b);
    assert_ne!(a, fp("1", "[x^2 + 1/3, f(2^80), \"t\", 1.5]"));
    // the fingerprint depends on the structure only, so another store gives the same one (in every process)
    let st = mc_api::RefStore::new();
    let t = st.call(st.sym("list"), &[st.int(1), st.sym_term(st.sym("x"))]);
    assert_eq!(fp("1", "[1, x]"), mc_core::shapes::fingerprint(&st, t));
}

#[test]
fn a_raised_flag_stops_the_evaluation() {
    let mut e = CoreEngine::new();
    eval(&mut e, "rule spin(n_) := spin(n + 1)");
    let stop = StopFlag::new();
    // raised before the evaluation: nothing runs
    stop.raise();
    assert_eq!(e.eval("spin(0)", &stop), Err(EngineError::Stopped));
    stop.clear();
    // raised during the evaluation, from another thread
    let raiser = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_millis(100) {
                std::thread::yield_now();
            }
            stop.raise();
        })
    };
    let t0 = Instant::now();
    assert_eq!(e.eval("spin(0)", &stop), Err(EngineError::Stopped));
    assert!(t0.elapsed() < Duration::from_secs(10));
    raiser.join().unwrap();
    // the session goes on
    stop.clear();
    assert_eq!(eval(&mut e, "2 + 2"), "4");
}

#[test]
fn the_depth_limit_is_an_engine_limit() {
    let mut e = CoreEngine::new();
    eval(&mut e, "rule deep(n_) := 1 + deep(n + 1)");
    match e.eval("deep(0)", &StopFlag::new()) {
        Err(EngineError::Limit(what)) => assert_eq!(what, "evaluation depth"),
        other => panic!("{other:?}"),
    }
    assert_eq!(eval(&mut e, "2 + 2"), "4");
}

/// `+`, `*` and `^` (and so `-` and `/`) thread over lists element-wise, broadcasting non-list operands and
/// nesting; lists of different lengths give `failure(shape_mismatch)` (semantics 7.2).
#[test]
fn arithmetic_threads_over_lists() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("[1, 2, 3] + [4, 5, 6]", "[5, 7, 9]"),
        ("2*[1, 2, 3]", "[2, 4, 6]"),
        ("[1, 2]*[3, 4]", "[3, 8]"),
        ("[1, 2] + 1", "[2, 3]"),
        ("x*[a, b] + [1, 2]", "[1 + a*x, 2 + b*x]"),
        ("[[1, 2], [3, 4]] + 1", "[[2, 3], [4, 5]]"),
        ("[[1, 2], [3, 4]]*[10, 100]", "[[10, 20], [300, 400]]"),
        ("[1, 2]^2", "[1, 4]"),
        ("2^[1, 2, 3]", "[2, 4, 8]"),
        ("[1, 2] - [3, 4]", "[-2, -2]"),
        ("[1, 2]/2", "[1/2, 1]"),
        ("[1, 2]/[3, 4]", "[1/3, 1/2]"),
        ("[1.5, 2.5]*2", "[3.0, 5.0]"),
        ("[] + []", "[]"),
        ("[1, 2] + [1, 2, 3]", "failure(shape_mismatch)"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}
