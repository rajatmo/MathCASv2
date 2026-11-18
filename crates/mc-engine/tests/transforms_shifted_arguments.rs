//! Transforms of shifted arguments: bilateral Laplace transforms of a step `unit_step(k (t - a))` times a function
//! (the one-sided transform of the shifted function, on the right or on the left of a), Mellin transforms of
//! sin/cos of `g(x) + c` by the addition theorems, and the conditions of transforms in several variables joined.
//! Values checked against numerical integrals or the defining formulas at sample points. Time-bounded.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn close(e: &mut CoreEngine, a: &str, b: &str) {
    let r = ev(e, &format!("abs(n({a}) - n({b})) < 10^-8*(1 + abs(n({b})))"));
    assert_eq!(r, "true", "{a} vs {b}: {} vs {}", ev(e, &format!("n({a})")), ev(e, &format!("n({b})")));
}

#[test]
fn bilateral_laplace_of_steps_on_either_side() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    // right of the step
    close(&mut e, "bilateral_laplace_transform(unit_step(t - 1/2)*exp(-t), t, 2)", "n_integrate(exp(-3*t), [t, 1/2, inf])");
    assert_eq!(ev(&mut e, "bilateral_laplace_transform(unit_step(t - 0.5)*exp(-t), t, 2.0)").parse::<f64>().map(|v| (v - 0.0743767200494).abs() < 1e-9), Ok(true));
    // left of the step: integral over t < a
    close(&mut e, "bilateral_laplace_transform(unit_step(3 - 2*t)*exp(t), t, -1)", "n_integrate(exp(2*t), [t, -inf, 3/2])");
    close(&mut e, "bilateral_laplace_transform(unit_step(2 - t)*exp(3*t), t, 1)", "n_integrate(exp(2*t), [t, -inf, 2])");
    close(&mut e, "rewrite(bilateral_laplace_transform(unit_step(-t - 1)*t^2, t, s), s -> -2)", "n_integrate(t^2*exp(2*t), [t, -inf, -1])");
    close(&mut e, "rewrite(bilateral_laplace_transform(unit_step(t - a)*sin(t), t, s), [a -> 1/3, s -> 3/2])", "n_integrate(sin(t)*exp(-3*t/2), [t, 1/3, inf])");
    // strips: left of the step the region is a left half-plane
    assert_eq!(ev(&mut e, "bilateral_laplace_transform(unit_step(-t)*exp(t), t, s)"), "conditional_expression((1 - s)^-1, re(1 - s) > 0)");
    // without conditions asked for: the value alone
    assert!(!ev(&mut e, "bilateral_laplace_transform(unit_step(t + 5)*exp(-t), t, s, generate_conditions: false)").contains("conditional"));
    // a step direction of unknown sign, and a divergent transform: unevaluated
    assert!(ev(&mut e, "bilateral_laplace_transform(unit_step(c*t - 1)*exp(-t), t, s)").starts_with("bilateral_laplace_transform("));
    assert!(ev(&mut e, "bilateral_laplace_transform(unit_step(t)*exp(t), t, 1/2)").starts_with("bilateral_laplace_transform("));
    // two variables: the condition of each step joined
    let v = ev(&mut e, "vv = bilateral_laplace_transform(unit_step(y)*unit_step(x - y), [x, y], [s, p]); vv");
    assert!(v.starts_with("conditional_expression(") && v.ends_with(", re(s) > 0 and re(p + s) > 0)"), "{v}");
    assert_eq!(ev(&mut e, "rewrite(vv, [s -> 2, p -> 1])"), "1/6");
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}

#[test]
fn mellin_of_shifted_sines_and_cosines() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    close(&mut e, "mellin_transform(sin(x + 1), x, 1/2)", "gamma(1/2)*sin(1 + pi/4)");
    assert_eq!(ev(&mut e, "mellin_transform(sin(x + 1.5), x, 0.5)").parse::<f64>().map(|v| (v - 1.3388305034964).abs() < 1e-9), Ok(true));
    // cos(2x - c) = cos(c) cos(2x) + sin(c) sin(2x)
    close(&mut e, "rewrite(mellin_transform(cos(2*x - c), x, s), [c -> 1/3, s -> 2/5])", "2^(-2/5)*gamma(2/5)*cos(pi/5 - 1/3)");
    // a power inside: M[sin(c - y^2)](t) = M[sin(c - u)](t/2)/2
    close(&mut e, "rewrite(mellin_transform(sin(c - y^2), y, t), [c -> 1/3, t -> 1])", "gamma(1/2)*sin(1/3 - pi/4)/2");
    let c = ev(&mut e, "mellin_transform(sin(x + 1), x, s, generate_conditions: true)");
    assert!(c.starts_with("conditional_expression(") && c.ends_with(", 0 < re(s) and re(s) < 1)"), "{c}");
    // two variables: sin(x - y^2), with the strips of both steps
    let v = ev(&mut e, "vv = mellin_transform(sin(x - y^2), [x, y], [s, t]); vv");
    assert!(!v.contains("mellin"), "{v}");
    close(&mut e, "rewrite(vv, [s -> 1/3, t -> 1])", "cos(pi*(2 + 1 - 2/3)/4)*gamma(1/3)*gamma(1/2)/2");
    let w = ev(&mut e, "mellin_transform(sin(x - y^2), [x, y], [s, t], generate_conditions: true)");
    assert!(w.starts_with("conditional_expression(") && w.contains("re(s) < 1") && w.contains("re(t) < 1"), "{w}");
    // outside the strip: unevaluated
    assert!(ev(&mut e, "mellin_transform(sin(x + 1), x, 3/2)").starts_with("mellin_transform("));
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}
