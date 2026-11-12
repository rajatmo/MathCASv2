//! Inputs the fuzz campaign (tests/fuzz.rs) found ending the process or panicking in a native, with what they give
//! now. Each one used to crash: a stack overflow, an allocation failure or a panic.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// (input, the start of its printed value)
const FOUND: &[(&str, &str)] = &[
    // a subnormal order: splitting an infinite double-double recursed until the stack overflowed
    ("bessel_i(1e-320, 5)", "27.23987182360444"),
    ("bessel_k(5e-324, 5.0)", "0.00369109833404"),
    // a huge parameter: the double-double reduction of sin(pi x) left the low part unreduced and its series never
    // converged
    ("jacobi_dn(1.0, 1.0e40)", "-0."),
    ("jacobi_sn(1.0, 10.0^40)", "-7.44"),
    // |z|^2 overflowed in the inversion z -> 1/z, which then recursed on NaN until the stack overflowed
    ("polylog(3, 1e300)", "complex(-54934049.43"),
    // dense algorithms allocated one coefficient per degree, tens of gigabytes
    ("factor(x^(2^31) - x + 1)", "failure(limit, memory)"),
    // round 24: solve declines degrees above 100000 at once, before any allocation
    ("solve(x^(2^31) - x + 1 == 0, x)", "solve("),
    ("n_solve(x^(2^31) - x + 1 == 0, x)", "n_solve("),
    ("coefficient_list(x^(2^31) - x + 1, x)", "failure(limit, memory)"),
    ("polynomial_gcd(x^(2^31) - 1, x^(2^30) - 1)", "failure(limit, memory)"),
    ("discriminant(x^(2^31) - x + 1, x)", "failure(limit, memory)"),
    ("integrate(1/(x^(2^31) + 1), x)", "integrate("),
    // degree 10^6 is below the dense limit, but the rational integrator ran for minutes without polling
    ("integrate(5/(x^(10^6) + 1), x)", "integrate("),
    // 32-bit exponents wrapped: the square lost its x^(2^32) term
    ("expand((1 + x^(2^31))^2)", "failure(limit, memory)"),
    // a divergent integral at high precision walked its first level on without end, allocating until the process died
    ("n_integrate(x^2, [x, 0, inf], digits: 30)", ""),
    // matrices and arrays allocated before checking their size
    ("diagonal_matrix(range(10^5))", "failure(limit, memory)"),
    ("kronecker_product(identity_matrix(3000), identity_matrix(3000))", "failure(limit, memory)"),
    ("identity_matrix(16000)", "failure(limit, memory)"),
    ("constant_array(0, 10^10)", "failure(limit, memory, constant_array(0, 10000000000))"),
    // I's series at x = 2^31 needs 2^31 terms; printing K's value there, 10^(-932640303), built 2^31-bit integers
    ("bessel_k(3.0~25, 2^31)", "1.34347561686217814550573"),
    ("bessel_i(3.0~25, 2^31)", "bessel_i("),
    ("n(exp(-2^31), 25)", "4.96746461423100930178722"),
    // a float loop counter at |nu| = 2^62 stopped advancing (k + 1.0 == k) in a loop that does not poll
    ("n(exp_integral_e(-2^62, 3), 30)", ""),
    // a root beyond the double range: log2(inf) as a shift of 2^63 bits
    ("n_solve(x^5 - x - 2.0^1023 == 0, x)", "[[x -> "),
    // a padded length of 2^32 digits allocated 32 GB
    ("integer_digits(2^64, 64, 2^32)", "failure(limit, memory, integer_digits("),
];

#[test]
fn inputs_found_by_fuzzing_give_values() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    for (src, want) in FOUND {
        let got = eval(&mut e, src);
        assert!(got.starts_with(want), "{src}: {got}");
    }
}

#[test]
fn printing_a_huge_value_stops_when_interrupted() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let stop = StopFlag::new();
    let raiser = {
        let stop = stop.clone();
        // a timer thread waiting on a channel (no sleep): it raises the flag after half a second
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let h = std::thread::spawn(move || {
            let _ = rx.recv_timeout(std::time::Duration::from_millis(500));
            stop.raise();
        });
        (tx, h)
    };
    let t0 = std::time::Instant::now();
    // a million copies of a 300-deep term: building it is quick, its text would be about 2 GB
    let r = e.eval("constant_array(nest(x => sin(x), y, 300), 10^6)", &stop);
    assert!(r.is_err(), "the evaluation was interrupted");
    assert!(t0.elapsed() < std::time::Duration::from_secs(20), "printing stopped promptly: {:?}", t0.elapsed());
    drop(raiser.0);
    raiser.1.join().unwrap();
    // the session goes on
    assert_eq!(eval(&mut e, "1 + 1"), "2");
}
