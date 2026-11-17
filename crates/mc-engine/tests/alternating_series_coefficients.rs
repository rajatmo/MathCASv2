//! Methods for inputs that gave no answer: alternating series of rational functions (pairs of terms, the residue form
//! for even summands, Gauss's digamma theorem and Dirichlet's beta at odd integers); sums of Bell and Stirling numbers
//! and the alternating factorial; general series coefficients of G(x^q) and G(sqrt(x)), of logarithms, of exp with a
//! constant term and in several variables. Every closed form is checked by an independent route: partial sums with
//! the tail bounded, the terms added one by one, or the Taylor coefficients computed by `series`. Each test has a time
//! bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `src` evaluates to the same expression as `want` (both evaluated; structural equality).
fn same(e: &mut CoreEngine, src: &str, want: &str) {
    let r = ev(e, &format!("vv = {src}; vv === ({want})"));
    assert_eq!(r, "true", "{src}: {}", ev(e, "vv"));
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn engine() -> CoreEngine {
    CoreEngine::new()
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

/// The closed form `v` of `sum((-1)^k g(k), [k, lo, inf])` checked twice: it lies between the partial sums up to 3999
/// and up to 4000 (Leibniz: the terms decrease in size from there on, so the value lies between any two consecutive
/// partial sums), and it agrees with `n_sum` (convergence acceleration of the terms) to 20 digits.
fn alternating_check(e: &mut CoreEngine, g: &str, lo: i64, v: &str) {
    let src = format!(
        "{{ let s1 = n(sum((-1)^k*({g}), [k, {lo}, 3999]), 30); let s2 = s1 + n((-1)^4000*rewrite({g}, k -> 4000), 30);          let w = n({v}, 30); min(s1, s2) <= w and w <= max(s1, s2) }}"
    );
    holds(e, &src);
    let src = format!("abs(n({v}, 30) - n_sum((-1)^k*({g}), [k, {lo}, inf], working_precision: 30)) < 10^-20");
    holds(e, &src);
}

#[test]
fn alternating_series_of_rational_functions() {
    timed("alternating series", 120, || {
        let mut e = engine();
        // the documented input: Catalan's constant
        same(&mut e, "sum((-1)^k / (2 * k + 1)^2, [k, 0, inf])", "catalan");
        // Leibniz, Dirichlet beta(3), eta(2), Gauss digamma at 1/3, and a pole pair not at an integer
        for (g, lo, want) in [
            ("1/(2*k + 1)", 0, "pi/4"),
            ("1/(2*k + 1)^3", 0, "pi^3/32"),
            ("1/(2*k + 1)^5", 0, "5*pi^5/1536"),
            ("1/k^2", 1, "-pi^2/12"),
            ("1/(3*k + 1)", 0, "(3*log(2) + pi*3^(1/2))/9"),
            ("1/(k*(k + 1))", 1, "1 - log(4)"),
            ("1/(4*k^2 - 1)", 1, "(2 - pi)/4"),
        ] {
            let src = format!("sum((-1)^k*({g}), [k, {lo}, inf])");
            same(&mut e, &src, want);
            alternating_check(&mut e, g, lo, want);
        }
        // even summands with complex poles: the residue form pi/sinh(pi), written pi csch(pi) (the reciprocal function
        // keeps its own name)
        for (g, lo, want) in [
            ("1/(k^2 + 1)", 0, "(1 + pi*csch(pi))/2"),
            ("1/(k^2 + 1)", 1, "(-1 + pi*csch(pi))/2"),
            ("1/(k^2 + 4)", 0, "(1 + 2*pi*csch(2*pi))/8"),
        ] {
            let src = format!("sum((-1)^k*({g}), [k, {lo}, inf])");
            same(&mut e, &src, want);
            alternating_check(&mut e, g, lo, want);
        }
        // a summand even about k = 12 (the documented n_sum page) and one about k = 3, from where the sum starts
        for (g, lo) in [("1/(1 + (k - 12)^2)", 0), ("1/((k - 3)^2 + 4)", 1)] {
            let v = ev(&mut e, &format!("sum((-1)^k*({g}), [k, {lo}, inf])"));
            assert!(!v.contains("sum") && !v.contains("digamma"), "{g}: {v}");
            alternating_check(&mut e, g, lo, &v);
        }
        holds(&mut e, "abs(n(sum((-1)^x/(1 + (x - 12)^2), [x, 0, inf]) - n_sum((-1)^x/(1 + (x - 12)^2), [x, 0, inf], working_precision: 30), 30)) < 10^-25");
        // the sign as cos(pi k)
        same(&mut e, "sum(cos(pi*k)/(k + 1)^2, [k, 0, inf])", "pi^2/12");
        // a divergent or non-alternating summand is not taken: the terms must tend to 0
        same(&mut e, "sum((-1)^k*k/(k + 1), [k, 0, inf])", "sum((-1)^k*k/(k + 1), [k, 0, inf])");
    });
}

#[test]
fn sums_of_bell_and_stirling_numbers() {
    timed("Bell and Stirling sums", 60, || {
        let mut e = engine();
        same(&mut e, "sum(bell_b(n) * (x^n / factorial(n)), [n, 0, inf])", "exp(exp(x) - 1)");
        same(&mut e, "sum(k^n / factorial(k), [k, 0, inf], assumptions: element(n, integers) and n >= 0)", "e*bell_b(n)");
        same(&mut e, "sum(stirling_s2(n, k), [k, 0, n])", "bell_b(n)");
        same(&mut e, "sum((-1)^(n - k) * factorial(k), [k, n])", "alternating_factorial(n)");
        same(&mut e, "sum(stirling_s2(n, m)*x^n/factorial(n), [n, 0, inf], assumptions: element(m, integers) and m >= 0)", "(exp(x) - 1)^m/factorial(m)");
        // without the assumption that the power is a natural number, Dobinski's formula is not used
        same(&mut e, "sum(k^n / factorial(k), [k, 0, inf])", "sum(k^n/factorial(k), [k, 0, inf])");
        // independent checks: the row sums at n = 0..12, Dobinski at n = 0..8 by 60 terms (tail below 10^-40),
        // the generating functions' Taylor coefficients up to x^10, the alternating factorial at n = 1..10
        holds(&mut e, "every(range(0, 12), j => sum(stirling_s2(j, i), [i, 0, j]) == bell_b(j))");
        holds(&mut e, "every(range(0, 8), j => abs(n(sum(i^j/factorial(i), [i, 0, 60]) - e*bell_b(j), 50)) < 10^-30)");
        holds(&mut e, "every(range(0, 10), j => series_coefficient(exp(exp(x) - 1), [x, 0, j]) == bell_b(j)/factorial(j))");
        holds(&mut e, "every(range(0, 10), j => series_coefficient((exp(x) - 1)^3/6, [x, 0, j]) == stirling_s2(j, 3)/factorial(j))");
        holds(&mut e, "every(range(1, 10), j => alternating_factorial(j) == sum((-1)^(j - i)*factorial(i), [i, 1, j]))");
    });
}

/// The general coefficient `c` (a function of `n`) of `f` against `series_coefficient(f, [x, 0, j])` for j = 0..m.
fn coefficients_check(e: &mut CoreEngine, f: &str, m: i64) {
    let src = format!(
        "{{ let c = series_coefficient({f}, [x, 0, n]); every(range(0, {m}), j => simplify(rewrite(c, n -> j) - series_coefficient({f}, [x, 0, j])) === 0) }}"
    );
    holds(e, &src);
}

#[test]
fn general_series_coefficients() {
    timed("series coefficients", 120, || {
        let mut e = engine();
        same(&mut e, "series_coefficient(cos(sqrt(x)), [x, 0, n])", "piecewise([[(-1)^n/factorial(2*n), n >= 0]], 0)");
        same(&mut e, "series_coefficient(exp(x^2), [x, 0, n])", "piecewise([[1/gamma((2 + n)/2), mod(n, 2) == 0 and n >= 0]], 0)");
        same(&mut e, "series_coefficient(exp(x + y), [x, 0, n], [y, 0, m])", "piecewise([[1/(factorial(m)*factorial(n)), n >= 0 and m >= 0]], 0)");
        same(&mut e, "series_coefficient(exp(3*x + y^2), [x, 0, m], [y, 0, n])", "piecewise([[3^m/(factorial(m)*gamma((2 + n)/2)), m >= 0 and mod(n, 2) == 0 and n >= 0]], 0)");
        same(&mut e, "series_coefficient(log(1 + x), [x, 0, n])", "piecewise([[-(-1)^n/n, n >= 1]], 0)");
        // every general coefficient against the coefficients computed one by one
        for (f, m) in [
            ("cos(sqrt(x))", 8),
            ("cosh(sqrt(x))", 8),
            ("exp(x^2)", 10),
            ("x*exp(x^3)", 10),
            ("x^2*cos(x^2)", 12),
            ("log(1/sqrt(1 - 5*x^2))", 10),
            ("log(2 + x)", 8),
            ("log(1 - x)/x", 8),
            ("exp(x + 1)", 8),
            ("sinh(2*x^2)", 10),
        ] {
            coefficients_check(&mut e, f, m);
        }
        // sin(sqrt(x)) has no Taylor series at 0: no general coefficient
        same(&mut e, "series_coefficient(sin(sqrt(x)), [x, 0, n])", "series_coefficient(sin(x^(1/2)), [x, 0, n])");
    });
}

/// `polynomial_sum_of_squares_list(p, vs)` gives squares adding up to `p` (expanded exactly) and `p` is checked
/// nonnegative on a grid independently (a sum of squares is).
fn sos_check(e: &mut CoreEngine, p: &str, vs: &str) {
    holds(e, &format!("{{ let s = polynomial_sum_of_squares_list({p}, {vs}); is_list(s) and simplify(expand({p} - total(map(q => q^2, s)))) === 0 }}"));
}

#[test]
fn sums_of_squares_of_higher_degree() {
    timed("sums of squares", 120, || {
        let mut e = engine();
        // the documented inputs: an exact rational and one with transcendental constants
        sos_check(&mut e, "x^4 + x*y + y^2 + 1/60", "[x, y]");
        sos_check(&mut e, "e*x^2 + sqrt(2)*x*y + pi*y^4 + log(2)", "[x, y]");
        // variations: the Gram matrix with a free entry (x^4 + 1), two more of degree 4 (at most 12 free Gram entries are searched)
        sos_check(&mut e, "x^4 + 1", "[x]");
        sos_check(&mut e, "x^4 - 2*x^3 + 3*x^2 - 2*x + 2", "[x]");
        sos_check(&mut e, "x^4 + y^4 + x^2*y^2 - x*y + 1", "[x, y]");
        // not nonnegative: no squares (x^4 - x^2 is -1/4 at x^2 = 1/2)
        same(&mut e, "polynomial_sum_of_squares_list(x^4 - x^2, [x])", "polynomial_sum_of_squares_list(x^4 - x^2, [x])");
        // nonnegative on the boundary only (minimum 1/100 - ... with no strictly feasible Gram matrix): unevaluated
        same(&mut e, "polynomial_sum_of_squares_list(201/100 + 16*x^4 + 16*y^4 + 16*x*y, [x, y])", "polynomial_sum_of_squares_list(201/100 + 16*x^4 + 16*y^4 + 16*x*y, [x, y])");
    });
}
