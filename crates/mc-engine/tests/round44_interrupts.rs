//! Round 44: interrupts and budgets of work, adversarially.
//!
//! - A stop raised at a random moment into a heavy input of every area (big integers, polynomial gcd, Gröbner
//!   bases, integration, plots, linear algebra, sums, number theory, simplification, limits, differential
//!   equations) ends the evaluation within 300 ms (scaled by `MC_TEST_TIME_FACTOR`).
//! - The same inputs, run to their end or their budget, give the same printed value on 1, 4 and 16 threads and with
//!   every block of fuel delayed (a slower machine): a budget is work, not time.
//!
//! The suite stops each heavy input once, at a moment drawn from a fixed seed; `MC_R44_STOPS=<n>` stops each one at
//! `n` moments (and `MC_R44_SEED` changes them), printing `[k/N pct%]` progress.
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Origin};
use std::time::{Duration, Instant};

/// Inputs that take seconds or more, one or more per area.
const HEAVY: &[&str] = &[
    "integer_length(factorial(3*10^6))",
    "factor_integer(2^251 - 1)",
    "factor_integer(2^521 + 3)",
    "next_prime(10^3000)",
    "polynomial_gcd(expand((x + y + z + 1)^40*(x - y)), expand((x + y + z + 1)^40*(x + y)))",
    "together(expand((x + y + z + 1)^40*(x - y))/expand((x + y + z + 1)^40*(x + y)))",
    "factor(expand((x + y + z + 1)^40*(x^2 - y*z + 3)))",
    "expand((1 + x + y + z)^120)",
    "groebner_basis([a + b + c + d + f + g, a*b + b*c + c*d + d*f + f*g + g*a, a*b*c + b*c*d + c*d*f + d*f*g + f*g*a + g*a*b, a*b*c*d + b*c*d*f + c*d*f*g + d*f*g*a + f*g*a*b + g*a*b*c, a*b*c*d*f + b*c*d*f*g + c*d*f*g*a + d*f*g*a*b + f*g*a*b*c + g*a*b*c*d, a*b*c*d*f*g - 1], [a, b, c, d, f, g])",
    "groebner_basis([x^5 + y^4 + z^3 - 1, x^3 + y^3 + z^2 - 1, x^2 + y^2 + z^7 - 1], [x, y, z])",
    "integrate(1/(x^31 + x^4 + 1), x)",
    "plot3d(sin(x*y) + bessel_j(0, x + y), [x, 0, 40], [y, 0, 40], plot_points: 400)",
    "contour_plot(sin(x^2 + y^3) + cos(x*y), [x, -8, 8], [y, -8, 8], plot_points: 300)",
    "det(table(1/(i + j + 1) + (i - j)^3, [i, 120], [j, 120]))",
    "eigenvalues(table(1/(i + j - 1), [i, 120], [j, 120]))",
    "matrix_rank(table(mod(i^3 + 7*j^2 + i*j, 1009), [i, 1500], [j, 1500]))",
    "n_sum(sin(k)/sqrt(k), [k, 1, inf], working_precision: 60)",
    "prime_pi(10^17)",
    "n_dsolve([y''(t) + sin(y(t)) == 0, y(0) == 1, y'(0) == 0], y, [t, 0, 10^5], working_precision: 30)",
    "n(pi, 10^7)",
    "reduce(x^4 + y^4 + z^4 < 1 and x + y + z > 1 and x*y*z > 1/100, [x, y, z], reals)",
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
    let stops: usize = std::env::var("MC_R44_STOPS").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut rng = std::env::var("MC_R44_SEED").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0x5709_2026) | 1;
    let allowed = Duration::from_secs_f64(0.3 * time_factor());
    let total = HEAVY.len() * stops;
    let mut late = vec![];
    let mut e = engine(4);
    for (k, src) in HEAVY.iter().flat_map(|s| std::iter::repeat_n(s, stops)).enumerate() {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let after = Duration::from_millis(50 + rng % 1450);
        let (v, since, took) = stopped_at(&mut e, src, after);
        if stops > 1 {
            eprintln!("[{}/{total} {}%] {src} at {after:?}: {:?}", k + 1, (k + 1) * 100 / total, since);
        }
        assert!(!v.starts_with("PANIC") && !v.contains("failure(internal"), "{src}: {v}");
        if let Some(d) = since {
            if d > allowed {
                late.push(format!("{src}: stopped at {after:?}, ended {d:?} later (all {took:?}): {}", &v[..v.len().min(80)]));
            }
        }
        // a fresh session now and then: a stopped evaluation leaves nothing behind that a later one depends on
        if k % 8 == 7 {
            e = engine(4);
        }
    }
    assert!(late.is_empty(), "{} late stops:\n{}", late.len(), late.join("\n"));
}

/// Inputs that end (with a value or at a budget of work) within a few seconds, from every area.
const MEDIUM: &[&str] = &[
    "integer_length(factorial(10^5))",
    "factor_integer(2^101 - 1)",
    "polynomial_gcd(expand((x + y + 1)^12*(x - y)), expand((x + y + 1)^12*(x + 2*y)))",
    "groebner_basis([x^2 + y*z - 2, y^2 + x*z - 3, z^2 + x*y - 5], [x, y, z])",
    "integrate(sqrt(1 + x^3), x)",
    "integrate(x^3*exp(x)*sin(x), x)",
    "integrate(1/(x^5 + x + 1), x)",
    "det(table(1/(i + j + 1), [i, 12], [j, 12]))",
    "eigenvalues([[2, 1, 0], [1, 3, 1], [0, 1, 4]])",
    "sum(1/k^2, [k, 1, inf])",
    "sum(binomial(n, k)^2, [k, 0, n])",
    "n_sum(1/k^3, [k, 1, inf])",
    "prime_pi(10^9)",
    "simplify(sum(1/(x^k + y^(k + 1) + k), [k, 1, 6]))",
    "full_simplify((x^12 - y^12)/(x^4 - y^4) - (x^8 + x^4*y^4 + y^8))",
    "limit((1 + 1/x)^x, x -> inf)",
    "limit((sin(tan(x)) - tan(sin(x)))/x^7, x -> 0)",
    "d_solve(y''(x) + x*y(x) == 0, y(x), x)",
    "d_solve(y'(x) == y(x)^2 + x, y(x), x)",
    "solve(x^5 - x + 1 == 0, x)",
    "reduce(x^2 + y^2 < 1 and x + y > 1, [x, y], reals)",
    "n(zeta(1/2 + 100*im), 40)",
    "series(tan(sin(x)) - sin(tan(x)), [x, 0, 15])",
    "find_root(cos(x) == x, [x, 1])",
    "n_integrate(exp(-x^2)*cos(x), [x, -inf, inf])",
    "function_sign(x^2 - 2*x + 2, x)",
    "map(i => if i == 4 then break() else i^2, range(10), parallel: true)",
    "parallel_table(if is_prime(i) then continue() else i, [i, 30])",
];

fn values(threads: usize) -> Vec<String> {
    MEDIUM
        .iter()
        .map(|src| {
            let mut e = engine(threads);
            let (v, _, _) = stopped_at(&mut e, src, Duration::from_secs(600));
            v
        })
        .collect()
}

/// The same inputs give the same value on 1, 4 and 16 threads and on a slowed clock. One test, the runs in order:
/// the libraries keep caches for the life of the process, so a first run fills them and the runs compared come
/// after it.
#[test]
fn the_same_value_on_every_thread_count_and_speed() {
    let _warm = values(1);
    let one = values(1);
    for threads in [4, 16] {
        let other = values(threads);
        for ((src, a), b) in MEDIUM.iter().zip(&one).zip(&other) {
            assert_eq!(a, b, "{src}: {threads} threads give the value of one");
        }
    }
    mc_eval::set_test_slowdown(300);
    let slow = values(1);
    mc_eval::set_test_slowdown(0);
    for ((src, a), b) in MEDIUM.iter().zip(&one).zip(&slow) {
        assert_eq!(a, b, "{src}: the same value on a slower machine");
    }
}
