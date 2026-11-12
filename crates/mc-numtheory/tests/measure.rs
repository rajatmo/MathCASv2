//! Timings of the benchmark cases of this crate through source text (run explicitly, in release, under the
//! guard): `cargo test --release --test measure -- --ignored --nocapture`.
mod common;
use common::Nt;
use std::time::Instant;

/// (setup, measured expression) pairs, after the benchmark cases of bench/cases/N.
const CASES: &[(&str, &str, &str)] = &[
    ("N5 10^4", "", "integer_length(factorial(10^4))"),
    ("N5 10^5", "", "integer_length(factorial(10^5))"),
    ("N5 10^6", "", "integer_length(factorial(10^6))"),
    ("N6 8", "", "[prime_pi(10^8), prime(10^8)]"),
    ("N6 10", "", "[prime_pi(10^10), prime(10^10)]"),
    ("N6 12", "", "[prime_pi(10^12), prime(10^12)]"),
    (
        "N7 30",
        "d = 30\nh = floor(d/2)\np = next_prime(2*10^(h - 1))\nq = next_prime(5*10^(d - h - 1))",
        "factor_integer(p*q) == [[p, 1], [q, 1]]",
    ),
    (
        "N7 45",
        "d = 45\nh = floor(d/2)\np = next_prime(2*10^(h - 1))\nq = next_prime(5*10^(d - h - 1))",
        "factor_integer(p*q) == [[p, 1], [q, 1]]",
    ),
    (
        "N7 60",
        "d = 60\nh = floor(d/2)\np = next_prime(2*10^(h - 1))\nq = next_prime(5*10^(d - h - 1))",
        "factor_integer(p*q) == [[p, 1], [q, 1]]",
    ),
    ("N9 10^5", "a = 7^(10^5)\nb = 11^(10^5) + 1\nc = a*b", "[quotient(c + 5, b), gcd(c, a^2)] == [a, a]"),
    ("N9 10^6", "a = 7^(10^6)\nb = 11^(10^6) + 1\nc = a*b", "[quotient(c + 5, b), gcd(c, a^2)] == [a, a]"),
    ("primitive root", "", "primitive_root(prime(10^6)^(10^6))"),
];

#[test]
#[ignore = "measurement: run explicitly"]
fn measure() {
    let n = CASES.len();
    for (i, (name, setup, expr)) in CASES.iter().enumerate() {
        // the median of three runs (the memo is per session, so each run is a fresh session)
        let mut times = vec![];
        let mut out = String::new();
        for _ in 0..3 {
            let mut nt = Nt::new();
            if !setup.is_empty() {
                nt.eval(setup);
            }
            let t = Instant::now();
            out = nt.eval(expr);
            times.push(t.elapsed());
        }
        times.sort();
        eprintln!("[{}/{n} {}%] {name}: {:.3} s -> {}", i + 1, 100 * (i + 1) / n, times[1].as_secs_f64(), out);
    }
}
