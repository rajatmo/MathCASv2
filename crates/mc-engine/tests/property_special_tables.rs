//! Properties of special functions read from the table of their facts (Lommel s and t, parabolic cylinder D,
//! q-factorial, Euler function, modular functions, spherical Hankel functions, Weierstrass functions of a degenerate
//! lattice), each answer checked independently: by values at exact points (30 digits), by symmetry, or by the
//! elementary form the function takes. Every call is bounded in time.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The answer, within a time bound.
fn timed(e: &mut CoreEngine, src: &str, secs: u64) -> String {
    let t = Instant::now();
    let r = ev(e, src);
    assert!(t.elapsed() < Duration::from_secs(secs), "{src} took {:?}", t.elapsed());
    r
}

/// A machine value of a closed real expression.
fn num(e: &mut CoreEngine, src: &str) -> f64 {
    let r = ev(e, &format!("n({src}, 30)"));
    let r = ev(e, &format!("n({r})"));
    r.parse::<f64>().unwrap_or_else(|_| panic!("{src} = {r}"))
}

/// Is the closed expression 0 within `1e-20` at 30 digits (a ball around 0 or a small number)?
fn tiny(e: &mut CoreEngine, src: &str) -> bool {
    let r = ev(e, &format!("n({src}, 30)"));
    let r = ev(e, &format!("n({r})"));
    if let Some(rad) = r.strip_prefix("ball(0.0, ").and_then(|t| t.strip_suffix(')')) {
        return rad.parse::<f64>().is_ok_and(|v| v < 1e-20);
    }
    r.parse::<f64>().is_ok_and(|v| v.abs() < 1e-20)
}

#[test]
fn lommel_s_and_t_of_an_integer_order_are_entire() {
    let mut e = CoreEngine::new();
    // z^3 times an even entire series: odd, so no cut on the negative axis
    for (f, a) in [("lommel_s1(2, 1/3, z)", "lommel_s1(2, 1/3, 3/2) + lommel_s1(2, 1/3, -3/2)"), ("lommel_t1(2, 1/2, z)", "lommel_t1(2, 1/2, 3/2) + lommel_t1(2, 1/2, -3/2)")] {
        assert_eq!(timed(&mut e, &format!("function_singularities({f}, z)"), 5), "false");
        assert!(tiny(&mut e, &format!("abs({a})")), "{a}");
    }
    // a non-integer order: the cut; an integer order below -1: a pole at 0 (s ~ 4/(3 z) for mu = -2, nu = 1/2)
    assert_eq!(timed(&mut e, "function_singularities(lommel_s1(1/2, 1/3, z), z)", 5), "z <= 0");
    assert_eq!(timed(&mut e, "function_singularities(lommel_s1(-3, 1/2, z), z)", 5), "z == 0");
    assert!((num(&mut e, "lommel_s1(-2, 1/2, 1/1000)") - 1333.33).abs() < 0.01);
    // mu - nu = -1: undefined for every z
    assert_eq!(ev(&mut e, "n(lommel_s1(1, 2, 3/2))"), "complex_infinity");
    let s = timed(&mut e, "function_singularities(lommel_s1(m, n_u, z), z)", 5);
    assert!(s.contains("not_element(m, integers) and z <= 0") && s.contains("element(m, integers) and m < -1 and z == 0"), "{s}");
}

#[test]
fn parabolic_cylinder_d_tables() {
    let mut e = CoreEngine::new();
    assert_eq!(timed(&mut e, "table(function_injective(parabolic_cylinder_d(n_u, x), x), n_u, range(-3, 3))", 5), "[true, true, true, false, false, false, false]");
    assert_eq!(timed(&mut e, "table(function_surjective(parabolic_cylinder_d(n_u, x), x), n_u, range(-3, 3))", 5), "[false, false, false, false, false, false, false]");
    // D_(-1) positive and decreasing at exact points
    let vs: Vec<f64> = ["-3", "-1", "0", "2", "5"].iter().map(|x| num(&mut e, &format!("parabolic_cylinder_d(-1, {x})"))).collect();
    assert!(vs.iter().all(|&v| v > 0.0) && vs.windows(2).all(|w| w[0] > w[1]), "{vs:?}");
}

#[test]
fn q_factorial_and_euler_function() {
    let mut e = CoreEngine::new();
    assert_eq!(timed(&mut e, "function_injective(q_factorial(z, q), z, assumptions: q > 0)", 5), "false");
    assert_eq!(timed(&mut e, "function_surjective(q_factorial(z, q), z, assumptions: q > 0)", 5), "false");
    assert_eq!(timed(&mut e, "function_analytic(q_factorial(z, q), [z, q])", 5), "false");
    // between the poles -2 and -1 the same value twice: values near both ends exceed the value in the middle
    let (l, m, r) = (num(&mut e, "q_factorial(-199/100, 1/2)"), num(&mut e, "q_factorial(-3/2, 1/2)"), num(&mut e, "q_factorial(-101/100, 1/2)"));
    assert!(l.signum() == m.signum() && r.signum() == m.signum() && l.abs() > m.abs() && r.abs() > m.abs(), "{l} {m} {r}");
    // the singular set holds the poles at the negative integers for q < 0 as for q > 0 (the factor 1 - q^0 of the
    // denominator): q_factorial(-1, -1/2) is infinite
    let sing = timed(&mut e, "function_singularities(q_factorial(z, q), z)", 5);
    for (z, q) in [("-1", "-1/2"), ("-2", "1/2"), ("-3", "-2"), ("-1/2", "-1/2")] {
        assert_eq!(timed(&mut e, &format!("rewrite({sing}, [z -> {z}, q -> {q}])"), 5), "true", "{sing} at {z}, {q}");
    }
    for (z, q) in [("2", "-1/2"), ("-1/2", "1/2")] {
        assert_eq!(timed(&mut e, &format!("rewrite({sing}, [z -> {z}, q -> {q}])"), 5), "false", "{sing} at {z}, {q}");
    }
    let near = num(&mut e, "q_factorial(-1.0001, -0.5)");
    assert!(near.abs() > 500.0, "{near}");
    assert_eq!(timed(&mut e, "function_surjective(q_pochhammer(x), x)", 5), "false");
    assert_eq!(timed(&mut e, "function_surjective(factorial2(z), z)", 5), "false");
}

#[test]
fn modular_functions_and_spherical_hankel() {
    let mut e = CoreEngine::new();
    assert_eq!(timed(&mut e, "function_period(dedekind_eta(x + im), x)", 5), "24");
    assert_eq!(timed(&mut e, "function_period(modular_lambda(x + im), x)", 5), "2");
    assert_eq!(timed(&mut e, "function_injective(klein_invariant_j(x), x, complexes)", 5), "false");
    // J(i) = J(i + 1)
    assert!(tiny(&mut e, "abs(klein_invariant_j(im + 1) - klein_invariant_j(im))"));
    assert_eq!(timed(&mut e, "function_injective(spherical_hankel_h1(1, x), x, complexes)", 5), "false");
    assert_eq!(timed(&mut e, "function_meromorphic(product_log(x), x)", 5), "false");
    assert_eq!(timed(&mut e, "function_domain(acosh(z), z, complexes)", 5), "true");
    assert_eq!(timed(&mut e, "function_domain(alternating_harmonic_number(z), z, complexes)", 5), "re(z) > -1 or not_element(z, integers)");
    // the pole at -1
    assert!(num(&mut e, "abs(alternating_harmonic_number(-9999/10000))") > 1000.0);
}

#[test]
fn degenerate_weierstrass_functions_onto() {
    let mut e = CoreEngine::new();
    // g2^3 = 27 g3^2: p = -1/2 + 3/2 csc(x 6^(1/2)/2)^2 >= 1
    assert_eq!(timed(&mut e, "function_surjective(weierstrass_p(x, [3, 1]), x)", 5), "false");
    assert!(num(&mut e, "weierstrass_p(1, [3, 1])") >= 1.0);
    for f in ["weierstrass_p_prime", "weierstrass_sigma", "weierstrass_zeta"] {
        assert_eq!(timed(&mut e, &format!("function_surjective({f}(x, [3, 1]), x)"), 5), "true", "{f}");
    }
    assert_eq!(timed(&mut e, "function_range(csc(x*2^(1/2))^2, x, y)", 5), "y >= 1");
    assert_eq!(timed(&mut e, "function_range(tan(x) + cot(x), x, y)", 5), "y <= -2 or y >= 2");
}

#[test]
fn injectivity_by_order_parity_and_convexity_on_lines() {
    let mut e = CoreEngine::new();
    assert_eq!(timed(&mut e, "function_injective(beta_regularized(x, 2*k, 1), x, assumptions: k > 0 and element(k, integers))", 5), "false");
    assert_eq!(timed(&mut e, "function_injective(beta_regularized(x, 2*k + 1, 1), x, assumptions: k > 0 and element(k, integers))", 5), "true");
    assert_eq!(timed(&mut e, "function_injective(bessel_i(n_u, x), x, assumptions: element((n_u + 1)/2, integers))", 5), "true");
    // dn(x|1/2) has a maximum at 0 and a minimum at K(1/2): neither convex nor concave in x
    assert_eq!(timed(&mut e, "function_convexity(jacobi_dn(x, m), [x, m])", 5), "indeterminate");
    assert_eq!(timed(&mut e, "function_convexity(x^4 + y^2, [x, y])", 5), "1");
}
