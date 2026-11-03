//! Fuel, interrupts, and the same results at 1 and 4 threads and under collection stress.
mod common;
use common::Env;
use std::time::{Duration, Instant};

#[test]
fn long_native_loops_charge_fuel() {
    let mut env = Env::new();
    env.eval("ys = range(100000)\nzs = reverse(ys)\nfn g(k) = k");
    for src in [
        "length(range(10^6))",
        "length(sort(zs))",
        "length(flatten([ys, [ys]]))",
        "sum(ys)",
        "max(ys)",
        "length(cases(ys, _: Int))",
        "ordered(ys)",
        "depth(ys)",
        "length(select(ys, g))",
        "length(array(g, 10^5))",
        "nest(g, 1, 10^6)",
    ] {
        assert_eq!(env.eval_with(src, 1000), "status Fuel", "{src}");
        assert!(!env.eval(src).starts_with("status"), "{src}");
    }
}

#[test]
fn an_interrupt_stops_a_long_iteration() {
    let mut env = Env::new();
    env.eval("fn g(k) = imod(k + 1, 7)");
    let ctl = env.control();
    let t0 = Instant::now();
    let raiser = std::thread::spawn(move || {
        // busy-wait (no sleeping) until the evaluation is surely running
        while t0.elapsed() < Duration::from_millis(100) {
            std::hint::spin_loop();
        }
        ctl.interrupt();
    });
    let r = env.eval("nest(g, 0, 10^12)");
    raiser.join().unwrap();
    assert_eq!(r, "status Interrupted");
    assert!(t0.elapsed() < Duration::from_secs(20));
}

const SOURCES: &[&str] = &[
    "fn sq(k) = k^2\nfn odd(k) = imod(k, 2) == 1\nfn item(k) = [k, x^imod(k, 7), f(imod(7919k, 1000)), string_join(\"s\", to_string(k))][imod(k, 4) + 1]",
    "xs = range(3000) |> map(item, _)\nys = sort(xs)\n[length(ys), ordered(ys), sort(reverse(xs)) == ys, ys[1], ys[-1]]",
    "sum(select(array(sq, 20000), odd))",
    "length(cases(xs, f(_: Int)))",
    "total(transpose([range(1000), range(1001, 2000)]))",
    "to_string(take(sort(xs), 5))",
    "fold(add, 0, range(5000))",
    "depth(nest(f, x, 1000))",
];

fn session(threads: usize, heap: mc_core::Config) -> Vec<String> {
    let mut env = Env::with(threads, heap);
    SOURCES.iter().map(|s| env.eval(s)).collect()
}

#[test]
fn results_are_the_same_at_1_and_4_threads_and_under_collection_stress() {
    let reference = session(1, mc_core::Config::default());
    assert_eq!(reference[1], "[3000, true, true, 1, x^6]");
    assert_eq!(reference[3], "750");
    assert_eq!(session(4, mc_core::Config::default()), reference);
    assert_eq!(session(1, mc_core::Config::stress()), reference);
    assert_eq!(session(4, mc_core::Config::stress()), reference);
}
