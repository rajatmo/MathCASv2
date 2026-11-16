//! Decisions left open by the decision service and the simplifier, singular sets of roots and absolute values at
//! zeros of even order, and linear algebra of balls at their own precision. Each answer is checked by a second route.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn order_of_equal_constants() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "pi < pi"), "false");
    assert_eq!(ev(&mut e, "pi <= pi"), "true");
    assert_eq!(ev(&mut e, "e > e"), "false");
    // independent: the difference is exactly 0
    assert_eq!(ev(&mut e, "pi - pi"), "0");
    assert_eq!(ev(&mut e, "pi < pi + 10^-40"), "true");
}

#[test]
fn facts_joined_through_another_variable() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    // a > b and a > -b: 2 a > 0
    assert_eq!(ev(&mut e, "refine(a > 0, a > b and a > -b)"), "true");
    assert_eq!(ev(&mut e, "refine(conjugate(sqrt(a)), a > b and a > -b)"), "a^(1/2)");
    assert_eq!(
        ev(&mut e, "full_simplify(dot(conjugate_transpose(cholesky([[a, b], [b, a]])), cholesky([[a, b], [b, a]])), a > b and a > -b)"),
        "[[a, b], [b, a]]"
    );
    // independent: a = 1, b = 1/2 satisfies the facts and gives the matrix back
    assert_eq!(ev(&mut e, "c = cholesky([[1, 1/2], [1/2, 1]]); simplify(dot(conjugate_transpose(c), c))"), "[[1, 1/2], [1/2, 1]]");
    assert!(t.elapsed() < Duration::from_secs(30));
}

#[test]
fn together_keeps_a_repeated_factor() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "together(find_generating_function([3, 7, 13, 21, 31, 43, 57], x))"), "(-3 - x^2 + 2*x)*(-1 + x)^-3");
    assert_eq!(ev(&mut e, "together(1/x + 1/(x - 1)^3)"), "(-1 + -3*x^2 + 4*x + x^3)/x*(-1 + x)^-3");
    // a single factor is left as before
    assert_eq!(ev(&mut e, "together(1/(1 - x))"), "-(-1 + x)^-1");
    // independent: the two forms agree
    assert_eq!(ev(&mut e, "simplify(together(1/x + 1/(x - 1)^3) - (1/x + 1/(x - 1)^3))"), "0");
}

#[test]
fn integer_instances_proved_absent() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    assert_eq!(ev(&mut e, "find_instance(x^2 < 2 and x > 1, x, integers)"), "[]");
    assert_eq!(ev(&mut e, "find_instance(x^2 < 5 and x > 1, x, integers)"), "[[x -> 2]]");
    // independent: the real solutions lie strictly between 1 and 2
    assert_eq!(ev(&mut e, "reduce(x^2 < 2 and x > 1, x, reals)"), "1 < x and x < 2^(1/2)");
    assert!(t.elapsed() < Duration::from_secs(20));
}

#[test]
fn series_of_roots_on_the_assumed_side() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "series(sqrt(1/x^2 + 1), x, 0, 3, assumptions: x < 0)"), "-x^-1 - x/2 + x^3/8 + O(x^4)");
    assert_eq!(ev(&mut e, "series(sqrt(x^2), x, 0, 3, assumptions: x < 0)"), "-x + O(x^4)");
    assert_eq!(ev(&mut e, "series(sqrt(1/x^2 + 1), x, 0, 3, assumptions: x > 0)"), "x^-1 + x/2 - x^3/8 + O(x^4)");
    // independent: at x = -1/100 the value and the three terms agree to 1e-8
    let d = ev(&mut e, "n(abs(sqrt(1/x^2 + 1) - (-1/x - x/2 + x^3/8)) /. x -> -1/100, 20)");
    let v: f64 = d.split('`').next().unwrap().parse().unwrap();
    assert!(v < 1e-8, "{d}");
}

#[test]
fn roots_and_absolute_values_at_zeros_of_even_order() {
    let mut e = CoreEngine::new();
    // sqrt(x^2) = |x| and (x^4)^(1/4) = |x| are not analytic at 0; (x^4)^(1/2) = x^2 and abs(x^2) are
    assert_eq!(ev(&mut e, "function_analytic(sqrt(x^2), x)"), "false");
    assert_eq!(ev(&mut e, "function_analytic((x^4)^(1/4), x)"), "false");
    assert_eq!(ev(&mut e, "function_analytic((x^4)^(1/2), x)"), "true");
    assert_eq!(ev(&mut e, "function_analytic(abs(x^2), x)"), "true");
    assert_eq!(ev(&mut e, "function_singularities(abs(x^3), x)"), "x == 0");
    // sqrt(x^2 + a) is continuous for a >= 0 (at a = 0 it is |x|), analytic for a > 0
    assert_eq!(ev(&mut e, "function_continuous(sqrt(x^2 + a), x)"), "conditional_expression(true, a >= 0)");
    assert_eq!(ev(&mut e, "function_analytic(sqrt(x^2 + a), x)"), "conditional_expression(true, a > 0)");
    assert_eq!(ev(&mut e, "function_analytic(abs(x^2 + a), x)"), "conditional_expression(true, a >= 0)");
    assert_eq!(ev(&mut e, "function_discontinuities(sqrt(x^2 - 2*x + 1), x)"), "false");
    assert_eq!(ev(&mut e, "function_discontinuities(sqrt(x^3), x)"), "x^3 <= 0");
    // independent: the one-sided difference quotients of sqrt(x^2) at 0 are -1 and 1
    assert_eq!(ev(&mut e, "[limit(sqrt(x^2)/x, x -> 0, direction: from_below), limit(sqrt(x^2)/x, x -> 0, direction: from_above)]"), "[-1, 1]");
}

#[test]
fn ranges_without_closed_critical_points() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_range(binomial(x, 2), x, y)"), "y >= -1/8");
    // sinh(x)/sqrt(x) increases on x > 0 from 0 (tanh(x) = 2 x has no positive root)
    assert_eq!(ev(&mut e, "function_range(bessel_i(1/2, x), x, y)"), "y > 0");
    assert_eq!(ev(&mut e, "function_range(struve_l(1/2, x), x, y)"), "y > 0");
    // even, monotonic on each side of 0, -1 at 0, unbounded at the ends
    assert_eq!(ev(&mut e, "function_range(legendre_q(1, x), x, y)"), "y >= -1");
    // independent: values at 1/2 and 0.999 lie in the range, the value at 0 is its least
    assert_eq!(ev(&mut e, "[n(legendre_q(1, 1/2)) > -1, n(legendre_q(1, 999/1000)) > 2, legendre_q(1, 0)]"), "[true, true, -1]");
}

#[test]
fn eigenvectors_and_svd_of_balls_at_their_precision() {
    let mut e = CoreEngine::new();
    let es = ev(&mut e, "m = n([[1, 2], [3, 4]], 30); es = eigensystem(m); es");
    assert!(es.contains("0.41597355791928427257510538486"), "{es}");
    // independent: the residual m v - lambda v is below 10^-30
    let r = ev(&mut e, "max(abs(flatten(map(&(dot(m, es[2][&1]) - es[1][&1]*es[2][&1]), [1, 2])))) < 10^-30");
    assert_eq!(r, "true");
    let f = ev(&mut e, "f = svd(m); f[2]");
    assert!(f.starts_with("[[5.4649857042190426504511884932"), "{f}");
    assert_eq!(ev(&mut e, "max(abs(flatten(dot(f[1], f[2], transpose(f[3])) - m))) < 10^-30"), "true");
    assert_eq!(ev(&mut e, "max(abs(flatten(dot(transpose(f[1]), f[1]) - identity_matrix(2)))) < 10^-30"), "true");
    // a 2 x 3 matrix through its transpose
    assert_eq!(ev(&mut e, "a = n([[1, 2, 3], [3, 4, 1]], 30); g = svd(a); max(abs(flatten(dot(g[1], g[2], transpose(g[3])) - a))) < 10^-30"), "true");
    // equal singular values: the machine factors stand
    assert_eq!(ev(&mut e, "svd(n([[1, 0], [0, 1]], 30))[2]"), "[[1.0, 0.0], [0.0, 1.0]]");
    // the minimal polynomial of a machine matrix is monic
    assert_eq!(ev(&mut e, "coefficient(matrix_minimal_polynomial([[1., 2.], [3., 4.]], x), x, 2)"), "1.0");
}
