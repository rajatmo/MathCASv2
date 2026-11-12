//! The benchmark cases this crate unblocks (`bench/cases`), evaluated on the assembled core with `mc-lists`
//! installed. Lambdas (item H) and `mod` (item J) are not available yet, so the cases use named functions and
//! the evaluator's `imod`; the original texts are kept below, ignored until those land.
//!
//! Debug builds run the smallest size of each case; optimised builds run every size up to 10^6, and 10^7 as well
//! with `MC_LISTS_FULL=1` (run that under the guard with a cap of 8 GB or more: S8 at 10^7 alone holds about 4 GB).
//! Each case prints its time and `[k/N pct%]` progress.
//!
//! The cases run one at a time, whatever the test runner's thread count (decision sp-core.7): each builds
//! millions of terms (S2 at 10^6 peaks at about 2.1 GB, S1, S7 and N10 at 0.6 to 0.8 GB, S8 at 0.4 GB), so run side
//! by side they passed the 3 GB cap of a guarded suite run (3.0 GB measured with three threads), and their times,
//! single-thread throughput measurements, were distorted by each other. Serialised, the binary's peak is that of
//! its largest case.
mod common;
use common::Env;
use num_bigint::BigInt;
use num_traits::{One, Zero};
use std::time::Instant;

/// The sizes to run: the first only in debug builds.
fn sizes(all: &[u64]) -> Vec<u64> {
    let full = std::env::var("MC_LISTS_FULL").is_ok_and(|v| v == "1");
    if cfg!(debug_assertions) {
        all[..1].to_vec()
    } else {
        all.iter().copied().filter(|&m| full || m <= 1_000_000).collect()
    }
}

/// Held by the case running: the cases run one at a time (see the module docs).
static ALONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `measure` with `m` bound, after `setup`, for each size; `check(m, printed)` judges the result.
fn run(name: &str, all: &[u64], setup: &str, measure: &str, check: impl Fn(u64, &str)) {
    // a case that failed (panicked holding the lock) does not stop the others
    let _alone = ALONE.lock().unwrap_or_else(|p| p.into_inner());
    let sizes = sizes(all);
    for (k, &m) in sizes.iter().enumerate() {
        let mut env = Env::new();
        env.eval(&format!("m = {m}"));
        if !setup.is_empty() {
            let r = env.eval(setup);
            assert!(!r.starts_with("status") && !r.starts_with("failure"), "{name} setup at {m}: {r}");
        }
        let t0 = Instant::now();
        let r = env.eval(measure);
        let dt = t0.elapsed();
        println!(
            "[{}/{} {}%] {name} m = {m}: {:.1} ms",
            k + 1,
            sizes.len(),
            100 * (k + 1) / sizes.len(),
            dt.as_secs_f64() * 1e3
        );
        check(m, &r);
    }
}

#[test]
fn s1_build_map_select_sum() {
    let expect = |m: u64| match m {
        100_000 => "222225555588889",
        1_000_000 => "222222555555888889",
        _ => "222222255555558888889",
    };
    run(
        "S1",
        &[100_000, 1_000_000, 10_000_000],
        "fn sq(k) = k^2\nfn p(k) = imod(k, 3) == 1",
        "range(m) |> map(sq, _) |> select(_, p) |> sum",
        |m, r| assert_eq!(r, expect(m)),
    );
}

#[test]
fn s2_sort_mixed_terms() {
    run(
        "S2",
        &[10_000, 100_000, 1_000_000],
        "fn item(k) = [k, x^imod(k, 7), f(imod(7919k, m)), string_join(\"s\", to_string(imod(k, 101)))][imod(k, 4) + 1]\n\
         xs = range(m) |> map(item, _)",
        "result = sort(xs)\nlength(result) == m and ordered(result) and sort(reverse(xs)) == result",
        |_, r| assert_eq!(r, "true"),
    );
}

#[test]
fn s6_flatten_transpose() {
    run(
        "S6",
        &[100, 316, 1000],
        "r = m\nfn row(i) = range(i*r + 1, i*r + r)\nmat = range(r) |> map(row, _)",
        "result = flatten(transpose(mat))\n\
         [length(result) == r^2, result[2] == mat[2, 1], result[r + 1] == mat[1, 2]]",
        |_, r| assert_eq!(r, "[true, true, true]"),
    );
}

#[test]
fn s7_pattern_selection() {
    run(
        "S7",
        &[10_000, 100_000, 1_000_000],
        "fn item(k) = if imod(k, 3) == 0 then f(k) else g(k)\nxs = range(m) |> map(item, _)",
        "length(cases(xs, f(k_: Int)))",
        |m, r| assert_eq!(r, (m / 3).to_string()),
    );
}

#[test]
fn s8_distinct_small_terms() {
    run(
        "S8",
        &[100_000, 1_000_000, 10_000_000],
        "fn pair(k) = h(k, k + 1)",
        "range(m) |> map(pair, _) |> length",
        |m, r| assert_eq!(r, m.to_string()),
    );
}

#[test]
fn r9_deep_nesting() {
    // debug builds have a smaller evaluation depth limit (decision G.7); the memo keeps `nest` shallow
    run("R9", &[10_000, 100_000, 1_000_000], "", "depth(nest(f, x, m))", |m, r| assert_eq!(r, (m + 1).to_string()));
}

#[test]
fn n10_harmonic_numbers() {
    let harmonic = |m: u64| {
        let (num, den) = split_sum(1, m + 1);
        let g = gcd(num.clone(), den.clone());
        format!("{}/{}", num / &g, den / &g)
    };
    run("N10", &[10_000, 100_000, 1_000_000], "fn recip(k) = 1/k", "range(m) |> map(recip, _) |> sum", |m, r| {
        if m <= 10_000 {
            assert_eq!(r, harmonic(m));
        } else {
            assert!(r.contains('/') && !r.starts_with("failure"), "{}", &r[..r.len().min(80)]);
        }
    });
}

/// The sum of 1/k for k in lo..hi as an unreduced fraction, by binary splitting.
fn split_sum(lo: u64, hi: u64) -> (BigInt, BigInt) {
    if hi - lo == 1 {
        return (BigInt::one(), BigInt::from(lo));
    }
    let mid = lo + (hi - lo) / 2;
    let (a, b) = split_sum(lo, mid);
    let (c, d) = split_sum(mid, hi);
    (a * &d + c * &b, b * d)
}

fn gcd(mut a: BigInt, mut b: BigInt) -> BigInt {
    while !b.is_zero() {
        let r = &a % &b;
        a = b;
        b = r;
    }
    a
}

/// The full engine (`mod` lives in mc-numtheory, outside this crate's test environment).
struct Full {
    engine: mc_engine::CoreEngine,
    stop: mc_api::StopFlag,
}

impl Full {
    fn eval(&mut self, src: &str) -> String {
        use mc_api::Engine;
        match self.engine.eval(src, &self.stop) {
            Ok(e) => e.printed,
            Err(err) => panic!("{src}: {err}"),
        }
    }
}

/// The original case texts, as they stand in `bench/cases` (`mod` and lambdas), on the full engine.
#[test]
fn original_texts() {
    let mut env = Full { engine: mc_engine::CoreEngine::new(), stop: mc_api::StopFlag::new() };
    assert_eq!(
        env.eval("m = 10^5\nrange(m) |> map(k => k^2, _) |> select(_, k => mod(k, 3) == 1) |> sum"),
        "222225555588889"
    );
    assert_eq!(
        env.eval("m = 10^4\nxs = range(m) |> map(k => if mod(k, 3) == 0 then f(k) else g(k), _)\nlength(cases(xs, f(k_: Int)))"),
        "3333"
    );
    assert_eq!(env.eval("m = 10^5\nrange(m) |> map(k => h(k, k + 1), _) |> length"), "100000");
    assert_eq!(env.eval("m = 10^4\n{ let t = nest(f, x, m); depth(t) }"), "10001");
    assert_eq!(
        env.eval(
            "r = 100\nmat = range(r) |> map(i => range(r) |> map(j => i*r + j, _), _)\nresult = flatten(transpose(mat))\n\
             length(result) == r^2 and result[2] == mat[2, 1] and result[r + 1] == mat[1, 2]"
        ),
        "true"
    );
}
