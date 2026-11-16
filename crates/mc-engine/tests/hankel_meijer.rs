//! Hankel transforms by the Mellin product (the Hankel transform of a Meijer G-function is again one).
//! Every closed form is checked against the defining integral `integrate(f bessel_j(nu, s r) r, [r, 0, inf])`
//! computed by `n_integrate` at two sample points; the generalized (Abel) transform of `erf(r)` is checked through
//! `erf = 1 - erfc`, the Abel limit `lim integrate(exp(-e r) bessel_j(0, s r) r) = lim e/(e^2 + s^2)^(3/2)` of the
//! constant being 0 for `s > 0`. Each test has a time bound.
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

/// `v = hankel_transform(f, r, s, nu)` evaluates (no `hankel_transform` left) and agrees with the integral
/// `integrand` (in `r`, with `s` and the parameters `subs` substituted) at each `s` to 10 digits.
fn checked(e: &mut CoreEngine, call: &str, integrand: &str, subs: &str, ss: &[&str]) {
    let v = ev(e, &format!("vv = {call}; vv"));
    assert!(!v.contains("hankel_transform") && !v.contains("failure"), "{call}: {v}");
    for s in ss {
        // the value at 40 digits (a sum of hypergeometric terms cancels)
        let src = format!(
            "{{ let w = n(rewrite(vv, join({subs}, [s -> {s}])), 40); let i = n_integrate(rewrite({integrand}, join({subs}, [s -> {s}])), [r, 0, inf], working_precision: 20); abs(w - i) < 10^-10*(1 + abs(i)) }}"
        );
        holds(e, &src);
    }
}

#[test]
fn step_and_exponential_difference() {
    timed("step, 1 - exp", 120, || {
        let mut e = CoreEngine::new();
        // the step: the integral over (0, a)
        holds(&mut e, "vv = function_expand(hankel_transform(unit_step(a - r), r, s)); vv === a*bessel_j(1, a*s)*unit_step(a)/s");
        for s in ["7/10", "13/5"] {
            holds(&mut e, &format!("abs(n(rewrite(vv, [a -> 3/2, s -> {s}]), 20) - n_integrate(bessel_j(0, {s}*r)*r, [r, 0, 3/2], working_precision: 20)) < 10^-12"));
        }
        // (1 - exp(-m r))/r^2: the summands diverge alone; asinh(m/s)
        checked(&mut e, "hankel_transform((1 - exp(-m * r)) / r^2, r, s)", "(1 - exp(-m*r))/r^2*bessel_j(0, s*r)*r", "[m -> 3/2]", &["7/10", "5/2"]);
        assert!(!ev(&mut e, "vv").contains("meijer_g"));
        holds(&mut e, "abs(n(rewrite(vv, [m -> 3/2, s -> 7/10]), 20) - n(asinh(15/7), 20)) < 10^-15");
        // the inverse is the same integral
        let w = ev(&mut e, "inverse_hankel_transform((1 - exp(-m * s)) / s^2, s, r)");
        assert!(!w.contains("hankel"), "{w}");
    });
}

#[test]
fn logarithm_and_airy() {
    timed("log, airy", 150, || {
        let mut e = CoreEngine::new();
        checked(&mut e, "hankel_transform(log(1 + a^2 / r^2), r, s)", "log(1 + a^2/r^2)*bessel_j(0, s*r)*r", "[a -> 3/2]", &["7/10", "9/4"]);
        let v = ev(&mut e, "vv = hankel_transform(airy_ai(r), r, s); vv");
        assert!(!v.contains("hankel_transform") && !v.contains("failure"), "{v}");
        // over (0, 30) (airy_ai(30) < 10^-47) at 30 digits: the infinite range gives n_integrate only 7
        for s in ["7/10", "2"] {
            holds(&mut e, &format!("abs(n(rewrite(vv, s -> {s}), 40) - n_integrate(airy_ai(r)*bessel_j(0, {s}*r)*r, [r, 0, 30], working_precision: 30)) < 10^-11"));
        }
        let w = ev(&mut e, "inverse_hankel_transform(airy_ai(s), s, r)");
        assert!(w.contains("hypergeometric_pfq") && !w.contains("hankel"), "{w}");
        // M[airy_ai] (DLMF 9.10(vi)) against its integral
        holds(&mut e, "abs(n(rewrite(mellin_transform(airy_ai(r), r, z), z -> 3/2), 20) - n_integrate(airy_ai(r)*r^(1/2), [r, 0, 30], working_precision: 30)) < 10^-12");
    });
}

#[test]
fn generalized_transform_of_erf() {
    timed("erf", 120, || {
        let mut e = CoreEngine::new();
        let v = ev(&mut e, "vv = hankel_transform(erf(r), r, s); vv");
        assert!(!v.contains("hankel"), "{v}");
        for s in ["7/10", "3"] {
            // the closed form (1/4) (I1(s^2/8) - I0(s^2/8)) exp(-s^2/8)
            holds(&mut e, &format!("abs(n(rewrite(vv - (1/4)*(bessel_i(1, (1/8)*s^2) - bessel_i(0, (1/8)*s^2))*exp((-1/8)*s^2), s -> {s}), 30)) < 10^-25"));
            holds(&mut e, &format!("abs(n(rewrite(vv, s -> {s}), 20) + n_integrate(erfc(r)*bessel_j(0, {s}*r)*r, [r, 0, inf], working_precision: 20)) < 10^-12"));
        }
        // M[erf(x) - 1] by the continuation at infinity
        holds(&mut e, "abs(n(rewrite(mellin_transform(erf(r) - 1, r, z), z -> 3/2), 20) - n_integrate((erf(r) - 1)*r^(1/2), [r, 0, inf], working_precision: 20)) < 10^-12");
    });
}

#[test]
fn declines_outside_the_strips() {
    timed("declines", 120, || {
        let mut e = CoreEngine::new();
        // divergent at infinity: no generalized value
        assert!(ev(&mut e, "hankel_transform(airy_bi(r), r, s)").starts_with("hankel_transform("));
        assert!(ev(&mut e, "hankel_transform(exp(r), r, s)").starts_with("hankel_transform("));
        // the earlier closed forms are unchanged
        holds(&mut e, "hankel_transform(exp(-a*r)/r, r, s) === 1/(a*(1 + s^2/a^2)^(1/2))");
    });
}
