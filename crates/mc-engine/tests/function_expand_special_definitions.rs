//! `function_expand` of the Weber function off the integers, the parabolic cylinder function of negative integer and
//! of non-integer rational order, the q-factorial off the integers, the incomplete beta function with an integer
//! parameter and Appell's F1 with c = b1 + b2; and the regularized pFq with lower parameters at poles. Every form is
//! compared with the numerical value of the function at 30 digits (another algorithm: the series or quadrature of
//! the function itself), the pFq form with the direct sum of its terms. Each test has a time bound.
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

/// The expanded form and the function agree at 30 digits (machine numbers where `digits` is 0).
fn agrees(e: &mut CoreEngine, f: &str, tol: &str) {
    let fe = ev(e, &format!("function_expand({f})"));
    assert!(fe != f && !fe.starts_with("error"), "{f} -> {fe}");
    holds(e, &format!("abs(n({fe}, 30) - n({f}, 30)) < {tol}"));
}

#[test]
fn expanded_forms_agree_with_the_functions() {
    timed("expanded forms", 240, || {
        let mut e = CoreEngine::new();
        for f in ["weber_e(-1/3, 17/10)", "weber_e(1/2, 3)", "weber_e(5/4, -2/3)"] {
            agrees(&mut e, f, "10^-20");
        }
        for f in ["parabolic_cylinder_d(-1, 13/10)", "parabolic_cylinder_d(-3, 13/10)", "parabolic_cylinder_d(-6, -1/2)", "parabolic_cylinder_d(1/2, 7/5)", "parabolic_cylinder_d(-5/3, 2)"] {
            agrees(&mut e, f, "10^-20");
        }
        for f in ["q_factorial(-1/2, 3/10)", "q_factorial(1/2, 2)", "q_factorial(7/3, 4/5)", "q_factorial(-5/4, 3)"] {
            agrees(&mut e, f, "10^-20");
        }
        for f in ["beta(3/10, 3, 5/2)", "beta(1/5, 1/3, 4)", "beta(2/3, 1, -1/2)"] {
            agrees(&mut e, f, "10^-20");
        }
        for f in ["appell_f1(1/2, 1/3, 1/6, 1/2, 3/10, -2/5)", "appell_f1(1, 1, 1, 2, 1/4, -1/3)", "appell_f1(1/2, 1/2, 1, 3/2, 1/5, 1/7)"] {
            agrees(&mut e, f, "10^-20");
        }
        // the documented symbolic forms
        holds(&mut e, "function_expand(q_factorial(-1/2, q)) === (1 - q)^(1/2)*q_pochhammer(q, q)/q_pochhammer(q^(1/2), q)");
        holds(&mut e, "function_expand(q_factorial(n_u, -1)) === q_gamma(1 + n_u, -1)");
        holds(&mut e, "full_simplify(function_expand(appell_f1(1, 1, 1, 2, x, y)) + log((1 - x)/(1 - y))/(x - y)) == 0");
        holds(&mut e, "full_simplify(function_expand(beta(z, 3, b)) - (2 - (1 - z)^b*(2 + 2*b*z + b*z^2 + b^2*z^2))/(b*(1 + b)*(2 + b))) == 0");
        // integer orders keep their other forms
        holds(&mut e, "free_of(function_expand(q_factorial(3, q)), q_pochhammer) and not free_of(function_expand(weber_e(2, x)), struve_h)");
        // a negative base off the integers: the quotient with the principal power q^(z + 1), the definition the
        // numeric value takes (complex there)
        agrees(&mut e, "q_factorial(7/3, -1/2)", "10^-25");
    });
}

#[test]
fn regularized_pfq_with_lower_parameters_at_poles() {
    timed("regularized pFq at poles", 120, || {
        let mut e = CoreEngine::new();
        // the series of pFq~ starts at k = 3: its direct sum
        holds(
            &mut e,
            "{ let f = hypergeometric_pfq_regularized([1, 2, -1/2], [-2, -2], x); \
             let s = sum(pochhammer(1, k)*pochhammer(2, k)*pochhammer(-1/2, k)/(gamma(k - 2)^2*gamma(k + 1))*(1/5)^k, [k, 3, 200]); \
             free_of(f, hypergeometric_pfq_regularized) and abs(n(rewrite(f, x -> 1/5), 30) - n(s, 30)) < 10^-25 }",
        );
        holds(
            &mut e,
            "{ let s = sum(pochhammer(1, k)*pochhammer(1/3, k)*pochhammer(5, k)/(gamma(k - 2)*gamma(k - 3)*gamma(k + 1))*(1/3)^k, [k, 4, 400]); \
             abs(n(hypergeometric_pfq_regularized([1, 1/3, 5], [-2, -3], 1/3), 50) - n(s, 50)) < 10^-40 }",
        );
        // an upper parameter at a pole before the start makes every term 0
        holds(&mut e, "n(hypergeometric_pfq_regularized([-1, 2], [-3], 1/4)) == 0");
    });
}

#[test]
fn hankel_airy_kelvin_digamma_ferrers_and_erf_forms_agree() {
    timed("Hankel, Airy, Kelvin, digamma, Ferrers and erf forms", 240, || {
        let mut e = CoreEngine::new();
        // DLMF 10.47.10 with 10.49.6 and 10.4.6 (half-integer orders), 9.6.6 (order 1/3), at points of the right half
        // plane, the imaginary axis and the negative axis
        for f in [
            "hankel_h1(1/2, 17/10)", "hankel_h2(-1/2, -7/5 + 2/3*im)", "hankel_h1(3/2, 3*im)", "hankel_h2(5/2, -2)",
            "hankel_h1(-5/2, 7/10)", "hankel_h2(-3/2, 1/2 - im)", "hankel_h1(1/3, 17/10)", "hankel_h2(1/3, -7/5 + 2/3*im)",
            "hankel_h1(-1/3, 3*im)", "hankel_h2(-1/3, -2)", "hankel_h1(1/3, -2 - im/10)", "spherical_hankel_h1(3, 7/5 - im)",
            "spherical_hankel_h2(2, -3/2)",
        ] {
            agrees(&mut e, f, "10^-20");
        }
        for f in [
            "kelvin_ber(1/2, 3/2*sqrt(2))", "kelvin_kei(3/2, 5/2*sqrt(3))", "q_factorial(7/3, 1/2)", "q_factorial(7/3, 2)",
            "q_factorial(-5/4, 1/3)", "lucas_l(1/2, 3/2)", "lucas_l(7/3, -2 + im)", "digamma(7/13)", "digamma(-23/17)",
            "digamma(5/24)", "harmonic_number(7/13)", "legendre_p(3/2, 1/3, 2/5)", "legendre_p(-2/7, -5/4, 1/3 + im/2)",
            "hypergeometric_1f1(1, 3/2, 2)", "hypergeometric_1f1(1, 3/2, -3)", "hypergeometric_1f1(1/2, 3/2, 1 + im)",
            "hypergeometric_1f1(1/2, 3/2, -5/2)", "elliptic_e(-1)", "elliptic_e(1/2)", "whittaker_m(-1/4, 1/4, 9/5)",
            "hypergeometric_1f1(1/2, 2, -3 - im/2)", "hypergeometric_1f1(5/2, 6, 7/5 - 2*im)", "hypergeometric_u(3/2, 2, -3 + im/2)",
            "hypergeometric_u(5/2, 4, 7/5 - 2*im)", "whittaker_m(1/2, 1/2, 13/4)", "whittaker_w(-1/2, 1/2, 2/3 + im)",
        ] {
            agrees(&mut e, f, "10^-20");
        }
        // the symbolic Ferrers form against the function at a complex point
        holds(&mut e, "abs(n(rewrite(function_expand(legendre_p(n, m, x)), [n -> 2/3, m -> 1/4, x -> 1/5 - im/3]) - legendre_p(2/3, 1/4, 1/5 - im/3), 30)) < 10^-20");
        // the triangle wave of a real argument at points of several periods
        for x in ["3/10", "19/10", "-13/5", "7/8"] {
            holds(&mut e, &format!("abs(n(rewrite(function_expand(triangle_wave(x), element(x, reals)), x -> {x}) - triangle_wave({x}), 30)) < 10^-20"));
        }
    });
}

// The new paths end for arguments outside their ranges: machine numbers, intervals, the bounds of the orders,
// degrees and denominators, and arguments at 0.
#[test]
fn function_expand_new_paths_end_outside_their_ranges() {
    timed("arguments outside the ranges", 120, || {
        let mut e = CoreEngine::new();
        for f in [
            "hankel_h1(0.5, x)", "hankel_h1(1/2, 1.7)", "hankel_h1(1/2, interval([1, 2]))", "hankel_h1(1/3, 0)", "hankel_h2(1/2, 0)",
            "hankel_h1(41/2, x)", "hankel_h1(43/2, x)", "hankel_h1(1/3, 0.5)", "dirac_delta(x^25 - 1)", "dirac_delta(x^24 - 1)",
            "dirac_delta(1.5*x^2 - 1)", "dirac_delta(x^2 - y)", "dirac_delta(2*x - 1)", "digamma(1/25)", "digamma(1201/24)",
            "digamma(-1199/24)", "q_factorial(n, 0.5)", "q_factorial(n, interval([1/3, 1/2]))", "legendre_p(n, 0.5, x)",
            "legendre_p(n, 2, x)", "legendre_p(n, m, 0.3)", "lucas_l(n, 1.5)", "lucas_l(1/2, interval([1, 2]))", "elliptic_e(-1.0)",
            "hypergeometric_1f1(1, 3/2, 0)", "hypergeometric_1f1(1, 3/2, 0.0)", "kelvin_ber(1/2, 2.0*x)",
            "hypergeometric_1f1(1/2, 2, 0)", "hypergeometric_1f1(3/2, 4, 0.0)", "hypergeometric_u(3/2, 2, 0)", "hypergeometric_u(3/2, 2, 1.5)",
            "hypergeometric_1f1(1/2, 2, interval([1, 2]))", "hypergeometric_u(3/2, 2, interval([-1, 2]))", "hypergeometric_1f1(41/2, 42, x)",
            "hypergeometric_1f1(43/2, 44, x)", "hypergeometric_u(1/2, 2, x)",
        ] {
            let r = ev(&mut e, &format!("function_expand({f})"));
            assert!(!r.starts_with("error"), "{f}: {r}");
        }
        assert_eq!(ev(&mut e, "function_expand(dirac_delta(x^25 - 1))"), "dirac_delta(-1 + x^25)");
        assert_eq!(ev(&mut e, "function_expand(digamma(1/25))"), "digamma(1/25)");
        assert_eq!(ev(&mut e, "function_expand(hypergeometric_u(3/2, 2, 0))"), "hypergeometric_u(3/2, 2, 0)");
        assert_eq!(ev(&mut e, "function_expand(hypergeometric_1f1(43/2, 44, x))"), "hypergeometric_1f1(43/2, 44, x)");
    });
}
