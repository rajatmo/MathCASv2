//! The time of inputs on a busy machine, of large tables, and of 3D pictures.
//!
//! - Every statement was handed from the engine's thread to the session's worker and waited for (two thread
//!   wake-ups, which on a busy machine wait for a time slice each). A front end runs its loop of requests on the
//!   worker itself (`CoreEngine::on_pool`), and the steps it makes run there directly.
//! - `table` over closed ranges of machine numbers compiles its body once (`mc_jit::compile_expr`, which gives the
//!   interpreter's value exactly or declines) and builds the packed array the interpreter's nested lists are.
//! - The compiled `sqrt` of a machine real is the correctly rounded root, as the interpreter's (it was the power
//!   `x^(1/2)`, whose last bit can differ).
//!
//! Each test has a time bound (scaled by `MC_TEST_TIME_FACTOR`).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn factor() -> f64 {
    std::env::var("MC_TEST_TIME_FACTOR").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0)
}

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn steps_on_the_pool_run_on_the_calling_worker() {
    let mut e = CoreEngine::new();
    let seen: Arc<Mutex<Vec<std::thread::ThreadId>>> = Arc::default();
    let s = seen.clone();
    let caller = e.on_pool(move |e| {
        assert!(e.on_own_pool());
        assert_eq!(ev(e, "x = 2; x + 1"), "3");
        let s2 = s.clone();
        e.install(move |_, _| s2.lock().unwrap().push(std::thread::current().id()));
        // a nested call runs in place
        e.on_pool(|e| assert_eq!(ev(e, "x^10"), "1024"));
        std::thread::current().id()
    });
    assert_eq!(seen.lock().unwrap().as_slice(), &[caller]);
    // outside, the steps are handed to the pool as before
    assert!(!e.on_own_pool());
    assert_eq!(ev(&mut e, "x + 1"), "3");
}

/// `table(body, iterators)` against the same values built by nested `map`s (which the interpreter evaluates
/// element by element), as one term: packed or not, the same.
fn same_as_maps(e: &mut CoreEngine, body: &str, its: &[(&str, &str)]) {
    let table = format!("table({body}, {})", its.iter().map(|(k, r)| format!("{k}, {r}")).collect::<Vec<_>>().join(", "));
    let mut maps = body.to_string();
    for (k, r) in its.iter().rev() {
        maps = format!("map({k} => {maps}, {r})");
    }
    assert_eq!(ev(e, &format!("({table}) === ({maps})")), "true", "{table}");
}

#[test]
fn compiled_tables_are_the_interpreters_values() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    let r = "range(-3, 3, 0.1)";
    same_as_maps(&mut e, "x^2 + y^2 + z^2", &[("x", r), ("y", r), ("z", r)]);
    same_as_maps(&mut e, "sqrt(x^2 + y^2 + z^2)", &[("x", r), ("y", r), ("z", r)]);
    same_as_maps(&mut e, "sin(x) + log(x) - x/3", &[("x", "range(0.001, 10, 0.001)")]);
    same_as_maps(&mut e, "sqrt(x)", &[("x", "range(0, 10, 0.0007)")]);
    same_as_maps(&mut e, "x^(1/2) + exp(-x^2)", &[("x", "range(0, 10, 0.0007)")]);
    // integers: exact as long as they fit; a quotient that is not an integer declines (the interpreter gives
    // rationals), and so does an overflow (big integers)
    same_as_maps(&mut e, "i^2 + 3*j - 7", &[("i", "range(100)"), ("j", "range(-50, 50)")]);
    same_as_maps(&mut e, "i/2", &[("i", "range(5000)")]);
    assert_eq!(ev(&mut e, "table(i^2 + 3*j - 7, i, 100, j, 101) === table(i^2 + 3*j - 7, i, range(100), j, range(101))"), "true");
    same_as_maps(&mut e, "i^7 * j^3", &[("i", "range(1, 1000)"), ("j", "[10, 1000, 100000, 10000000, 1000000000]")]);
    // a negative argument of sqrt or log: complex values, left to the interpreter
    same_as_maps(&mut e, "sqrt(x) + log(x)", &[("x", "range(-3, 3, 0.001)")]);
    // a global value is read; a symbol without one stays symbolic
    ev(&mut e, "c0 = 2.5;");
    same_as_maps(&mut e, "c0 * x + 1", &[("x", "range(0, 10, 0.001)")]);
    same_as_maps(&mut e, "a * x + 1", &[("x", "range(0, 10, 0.001)")]);
    // a user function is inlined with its version recorded
    ev(&mut e, "g(u) := u^3 - u;");
    same_as_maps(&mut e, "g(x) + g(y)", &[("x", "range(-1, 1, 0.01)"), ("y", "range(-1, 1, 0.01)")]);
    // mixed kinds of values, ranges that depend on an outer iterator: the interpreter
    same_as_maps(&mut e, "x + 1", &[("x", "join(range(4000), [0.5])")]);
    assert_eq!(ev(&mut e, "table(i + j, i, 70, j, range(i))[3]"), "[4, 5, 6]");
    assert_eq!(ev(&mut e, "dimensions(table(x*y, x, range(0, 1, 0.01), y, range(0, 1, 0.01)))"), "[101, 101]");
    assert!(t.elapsed() < Duration::from_secs_f64(60.0 * factor()), "{:?}", t.elapsed());
    // a million values in well under a second (several seconds element by element)
    let t = Instant::now();
    let d = ev(&mut e, "dimensions(table(x^2 + y^2 + z^2, z, range(-1, 1, 0.02), y, range(-1, 1, 0.02), x, range(-1, 1, 0.02)))");
    assert_eq!(d, "[101, 101, 101]");
    assert!(t.elapsed() < Duration::from_secs_f64(2.0 * factor()), "{:?}", t.elapsed());
}

#[test]
fn large_3d_pictures_are_drawn_quickly() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    // a surface of tens of thousands of faces, clipped by its plot range
    let p = ev(&mut e, "region_plot3d(x^2 + (2*y)^2 + (3*z)^2 <= 1, [x, -1, 1], [y, -(1/2), 1/2], [z, -(1/3), 1/3])");
    assert!(p.starts_with("image(\""), "{p}");
    let q = ev(&mut e, "graphics3d(polygon(table([[i, j, 0], [i + 1, j, 0.5], [i, j + 1, 1.0]], i, 150, j, 150)), plot_range: [[0, 100], [0, 100], all])");
    assert!(q.starts_with("image(\""), "{q}");
    assert!(t.elapsed() < Duration::from_secs_f64(10.0 * factor()), "{:?}", t.elapsed());
}
