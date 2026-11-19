//! Domains and singular sets of special functions with symbolic parameters (real over the reals), the complex
//! domain of the inverse regularized gamma function, and the real singular set of the inverse Weierstrass function.
mod common;
use common::{check, Sys};
use std::time::Duration;

#[test]
fn real_domains_read_symbolic_parameters_as_real() {
    check(&[
        // the characteristic values are real for a real order and a real q (DLMF 28.2.(v), 28.12.(i))
        ("function_domain(mathieu_characteristic_a(r, x), x)", "true"),
        ("function_domain(mathieu_characteristic_b(r, x), x)", "true"),
        // a non-real order is not read as real
        ("function_domain(mathieu_characteristic_a(1 + im, x), x)", "function_domain(mathieu_characteristic_a(complex(1, 1), x), x)"),
        ("function_domain(mathieu_characteristic_a(r, x), x, complexes)", "function_domain(mathieu_characteristic_a(r, x), x, complexes)"),
        ("function_domain(kelvin_ker(n_u, x), x)", "x > 0 or x == 0 and cos(pi*((1/4)*abs(n_u) + (1/2)*n_u)) == 0 and abs(n_u) <= 2 and n_u != 0"),
        ("function_domain(bessel_y(n_u, x), x)", "x > 0"),
    ]);
}

#[test]
fn coulomb_f_domain_only_where_m_is_defined() {
    check(&[
        // 2 l + 2 = 0, -1, ...: M(a, b, z) is not defined (the values stay unevaluated), so no domain is claimed
        ("function_domain(coulomb_f(-1, 1/2, x), x)", "function_domain(coulomb_f(-1, 1/2, x), x)"),
        ("function_domain(coulomb_f(-3, 1/2, x), x)", "function_domain(coulomb_f(-3, 1/2, x), x)"),
        ("function_domain(coulomb_f(n_u, et, x), x)", "function_domain(coulomb_f(n_u, et, x), x)"),
        ("[n(coulomb_f(-1, 1/2, 1)), n(coulomb_f(-3/2, 1/2, 1))]", "[coulomb_f(-1.0, 0.5, 1.0), coulomb_f(-1.5, 0.5, 1.0)]"),
        // elsewhere the real domain of DLMF 33.2.4
        ("function_domain(coulomb_f(-1/2, 1/2, x), x)", "x >= 0"),
        ("function_domain(coulomb_f(-5/4, 1/2, x), x)", "x > 0"),
    ]);
}

#[test]
fn inverse_gamma_regularized_complex_domain_is_the_real_one() {
    check(&[
        ("function_domain(inverse_gamma_regularized(a, x), [a, x], complexes)", "a > 0 and 0 < x and x <= 1"),
        ("reduce(function_domain(inverse_gamma_regularized(a, x), [a, x], complexes))", "a > 0 and 0 < x and x <= 1"),
        ("function_domain(inverse_gamma_regularized(2, x), x, complexes)", "0 < x and x <= 1"),
        // the evaluator agrees: no value at a non-real argument or outside (0, 1]
        ("n(inverse_gamma_regularized(2, 1/2 + im/10))", "inverse_gamma_regularized(2.0, complex(0.5, 0.1))"),
        ("n(inverse_gamma_regularized(2 + im, 1/2))", "inverse_gamma_regularized(complex(2.0, 1.0), 0.5)"),
        ("n(inverse_gamma_regularized(2, 3/2))", "inverse_gamma_regularized(2.0, 1.5)"),
    ]);
}

#[test]
fn inverse_weierstrass_p_real_singular_set_one_real_root() {
    check(&[
        ("function_singularities(inverse_weierstrass_p(x, [1, 2]), x)", "-x + 4*x^3 <= 2"),
        ("function_discontinuities(inverse_weierstrass_p(x, [1, 2]), x)", "-x + 4*x^3 <= 2"),
        ("function_domain(inverse_weierstrass_p(x, [1, 2]), x)", "root(&(-2 - &1 + 4*&1^3), 1) < x"),
        ("function_singularities(inverse_weierstrass_p(x, [0, 1]), x)", "4*x^3 <= 1"),
        ("function_singularities(inverse_weierstrass_p(x, [-1, -2]), x)", "x + 4*x^3 <= -2"),
        // the other properties are unchanged
        ("function_sign(inverse_weierstrass_p(x, [1, 2]), x)", "indeterminate"),
        ("function_convexity(inverse_weierstrass_p(x, [1/2, 1/2]), x)", "indeterminate"),
        ("function_injective(inverse_weierstrass_p(x, [2, 1]), x)", "true"),
        // the values: real beyond the real root e1 = 0.8981..., not real below it
        ("map(t => is_real(n(inverse_weierstrass_p(t, [1, 2]))), [0.89, 0.9, 5., 100.])", "[false, true, true, true]"),
        ("map(t => is_real(n(inverse_weierstrass_p(t, [-1, -2]))), [-0.7, -0.68, 0., 3.])", "[false, true, true, true]"),
    ]);
}

#[test]
fn inverse_weierstrass_p_sets_outside_the_case_end_undecided() {
    // three real roots, a double root, machine or irrational invariants, the two-argument form: no set, and the
    // calls end
    let open = [
        "function_singularities(inverse_weierstrass_p(x, [4, 1]), x)",
        "function_singularities(inverse_weierstrass_p(x, [3, 1]), x)",
        "function_singularities(inverse_weierstrass_p(x, [1., 2.]), x)",
        "function_singularities(inverse_weierstrass_p(x, [2^(1/2), 2]), x)",
        "function_singularities(inverse_weierstrass_p([x, 1], [1, 2]), x)",
        "function_singularities(inverse_weierstrass_p(x, [1, 2]), x, complexes)",
        "function_domain(inverse_weierstrass_p(x, [1, 2]), x, complexes)",
    ];
    // g2 = g3 = 0: the value -x/sqrt(x^3) (the sign rule p'(u) = sqrt(4 p^3)), singular for x^3 <= 0, i.e. x <= 0
    let src = format!("[{}, function_singularities(inverse_weierstrass_p(x, [0, 0]), x)]", open.join(", "));
    let want = format!("[{}, x^3 <= 0]", open.join(", ").replace("[1., 2.]", "[1.0, 2.0]"));
    // the stop flag is raised after the limit (the helper waits that long in any case)
    let (r, _) = Sys::new().interrupted(&src, Duration::from_secs(30));
    assert_eq!(r.expect("the calls end within 30 s"), want);
}
