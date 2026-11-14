//! Round 40: total derivatives keep the constants they were taken under; shifts of integrals and limits respect
//! the variables those calls bind. Shifts are checked on known functions against the shifted closed form.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn total_derivative_constants() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "dt(a*x^2 + b, x, constants: [a])"), "dt(b, x, constants: [a]) + 2*a*x");
    // variations: two constants; a second derivative (the constants survive the second step)
    assert_eq!(ev(&mut e, "dt(a*x^2 + b*c, x, constants: [a, c])"), "c*dt(b, x, constants: [a, c]) + 2*a*x");
    assert_eq!(ev(&mut e, "dt(a*x^3 + b, [x, 2], constants: [a])"), "dt(b, [x, 2], constants: [a]) + 6*a*x");
    // without constants nothing changes
    assert_eq!(ev(&mut e, "dt(x^2 + b, x)"), "dt(b, x) + 2*x");
}

#[test]
fn shifts_of_binding_calls() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "discrete_shift(integrate(f(i), i), i)"), "integrate(f(1 + i), i)");
    assert_eq!(ev(&mut e, "discrete_shift(limit(f(i), i -> j), i)"), "limit(f(i), i -> j)");
    // variations: a definite integral in i is constant; an antiderivative of a known integrand shifted equals the
    // antiderivative at i + 1 up to a constant
    assert_eq!(ev(&mut e, "discrete_shift(integrate(f(i), [i, 0, 1]), i)"), "integrate(f(i), [i, 0, 1])");
    assert_eq!(ev(&mut e, "simplify(d(discrete_shift(integrate(cos(i)*i, i), i) - rewrite(integrate(cos(t)*t, t), t -> i + 1), i))"), "0");
    // a limit whose point moves with i: f(i + 1, x) at x -> i + 1, checked with a known function
    assert_eq!(ev(&mut e, "discrete_shift(limit(f(i, j), j -> i), i)"), "limit(f(1 + i, 1 + j), j -> i)");
    assert_eq!(ev(&mut e, "expand(discrete_shift(limit((j^2 - i^2)/(j - i), j -> i), i))"), "2 + 2*i");
}
