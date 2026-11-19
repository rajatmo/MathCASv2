//! Special-function values, properties and expansions, each checked against a route that does not use the code
//! under test: a defining integral or recurrence evaluated numerically, a difference quotient, substitution back
//! into the defining equation, values of the function at points of a claimed set. Every evaluation is bounded in
//! time.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

/// `src` evaluated within 30 s (asserted).
fn ev(e: &mut CoreEngine, src: &str) -> String {
    let t0 = Instant::now();
    let out = e
        .eval(&format!("time_constrained({{ {src} }}, 30)"), &StopFlag::new())
        .map(|r| r.printed)
        .unwrap_or_else(|err| format!("error: {err:?}"));
    assert!(t0.elapsed() < Duration::from_secs(35), "{src}: {:?}", t0.elapsed());
    out
}

fn holds(e: &mut CoreEngine, src: &str) -> bool {
    ev(e, src) == "true"
}

#[test]
fn positive_constants_beside_infinity() {
    let mut e = CoreEngine::new();
    for (src, want) in [("pi*inf", "inf"), ("inf/e", "inf"), ("-pi*inf", "-inf"), ("2*e^2*inf", "inf"), ("x*pi*inf", "inf*x")] {
        assert_eq!(ev(&mut e, src), want, "{src}");
    }
    // T_n(0) = cos(n pi/2) (DLMF 18.5.1) has no limit as n -> inf: the values fill [-1, 1], as cos(inf) does
    assert_eq!(ev(&mut e, "chebyshev_t(inf, 0)"), ev(&mut e, "cos(inf)"));
    assert_eq!(ev(&mut e, "chebyshev_u(inf, 0)[1, 1]"), "-1");
}

#[test]
fn complex_domains_keep_parameters_complex() {
    let mut e = CoreEngine::new();
    let d = ev(&mut e, "function_domain(gamma_regularized(a, z), z, complexes)");
    // Q(a, z) at points of the set, evaluated: finite at a non-real a
    for (a, z) in [("im", "1"), ("1 + im", "1/3"), ("1 + im", "0"), ("-2", "1/2"), ("1/2", "0")] {
        assert!(holds(&mut e, &format!("rewrite({d}, [a -> {a}, z -> {z}])")), "{d} at {a}, {z}");
        assert!(holds(&mut e, &format!("abs(n(gamma_regularized({a}, {z}))) < 10")), "{a}, {z}");
    }
    assert_eq!(ev(&mut e, "gamma_regularized(1 + im, 0)"), "1");
    // Q(0, z) = 0 for z != 0 and Q(a, 0) = 1 for re(a) > 0: no value at a = z = 0
    assert!(!holds(&mut e, &format!("rewrite({d}, [a -> 0, z -> 0])")));
}

#[test]
fn terminating_hypergeometric_series_are_defined() {
    let mut e = CoreEngine::new();
    // a = -n, c = -k with n <= k: the series stops before the zero denominator (2F1(-1, 1; -2; z) = 1 + z/2)
    assert_eq!(ev(&mut e, "function_domain(hypergeometric_2f1(-1, 1, -2, z), z)"), "true");
    assert_eq!(ev(&mut e, "hypergeometric_2f1(-1, 1, -2, 3)"), "5/2");
    // n > k: the zero denominator is reached; only z = 0 (the first term) is left
    assert_eq!(ev(&mut e, "function_domain(hypergeometric_2f1(-3, 1, -2, z), z)"), "z == 0");
    let d = ev(&mut e, "function_domain(hypergeometric_2f1(a, b, c, z), z)");
    for (pt, want) in [("[a -> -1, b -> 1, c -> -2, z -> 3]", "true"), ("[a -> 1, b -> 1, c -> -1, z -> 0]", "true"), ("[a -> 1/2, b -> 1, c -> -1, z -> 1/2]", "false"), ("[a -> 0, b -> -1, c -> 2, z -> 2]", "true")] {
        assert_eq!(ev(&mut e, &format!("rewrite({d}, {pt})")), want, "{d} at {pt}");
    }
}

#[test]
fn q_factorial_singular_set() {
    let mut e = CoreEngine::new();
    let s = ev(&mut e, "function_singularities(q_factorial(z, q), z)");
    // q < 0: the values off the integers are not real (an imaginary part of size 1e-9 at 2 +- 1e-8), so the
    // integers are isolated points of the real domain and the function is analytic nowhere
    assert!(holds(&mut e, "abs(im_part(n(q_factorial(2 + 10^-8, -1/2), 20))) > 10^-10"));
    for (z, q, want) in [("2", "-1/2", "true"), ("-1/2", "1/2", "false"), ("-2", "1/2", "true"), ("-3/2", "0", "false"), ("-1", "0", "true")] {
        assert_eq!(ev(&mut e, &format!("rewrite({s}, [z -> {z}, q -> {q}])")), want, "{s} at {z}, {q}");
    }
}

#[test]
fn inverse_weierstrass_p_sign_and_period_rules() {
    let mut e = CoreEngine::new();
    // g2 = g3 = 0: p(u) = 1/u^2, p'(u) = -2/u^3; the rule p'(u) = sqrt(4 p^3) (principal root)
    for p in ["2", "-1", "3 + im"] {
        let u = ev(&mut e, &format!("inverse_weierstrass_p({p}, [0, 0])"));
        assert!(holds(&mut e, &format!("full_simplify(1/({u})^2 == {p} and -2/({u})^3 == sqrt(4*({p})^3))")), "{p}: {u}");
    }
    assert_eq!(ev(&mut e, "inverse_weierstrass_p(2, [0, 0])"), "(-1/2)*2^(1/2)");
    assert_eq!(ev(&mut e, "inverse_weierstrass_p([2, 4*sqrt(2)], [0, 0])"), "(-1/2)*2^(1/2)");
    // with p' given and real arguments a negative real u moves by the real period 2 omega_1, as without p'
    let u = ev(&mut e, "inverse_weierstrass_p([2., 2*sqrt(7.)], [1, 2])");
    let v: f64 = u.parse().unwrap_or(f64::NAN);
    assert!(v > 0.0, "{u}");
    assert!(holds(&mut e, &format!("abs(weierstrass_p({u}, [1, 2]) - 2) < 1e-12 and abs(weierstrass_p_prime({u}, [1, 2]) - 2*sqrt(7.)) < 1e-11")));
    assert!(holds(&mut e, &format!("abs({u} - (-0.7150958141802448 + 2*n(weierstrass_half_periods([1, 2]))[1])) < 1e-12")));
}

#[test]
fn kelvin_recurrences_in_simplification() {
    let mut e = CoreEngine::new();
    // DLMF 10.63.1: both identities, checked at a point by the numeric values first
    let ker = "kelvin_ker(n_u, x) == -(1/(2*sqrt(2)*n_u))*x*(kelvin_kei(-1 + n_u, x) + kelvin_kei(1 + n_u, x) + kelvin_ker(-1 + n_u, x) + kelvin_ker(1 + n_u, x))";
    let kei = "kelvin_kei(n_u, x) == -kelvin_kei(2 + n_u, x) - (sqrt(2)*(1 + n_u)*(kelvin_kei(1 + n_u, x) + kelvin_ker(1 + n_u, x)))/x";
    for id in [ker, kei] {
        assert!(holds(&mut e, &format!("abs(n(apply(subtract, rewrite({id}, [n_u -> 7/3, x -> 13/5])), 30)) < 10^-25")), "{id}");
        assert_eq!(ev(&mut e, &format!("full_simplify({id})")), "true", "{id}");
    }
    assert_eq!(ev(&mut e, "simplify(kelvin_ber(n - 1, x) + kelvin_ber(n + 1, x) + sqrt(2)*n/x*(kelvin_ber(n, x) - kelvin_bei(n, x)))"), "0");
    // an identity that does not hold stays an equation
    assert_ne!(ev(&mut e, "full_simplify(kelvin_ker(n, x) == kelvin_ker(n + 2, x))"), "true");
}

#[test]
fn spherical_harmonic_of_non_integer_order_expanded() {
    let mut e = CoreEngine::new();
    let f = ev(&mut e, "function_expand(spherical_harmonic_y(1/2, 1/3, t, p))");
    assert!(f.contains("hypergeometric_2f1") && !f.contains("spherical_harmonic_y"), "{f}");
    for (l, m, t, p) in [("1/2", "1/3", "0.7", "0.3"), ("7/3", "-2/5", "2.1", "1.3"), ("3/2", "1/2", "1.2", "-0.4")] {
        let g = ev(&mut e, &format!("function_expand(spherical_harmonic_y({l}, {m}, t, p))"));
        assert!(holds(&mut e, &format!("abs(n(rewrite({g}, [t -> {t}, p -> {p}])) - spherical_harmonic_y(n({l}), n({m}), {t}, {p})) < 1e-12")), "{l} {m}: {g}");
    }
}

#[test]
fn elliptic_pi_in_its_parameter() {
    let mut e = CoreEngine::new();
    // the m-derivative against a central difference at 40 digits, complete and incomplete
    for (f, m) in [("elliptic_pi(3/10, 4/5, M)", "9/20"), ("elliptic_pi(3/10, M)", "9/20"), ("elliptic_pi(-2, 1/3, M)", "-1/2")] {
        let at = |v: &str| f.replace('M', v);
        let d = format!("n(rewrite(d({}, m), m -> {m}), 40)", at("m"));
        let q = format!("n(({} - {})/(2*10^-10), 40)", at(&format!("({m} + 10^-10)")), at(&format!("({m} - 10^-10)")));
        assert!(holds(&mut e, &format!("abs({d} - {q}) < 10^-15")), "{f} {m}");
    }
    // the series at m = 0 (the integrand expanded): its terms to m^3 against the value at m = 1/100
    let s = ev(&mut e, "series(elliptic_pi(n_u, p, m), m, 0, 1)");
    assert!(!s.contains("derivative"), "{s}");
    assert!(holds(&mut e, "abs(n(rewrite(normal(series(elliptic_pi(n, p, m), m, 0, 3)) - elliptic_pi(n, p, m), [n -> 3/10, p -> 4/5, m -> 1/100]), 20)) < 10^-9"));
    assert!(holds(&mut e, "abs(n(rewrite(normal(series(elliptic_e(p, m), m, 0, 3)) - elliptic_e(p, m), [p -> 4/5, m -> 1/100]), 20)) < 10^-9"));
    assert!(holds(&mut e, "abs(n(rewrite(normal(series(elliptic_f(p, m), m, 0, 3)) - elliptic_f(p, m), [p -> 4/5, m -> 1/100]), 20)) < 10^-9"));
}

#[test]
fn spheroidal_ps_to_second_order_in_gamma() {
    let mut e = CoreEngine::new();
    let s = ev(&mut e, "series(spheroidal_ps(n_u, m, g, x), g, 0, 3)");
    assert!(!s.contains("derivative") && s.contains("O(g^4)"), "{s}");
    // against the function: the error at g = 0.1 is O(g^4)
    for (nn, mm, x) in [("3", "1", "0.3"), ("2", "0", "-0.6"), ("4", "2", "0.45")] {
        assert!(holds(&mut e, &format!("abs(n(rewrite(normal({s}), [n_u -> {nn}, m -> {mm}, g -> 0.1, x -> {x}])) - spheroidal_ps({nn}, {mm}, 0.1, {x})) < 2e-6")), "{nn} {mm} {x}");
    }
    // higher orders are not claimed
    assert!(ev(&mut e, "series(spheroidal_ps(n_u, m, g, x), g, 0, 4)").contains("derivative"));
}


#[test]
fn alternating_harmonic_number_recurrence_at_half_integers() {
    let mut e = CoreEngine::new();
    // the continuation with (-1)^m = exp(i pi m): H(z) - H(z - 1) = exp(i pi (z + 1))/z, checked at 30 digits
    for z in ["-3/2", "1/2", "5/2"] {
        assert!(
            holds(&mut e, &format!("abs(n(alternating_harmonic_number({z}) - alternating_harmonic_number({z} - 1) - exp(im*pi*({z} + 1))/({z}), 30)) < 10^-25")),
            "{z}"
        );
    }
}
