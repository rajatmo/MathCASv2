//! Calls that used to stay inside an answer and now evaluate: the conjugate taken through sums, products, `exp` and
//! the mirror-symmetric functions in `simplify`; one-sided limits of boxes and triangles at their jumps; closed forms
//! of `function_expand` (logistic sigmoid, double factorial, regularized gamma at half-integers, spherical Hankel
//! functions); the shift families of digamma, harmonic numbers and the Hurwitz zeta (in `function_expand` and in
//! `difference_quotient`); `not element`; Barnes G at quarters and the hyperfactorial at halves and quarters;
//! log Gamma and E at infinity; the prime zeta function at 1/k; algebraic roots. Every new value is checked by an
//! independent route: the numerical value of the original call at 30 digits (or machine precision), or the
//! definition. Each test has a time bound.
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

/// `abs(n(a - b, 30)) < tol` where `a` is a closed form and `b` the function evaluated directly.
fn close(e: &mut CoreEngine, a: &str, b: &str) {
    // at machine precision, relative to the size (a value 0 has no 30-digit approximation)
    holds(e, &format!("abs(n({a}) - n({b})) < 10^-11*(1 + abs(n({b})))"));
}

#[test]
fn mirror_symmetry_in_simplify() {
    timed("mirror symmetry", 60, || {
        let mut e = CoreEngine::new();
        for f in ["dawson_f", "logistic_sigmoid", "factorial2", "airy_ai", "sinh_integral"] {
            holds(&mut e, &format!("full_simplify({f}(conjugate(z)) == conjugate({f}(z)))"));
        }
        for src in [
            "full_simplify(exp_integral_e(0, conjugate(z)) == conjugate(exp_integral_e(0, z)))",
            "full_simplify(mittag_leffler_e(1, conjugate(z)) == conjugate(mittag_leffler_e(1, z)))",
            "full_simplify(pochhammer(conjugate(z), 2) == conjugate(pochhammer(z, 2)))",
            "full_simplify(multinomial(conjugate(z), 2, 2) == conjugate(multinomial(z, 2, 2)))",
            "simplify(conjugate(1 + 3*z^2) == 1 + 3*conjugate(z)^2)",
        ] {
            holds(&mut e, src);
        }
        // the identity checked at a point: conj(e^(-z)/z) at z = 1 + 2 i
        close(&mut e, "conjugate(exp(-(1 + 2*im))/(1 + 2*im))", "exp(-(1 - 2*im))/(1 - 2*im)");
        // a function with a cut off the negative axis keeps the conjugate outside
        assert_eq!(ev(&mut e, "full_simplify(asinh(conjugate(z)) == conjugate(asinh(z)))"), "asinh(conjugate(z)) == conjugate(asinh(z))");
    });
}

#[test]
fn bessel_orders_and_parity() {
    timed("Bessel parity", 60, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "table(full_simplify(hankel_h1(-n_u, z) == (-1)^n_u*hankel_h1(n_u, z)), n_u, range(0, 4)) == table(true, 5)");
        holds(&mut e, "full_simplify(spherical_bessel_j(n_u, z) == (-1)^n_u*spherical_bessel_j(n_u, -z), element(n_u, integers))");
        holds(&mut e, "full_simplify(spherical_bessel_y(n_u, z) == (-1)^(n_u + 1)*spherical_bessel_y(n_u, -z), element(n_u, integers))");
        // the order -n against the numerical functions at a complex point
        for f in ["bessel_j", "bessel_y", "bessel_i", "bessel_k", "hankel_h1", "hankel_h2"] {
            for k in [1, 2, 3] {
                close(&mut e, &format!("simplify({f}(-{k}, z)) /. z -> 7/3 + im/2"), &format!("{f}(-{k}, n(7/3 + im/2, 40))"));
            }
        }
        // a symbolic order keeps the parity rule off
        assert_eq!(ev(&mut e, "simplify(spherical_bessel_j(nu, -z))"), "spherical_bessel_j(nu, -z)");
    });
}

#[test]
fn inverse_hyperbolic_values_and_round() {
    timed("inverse hyperbolic", 60, || {
        let mut e = CoreEngine::new();
        for (src, want) in [
            ("acosh(sqrt(2)/2) == im*pi/4", "true"),
            ("acosh(-sqrt(3)/2) == 5*im*pi/6", "true"),
            ("asinh(im*sqrt(2)/2) == im*pi/4", "true"),
            ("asinh(-im*sqrt(3)/2) == -im*pi/3", "true"),
            ("round(-x) == -round(x)", "true"),
            ("round(-2*x)", "-round(2*x)"),
        ] {
            assert_eq!(ev(&mut e, src), want, "{src}");
        }
        for x in ["sqrt(2)/2", "-sqrt(3)/2", "(sqrt(6) - sqrt(2))/4"] {
            close(&mut e, &format!("acosh({x})"), &format!("acosh(n({x}, 40))"));
            close(&mut e, &format!("asinh(im*({x}))"), &format!("asinh(im*n({x}, 40))"));
        }
        // outside [-1, 1] nothing changes
        assert_eq!(ev(&mut e, "acosh(sqrt(2))"), "acosh(2^(1/2))");
        holds(&mut e, "round(-5/2) == -round(5/2) and round(-7/2) == -round(7/2)");
    });
}

#[test]
fn one_sided_limits_of_boxes() {
    timed("box limits", 60, || {
        let mut e = CoreEngine::new();
        for (src, want) in [
            ("[limit(unit_box(x), x -> -1/2, direction: from_below), limit(unit_box(x), x -> -1/2, direction: from_above)]", "[0, 1]"),
            ("[limit(heaviside_pi(x), x -> 1/2, direction: from_below), limit(heaviside_pi(x), x -> 1/2, direction: from_above)]", "[1, 0]"),
            ("limit(heaviside_lambda(x), x -> 0)", "1"),
            ("limit(unit_triangle(x), x -> 1, direction: from_below)", "0"),
            ("limit(unit_box(2*x - 1), x -> 3/4, direction: from_below)", "1"),
            ("limit(unit_box(2*x - 1), x -> 3/4, direction: from_above)", "0"),
        ] {
            assert_eq!(ev(&mut e, src), want, "{src}");
        }
        // against values just beside the jump
        holds(&mut e, "unit_box(-1/2 - 10^-9) == 0 and unit_box(-1/2 + 10^-9) == 1");
    });
}

#[test]
fn function_expand_closed_forms() {
    timed("function_expand", 120, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "function_expand(logistic_sigmoid(x))"), "(1 + exp(-x))^-1");
        for x in ["3/10", "-2", "1 + im"] {
            close(&mut e, &format!("function_expand(logistic_sigmoid(x)) /. x -> {x}"), &format!("logistic_sigmoid({x})"));
        }
        // the double factorial: the integers and points of the continuation
        holds(&mut e, "{ let f = function_expand(factorial2(n)); map(k => f /. n -> k, range(-1, 9)) == map(factorial2, range(-1, 9)) }");
        for x in ["1/2", "7/3", "-3/2 + im/3"] {
            close(&mut e, &format!("function_expand(factorial2(n)) /. n -> {x}"), &format!("factorial2(n({x}, 40))"));
        }
        // Q(a, z) at half-integers against the numerical value
        for (a, z) in [("13/2", "7/3"), ("3/2", "1/5"), ("9/2", "11")] {
            close(&mut e, &format!("function_expand(gamma_regularized({a}, z)) /. z -> {z}"), &format!("gamma_regularized({a}, n({z}, 40))"));
            close(&mut e, &format!("function_expand(gamma_regularized({a}, z0, z1)) /. [z0 -> 1/2, z1 -> {z}]"), &format!("gamma_regularized({a}, n(1/2, 40), n({z}, 40))"));
        }
        // spherical Hankel functions of integer order against h = j +- i y
        for n in 0..5 {
            for (f, s) in [("spherical_hankel_h1", "+"), ("spherical_hankel_h2", "-")] {
                close(
                    &mut e,
                    &format!("function_expand({f}({n}, x)) /. x -> 13/7"),
                    &format!("spherical_bessel_j({n}, n(13/7, 40)) {s} im*spherical_bessel_y({n}, n(13/7, 40))"),
                );
            }
        }
        // Fibonacci numbers of a symbolic order at the integers
        holds(&mut e, "{ let f = function_expand(fibonacci(n_u + 3)); map(k => simplify(f /. n_u -> k), range(-3, 6)) == map(k => fibonacci(k + 3), range(-3, 6)) }");
    });
}

#[test]
fn shifted_arguments_telescope() {
    timed("shift families", 120, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "function_expand(digamma(3 + x) - digamma(x))"), "x^-1 + (1 + x)^-1 + (2 + x)^-1");
        assert_eq!(ev(&mut e, "difference_quotient(harmonic_number(x, 2), [x, 3])"), "(1/3)*((1 + x)^-2 + (2 + x)^-2 + (3 + x)^-2)");
        for (f, at) in [
            ("difference_quotient(zeta(2, x), [x, 3])", "(zeta(2, 3 + x) - zeta(2, x))/3"),
            ("difference_quotient(digamma(x), [x, 2])", "(digamma(2 + x) - digamma(x))/2"),
            ("difference_quotient(polygamma(2, x), [x, 3])", "(polygamma(2, 3 + x) - polygamma(2, x))/3"),
            ("difference_quotient(harmonic_number(x, 3), [x, 2, 1])", "harmonic_number(2 + x, 3) - 2*harmonic_number(1 + x, 3) + harmonic_number(x, 3)"),
            ("function_expand(zeta(3/2, x + 2) - zeta(3/2, x))", "zeta(3/2, x + 2) - zeta(3/2, x)"),
        ] {
            let out = ev(&mut e, f);
            assert!(!out.contains("zeta(") && !out.contains("gamma(") && !out.contains("harmonic"), "{f}: {out}");
            // the closed form against the numerical value of the defining difference at x = 7/5 + im/3
            close(&mut e, &format!("({f}) /. x -> 7/5 + im/3"), &format!("({at}) /. x -> n(7/5 + im/3, 40)"));
        }
    });
}

#[test]
fn logic_and_domains() {
    timed("logic", 60, || {
        let mut e = CoreEngine::new();
        for (src, want) in [
            ("not element(a, reals)", "not_element(a, reals)"),
            ("not not_element(a, reals)", "element(a, reals)"),
            ("logical_expand(not element(x | y | z, reals))", "not_element(x, reals) or not_element(y, reals) or not_element(z, reals)"),
            ("logical_expand(element(x | y, integers))", "element(x, integers) and element(y, integers)"),
            ("logical_expand(unequal(x, y, z))", "x != y and x != z and y != z"),
            ("element(root(&(7 - 3*&1 + &1^5), 2), algebraics)", "true"),
            ("element(map(k => root(&(&1^5 - 3*&1 + 7), k), range(5)), algebraics)", "true"),
            ("element([pi, 2], algebraics)", "false"),
            ("[re(indeterminate), im_part(indeterminate)]", "[indeterminate, indeterminate]"),
        ] {
            assert_eq!(ev(&mut e, src), want, "{src}");
        }
        // the negation agrees with the truth values where they are decided
        holds(&mut e, "(not element(1/2, integers)) === not_element(1/2, integers)");
    });
}

#[test]
fn special_values() {
    timed("special values", 120, || {
        let mut e = CoreEngine::new();
        // Barnes G at quarters and the hyperfactorial at halves and quarters against the numerical functions
        for z in ["1/4", "3/4", "-3/4", "7/4", "-5/4", "9/4"] {
            close(&mut e, &format!("barnes_g({z})"), &format!("barnes_g(n({z}, 40))"));
        }
        for z in ["1/2", "-1/2", "3/2", "1/4", "-3/4", "11/4"] {
            close(&mut e, &format!("hyperfactorial({z})"), &format!("hyperfactorial(n({z}, 40))"));
        }
        holds(&mut e, "hyperfactorial(1/2) === 2^(-13/24)*exp(-1/8)*glaisher^(3/2)");
        // Neville theta functions at the quarter periods: the symbolic table and the numerical functions
        assert_eq!(
            ev(&mut e, "table(neville_theta_d(u1*elliptic_k(m) + u2*im*elliptic_k(1 - m), m), u1, [0, 1], u2, [0, 1])"),
            "[[1, m^(1/4)*elliptic_nome_q(m)^(-1/4)], [(1 - m)^(1/4), 0]]"
        );
        for m in ["3/10", "17/10", "-5/2", "3/10 + im/5"] {
            for f in ["neville_theta_c", "neville_theta_d", "neville_theta_n"] {
                for u in ["elliptic_k(m)", "im*elliptic_k(1 - m)", "elliptic_k(m) + im*elliptic_k(1 - m)"] {
                    let exact = format!("{{ let v = {f}({u}, m); if free_of(v, {f}) then v /. m -> ({m}) else failure(open) }}");
                    let mm = format!("n({m}, 40)");
                    let un = u.replace("(m)", &format!("({mm})")).replace("(1 - m)", &format!("(1 - {mm})"));
                    let num = format!("{f}({un}, {mm})");
                    close(&mut e, &exact, &num);
                }
            }
        }
        // binomial of a numeric constant over a small natural number: the polynomial
        assert_eq!(ev(&mut e, "binomial((5/6)*pi, 2)"), "(5/12)*pi*(-1 + (5/6)*pi)");
        close(&mut e, "binomial(e + 1, 5)", "binomial(n(e + 1, 40), 5)");
        assert_eq!(ev(&mut e, "binomial(x, 2)"), "binomial(x, 2)");
        // hyperbolic functions of logarithms
        for (f, x, want) in [("coth", "7", "25/24"), ("sinh", "3", "4/3"), ("cosh", "1/2", "5/4"), ("tanh", "2", "3/5")] {
            assert_eq!(ev(&mut e, &format!("{f}(log({x}))")), want, "{f}");
            close(&mut e, want, &format!("{f}(log(n({x}, 40)))"));
        }
        // infinities: |log Gamma(i y)| and |E(m)| grow without bound, log Gamma oscillates along the negative axis
        assert_eq!(ev(&mut e, "log_gamma([inf, -inf, im*inf, -im*inf])"), "[inf, indeterminate, complex_infinity, complex_infinity]");
        assert_eq!(ev(&mut e, "[elliptic_e(inf), elliptic_e(-inf), elliptic_e(complex_infinity)]"), "[complex_infinity, inf, complex_infinity]");
        holds(&mut e, "abs(n(log_gamma(10^6*im))) > 10^6 and n(elliptic_e(-10^8)) > 9000 and abs(n(elliptic_e(10^8))) > 9000");
        // the prime zeta function at 1/k for squarefree k (its logarithmic singularities); 1/4 is not one
        assert_eq!(ev(&mut e, "table(prime_zeta_p(s), s, [1/5, 1/3, 1/2, 1, 1/6])"), "[complex_infinity, complex_infinity, complex_infinity, complex_infinity, complex_infinity]");
        assert_eq!(ev(&mut e, "prime_zeta_p(1/4)"), "prime_zeta_p(1/4)");
        holds(&mut e, "abs(prime_zeta_p(0.5001)) > abs(prime_zeta_p(0.51)) and abs(prime_zeta_p(0.51)) > abs(prime_zeta_p(0.6))");
    });
}
