//! Poles of functions of a polynomial, root counts of meromorphic and special functions, q-products and
//! the expansions of the harmonic-type numbers. Each answer is checked by an independent route (numbers, a sum term
//! by term, the defining property at the points).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn poles_of_a_function_of_a_polynomial() {
    assert_eq!(
        ev("function_poles(gamma(x^2 + 1), x) === [[conditional_expression(-im*(1 - c(1))^(1/2), element(c(1), integers) and c(1) <= 0), 1], [conditional_expression(im*(1 - c(1))^(1/2), element(c(1), integers) and c(1) <= 0), 1]]"),
        "true"
    );
    assert_eq!(
        ev("function_poles([gamma(x^2 + 1), abs(x) <= 2], x) === [[-im, 1], [im, 1], [-2im, 1], [2im, 1], [-im*2^(1/2), 1], [im*2^(1/2), 1], [-im*3^(1/2), 1], [im*3^(1/2), 1]]"),
        "true"
    );
    assert_eq!(
        ev("function_poles(tan(x^2), x)"),
        "[[conditional_expression((-1/2)*2^(1/2)*pi^(1/2)*(1 + 2*c(1))^(1/2), element(c(1), integers)), 1], [conditional_expression((1/2)*2^(1/2)*pi^(1/2)*(1 + 2*c(1))^(1/2), element(c(1), integers)), 1]]"
    );
    // the poles inside the disc: residues summed and compared with the contour integral on the unit circle
    let f = "gamma(x^3 - x/3)";
    assert_eq!(
        ev(&format!("pp = function_poles([{f}, abs(x) <= 1], x); root_reduce(2*pi*im*total(map(&(residue({f}, [x, &1[1]])), pp))) === pi*root(&(46656 + 11664*&1^2 - 156600*&1^4 + 525625*&1^6), 4)")),
        "true"
    );
    assert_eq!(
        ev(&format!(
            "pp = function_poles([{f}, abs(x) <= 1], x); abs(n(n_integrate(rewrite({f}, x -> exp(im*t))*im*exp(im*t), [t, 0, 2*pi]) - 2*pi*im*total(map(&(residue({f}, [x, &1[1]])), pp)))) < 10^-8"
        )),
        "true"
    );
    // variations: a member of the family is a pole of gamma (u there is -3); a double root of u + 1 (x^2 + 2 x at
    // x = -1) leaves the unbounded call, and each listed point is a pole of order 1
    assert_eq!(ev("expand(rewrite(x^2 + 1, x -> im*(1 - (-3))^(1/2)))"), "-3");
    assert_eq!(ev("function_poles(gamma(x^2 + 2*x), x)"), "function_poles(gamma(2*x + x^2), x)");
    assert_eq!(ev("function_poles([tan(x^2), abs(x) < 2], x)[all, 2]"), "[1, 1, 1, 1]");
    // the parameter's name from generated_parameters
    assert!(ev("function_poles(tan(x^2), x, generated_parameters: &(subscript(k, &1)))").contains("subscript(k, 1)"));
}

#[test]
fn root_counts_of_meromorphic_and_special_functions() {
    assert_eq!(ev("count_roots(bessel_j(2, x) - sin(x^2), [x, -3, 3])"), "8");
    assert_eq!(ev("count_roots(gamma(x) + exp(x) - 7*x, [x, -3, 3])"), "6");
    assert_eq!(ev("count_roots(tan(z) - log(z + 3) - z^2, [z, -2 - 2*im, 2 + 2*im])"), "4");
    // variations: tan(x) - x has a triple root at 0 (x^3/3 + ...) and one root in each (k pi, k pi + pi/2) for
    // k = +-1 on [-5, 5] (sign changes below); gamma(x) - 1 has roots at 1 and 2 and none below on [-5/2, 3]
    assert_eq!(ev("count_roots(tan(x) - x, [x, -5, 5])"), "5");
    assert_eq!(ev("[sign(n(tan(4.4) - 4.4)), sign(n(tan(4.6) - 4.6))]"), "[-1, 1]");
    assert_eq!(ev("count_roots(gamma(x) - 1, [x, -5/2, 3])"), "2");
    assert_eq!(ev("count_roots(1/(x - 1) - 2, [x, 0, 3])"), "1");
}

#[test]
fn q_products() {
    assert_eq!(ev("product(1 + a^i, [i, inf], generate_conditions: true)"), "conditional_expression((1/2)*q_pochhammer(-1, a), abs(a) < 1)");
    assert_eq!(ev("product(1 + a^i, [i, inf], assumptions: -(1/2) < a and a < 1/2, generate_conditions: true)"), "(1/2)*q_pochhammer(-1, a)");
    // variations against the factors multiplied out: an infinite product at q = 1/3 and a finite one
    assert_eq!(ev("abs(n(product(1 + (1/3)^i, [i, inf]) - product(1 + (1/3)^i, [i, 1, 200]), 30)) < 10^-25"), "true");
    assert_eq!(ev("simplify(rewrite(product(3*(1 - 2^k), [k, 1, m]), m -> 5) - product(3*(1 - 2^k), [k, 1, 5]))"), "0");
    assert_eq!(ev("abs(n(rewrite(product(1 - x*q^k, [k, 0, inf]), [x -> 1/2, q -> 1/5])) - n(product(1 - (1/2)*(1/5)^k, [k, 0, 300]))) < 10^-14"), "true");
}

#[test]
fn expansions_of_harmonic_type_numbers() {
    assert_eq!(ev("function_expand(hyper_harmonic_number(4, n))"), "(1/6)*(1 + n)*(2 + n)*(3 + n)*(-11/6 + euler_gamma + digamma(4 + n))");
    assert_eq!(ev("simplify(function_expand(hyper_harmonic_number(p, 2)))"), "1/2 + p");
    assert_eq!(ev("function_expand(hyper_harmonic_number(1, 3/4))"), "4/3 - log(8) + (1/2)*pi");
    assert_eq!(
        ev("function_expand(alternating_factorial(n)) === -cos(n*pi) + e*gamma(2 + n)*gamma(-1 - n, 1) - e*cos(n*pi)*exp_integral_ei(-1)"),
        "true"
    );
    // variations: the expansions against the definitions at exact and inexact points
    assert_eq!(ev("n(function_expand(alternating_factorial(5)))"), "101.0");
    assert_eq!(ev("abs(n(function_expand(hyper_harmonic_number(1/3, 3/4)) - hyper_harmonic_number(1/3, 0.75))) < 10^-14"), "true");
    assert_eq!(ev("abs(n(function_expand(alternating_harmonic_number(7/3, 3)) - alternating_harmonic_number(7/3, 3.0))) < 10^-14"), "true");
    assert_eq!(ev("abs(n(function_expand(alternating_harmonic_number(6, 4))) - alternating_harmonic_number(6, 4)) < 10^-14"), "true");
    // Gauss's theorem against the numeric digamma
    for x in ["-7/6", "11/4", "5/3", "1/2", "-5/2"] {
        assert_eq!(ev(&format!("abs(n(function_expand(digamma({x}))) - digamma(n({x}))) < 10^-13")), "true", "{x}");
    }
}
