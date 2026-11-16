//! Hangs that were left: stops at random moments.
//!
//! - A stop raised at a random moment into a heavy input of every area (special functions at high precision,
//!   hypergeometric and Appell functions, numerical solvers, algebra, calculus, sums, solving, decompositions,
//!   linear algebra, plots, lists) ends the evaluation within 300 ms (scaled by `MC_TEST_TIME_FACTOR`). The suite
//!   stops each input once, at a moment drawn from a fixed seed; `MC_HANG_STOPS=<n>` stops each at `n` moments
//!   (`MC_HANG_SEED` changes them), printing `[k/N pct%]` progress; `MC_HANG_ONLY=a;;b` runs only those inputs.
//! - Inputs that were slow: `sign` of a complex constant, `count_roots` of `gamma(z) - z - 1` on a square, within a
//!   bound of time, with their values.
//! - Functions that walked a shared argument as a tree (the rule index, layouts, plots and the solvers): their
//!   inputs on deep shared terms end promptly after a stop.
//!
//! A stop is late when the evaluation ends after the allowance on the clock and also kept working: it used more
//! processor time after the stop than the allowance per worker thread, or ran twenty allowances on the clock (a
//! loaded machine stretches the clock, not the work). The tests run one at a time for that measure.
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Origin};
use std::time::{Duration, Instant};

#[path = "support/process_cpu.rs"]
mod process_cpu;
use process_cpu::{late, process_cpu, serial};

/// What one stopped evaluation gave: the printed value, the time on the clock from the stop to the end and the
/// processor time used in it (both `None` when it ended before the stop), and the whole time.
struct Stopped {
    value: String,
    since: Option<Duration>,
    cpu_since: Option<Duration>,
    took: Duration,
}

/// Inputs that take seconds or more, one or more per area.
const HEAVY: &[&str] = &[
    "n(appell_f1(2, 1/3, 1/5, 7/2, 9/10, 19/20), 400)",
    "n(appell_f1(1/2, 1/3, 1/5, 7/2, -9/10, 19/20), 300)",
    "n(hypergeometric_pfq([1/3, 1/5, 1/7], [2/3, 3/5], 1/2), 4000)",
    "n(hypergeometric_2f1(1/3, 2/5, 3/7, 99/100), 6000)",
    "n(hypergeometric_pfq([1/2, 1/3, 1/4, 1/5], [2/3, 3/4, 4/5], 1 - 10^-6), 300)",
    "n(appell_f1(1/3, 2/3, 1/5, 1/2, 3/4 + im/2, -2), 100)",
    "plot3d(re(appell_f1(1/2, 1/3, 1/4, 5/2, x, y)), [x, -2, 0.9], [y, -2, 0.9], plot_points: 30)",
    "n(bessel_k(7/3, 10^4 + 10^3*im), 2000)",
    "n(bessel_j(100, 1000), 3000)",
    "n(zeta(1/2 + 10^5*im), 60)",
    "n(polylog(3, 1/3 + im), 4000)",
    "n(lerch_phi(1/2, 3, 1/3), 3000)",
    "n(elliptic_k(1/3), 200000)",
    "n(struve_h(1/3, 50), 3000)",
    "n(gamma(1/3 + 7*im), 30000)",
    "n(exp(pi*sqrt(163)), 10^6)",
    "n(meijer_g([[1/2], []], [[0, 1/3], []], 2), 2000)",
    "n_integrate(sin(x^2)*exp(-x/100), [x, 0, inf], working_precision: 40)",
    "find_minimum(sin(x) + cos(3*x) + x^2/1000, [x, 1], working_precision: 300)",
    "n_solve(x^60 - 3*x^7 + 1 == 0, x, working_precision: 400)",
    "apart(1/((x^2 + 1)^25*(x - 3)^20*(x + 2)^15), x)",
    "factor(x^210 - 1)",
    "integrate(log(1 + x^2)*atan(x)/x^3, x)",
    "series(gamma(x)^gamma(x), [x, 0, 24])",
    "sum(binomial(3*k, k)^2/30^k, [k, 0, inf])",
    "solve([x^3 + y^3 + z^3 == 3, x + y + z == 3, x*y*z == 1/2], [x, y, z])",
    "cylindrical_decomposition(x^2 + y^2 + z^2 < 1 and x*y*z > 1/100, [x, y, z])",
    "factor_integer(10^70 + 33)",
    "inverse(table(1/(i + j + 1) + i, [i, 70], [j, 70]))",
    "matrix_exp(table(i - j, [i, 30], [j, 30])/7)",
    "d_solve(y''(x) + x^3*y'(x) + y(x) == 0, y(x), x)",
    "plot(sum(sin(k*x)/k, [k, 1, 400]), [x, 0, 10], plot_points: 4000)",
    "region_plot(sin(x*y) > cos(x + y^2), [x, -9, 9], [y, -9, 9], plot_points: 300)",
    "length(sort(table(mod(i^7, 1000003), [i, 3*10^6])))",
    "count_roots(gamma(z) - z - 1, [z, -6 - 6*im, 6 + 6*im])",
    "full_simplify(sum(binomial(n, k)*binomial(n + k, k)*(-1)^k, [k, 0, n]))",
];

fn time_factor() -> f64 {
    std::env::var("MC_TEST_TIME_FACTOR").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0)
}

fn engine(threads: usize) -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() })
}

/// Evaluate `src` with the stop flag raised `after` from the start.
fn stopped(e: &mut CoreEngine, src: &str, after: Duration) -> Stopped {
    let stop = StopFlag::new();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let watch = stop.clone();
    let dog = std::thread::spawn(move || match done_rx.recv_timeout(after) {
        Ok(()) => None,
        Err(_) => {
            let cpu = process_cpu();
            watch.raise();
            Some((Instant::now(), cpu))
        }
    });
    let t0 = Instant::now();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.eval_in(src, &Origin::input(), &stop)));
    let end = Instant::now();
    let cpu_end = process_cpu();
    let _ = done_tx.send(());
    let raised = dog.join().ok().flatten();
    let value = match r {
        Err(p) => format!("PANIC: {}", mc_engine::guard::payload_text(&*p)),
        Ok(Ok(s)) => s.last.printed,
        Ok(Err(err)) => format!("error: {err}"),
    };
    let since = raised.map(|(r, _)| end.saturating_duration_since(r));
    let cpu_since = raised.and_then(|(_, c)| Some(cpu_end?.saturating_sub(c?)));
    Stopped { value, since, cpu_since, took: end - t0 }
}

/// [`stopped`] as the value, the time after the stop and the whole time.
fn stopped_at(e: &mut CoreEngine, src: &str, after: Duration) -> (String, Option<Duration>, Duration) {
    let s = stopped(e, src, after);
    (s.value, s.since, s.took)
}

/// Every heavy input, stopped at a random moment between 50 ms and 1.5 s: it ends within 300 ms of the stop,
/// with an interrupted failure or (when it finished first) its value; never a panic.
#[test]
fn a_stop_ends_every_heavy_input_promptly() {
    let _one = serial();
    let stops: usize = std::env::var("MC_HANG_STOPS").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut rng = std::env::var("MC_HANG_SEED").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0x4646_2026) | 1;
    let only: Option<Vec<String>> = std::env::var("MC_HANG_ONLY").ok().map(|s| s.split(";;").map(str::to_string).collect());
    let inputs: Vec<String> = only.unwrap_or_else(|| HEAVY.iter().map(|s| s.to_string()).collect());
    let allowed = Duration::from_secs_f64(0.3 * time_factor());
    let total = inputs.len() * stops;
    let mut late_stops = vec![];
    let mut e = engine(THREADS as usize);
    for (k, src) in inputs.iter().flat_map(|s| std::iter::repeat_n(s, stops)).enumerate() {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let after = Duration::from_millis(50 + rng % 1450);
        let Stopped { value: v, since, cpu_since, took } = stopped(&mut e, src, after);
        if stops > 1 {
            eprintln!("[{}/{total} {}%] {src} at {after:?}: {since:?}, processor {cpu_since:?} (all {took:?})", k + 1, (k + 1) * 100 / total);
        }
        assert!(!v.starts_with("PANIC") && !v.contains("failure(internal"), "{src}: {v}");
        if let Some(d) = since {
            if late(d, cpu_since, allowed, THREADS) {
                late_stops.push(format!(
                    "{src}: stopped at {after:?}, ended {d:?} later using {cpu_since:?} of processor time (all {took:?}): {}",
                    v.chars().take(80).collect::<String>()
                ));
            }
        }
        // a fresh session now and then: a stopped evaluation leaves nothing behind that a later one depends on
        if k % 8 == 7 {
            e = engine(THREADS as usize);
        }
    }
    assert!(late_stops.is_empty(), "{} late stops:\n{}", late_stops.len(), late_stops.join("\n"));
}

/// Worker threads of the session that the heavy inputs are stopped in.
const THREADS: u32 = 4;

/// The measure of a late stop: work that goes on after the stop is late, a wait for the processor is not, and a
/// wait past twenty allowances is late whatever it used.
#[test]
fn a_stop_is_late_when_the_work_goes_on() {
    let _one = serial();
    let allowed = Duration::from_millis(100);
    let (t0, c0) = (Instant::now(), process_cpu());
    let mut x = 0u64;
    while t0.elapsed() < Duration::from_millis(400) {
        x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
    }
    let cpu = process_cpu().zip(c0).map(|(c1, c0)| c1.saturating_sub(c0));
    if cpu.is_some() {
        assert!(late(t0.elapsed(), cpu, allowed, 1), "busy {cpu:?}");
    }
    let (t1, c1) = (Instant::now(), process_cpu());
    let (_tx, rx) = std::sync::mpsc::channel::<()>();
    let _ = rx.recv_timeout(Duration::from_millis(400));
    let idle = process_cpu().zip(c1).map(|(c2, c1)| c2.saturating_sub(c1));
    if idle.is_some() {
        assert!(!late(t1.elapsed(), idle, allowed, 1), "waiting {idle:?}");
    }
    assert!(late(allowed * 21, Some(Duration::ZERO), allowed, 1));
    assert!(!late(allowed, Some(allowed * 10), allowed, 1));
}

/// Inputs that were slow: each within its bound of time, with its value. `sign` of a complex constant refined its
/// real comparison to the cap (20 s); `count_roots` on a square covered by boxes of gamma (9 s). The bound holds on
/// the clock or, on a loaded machine, for the processor time the process used.
#[test]
fn slow_inputs_are_fast() {
    let _one = serial();
    let mut e = engine(2);
    for (src, want, secs) in [
        ("sign(elliptic_k(2))", "sign(elliptic_k(2))", 3.0),
        ("sign(elliptic_k(2) + 1) == sign(elliptic_k(2) + 1)", "true", 3.0),
        ("count_roots(gamma(z) - z - 1, [z, -2 - 2*im, 2 + 2*im])", "3", 6.0),
        ("count_roots(gamma(z) - 2, [z, -3 - 2*im, 4 + 2*im])", "4", 4.0),
        // the same region cut at re z = 1/2 (gamma(1/2 + iy) is never 2: |gamma(1/2 + iy)| <= gamma(1/2) < 2): the
        // counts add up; the box around the root z = 3 (gamma(3) = 2! = 2) holds that root alone
        ("count_roots(gamma(z) - 2, [z, -3 - 2*im, 1/2 + 2*im]) + count_roots(gamma(z) - 2, [z, 1/2 - 2*im, 4 + 2*im])", "4", 4.0),
        ("count_roots(gamma(z) - 2, [z, 2 - im, 4 + im])", "1", 2.0),
        ("n(gamma(2 + im), 30)", "complex(0.6529654964201667278386462479460847, 0.3430658398165453575887359869783115)", 3.0),
        // Gamma(-3/2 + 5 i) by the reflection formula pi/(sin(pi z) Gamma(1 - z)), an independent route
        ("abs(n(gamma(-3/2 + 5*im), 40) - n(pi/(sin(pi*(-3/2 + 5*im))*gamma(5/2 - 5*im)), 40)) < 10^-35", "true", 3.0),
    ] {
        let (t0, c0) = (Instant::now(), process_cpu());
        let (v, _, _) = stopped_at(&mut e, src, Duration::from_secs(120));
        let took = t0.elapsed();
        let cpu = process_cpu().zip(c0).map(|(c1, c0)| c1.saturating_sub(c0));
        assert_eq!(v, want, "{src}");
        let bound = secs * time_factor();
        assert!(took.as_secs_f64() < bound || cpu.is_some_and(|c| c.as_secs_f64() < bound), "{src}: {took:?}, processor {cpu:?}");
    }
}

/// Definitions, layouts, plots, properties and solvers of a deep shared argument end within 300 ms of a stop raised
/// 200 ms in; a definition with a large shared literal is stored, found, replaced and ordered as any other.
#[test]
fn shared_arguments_end_promptly() {
    let _one = serial();
    let allowed = Duration::from_secs_f64(0.3 * time_factor());
    let mut e = engine(2);
    for f in [
        "f(dd) = 3; f(dd)",
        "register_exception_type(dd)",
        "grid(dd)",
        "pane(dd)",
        "labeled(dd, x)",
        "rasterize(dd)",
        "function_continuous(dd, x)",
        "function_meromorphic(dd, x)",
        "function_singularities(dd, x)",
        "solve(dd, x)",
        "reduce(dd, x)",
        "n_roots(dd, x)",
        "partial_fraction_elements(dd, x)",
        "difference_delta(dd, x)",
        "product(dd, x)",
        "integrate(dd, x)",
        "moment_of_inertia(dd)",
        "disk_matrix(dd, x)",
        "mean_around(dd)",
    ] {
        let src = format!("dd = nest(x => [x, x], 1, 60); {f}");
        let Stopped { value: v, since, cpu_since, .. } = stopped(&mut e, &src, Duration::from_millis(200));
        assert!(!v.starts_with("PANIC") && !v.contains("failure(internal"), "{src}: {v}");
        assert!(since.is_none_or(|d| !late(d, cpu_since, allowed, 2)), "{src}: ended {since:?} after the stop, processor {cpu_since:?}");
    }
    // a literal of 2^21 leaves (the left side of a definition made inside a function holds the value): stored and
    // found, replaced by its redefinition, before a more general rule
    for (src, want) in [
        ("fn defg(v, r) = { g(v) = r }; fn defk(v, r) = { k(v) = r }; 0", "0"),
        ("defg(nest(x => [x, x], 1, 20), 5); g(nest(x => [x, x], 1, 20))", "5"),
        ("defg(nest(x => [x, x], 1, 20), 6); g(nest(x => [x, x], 1, 20))", "6"),
        ("g(nest(x => [x, x], 2, 20)) === 6", "false"),
        ("k(y_) = 1; defk(nest(x => [x, x], 1, 20), 2); [k(nest(x => [x, x], 1, 20)), k(3)]", "[2, 1]"),
        ("defk(nest(x => [x, x], 1, 3), 7); [k(nest(x => [x, x], 1, 3)), k(4), k(nest(x => [x, x], 1, 20))]", "[7, 1, 2]"),
    ] {
        let (v, _, _) = stopped_at(&mut e, src, Duration::from_secs(60));
        assert_eq!(v, want, "{src}");
    }
}

/// Pure functions print as they were written whatever the session defined before: the slot of a pure function is
/// renamed only when a substituted value holds it free. Unrelated definitions first (each count changes the layout
/// of the symbols, whose masks are hashes), then nested pure functions in `root_sum`, `root`, `map` and function
/// bodies; every printed value is the one of a fresh session and has no renamed slot.
#[test]
fn pure_functions_print_as_written_after_other_definitions() {
    let _one = serial();
    let inputs = [
        "integrate(1/(x^5 + 11*x + 1), [x, 1, 3]); n(root_sum(&(&1^5 - 3*&1 - 7), sin), 50); integrate(root_sum(&(&1^5 + 11*&1 + 1), &(exp(-a*&1))), a)",
        "integrate(root_sum(&(&1^5 + 11*&1 + 1), &(sin(a*&1))), [a, 0, 1])",
        "d(root_sum(&(&1^5 + a*&1 + 1), &(exp(&1))), a)",
        "root(&(&1^5 - &1 - 1), 2)",
        "map(&(map(&(&1 + 1), [&1, 2*&1])), [1, 2])",
        "fn wrap9(u) = &(&1 + u); wrap9(&(&1^2))",
        "fn twice9(f) = &(f(f(&1))); twice9(&(&1 + 1))",
        "function(x, root_sum(&(&1^3 - x), &(&1^2)))",
    ];
    let run = |defs: usize| -> Vec<String> {
        let mut e = engine(2);
        for k in 0..defs {
            let (v, _, _) = stopped_at(&mut e, &format!("fn unrelated{k}(q{k}, r{k}) = q{k}^{k} + r{k}*z{k}; 0"), Duration::from_secs(60));
            assert_eq!(v, "0");
        }
        inputs.iter().map(|src| stopped_at(&mut e, src, Duration::from_secs(120)).0).collect()
    };
    let fresh = run(0);
    for (src, v) in inputs.iter().zip(&fresh) {
        assert!(!v.contains('$'), "{src}: {v}");
    }
    for defs in [1, 2, 3, 5, 8, 13, 21] {
        for ((src, a), b) in inputs.iter().zip(&fresh).zip(run(defs)) {
            assert_eq!(*a, b, "{src} after {defs} definitions");
        }
    }
}
