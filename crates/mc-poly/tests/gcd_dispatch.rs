//! `gcd` of integers reaches mc-numtheory's half-gcd (decision 9.1): this crate owns the name and dispatches by
//! argument kind, so big integers take the subquadratic path, not the quadratic Lehmer gcd of polynomial contents.
use mc_arith::num::gcd::gcd as lehmer;
use mc_arith::num::Nat;
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::EvalConfig;
use mc_numtheory::budget::Unlimited;
use std::time::{Duration, Instant};

/// The median of three timings of `f`.
fn median3(mut f: impl FnMut()) -> Duration {
    let mut t: Vec<Duration> = (0..3)
        .map(|_| {
            let s = Instant::now();
            f();
            s.elapsed()
        })
        .collect();
    t.sort();
    t[1]
}

/// a = 7^(90000 k) g and b = (11^(72000 k) + 1) g with g = 3^(60000 k): about 10^5 k digits each, gcd g (7 and 3
/// divide no 11^(72000 k) + 1). Times the gcd through an engine (memo off, so every repetition computes), by the
/// half-gcd directly and by Lehmer's algorithm, after checking all three agree; `reps` repetitions (median).
fn times(k: u64, reps: usize) -> (String, Duration, Duration, Duration) {
    let eval = EvalConfig { memo: false, ..EvalConfig::default() };
    let cfg = EngineConfig { threads: 1, eval, ..EngineConfig::default() };
    let mut e = CoreEngine::with_config(cfg);
    let stop = StopFlag::new();
    let setup = format!("g = 3^{}; a = 7^{}*g; b = (11^{} + 1)*g", 60000 * k, 90000 * k, 72000 * k);
    e.eval(&setup, &stop).expect("setup");
    let digits = e.eval("integer_length(a)", &stop).expect("length").printed;
    let g = Nat::from_u64(3).pow_u64(60000 * k);
    let a = Nat::from_u64(7).pow_u64(90000 * k).mul(&g);
    let b = Nat::from_u64(11).pow_u64(72000 * k).add_u64(1).mul(&g);
    let time = |f: &mut dyn FnMut()| if reps == 1 { let s = Instant::now(); f(); s.elapsed() } else { median3(f) };
    let mut answer = String::new();
    let engine = time(&mut || answer = e.eval("gcd(a, b) == g", &stop).expect("gcd").printed);
    assert_eq!(answer, "true");
    let mut r = Nat::zero();
    let half = time(&mut || r = mc_numtheory::gcd::gcd(&a, &b, &mut Unlimited).expect("unlimited"));
    assert_eq!(r, g);
    let quadratic = time(&mut || r = lehmer(&a, &b));
    assert_eq!(r, g);
    (digits, engine, half, quadratic)
}

#[test]
fn integer_gcd_takes_the_half_gcd() {
    let (digits, engine, half, quadratic) = times(1, 3);
    println!("gcd of {digits}-digit integers: {engine:?} through the engine, {half:?} by the half-gcd, {quadratic:?} by Lehmer");
    if cfg!(debug_assertions) {
        return;
    }
    // at 10^5 digits the half-gcd is only somewhat faster than Lehmer; at 4 10^5 it is about four times faster, which
    // tells the two paths apart on a busy machine
    let (digits, engine, half, quadratic) = times(4, 1);
    println!("gcd of {digits}-digit integers: {engine:?} through the engine, {half:?} by the half-gcd, {quadratic:?} by Lehmer");
    assert!(2 * engine < quadratic, "gcd of {digits}-digit integers took {engine:?}; Lehmer takes {quadratic:?}");
}
