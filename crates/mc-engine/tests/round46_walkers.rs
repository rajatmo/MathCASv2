//! Round 46: the hangs that are left.
//!
//! - A stop raised at a random moment into a heavy input of every area (special functions at high precision,
//!   hypergeometric and Appell functions, numerical solvers, algebra, calculus, sums, solving, decompositions,
//!   linear algebra, plots, lists) ends the evaluation within 300 ms (scaled by `MC_TEST_TIME_FACTOR`). The suite
//!   stops each input once, at a moment drawn from a fixed seed; `MC_R46_STOPS=<n>` stops each at `n` moments
//!   (`MC_R46_SEED` changes them), printing `[k/N pct%]` progress; `MC_R46_ONLY=a;;b` runs only those inputs.
//! - Inputs that were slow: `sign` of a complex constant, `count_roots` of `gamma(z) - z - 1` on a square, within a
//!   bound of time, with their values.
//! - Functions that walked a shared argument as a tree (the rule index, layouts, plots and the solvers): their
//!   inputs on deep shared terms end promptly after a stop.
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Origin};
use std::time::{Duration, Instant};

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

/// Evaluate `src` with the stop flag raised `after` from the start: the printed value, how long the evaluation took
/// after the stop was raised (`None` when it ended before), and the whole time.
fn stopped_at(e: &mut CoreEngine, src: &str, after: Duration) -> (String, Option<Duration>, Duration) {
    let stop = StopFlag::new();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let watch = stop.clone();
    let dog = std::thread::spawn(move || match done_rx.recv_timeout(after) {
        Ok(()) => None,
        Err(_) => {
            watch.raise();
            Some(Instant::now())
        }
    });
    let t0 = Instant::now();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.eval_in(src, &Origin::input(), &stop)));
    let end = Instant::now();
    let _ = done_tx.send(());
    let raised = dog.join().ok().flatten();
    let text = match r {
        Err(p) => format!("PANIC: {}", mc_engine::guard::payload_text(&*p)),
        Ok(Ok(s)) => s.last.printed,
        Ok(Err(err)) => format!("error: {err}"),
    };
    (text, raised.map(|r| end.saturating_duration_since(r)), end - t0)
}

/// Every heavy input, stopped at a random moment between 50 ms and 1.5 s: it ends within 300 ms of the stop,
/// with an interrupted failure or (when it finished first) its value; never a panic.
#[test]
fn a_stop_ends_every_heavy_input_promptly() {
    let stops: usize = std::env::var("MC_R46_STOPS").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut rng = std::env::var("MC_R46_SEED").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0x4646_2026) | 1;
    let only: Option<Vec<String>> = std::env::var("MC_R46_ONLY").ok().map(|s| s.split(";;").map(str::to_string).collect());
    let inputs: Vec<String> = only.unwrap_or_else(|| HEAVY.iter().map(|s| s.to_string()).collect());
    let allowed = Duration::from_secs_f64(0.3 * time_factor());
    let total = inputs.len() * stops;
    let mut late = vec![];
    let mut e = engine(4);
    for (k, src) in inputs.iter().flat_map(|s| std::iter::repeat_n(s, stops)).enumerate() {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let after = Duration::from_millis(50 + rng % 1450);
        let (v, since, took) = stopped_at(&mut e, src, after);
        if stops > 1 {
            eprintln!("[{}/{total} {}%] {src} at {after:?}: {since:?} (all {took:?})", k + 1, (k + 1) * 100 / total);
        }
        assert!(!v.starts_with("PANIC") && !v.contains("failure(internal"), "{src}: {v}");
        if let Some(d) = since {
            if d > allowed {
                late.push(format!("{src}: stopped at {after:?}, ended {d:?} later (all {took:?}): {}", v.chars().take(80).collect::<String>()));
            }
        }
        // a fresh session now and then: a stopped evaluation leaves nothing behind that a later one depends on
        if k % 8 == 7 {
            e = engine(4);
        }
    }
    assert!(late.is_empty(), "{} late stops:\n{}", late.len(), late.join("\n"));
}

/// Inputs that were slow: each within its bound of time, with its value. `sign` of a complex constant refined its
/// real comparison to the cap (20 s); `count_roots` on a square covered by boxes of gamma (9 s).
#[test]
fn slow_inputs_are_fast() {
    let mut e = engine(2);
    for (src, want, secs) in [
        ("sign(elliptic_k(2))", "sign(elliptic_k(2))", 3.0),
        ("sign(elliptic_k(2) + 1) == sign(elliptic_k(2) + 1)", "true", 3.0),
        ("count_roots(gamma(z) - z - 1, [z, -2 - 2*im, 2 + 2*im])", "3", 6.0),
        ("count_roots(gamma(z) - 2, [z, -3 - 2*im, 4 + 2*im])", "4", 6.0),
        ("n(gamma(2 + im), 30)", "complex(0.6529654964201667278386462479460847, 0.3430658398165453575887359869783115)", 3.0),
        // Gamma(-3/2 + 5 i) by the reflection formula pi/(sin(pi z) Gamma(1 - z)), an independent route
        ("abs(n(gamma(-3/2 + 5*im), 40) - n(pi/(sin(pi*(-3/2 + 5*im))*gamma(5/2 - 5*im)), 40)) < 10^-35", "true", 3.0),
    ] {
        let t0 = Instant::now();
        let (v, _, _) = stopped_at(&mut e, src, Duration::from_secs(120));
        let took = t0.elapsed();
        assert_eq!(v, want, "{src}");
        assert!(took.as_secs_f64() < secs * time_factor(), "{src}: {took:?}");
    }
}

/// Definitions, layouts, plots, properties and solvers of a deep shared argument end within 300 ms of a stop raised
/// 200 ms in; a definition with a large shared literal is stored, found, replaced and ordered as any other.
#[test]
fn shared_arguments_end_promptly() {
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
        let (v, since, _) = stopped_at(&mut e, &src, Duration::from_millis(200));
        assert!(!v.starts_with("PANIC") && !v.contains("failure(internal"), "{src}: {v}");
        assert!(since.is_none_or(|d| d <= allowed), "{src}: ended {since:?} after the stop");
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
