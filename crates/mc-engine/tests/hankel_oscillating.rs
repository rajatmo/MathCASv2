//! Hankel transforms of oscillating functions: `cos(a r)` and `sin(a r)` (generalized, Abel transforms),
//! `cos(a r)/(r^2 + b^2)` and `sin(a r)/(r^2 + b^2)` on the range of s where they have a closed form, the
//! generalized transform of `sin_integral(r^2)`, and the step `unit_step(a - r)` under `a > 0`. Each value is checked
//! by another route: the defining integral by `n_integrate` at two values of s, or the derivative in a of the
//! transform of `sin(a r)/r` found by the Mellin product. Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

/// `call` evaluates to `value` (compared with `===` after both are evaluated).
fn same(e: &mut CoreEngine, call: &str, value: &str) {
    // an assignment `vv = ...` first, then vv compared
    let lhs = match call.strip_prefix("vv = ") {
        Some(_) => {
            ev(e, call);
            "vv".to_string()
        }
        None => format!("({call})"),
    };
    let src = format!("{lhs} === ({value})");
    assert_eq!(ev(e, &src), "true", "{src}: {}", ev(e, &lhs));
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

/// `vv` at `s` (and the parameters `subs`) agrees with `n_integrate(integrand, [r, 0, inf])` to 8 digits.
fn agrees(e: &mut CoreEngine, integrand: &str, subs: &str, s: &str) {
    holds(
        e,
        &format!(
            "{{ let w = n(rewrite(vv, join({subs}, [s -> {s}])), 30); let i = n_integrate(rewrite({integrand}, join({subs}, [s -> {s}])), [r, 0, inf]); abs(w - i) < 10^-8*(1 + abs(i)) }}"
        ),
    );
}

#[test]
fn trigonometric_over_quadratic() {
    timed("cos, sin over r^2 + b^2", 120, || {
        let mut e = CoreEngine::new();
        // cosh(1) bessel_k(0, s) holds for s > 1 only: the value carries its range
        same(&mut e, "vv = hankel_transform(cos(r) / (1 + r^2), r, s)", "conditional_expression(bessel_k(0, s)*cosh(1), s > 1)");
        holds(&mut e, "vv = vv[1]; true");
        for s in ["2", "5/2"] {
            agrees(&mut e, "cos(r)/(1 + r^2)*bessel_j(0, s*r)*r", "[]", s);
        }
        same(&mut e, "hankel_transform(cos(r) / (1 + r^2), r, s, assumptions: s > 1)", "bessel_k(0, s)*cosh(1)");
        same(&mut e, "hankel_transform(cos(r) / (1 + r^2), r, 3)", "bessel_k(0, 3)*cosh(1)");
        // outside the range: no closed form, unevaluated
        assert!(ev(&mut e, "hankel_transform(cos(r) / (1 + r^2), r, 1/2)").starts_with("hankel_transform("));
        // the sine: (pi/2) exp(-b) bessel_i(0, b s) for s < 1
        same(&mut e, "vv = hankel_transform(sin(r) / (b^2 + r^2), r, s)", "conditional_expression(pi/2*exp(-(b^2)^(1/2))*bessel_i(0, (b^2)^(1/2)*s), s < 1)");
        holds(&mut e, "vv = vv[1]; true");
        // r/(r^2 + b^2) = integrate(exp(-b u) sin(r u), [u, 0, inf]) and the transforms of cos(c r)/r give the
        // integral as (1/2) integrate(exp(-b (1 + v))/sqrt(s^2 - v^2), [v, -s, s]) for s < 1 (the oscillatory
        // integral itself defeats the quadrature): an independent route
        for s in ["1/2", "1/5"] {
            holds(&mut e, &format!("abs(n(rewrite(vv, [b -> 3/2, s -> {s}]), 20) - n_integrate(exp(-3/2*(1 + v))/(2*sqrt({s}^2 - v^2)), [v, -{s}, {s}])) < 10^-9"));
        }
        assert!(ev(&mut e, "inverse_hankel_transform(sin(s) / (b^2 + s^2), s, r, assumptions: r > 2)").starts_with("inverse_hankel_transform("));
        same(&mut e, "inverse_hankel_transform(cos(s) / (1 + s^2), s, r)", "conditional_expression(bessel_k(0, r)*cosh(1), r > 1)");
    });
}

#[test]
fn generalized_transforms_of_cos_and_sin() {
    timed("cos, sin", 120, || {
        let mut e = CoreEngine::new();
        same(&mut e, "hankel_transform(cos(r), r, s)", "-heaviside_theta(1 - s)/(1 - s^2)^(3/2)");
        same(&mut e, "inverse_hankel_transform(cos(s), s, r)", "-heaviside_theta(1 - r)/(1 - r^2)^(3/2)");
        // the derivative in a of the transform of sin(a r)/r (heaviside_theta(a^2 - s^2)/sqrt(a^2 - s^2), by the
        // Mellin product) is the transform of cos(a r), on both sides of s = a
        for (a, s) in [("1", "1/2"), ("2", "3/2"), ("1", "3/2"), ("3/2", "1/3")] {
            holds(&mut e, &format!(
                "abs(n(rewrite(d(hankel_transform(sin(a*r)/r, r, s), a), [a -> {a}, s -> {s}]), 20) - n(rewrite(hankel_transform(cos(a*r), r, s), [a -> {a}, s -> {s}]), 20)) < 10^-15"
            ));
        }
        // the Abel limit: the transform of exp(-(e - i a) r) with damping e = 1/2 against its integral, and the
        // value of sin(2 r) at e = 0
        holds(&mut e, "{ let p = 1/2 - im*2; let v = n(re(p/(p^2 + (5/2)^2)^(3/2)), 20); abs(v - n_integrate(exp(-r/2)*cos(2*r)*bessel_j(0, 5/2*r)*r, [r, 0, inf])) < 10^-8 }");
        holds(&mut e, "abs(n(rewrite(hankel_transform(sin(2*r), r, s), s -> 5/2), 20) - n(-2/((5/2)^2 - 4)^(3/2), 20)) < 10^-15");
        same(&mut e, "hankel_transform(sin(2*r), r, 1)", "0");
    });
}

#[test]
fn sine_integral_and_step() {
    timed("sin_integral(r^2), step", 120, || {
        let mut e = CoreEngine::new();
        // sin_integral(r^2) - pi/2 = O(r^-2): its transform converges; the constant pi/2 adds 0 for s > 0
        same(&mut e, "vv = hankel_transform(sin_integral(r^2), r, s)", "-2*sin(s^2/4)/s^2");
        for s in ["2", "5/2"] {
            agrees(&mut e, "(sin_integral(r^2) - pi/2)*bessel_j(0, s*r)*r", "[]", s);
        }
        same(&mut e, "inverse_hankel_transform(sin_integral(s^2), s, r)", "-2*sin(r^2/4)/r^2");
        // the Mellin transform behind it, against its integral
        holds(&mut e, "abs(n(rewrite(mellin_transform(sin_integral(r^2) - pi/2, r, z), z -> 1/2), 20) - n_integrate((sin_integral(r^2) - pi/2)*r^(-1/2), [r, 0, inf])) < 10^-10");
        // unit_step(a - r): 0 for a < 0, so unit_step(a) stays unless a > 0 is assumed
        same(&mut e, "function_expand(hankel_transform(unit_step(a - r), r, s))", "a*bessel_j(1, a*s)*unit_step(a)/s");
        same(&mut e, "hankel_transform(unit_step(a - r), r, s, assumptions: a > 0)", "a*bessel_j(1, a*s)/s");
        holds(&mut e, "abs(n(rewrite(hankel_transform(unit_step(a - r), r, s, assumptions: a > 0), [a -> 3/2, s -> 7/10]), 20) - n_integrate(bessel_j(0, 7/10*r)*r, [r, 0, 3/2], working_precision: 20)) < 10^-12");
        // the Mellin transform of the step keeps its strip
        same(&mut e, "mellin_transform(unit_step(1 - r), r, z, generate_conditions: true)", "conditional_expression(1/z, re(z) > 0)");
    });
}

#[test]
fn balanced_product_with_unevaluable_side_declines_quickly() {
    // the inversion gives both sides (2F1 inside, 3F2 outside); the 3F2 does not evaluate at the jump, so the
    // fixed-node check of the forward Mellin integral fails at its first node and the transform stays unevaluated
    timed("(1 - J0(a r))/r^2, order 1", 6, || {
        let mut e = CoreEngine::new();
        assert!(ev(&mut e, "hankel_transform((1 - bessel_j(0, a*r))/r^2, r, s, 1)").starts_with("hankel_transform("));
    });
    // the fixed-node rule against a known integral with endpoint singularities at 0 and k
    timed("double-exponential nodes", 6, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "abs(calc_mb_de(u => u^(-1/2)*exp(-u), u => u^(-1/2)*exp(-u), 1) - n(sqrt(pi))) < 10^-12");
        holds(&mut e, "abs(calc_mb_de(u => log(abs(1 - u))/(1 + u^2), u => log(abs(1 - u))/(1 + u^2), 1) - n_integrate(log(abs(1 - u))/(1 + u^2), [u, 0, 1, inf])) < 10^-9");
    });
}
