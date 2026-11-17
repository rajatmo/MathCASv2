//! Machine values from the double-double fast paths against 30-digit values of the ball path at the exact binary
//! value of each argument: `log_barnes_g` (x > 0) and `hyperfactorial` (z > -1), the Lerch transcendent for |z| <=
//! 0.95, Euler's function q_pochhammer(q) near q = 1 and q = -1. At most one ulp apart, and fast.
use mc_api::{Engine, StopFlag};
use std::time::{Duration, Instant};

fn eval(e: &mut mc_engine::CoreEngine, src: &str) -> String {
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The number of a printed real (the digits before a precision mark).
fn number(s: &str) -> f64 {
    let s = s.split('`').next().unwrap_or(s).replace("*10^", "e");
    s.parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}

fn ulps(got: f64, want: f64) -> f64 {
    let a = want.abs();
    let ulp = f64::from_bits(a.to_bits() + 1) - a;
    (got - want).abs() / ulp
}

/// Each case: the call with machine arguments, and the same call at the arguments' exact values (`set_precision`).
fn check(cases: &[(String, String)]) {
    let mut e = mc_engine::CoreEngine::new();
    let mut bad = vec![];
    for (machine, exact) in cases {
        let got = number(&eval(&mut e, machine));
        let want = number(&eval(&mut e, &format!("n({exact}, 30)")));
        let u = ulps(got, want);
        if u > 1.0 {
            bad.push(format!("{machine} = {got:e}, want {want:e} ({u:.1} ulps)"));
        }
    }
    assert!(bad.is_empty(), "{} of {}:\n{}", bad.len(), cases.len(), bad.join("\n"));
}

fn sp(x: f64) -> String {
    format!("set_precision({x:?}, 40)")
}

fn one(f: &str, xs: &[f64]) -> Vec<(String, String)> {
    xs.iter().map(|&x| (format!("{f}({x:?})"), format!("{f}({})", sp(x)))).collect()
}

fn grid() -> Vec<f64> {
    let mut xs: Vec<f64> = (1..=120).map(|k| k as f64 * 0.0371 - 0.99).collect();
    xs.extend([1e-300, 1e-8, 0.5, 0.999, 1.001, 1.5, 1.999999, 2.000001, 2.5, 7.25, 19.9, 20.5, 21.0, 33.3, 80.0, 150.5]);
    xs
}

#[test]
fn log_barnes_g_machine_values() {
    let xs: Vec<f64> = grid().into_iter().filter(|&x| x > 0.0).collect();
    check(&one("log_barnes_g", &xs));
}

#[test]
fn hyperfactorial_machine_values() {
    let xs: Vec<f64> = grid().into_iter().filter(|&x| x < 20.0).collect();
    check(&one("hyperfactorial", &xs));
}

#[test]
fn lerch_phi_machine_values() {
    let mut cases = vec![];
    for z in [-0.95, -0.7, -0.3, 0.001, 0.25, 0.6, 0.9, 0.95] {
        // s and a exact (as a plot of lerch_phi(x, 1, 1/2) gives them) and machine
        for (s, a) in [("1", "1/2"), ("2", "3"), ("1/2", "1/3"), ("-3/2", "2"), ("5", "1/10")] {
            cases.push((format!("lerch_phi({z:?}, {s}, {a})"), format!("lerch_phi({}, {s}, {a})", sp(z))));
        }
        for (s, a) in [(2.0, 0.5), (0.7, 1.3), (-0.4, 4.5), (3.0, 0.05)] {
            cases.push((format!("lerch_phi({z:?}, {s:?}, {a:?})"), format!("lerch_phi({}, {}, {})", sp(z), sp(s), sp(a))));
        }
    }
    check(&cases);
}

#[test]
fn euler_function_near_one_machine_values() {
    // against Euler's pentagonal number theorem, (q; q)_inf = sum_k (-1)^k q^(k (3k - 1)/2) over all integers k (the
    // terms fall as |q|^(3k^2/2): 800 on each side reach far below the digits kept here); the sum cancels to the size
    // of the value (down to 10^-238), so the data carry 400 digits
    let qs = [0.9, 0.93, 0.97, 0.985, 0.99, 0.991, 0.995, 0.997, -0.9, -0.95, -0.99, -0.995, -0.999];
    let cases: Vec<(String, String)> = qs
        .iter()
        .map(|&q| (format!("q_pochhammer({q:?})"), format!("sum((-1)^k * set_precision({q:?}, 400)^(k*(3*k - 1)/2), [k, -800, 800])")))
        .collect();
    check(&cases);
}

#[test]
fn machine_values_are_fast() {
    let mut e = mc_engine::CoreEngine::new();
    for f in ["hyperfactorial(x)", "log_barnes_g(x)", "lerch_phi(x/3.2, 1, 1/2)", "q_pochhammer(x/3.0 - 0.0001)"] {
        let t0 = Instant::now();
        let r = eval(&mut e, &format!("length(table({f}, [x, 0.001, 3.0, 0.001]))"));
        assert_eq!(r, "3000");
        // the ball path took milliseconds per value
        assert!(t0.elapsed() < Duration::from_secs(2), "{f}: {:?}", t0.elapsed());
    }
}
