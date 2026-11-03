//! Failures, limits, interrupts, and the benchmark cases N1 and N12 through the evaluator.
mod common;
use common::Engine;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

#[test]
fn argument_errors_and_unreachable_precision_are_failures() {
    let mut e = Engine::new();
    assert_eq!(e.eval("n(pi, 0)"), "failure(invalid_argument, n(pi, 0))");
    assert_eq!(e.eval("n(pi, x)"), "failure(invalid_argument, n(pi, x))");
    assert_eq!(e.eval("n(pi, 10^12)"), "failure(limit, memory, n(pi, 1000000000000))");
    // an exact zero that the canonical form does not see: no precision gives a relative digit (spike 3.6)
    let z = e.eval("n(sin(1)^2 + cos(1)^2 - 1, 10)");
    assert!(z.starts_with("failure(precision_not_reached, "), "{z}");
    // a fixed-accuracy ball cannot be refined
    let b = e.eval("n(1.1~10 + pi, 30)");
    assert!(b.starts_with("failure(precision_not_reached, 4.24"), "{b}");
    // undefined numerically everywhere: log of a negative real stays a call, and n reports it
    assert_eq!(e.eval("log(-2.5~20)"), "log(-2.5000000000000000000)");
    // a failure argument is returned (strictness)
    assert_eq!(e.eval("n(1/0, 10)"), "failure(division_by_zero)");
}

#[test]
fn n_maps_through_structure_and_keeps_machine_contagion() {
    let mut e = Engine::new();
    assert_eq!(e.eval("n([pi, f(e), x + 1/4], 10)"), "[3.14159265358979, f(2.7182818284590), 0.250000000000000 + x]");
    // a machine real inside an island makes it a machine computation (contagion, 2.2)
    assert_eq!(e.eval("n(1.5 + pi, 30)"), "4.641592653589793");
    assert_eq!(e.eval("n(log(-2))"), "complex(0.6931471805599453, 3.141592653589793)");
    assert_eq!(e.eval("n(2 + 3*im, 5)"), "complex(2.000000000, 3.000000000)");
    assert_eq!(e.eval("n((-1)^(1/3))"), "complex(0.5000000000000001, 0.8660254037844386)");
    // n always gives inexact numbers
    assert_eq!(e.eval("n(7)"), "7.0");
}

#[test]
fn n1_digits_of_pi() {
    let mut e = Engine::new();
    let reference =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/cases/N/ref/N1-pi-1e4.txt")).unwrap();
    let want: String = reference.chars().filter(|c| c.is_ascii_digit()).take(10_000).collect();
    let got: String = e.eval("n(pi, 10000)").chars().filter(|c| c.is_ascii_digit()).collect();
    assert!(got.len() >= 10_000);
    assert_eq!(&got[..10_000], want);
    // the larger size against a computation at a higher precision (the cache holds the larger value)
    if !cfg!(debug_assertions) {
        let a: String = e.eval("n(pi, 100000)").chars().filter(|c| c.is_ascii_digit()).collect();
        let b: String = e.eval("n(pi, 100100)").chars().filter(|c| c.is_ascii_digit()).collect();
        assert_eq!(&a[..100_000], &b[..100_000]);
        assert_eq!(&a[..10_000], want);
    }
}

#[test]
fn n12_comparisons_that_need_refinement() {
    let mut e = Engine::new();
    let src = "[exp(pi*sqrt(163)) < 640320^3 + 744, pi^4 + pi^5 < exp(6), sqrt(1000001) - 1000 < 1/2000]";
    assert_eq!(e.eval(src), "[true, true, true]");
    // the same at 4 threads, where the list elements may be evaluated speculatively
    let mut e4 = Engine::with_threads(4);
    assert_eq!(e4.eval(src), "[true, true, true]");
}

#[test]
fn a_raised_flag_stops_a_long_computation() {
    let mut e = Engine::new();
    let (stop, ctl) = e.stopper();
    // raised before: the numeric loops end at once
    stop.store(true, Relaxed);
    let t0 = Instant::now();
    assert_eq!(e.eval("n(pi, 10000000)"), "failure(interrupted, n(pi, 10000000))");
    assert!(t0.elapsed() < Duration::from_secs(5));
    stop.store(false, Relaxed);
    // raised during, from another thread, together with the evaluator's interrupt as the engine does
    let raiser = {
        let (stop, ctl) = (stop.clone(), ctl.clone());
        std::thread::spawn(move || {
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_millis(50) {
                std::thread::yield_now();
            }
            stop.store(true, Relaxed);
            ctl.interrupt();
        })
    };
    let t0 = Instant::now();
    let r = e.eval("n(pi, 20000000)");
    raiser.join().unwrap();
    assert!(r.starts_with("failure(interrupted"), "{r}");
    assert!(t0.elapsed() < Duration::from_secs(10), "{:?}", t0.elapsed());
    stop.store(false, Relaxed);
    ctl.clear_interrupt();
    // nothing half-computed was cached
    assert_eq!(e.eval("n(pi, 20)"), "3.14159265358979323846264");
}
