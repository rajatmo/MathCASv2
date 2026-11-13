//! Round 39: sequences and series. Upper and lower limits of sequences with `mod`, Fibonacci numbers and dense
//! oscillation; convergence of sums from the tail (step functions that are eventually constant, harmonic numbers);
//! multiple harmonic numbers of depth 2; generating functions of n f(n). Each value is checked against direct
//! computation at numbers or by an identity.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn upper_and_lower_limits_of_sequences() {
    assert_eq!(ev("discrete_max_limit(mod(n_u, 5), n_u -> inf)"), "4");
    assert_eq!(ev("discrete_min_limit(mod(n_u, 5), n_u -> inf)"), "0");
    // n! is a multiple of 5 from n = 5 on: mod((-1)^n, 5) takes 1 and 4
    assert_eq!(ev("discrete_max_limit(mod(factorial(n_u) + (-1)^n_u, 5), n_u -> inf)"), "4");
    assert_eq!(ev("discrete_min_limit(mod(factorial(n_u) + (-1)^n_u, 5), n_u -> inf)"), "1");
    assert_eq!(ev("table(mod(factorial(k) + (-1)^k, 5), [k, 10, 13])"), "[1, 4, 1, 4]");
    // the period 14 of mod(3 n + 1, 7) + (-1)^n reaches 6 + 1
    assert_eq!(ev("discrete_max_limit(mod(3*n_u + 1, 7) + (-1)^n_u, n_u -> inf)"), "7");
    assert_eq!(ev("max(table(mod(3*k + 1, 7) + (-1)^k, [k, 100, 140]))"), "7");
    // F(n)/phi^n tends to 1/sqrt(5) (Binet)
    assert_eq!(ev("discrete_min_limit(fibonacci(n_u) * (sin(n_u * (pi / 2)) / golden_ratio^n_u), n_u -> inf)"), "(-1/5)*5^(1/2)");
    assert_eq!(ev("discrete_max_limit(lucas_l(n_u)/golden_ratio^n_u, n_u -> inf)"), "1");
    assert_eq!(ev("abs(n(fibonacci(81)*sin(81*pi/2)/golden_ratio^81 - 1/sqrt(5))) < 10^-10"), "true");
}

#[test]
fn a_sequence_without_a_limit() {
    // sin(n) is dense in [-1, 1]: no limit
    assert!(ev("discrete_limit(sin(n_u), n_u -> inf)").starts_with("failure(no_limit, discrete_limit, sin(n_u)"));
    assert!(ev("discrete_limit(cos(n_u*pi/3), n_u -> inf)").starts_with("failure(no_limit, discrete_limit"));
    assert_eq!(ev("discrete_limit(sin(n_u)/n_u, n_u -> inf)"), "0");
}

#[test]
fn convergence_from_the_tail() {
    assert_eq!(ev("sum_convergence(harmonic_number(n_u) / ((2 * n_u + 1) * (3 * n_u + 1)), n_u)"), "true");
    assert_eq!(ev("sum_convergence(harmonic_number(n_u) / n_u, n_u)"), "false");
    assert_eq!(ev("sum_convergence(harmonic_number(n_u)^2 / n_u^(3/2), n_u)"), "true");
    // floor(n/(1 + n) + floor(12/(1 + n^2))) is 0 from n = 4 on: the terms are 1/n^2
    assert_eq!(ev("sum_convergence(1 / (n_u^2 + floor(n_u / (1 + n_u) + floor(12 / (1 + n_u^2)))), n_u)"), "true");
    assert_eq!(ev("table(floor(k/(1 + k) + floor(12/(1 + k^2))), [k, 4, 8])"), "[0, 0, 0, 0, 0]");
    assert_eq!(ev("sum_convergence(1/(n_u + floor(1/n_u)), n_u)"), "false");
    // finitely many nonzero terms; a step that switches the harmonic series on
    assert_eq!(ev("sum_convergence(boole(n_u^2 < 1000) * n_u, n_u)"), "true");
    assert_eq!(ev("sum_convergence(boole(n_u > 3) / n_u, n_u)"), "false");
    assert_eq!(ev("sum_convergence(boole(n_u > 3) / n_u^2, n_u)"), "true");
}

#[test]
fn multiple_harmonic_numbers_of_depth_two() {
    assert_eq!(ev("multiple_harmonic_number(n_u, [1, 0])"), "n_u - harmonic_number(n_u)");
    assert_eq!(ev("multiple_harmonic_number(n_u, [1, 1])"), "(1/2)*(-harmonic_number(n_u, 2) + harmonic_number(n_u)^2)");
    // at numbers, against the definition by nested sums
    for (rs, m) in [("[3, 3]", 7), ("[0, 2]", 9), ("[2, 0]", 6), ("[0, 0]", 6), ("[1, 1]", 11)] {
        assert_eq!(ev(&format!("rewrite(multiple_harmonic_number(n_u, {rs}), n_u -> {m}) - multiple_harmonic_number({m}, {rs})")), "0", "{rs} at {m}");
    }
}

#[test]
fn generating_function_of_n_times_an_undefined_sequence() {
    assert_eq!(
        ev("[generating_function(n_u * f(n_u), n_u, z), generating_function(n_u * (n_u + 1) * f(n_u), n_u, z)]"),
        "[z*d(generating_function(f(n_u), n_u, z), z), d(generating_function(f(n_u), n_u, z), [z, 2])*z^2 + 2*z*d(generating_function(f(n_u), n_u, z), z)]"
    );
    // with f(n) = 3^n: sum n (n + 1) 3^n z^n = 6 z/(1 - 3 z)^3
    assert_eq!(ev("simplify(rewrite(generating_function(n_u*(n_u + 1)*f(n_u), n_u, z), f -> (k => 3^k)) - 6*z/(1 - 3*z)^3)"), "0");
    assert_eq!(ev("simplify(rewrite(generating_function(n_u^2*f(n_u), n_u, z), f -> (k => 1/factorial(k))) - (z + z^2)*exp(z))"), "0");
}
