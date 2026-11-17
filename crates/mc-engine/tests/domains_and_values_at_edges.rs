//! Domains of functions of real arguments over the complexes, domains at special parameters, values on and beyond
//! cuts and at infinity, orders by keys, and linear-memory random polygons: each answer against an independent
//! route (the function evaluated at a point of the set, a numeric integral, the definition), every evaluation
//! bounded in time.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

/// `src` evaluated within 20 s (asserted).
fn ev(e: &mut CoreEngine, src: &str) -> String {
    let t0 = Instant::now();
    let out = e
        .eval(&format!("time_constrained({{ {src} }}, 20)"), &StopFlag::new())
        .map(|r| r.printed)
        .unwrap_or_else(|err| format!("error: {err:?}"));
    assert!(t0.elapsed() < Duration::from_secs(25), "{src}: {:?}", t0.elapsed());
    out
}

fn holds(e: &mut CoreEngine, src: &str) -> bool {
    ev(e, src) == "true"
}

#[test]
fn real_only_functions_over_the_complexes() {
    let mut e = CoreEngine::new();
    for f in ["ramp", "real_abs", "real_sign", "unit_box", "unit_triangle", "unit_step", "clip"] {
        assert_eq!(ev(&mut e, &format!("function_domain({f}(x), x, complexes)")), "element(x, reals)", "{f}");
        // independent: the call at a non-real point stays unevaluated
        assert_eq!(ev(&mut e, &format!("head({f}(im)) === {f}")), "true", "{f}");
    }
    assert_eq!(ev(&mut e, "function_domain(max(x1, x2), [x1, x2], complexes)"), "element(x1, reals) and element(x2, reals)");
    assert!(holds(&mut e, "head(max(im, 2)) === max and head(min(2, im)) === min"));
    // over the reals nothing changes
    assert_eq!(ev(&mut e, "function_domain(unit_box(x), x)"), "true");
}

#[test]
fn domains_at_special_parameters() {
    let mut e = CoreEngine::new();
    // U_(-1) = 0 everywhere (U_(-n) = -U_(n-2)): every integer degree is a polynomial
    assert_eq!(ev(&mut e, "function_domain(chebyshev_u(n, x), x)"), "element(-n, integers) or x > -1");
    assert_eq!(ev(&mut e, "[chebyshev_u(-1, x), chebyshev_u(-1, -3/2)]"), "[0, 0]");
    // 0F1(; a; z) is undefined at a pole of its parameter
    assert_eq!(ev(&mut e, "function_domain(hypergeometric_0f1(a, z), z, complexes)"), "re(a) > 0 or not_element(a, integers)");
    assert_eq!(ev(&mut e, "function_domain(hypergeometric_0f1(1/3, z), z, complexes)"), "true");
    // Q(a, 0) = Gamma(a, 0)/Gamma(a) is undefined for a = 0, -1, ...; Q(-1, x) = 0 for x != 0
    assert!(holds(&mut e, "d = function_domain(gamma_regularized(a, x), x); [rewrite(d, [a -> -1, x -> 0]), rewrite(d, [a -> -1, x -> -1]), rewrite(d, [a -> 1/2, x -> -1])] == [false, true, false]"));
    // the discontinuity of clip at its lower bound only where the value there differs from the bound
    assert_eq!(ev(&mut e, "function_discontinuities(clip(x, [lo, hi], [v, hi]), x)"), "x == lo and v != lo");
}

#[test]
fn values_beyond_cuts_and_at_infinity() {
    let mut e = CoreEngine::new();
    // Pi(n | 0): pi/(2 sqrt(1 - n)) off the cut, the principal value 0 beyond 1 (as the numeric value), a pole at 1
    assert_eq!(ev(&mut e, "[elliptic_pi(2, 0), elliptic_pi(1/2, 0), elliptic_pi(sqrt(2), 0)]"), "[0, (1/2)*pi*2^(1/2), 0]");
    assert_eq!(ev(&mut e, "elliptic_pi(1, 0)"), "failure(pole, elliptic_pi(1, 0))");
    assert!(holds(&mut e, "abs(elliptic_pi(2., 0.)) < 10^-12 and abs(elliptic_pi(3., 0.)) < 10^-12"));
    assert!(holds(&mut e, "abs(n(elliptic_pi(1/3, 0)) - elliptic_pi(1/3., 0.)) < 10^-12"));
    // |log_gamma| grows without bound in every direction
    assert_eq!(ev(&mut e, "[log_gamma(complex_infinity), gamma(complex_infinity)]"), "[complex_infinity, indeterminate]");
    // closed terms: the modulus of exp of a non-real number, the argument of a negative closed real
    assert_eq!(ev(&mut e, "[abs(exp(im)), abs(exp(2 + im)), abs(exp(-1 + 2*im)), arg(1 - sqrt(2)), arg(sqrt(3) - 1)]"), "[1, exp(2), exp(-1), pi, 0]");
    assert!(holds(&mut e, "abs(n(abs(exp(2 + im))) - abs(exp(2. + 1.*im))) < 10^-12"));
    // Dawson's function for large machine arguments against the 30-digit value of its definition
    for x in ["20.5", "26.7", "30.", "-40.", "1000."] {
        assert!(holds(&mut e, &format!("abs(dawson_f({x}) - n(dawson_f(set_precision({x}, 40)), 30)) < 10^-14*abs(dawson_f({x}))")), "{x}");
    }
    // inverse_jacobi_sn beyond its real values: F(asin v | m), mapped back by sn
    assert!(holds(&mut e, "z = n(inverse_jacobi_sn(2, 1/3)); abs(jacobi_sn(z, 1/3.) - 2) < 10^-10 and abs(z - elliptic_f(asin(2.), 1/3.)) < 10^-12"));
    // the q-factorial of a negative q at a non-integer count holds the principal power q^(n + 1): complex
    assert!(holds(&mut e, "v = n(q_factorial(56/27, -1/2), 20); w = n((3/2)^(-56/27)*q_pochhammer(-1/2, -1/2)/q_pochhammer((-1/2)^(83/27), -1/2), 20); abs(v - w) < 10^-15 and abs(im_part(v)) > 10^-3"));
    assert!(holds(&mut e, "abs(q_factorial(2.5, -0.5) - n((3/2)^(-5/2)*q_pochhammer(-1/2, -1/2)/q_pochhammer((-1/2)^(7/2), -1/2))) < 10^-12"));
}

#[test]
fn transforms_and_sides() {
    let mut e = CoreEngine::new();
    // the strip lies right of the pole of 1/s (Mellin transform of sqrt(pi) erfc(x) converges for re(s) > 0 only)
    assert_eq!(ev(&mut e, "inverse_mellin_transform(gamma(1/2 + s/2)/s, s, x, generate_conditions: true)"), "conditional_expression((1 - erf(x))*pi^(1/2), re(s) > 0)");
    assert!(holds(&mut e, "abs(n_integrate(x^(1/2 - 1)*pi^(1/2)*erfc(x), [x, 0, inf]) - n(gamma(3/4)/(1/2))) < 10^-6"));
    // a parameter entering only through (r^2 + a^2): the half-plane re(a^2) > 0
    assert_eq!(ev(&mut e, "hankel_transform(1/sqrt(r^2 + a^2), r, s, generate_conditions: true)[2]"), "re(a^2) > 0 and s >= 0");
    // multiplying an inequation by the sides of an equation: at c = 0 both products are 0
    assert_eq!(ev(&mut e, "multiply_sides(a != b, c == d)"), "piecewise([[a*c != b*d, c != 0]], a*c == b*d)");
    assert_eq!(ev(&mut e, "multiply_sides(a < b, c == d)"), "piecewise([[a*c < b*d, c > 0], [a*c > b*d, c < 0]], a*c == b*d)");
    assert_eq!(ev(&mut e, "multiply_sides(a == b, c == d)"), "a*c == b*d");
}

#[test]
fn orders_by_keys_and_continued_fractions() {
    let mut e = CoreEngine::new();
    // a list of key functions: ordered by each in turn, ties in input order
    assert_eq!(ev(&mut e, "sort_by([3, -1, 1, -3], [abs])"), "[-1, 1, 3, -3]");
    assert_eq!(ev(&mut e, "reverse_sort_by(range(-3, 3), [abs])"), "[-3, 3, -2, 2, -1, 1, 0]");
    assert_eq!(ev(&mut e, "sort_by([[2, b], [1, a], [2, a]], [first, last])"), "[[1, a], [2, a], [2, b]]");
    // an association: the positions of its values in order
    assert_eq!(ev(&mut e, "ordering(assoc(1 -> c, 2 -> a, 3 -> b))"), "[2, 3, 1]");
    assert_eq!(ev(&mut e, "ordering(assoc(x -> 3, y -> 1, z -> 2))"), "[2, 3, 1]");
    // machine reals sorted as floats: the canonical order (signed zeros by it), same as through the predicate
    assert!(holds(&mut e, "xs = join(n(table(sin(k^2)*10, k, range(500))), [1.5, 1.5]); sort(xs) == sort(xs, less_equal) and xs[ordering(xs)] == sort(xs)"));
    assert_eq!(ev(&mut e, "sort([0., -0., 0., -0.])"), "[-0.0, -0.0, 0.0, 0.0]");
    // the integer part before the period
    assert_eq!(ev(&mut e, "[continued_fraction(golden_ratio), continued_fraction(1 + sqrt(2))]"), "[[1, [1]], [2, [2]]]");
    assert!(holds(&mut e, "from_continued_fraction(continued_fraction(1 + sqrt(2))) == 1 + sqrt(2)"));
}

#[test]
fn random_convex_polygons_in_linear_memory() {
    let mut e = CoreEngine::new();
    // 10^4 vertices: the polygon is convex (the turning test, linear) and the work is linear (it took 1.3 GB)
    let t0 = Instant::now();
    assert_eq!(ev(&mut e, "p = random_polygon([convex, 10000]); [length(p[1]), is_convex_polygon(p)]"), "[10000, true]");
    assert!(t0.elapsed() < Duration::from_secs(15), "{:?}", t0.elapsed());
    // independent: every turn of the polygon is to the left, and it winds once
    assert!(holds(&mut e, "q = random_polygon([convex, 200])[1]; m = length(q); every(range(m), i => { let a = q[i]; let b = q[mod(i, m) + 1]; let c = q[mod(i + 1, m) + 1]; (b[1] - a[1])*(c[2] - b[2]) - (b[2] - a[2])*(c[1] - b[1]) >= 0 })"));
    // a pentagram turns the same way at every corner but winds twice: not convex
    assert_eq!(ev(&mut e, "is_convex_polygon(polygon(table([cos(4*pi*k/5), sin(4*pi*k/5)], k, range(5))))"), "false");
    assert_eq!(ev(&mut e, "is_convex_polygon(polygon([[0, 0], [1, 0], [1, 1], [0, 1]]))"), "true");
}
