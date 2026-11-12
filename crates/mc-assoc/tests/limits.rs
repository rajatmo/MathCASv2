//! Fuel: lookups do not scan (the fuel they charge does not grow with the association), and long native loops stop
//! at the fuel limit. Determinism: one session gives the same results at 1, 2 and 4 threads and under collection
//! stress, where every statement ends in a collection that moves terms (and empties the index cache).
mod common;
use mc_core::Session;
use mc_eval::{EvalConfig, Kernel, Status};
use mc_syntax::Printer;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A session of the core with the arithmetic, `mc-lists` and `mc-assoc` (no prelude), reporting fuel.
struct Env {
    session: Session<Kernel>,
}

impl Env {
    fn new() -> Env {
        let stop = Arc::new(AtomicBool::new(false));
        let session = Session::with_state(mc_core::Config::default(), 1, |h| {
            let mut k = Kernel::new(h, EvalConfig::default());
            mc_engine::arith::install(&mut k, h, &stop);
            mc_lists::install(&mut k, h, &stop);
            mc_assoc::install(&mut k, h);
            k
        });
        Env { session }
    }

    /// Evaluate the single statement `src` with this fuel budget: its printed value or status, and the fuel used.
    fn eval(&mut self, src: &str, fuel: i64) -> (String, u64) {
        self.session.step(|h, k| {
            let ts = mc_syntax::read(h, src).unwrap_or_else(|d| panic!("{src}: {}", d[0].message));
            assert_eq!(ts.len(), 1, "{src}");
            let e = k.eval_with(h, ts[0], fuel);
            let shown = if e.status == Status::Running {
                Printer::with_numbers(h, Box::new(mc_engine::format::ArithNumbers)).expr(e.value)
            } else {
                format!("status {:?}", e.status)
            };
            (shown, e.stats.fuel)
        })
    }
}

/// The fuel of 2000 lookups (and of the loop around them) is the same in an association of 100 keys and in one of
/// 10^5 keys, apart from building the index once; scanning to the last keys would cost about 2000 * 10^5 / 16 more.
#[test]
fn lookups_do_not_scan() {
    let lookups = |n: u64| {
        let mut env = Env::new();
        env.eval(&format!("a = assoc(map(i => i -> i, range({n})))"), i64::MAX);
        // one statement: the index built by the first lookup serves the others; the keys are the last 100, which a
        // scan would reach last
        let src = format!("{{ var s = 0; for i in range(2000) {{ s = s + lookup(a, {n} - imod(i, 100)) }}; s }}");
        let (v, fuel) = env.eval(&src, i64::MAX);
        assert_eq!(v, (2000 * n - 99_000).to_string());
        fuel
    };
    let (small, large) = (lookups(100), lookups(100_000));
    let build = 100_000 / 16;
    // and at most one check of the stored association itself: whether its normal-form stamp survives the definition
    // of `a` depends on which symbols share a mask bit (`index % 63`), so adding built-ins can make the first read
    // re-check it once; a scan would cost 2000 * 10^5 / 16, far above this allowance
    let recheck = 100_000 + 100;
    assert!(large <= small + build + recheck + 1000, "fuel {small} for 100 keys, {large} for 10^5 keys");
}

#[test]
fn long_native_loops_charge_fuel() {
    let mut env = Env::new();
    env.eval("ys = range(100000)", i64::MAX);
    env.eval("big = assoc(map(i => i -> i, ys))", i64::MAX);
    env.eval("fn g(k) = k", i64::MAX);
    for src in [
        "length(union(ys, ys))",
        "length(intersection(ys, ys))",
        "length(complement(ys, ys))",
        "is_subset(ys, ys)",
        "length(assoc(map(i => i -> i, ys)))",
        "length(counts(ys))",
        "length(group_by(ys, g))",
        "length(merge([big, big]))",
        "length(merge([big, big], g))",
        "length(map_values(g, big))",
        "length(select_keys(big, g))",
        "length(key_drop(big, ys))",
        "length(key_take(big, ys))",
        "length(key_sort(big))",
        "length(keys(key_drop(big, 1)))",
    ] {
        assert_eq!(env.eval(src, 1000).0, "status Fuel", "{src}");
        assert!(!env.eval(src, i64::MAX).0.starts_with("status"), "{src}");
    }
}

/// Statements over associations large enough to be indexed, sets, and failures.
#[test]
fn dataset_loops_charge_fuel() {
    let mut env = Env::new();
    env.eval("ys = range(100000)", i64::MAX);
    env.eval("ds = dataset(assoc(\"k\" -> map(i => imod(i, 97), ys), \"v\" -> ys))", i64::MAX);
    env.eval("fn g(r) = r", i64::MAX);
    env.eval("fn yes(r) = true", i64::MAX);
    for src in [
        "length(dataset(map(i => assoc(a -> i), ys)))",
        "length(dataset(map(i => [i], ys), [a]))",
        "length(rows(ds))",
        "length(reverse(ds))",
        "length(drop(ds, 1))",
        "length(select(ds, yes))",
        "length(sort_by(ds, \"k\"))",
        "length(sort_by(ds, g))",
        "length(group_by(ds, \"k\"))",
        "length(aggregate(ds, \"k\", [\"v\" -> total]))",
        "length(aggregate(ds, [\"v\" -> mean]))",
        "length(pivot(ds, \"v\", \"k\", \"v\", total))",
        "length(join_on(ds, ds, \"v\"))",
        "length(join(ds, ds))",
        "length(add_column(ds, \"w\", g))",
        "length(ds[1:100000])",
    ] {
        assert_eq!(env.eval(src, 1000).0, "status Fuel", "{src}");
        assert!(!env.eval(src, i64::MAX).0.starts_with("status"), "{src}");
    }
}

/// Dataset statements for the thread and stress comparison.
const DATASETS: &[&str] = &[
    "ds = dataset(map(i => assoc(\"k\" -> imod(i * 7, 13), \"v\" -> i, \"w\" -> 1/(i + 1)), range(3000)))",
    "aggregate(ds, \"k\", [\"v\" -> total, \"w\" -> max, count])[\"v\"]",
    "column(sort_by(ds, r => imod(r[\"v\"] * 31, 1009)), \"v\")[1:5]",
    "length(select(ds, r => r[\"k\"] > 6))",
    "map_values(length, group_by(ds, \"k\"))",
    "column(pivot(take(ds, 50), \"k\", \"k\", \"v\", total), 3)",
    "length(join_on(ds, take(ds, 100), \"k\"))",
    "total(column(add_column(ds, \"z\", r => r[\"v\"] + r[\"k\"]), \"z\"))",
];

#[test]
fn datasets_same_at_any_thread_count_and_under_stress() {
    let run = |threads: usize, stress: bool| {
        let mut e = common::engine(threads, stress);
        DATASETS.iter().map(|s| common::eval(&mut e, s)).collect::<Vec<String>>()
    };
    let reference = run(1, false);
    assert_eq!(reference[3], "1385");
    for (threads, stress) in [(1, true), (2, false), (4, true)] {
        assert_eq!(run(threads, stress), reference, "{threads} threads, stress {stress}");
    }
}

const SESSION: &[&str] = &[
    "a = assoc(map(i => i -> i^2, range(300)))",
    "lookup(a, 17)",
    "b = insert(a, 1000, x)",
    "[lookup(b, 1000), lookup(a, 1000, none), length(b)]",
    "c = assoc(map(i => k(i) -> i, range(200)))",
    "sum(map(i => c[k(i)], range(200)))",
    "d = merge([a, assoc(map(i => i -> -1, range(250, 400)))], total)",
    "[lookup(d, 260), lookup(d, 399), lookup(d, 10), length(d)]",
    "g = group_by(range(1000), i => imod(i, 13))",
    "[length(g), length(lookup(g, 0)), lookup(g, 12)[1]]",
    "counts(map(i => imod(i * i, 11), range(500)))",
    "key_sort(assoc(map(i => imod(i * 7, 31) -> i, range(30))))",
    "union(map(i => imod(i * i, 97), range(500)), range(90, 110))",
    "intersection(map(i => imod(i * 3, 101), range(400)), range(0, 300, 2))",
    "complement(range(100), map(i => imod(i * i, 101), range(100)))",
    "lookup(a, 17) + lookup(b, 300) + lookup(c, k(5))",
    "is_failure(lookup(c, k(201)))",
    "map_values(v => v + 1, key_take(a, [3, 1, 2]))",
];

#[test]
fn same_results_at_any_thread_count_and_under_stress() {
    let run = |threads: usize, stress: bool| {
        let mut e = common::engine(threads, stress);
        SESSION.iter().map(|s| common::eval(&mut e, s)).collect::<Vec<String>>()
    };
    let reference = run(1, false);
    assert_eq!(reference[1], "289");
    assert_eq!(reference[7], "[67599, -1, 100, 400]");
    assert_eq!(reference[16], "true");
    for (threads, stress) in [(1, true), (2, false), (2, true), (4, false), (4, true)] {
        assert_eq!(run(threads, stress), reference, "{threads} threads, stress {stress}");
    }
}
