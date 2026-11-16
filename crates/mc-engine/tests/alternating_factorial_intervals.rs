//! `alternating_factorial` of an interval, a centered interval and an around value: the interval values enclose
//! the function at sample points of the argument (computed at 30 digits from the sum's continuation), and the
//! around uncertainty agrees with a central difference quotient. Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

#[test]
fn enclosures_and_uncertainty() {
    timed("alternating_factorial intervals", 60, || {
        let mut e = CoreEngine::new();
        // the value at m by the Borel sum of the tail at 30 digits: an independent route to the function
        holds(&mut e, "af = m => n(-cos(pi*m)*(1 + e*exp_integral_ei(-1)) + n_integrate(exp(-t)*t^(m + 1)/(1 + t), [t, 0, inf], working_precision: 30), 25); true");
        holds(&mut e, "abs(af(3) - 5) < 10^-20");
        holds(&mut e, "{ let v = alternating_factorial(interval([1.45, 1.46])); head(v) === interval and every([29/20, 1453/1000, 1457/1000, 73/50], m => v[1][1] <= af(m) and af(m) <= v[1][2]) and v[1][2] - v[1][1] < 0.03 }");
        holds(&mut e, "{ let v = alternating_factorial(centered_interval(-1.2, 0.001)); head(v) === centered_interval and every([-1201/1000, -6/5, -2399/2000, -1199/1000], m => abs(af(m) - v[1]) <= v[2]) and v[2] < 0.0019 }");
        // around: the first-order propagation |a'(2)| 0.01
        holds(&mut e, "{ let v = alternating_factorial(around(2, 0.01)); let dq = (af(2 + 10^-8) - af(2 - 10^-8))/(2*10^-8); head(v) === around and abs(v[1] - 1) < 10^-12 and abs(v[2] - abs(dq)*0.01) < 10^-8 }");
    });
}
