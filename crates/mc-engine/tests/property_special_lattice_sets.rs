//! Property sets of special functions of several parameters: theta functions (domains, periods), Hankel functions
//! (not injective), basic hypergeometric series (entire), the Weierstrass half-periods, quasi-periods and roots of
//! real invariants (singular lines and the discriminant), periodic functions (not onto), the nome function and the
//! inverse Jacobi functions in two variables. Each claim is checked against values at 15 to 30 digits.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

#[test]
fn theta_domains_and_periods() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_domain(elliptic_theta(1, x, q), [x, q])"), "q >= 0 and q < 1");
    assert_eq!(ev(&mut e, "function_domain(elliptic_theta(2, x, q), [x, q])"), "q >= 0 and q < 1");
    assert_eq!(ev(&mut e, "function_domain(elliptic_theta_prime(3, x, q), [x, q])"), "q > -1 and q < 1");
    assert_eq!(ev(&mut e, "function_domain(elliptic_theta(4, z, q), [z, q], complexes)"), "im_part(q)^2 + re(q)^2 < 1");
    // theta_1 and theta_2 are not real for a negative nome, theta_3 and theta_4 are (the independent check)
    holds(&mut e, "im_part(n(elliptic_theta(1, 1/3, -1/2), 30)) != 0");
    holds(&mut e, "im_part(n(elliptic_theta(2, 1/3, -1/2), 30)) != 0");
    holds(&mut e, "im_part(n(elliptic_theta(3, 1/3, -1/2), 30)) == 0");
    holds(&mut e, "im_part(n(elliptic_theta(4, 1/3, -1/2), 30)) == 0");
    // periods: 2 pi, 2 pi, pi, pi, and no smaller one (the shift by half the period changes the value)
    assert_eq!(ev(&mut e, "table(function_period(elliptic_theta(a, z, q), z), a, 4)"), "[2*pi, 2*pi, pi, pi]");
    assert_eq!(ev(&mut e, "table(function_period(elliptic_theta_prime(a, z, q), z), a, 4)"), "[2*pi, 2*pi, pi, pi]");
    for (a, p) in [(1, "2*pi"), (2, "2*pi"), (3, "pi"), (4, "pi")] {
        holds(&mut e, &format!("abs(n(elliptic_theta({a}, 1/3 + {p}, 1/2), 30) - n(elliptic_theta({a}, 1/3, 1/2), 30)) < 10^(-25)"));
        holds(&mut e, &format!("abs(n(elliptic_theta({a}, 1/3 + {p}/2, 1/2), 30) - n(elliptic_theta({a}, 1/3, 1/2), 30)) > 10^(-3)"));
    }
}

#[test]
fn hankel_functions_are_not_injective() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "table(function_injective(hankel_h1(a, x), x, complexes), a, 4)"), "[false, false, false, false]");
    assert_eq!(ev(&mut e, "function_injective(hankel_h2(1, x), x, complexes)"), "false");
    // two different points with the same value, found by Newton's method and checked at 15 digits
    holds(&mut e, "abs(hankel_h1(1, 2.547516445615042 + 1.5971989510035058*im) - 0.1) < 10^(-12)");
    holds(&mut e, "abs(hankel_h1(1, 8.655009856982604 + 1.0016393157640884*im) - 0.1) < 10^(-12)");
}

#[test]
fn basic_hypergeometric_series_is_entire() {
    let mut e = CoreEngine::new();
    for p in ["function_singularities", "function_discontinuities"] {
        assert_eq!(ev(&mut e, &format!("{p}(q_hypergeometric_pfq([1 / 2], [2 / 3], 3 / 4, x), x)")), "false");
    }
    assert_eq!(ev(&mut e, "function_analytic(q_hypergeometric_pfq([1 / 2], [2 / 3], 3 / 4, x), x)"), "true");
    // a lower parameter that is a power q^(-k) leaves the series without a proof: 4/3 = (3/4)^(-1)
    let r = ev(&mut e, "function_analytic(q_hypergeometric_pfq([1 / 2], [4 / 3], 3 / 4, x), x)");
    assert!(r.starts_with("function_analytic("), "{r}");
    // the terms decay: the values at large arguments are finite
    holds(&mut e, "abs(n(q_hypergeometric_pfq([1/2], [2/3], 3/4, 40.0))) < 10^30");
    holds(&mut e, "abs(n(q_hypergeometric_pfq([1/2], [2/3], 3/4, -40.0))) < 10^30");
}

#[test]
fn weierstrass_lattice_singular_sets() {
    let mut e = CoreEngine::new();
    let all = "x == 0 or y == 0 or -27*y^2 + x^3 == 0";
    assert_eq!(ev(&mut e, "function_singularities(weierstrass_e3([x, y]), [x, y])"), all);
    for f in ["eta2", "eta3"] {
        for p in ["function_singularities", "function_discontinuities"] {
            assert_eq!(ev(&mut e, &format!("{p}(weierstrass_{f}([x, y]), [x, y])")), all, "{p} {f}");
        }
    }
    // in x with the parameter y: the jump across y = 0 (checked by values below) belongs to the set too
    for f in ["half_period_w2", "half_period_w3"] {
        for p in ["function_singularities", "function_discontinuities"] {
            assert_eq!(ev(&mut e, &format!("{p}(weierstrass_{f}([x, y]), x)")), all, "{p} {f}");
        }
    }
    // the root e3 is continuous at the discriminant (no discontinuity there), e1 analytic there, e2 a square-root branch
    // without a jump on y = 0
    assert_eq!(ev(&mut e, "function_discontinuities(weierstrass_e3([x, y]), [x, y])"), "x == 0 or y == 0");
    assert_eq!(ev(&mut e, "function_singularities(weierstrass_e1([x, y]), [x, y])"), "x == 0 or y == 0");
    assert_eq!(ev(&mut e, "function_singularities(weierstrass_e2([x, y]), [x, y])"), "x == 0 or -27*y^2 + x^3 == 0");
    // the jumps across x = 0 and y = 0 (values at 15 digits)
    for f in ["weierstrass_e3", "weierstrass_eta2", "weierstrass_eta3", "weierstrass_half_period_w2", "weierstrass_half_period_w3"] {
        holds(&mut e, &format!("abs({f}([0.001, 1.0]) - {f}([-0.001, 1.0])) > 0.2"));
        holds(&mut e, &format!("abs({f}([3.0, 0.001]) - {f}([3.0, -0.001])) > 0.2"));
    }
    // across the discriminant: e3 is continuous, w3 and eta3 grow without bound (log) on both sides
    holds(&mut e, "abs(weierstrass_e3([3.0, 1.0 - 10^(-9)]) - weierstrass_e3([3.0, 1.0 + 10^(-9)])) < 0.01");
    for f in ["weierstrass_half_period_w3", "weierstrass_eta3", "weierstrass_eta2"] {
        holds(&mut e, &format!("abs({f}([3.0, 1.0 - 10^(-9)])) > abs({f}([3.0, 1.0 - 10^(-6)]))"));
        holds(&mut e, &format!("abs({f}([3.0, 1.0 - 10^(-6)])) > abs({f}([3.0, 1.0 - 10^(-3)]))"));
        holds(&mut e, &format!("abs({f}([3.0, 1.0 + 10^(-9)])) > abs({f}([3.0, 1.0 + 10^(-6)]))"));
    }
    // the first half-period and quasi-period stay finite and continuous there
    for f in ["weierstrass_half_period_w1", "weierstrass_eta1"] {
        holds(&mut e, &format!("abs({f}([3.0, 1.0 - 10^(-9)]) - {f}([3.0, 1.0 + 10^(-9)])) < 10^(-6)"));
    }
}

#[test]
fn periodic_functions_are_not_onto() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_surjective(re(dedekind_eta(x + 0.2*im)), x)"), "false");
    assert_eq!(ev(&mut e, "function_surjective(re(klein_invariant_j(x + im)), x)"), "false");
    assert_eq!(ev(&mut e, "function_surjective(re(modular_lambda(x + im)), x)"), "false");
    assert_eq!(ev(&mut e, "function_discontinuities(re(modular_lambda(x + im)), x)"), "false");
    // on the real line itself the modular functions stay singular everywhere
    assert_eq!(ev(&mut e, "function_singularities(modular_lambda(x), x)"), "true");
    // the periods (values at 30 digits)
    holds(&mut e, "abs(re(n(dedekind_eta(7/5 + 24 + im/5), 30)) - re(n(dedekind_eta(7/5 + im/5), 30))) < 10^(-25)");
    holds(&mut e, "abs(re(n(klein_invariant_j(7/5 + 1 + im), 30)) - re(n(klein_invariant_j(7/5 + im), 30))) < 10^(-25)");
    holds(&mut e, "abs(re(n(modular_lambda(7/5 + 2 + im), 30)) - re(n(modular_lambda(7/5 + im), 30))) < 10^(-25)");
    // functions that are periodic but not continuous, and continuous but not periodic, stay as before
    assert_eq!(ev(&mut e, "function_surjective(x + sin(x), x)"), "true");
    assert_eq!(ev(&mut e, "function_surjective(tan(x), x)"), "true");
}

#[test]
fn nome_function_and_inverse_jacobi_in_two_variables() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_domain(inverse_elliptic_nome_q(z), z, complexes)"), "abs(z) < 1 or z == 1");
    // defined inside the disc and at 1, not outside (values)
    holds(&mut e, "im_part(n(inverse_elliptic_nome_q(1/2 + im/2), 30)) != 0");
    assert_eq!(ev(&mut e, "inverse_elliptic_nome_q(1)"), "1");
    let r = ev(&mut e, "n(inverse_elliptic_nome_q(0.9 + 0.5*im))");
    assert!(r.starts_with("elliptic_theta"), "{r}");
    // ds and sd are not analytic as functions of (x, m): not defined (real) on parts of the plane
    for f in ["inverse_jacobi_ds", "inverse_jacobi_sd"] {
        assert_eq!(ev(&mut e, &format!("function_analytic({f}(x, m), [x, m])")), "false");
    }
    holds(&mut e, "im_part(n(inverse_jacobi_ds(1/2, 1/2))) != 0");
    holds(&mut e, "im_part(n(inverse_jacobi_ds(4/5, 1/2))) == 0");
    holds(&mut e, "im_part(n(inverse_jacobi_sd(3/2, 1/2))) != 0");
    holds(&mut e, "im_part(n(inverse_jacobi_sd(7/5, 1/2))) == 0");
    // the integral Pi(n | m) diverges at m = 1: not meromorphic
    assert_eq!(ev(&mut e, "function_meromorphic(elliptic_pi(n, m), [n, m])"), "false");
    holds(&mut e, "n(elliptic_pi(0.5, 1 - 10^(-6))) < n(elliptic_pi(0.5, 1 - 10^(-10)))");
}

#[test]
fn list_of_one_component_is_injective_as_the_component() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_injective([hypergeometric_2f1(1, 1 / 2, 1, z)], z)"), "true");
    assert_eq!(ev(&mut e, "function_injective([exp(z)], z)"), "true");
    assert_eq!(ev(&mut e, "function_injective([z^2], z)"), "false");
}
