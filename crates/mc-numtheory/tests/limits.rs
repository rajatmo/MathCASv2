//! Long computations stop on the fuel budget and on an interrupt from another thread, and the session goes on.
mod common;
use common::Nt;
use mc_eval::Status;
use std::time::{Duration, Instant};

/// Calls that run for a long time without limits, each in a different algorithm.
const HEAVY: &[&str] = &[
    "prime_pi(2^47)",
    "prime(10^12)",
    "factorial(3*10^6)",
    "factor_integer(next_prime(10^34)*next_prime(10^35))",
    "primitive_root(prime(10^6)^(10^6))",
    "gcd(7^(10^6) * (11^(10^6) + 1), 7^(2*10^6))",
    "is_prime(next_prime(10^2000))",
    "integer_digits(3^(10^6), 7)",
];

#[test]
fn fuel_stops_every_heavy_algorithm() {
    let mut nt = Nt::new();
    for src in HEAVY {
        let t = Instant::now();
        let (status, _) = nt.eval_fuel(src, 20_000);
        assert_eq!(status, Status::Fuel, "{src}");
        assert!(t.elapsed() < Duration::from_secs(20), "{src} took {:?}", t.elapsed());
    }
    assert_eq!(nt.eval("next_prime(100)"), "101");
}

#[test]
fn an_interrupt_from_another_thread_stops_factoring() {
    let mut nt = Nt::new();
    let ctl = nt.control();
    let t0 = Instant::now();
    let raiser = std::thread::spawn(move || {
        // raise once the evaluation is well under way (busy-waiting: no sleeps in this repository)
        while t0.elapsed() < Duration::from_millis(100) {
            std::hint::spin_loop();
        }
        ctl.interrupt();
    });
    let (status, _) = nt.eval_fuel("factor_integer(next_prime(10^39)*next_prime(10^40))", i64::MAX);
    raiser.join().unwrap();
    assert_eq!(status, Status::Interrupted);
    assert!(t0.elapsed() < Duration::from_secs(10));
    nt.control().clear_interrupt();
    assert_eq!(nt.eval("prime_pi(1000)"), "168");
}
