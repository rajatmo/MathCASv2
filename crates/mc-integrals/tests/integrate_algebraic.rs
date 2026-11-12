//! Integrands with roots (item 14B): the substitution portfolio, Hermite reduction, Chebyshev's theorem,
//! Goursat's involutions and Legendre's forms; every answer checked by differentiation, proofs of
//! non-elementarity through `risch_integrate`, a few printed forms pinned.
mod common;
use common::Sys;

/// Points inside (0, 1), where every integrand of the lists below is defined and real.
const UNIT: &[&str] = &["(3/7)", "(5/11)", "(2/9)"];

fn all_found(s: &mut Sys, fs: &[&str]) {
    all_found_at(s, fs, UNIT)
}

fn all_found_at(s: &mut Sys, fs: &[&str], points: &[&str]) {
    let mut failures = Vec::new();
    for f in fs {
        if let Err(e) = s.check_antiderivative(f, points) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

/// Monomial, linear and linear-fractional radicals, nested radicals.
#[test]
fn rationalising_substitutions() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "sqrt(x + sqrt(x))",
            "1/(sqrt(x) + x^(1/3))",
            "sqrt(x)/(1 + x)",
            "x^(1/3)/(1 + x)",
            "1/(x*sqrt(x + 1))",
            "1/((x + 1)*sqrt(x + 2))",
            "1/(x*(1 + x)^(1/3))",
            "sqrt(x)/(x + 2)",
            "1/(x^(1/4) + 1)",
            "x^(1/2)/(1 + x^(1/4))",
            "sqrt(x + 1)/x",
            "sqrt((1 + x)/(1 - x))",
            "sqrt((x + 2)/(x + 1))/x",
            "((x + 1)/(2 - x))^(1/3)",
            "sqrt(x/(1 - x))",
            "1/(x^2 + sqrt(x))",
            "sqrt(x)*sqrt(1 + x)",
            "1/(sqrt(x + 2)*sqrt(x + 1))",
            "1/(x*sqrt(a + b*x))",
        ],
    );
}

/// Quadratic radicals: Hermite reduction with the table's forms, Euler's substitutions.
#[test]
fn quadratic_radicals() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "x^2/sqrt(1 - x^2)",
            "1/(x*sqrt(x^2 + 1))",
            "sqrt(x^2 + 1)/x",
            "x*sqrt(x^2 + x + 1)",
            "1/((x + 1)*sqrt(x^2 + 1))",
            "sqrt(x^2 + 4)/x^2",
            "(1 - x^2)^(3/2)",
            "1/(x*sqrt(1 - x^2))",
            "1/((x^2 + 1)*sqrt(x^2 + 2))",
            "1/((2*x + 1)*sqrt(x^2 + x + 1))",
            "x/sqrt(x^2 - x + 1)",
            "1/(x*sqrt(x^2 + 2*x + 3))",
            "x^2*sqrt(x^2 + 1)",
            "1/(x^3*sqrt(x^2 + 1))",
            "1/(1 + sqrt(x^2 + 1))",
            "(x^2 + 1)^(1/2)/(x^2 + 2)",
        ],
    );
}

/// Chebyshev's integrable cases, lowering the radicand's degree, Goursat's pseudo-elliptic integrals, rational
/// parts of higher radicals.
#[test]
fn binomials_lowering_goursat() {
    let mut s = Sys::new();
    all_found(
        &mut s,
        &[
            "(1 + x^4)^(-1/4)",
            "x^(1/2)*(1 + x^(1/4))^(1/3)",
            "1/(x*(1 + x^3)^(1/3))",
            "(1 + x^3)^(-1/3)",
            "x^5*(1 + x^3)^(2/3)",
            "x/sqrt(1 + x^4)",
            "x^3/sqrt(1 - x^8)",
            "x*sqrt(1 - x^4)",
            "(1 + x^2)/((1 - x^2)*sqrt(1 + x^4))",
            "(x^2 - 1)/((x^2 + 1)*sqrt(x^4 + 1))",
            "(x^2 - 1)/(x*sqrt(x^4 + 3*x^2 + 1))",
            "(x^2 + 1)/(x*sqrt(x^4 - x^2 + 1))",
            "x/((x^2 + 1)*sqrt(x^4 + 1))",
            "(5*x^3 + 2)/sqrt(x^3 + 1)",
            "(x^4 - 1)/(x^2*sqrt(x^4 + x^2 + 1))",
            // a decomposition of the radicand, u = x^2 + x
            "(2*x + 1)/sqrt(x^4 + 2*x^3 + x^2 + 1)",
            "1/((1 + x)^(1/3)*(1 - x)^(2/3))",
            "x/(x^3 + 1)^(2/3)",
            "1/(x*sqrt(1 + x^3 + x^6))",
        ],
    );
}

/// Chebyshev's pseudo-elliptic integral: the unit of `Q[x, y]` from the continued fraction of the square root (the
/// radicand is positive beyond 3.3).
#[test]
fn abel_unit() {
    let mut s = Sys::new();
    all_found_at(&mut s, &["x/sqrt(x^4 + 10*x^2 - 96*x - 71)", "(x + 1)/sqrt(x^4 + 4*x^3 + 2*x^2 - 4*x + 5)"], &["5", "11/2", "6"]);
}

/// Legendre's forms and hypergeometric forms of non-elementary integrals, pinned.
#[test]
fn elliptic_and_hypergeometric_forms() {
    let mut s = Sys::new();
    for (f, want) in [
        ("1/sqrt(1 - x^4)", "elliptic_f(asin(x), -1)"),
        ("1/sqrt((1 - x^2)*(1 - 2*x^2))", "elliptic_f(asin(x), 2)"),
        ("1/(sqrt(1 - x^2)*sqrt(1 - k*x^2))", "elliptic_f(asin(x), k)"),
        ("sqrt(1 - k*x^2)/sqrt(1 - x^2)", "elliptic_e(asin(x), k)"),
        ("1/((1 - n*x^2)*sqrt(1 - x^2)*sqrt(1 - k*x^2))", "elliptic_pi(n, asin(x), k)"),
        ("1/((1 + x^2)*sqrt(1 - x^4))", "elliptic_pi(-1, asin(x), -1)"),
        ("x^2/sqrt(1 - x^4)", "elliptic_e(asin(x), -1) - elliptic_f(asin(x), -1)"),
        ("sqrt((1 - x^2/4)/(1 - x^2))", "elliptic_e(asin(x), 1/4)"),
        ("1/sqrt(x*(1 - x)*(2 - x))", "elliptic_f(asin(x^(1/2)), 1/2)*2^(1/2)"),
        ("(1 + x^2)^(1/3)", "x*hypergeometric_2f1(-1/3, 1/2, 3/2, -x^2)"),
        ("sqrt(1 + x^3)", "x*hypergeometric_2f1(-1/2, 1/3, 4/3, -x^3)"),
    ] {
        assert_eq!(s.run(&format!("integrate({f}, x)")), s.run(want), "{f}");
        // and the check by differentiation (complex values for some: the absolute value)
        let e = s.run(&format!("n(1 + 10^10*abs(rewrite(d(integrate({f}, x), x) - {f}, [x -> 3/7, k -> 1/3, n -> 1/5])), 20)"));
        let v: f64 = e.trim_end_matches('.').parse().unwrap_or(f64::NAN);
        assert!((v - 1.0).abs() < 1e-3, "{f}: {e}");
    }
}

/// Proofs of non-elementarity: Chebyshev's theorem and a remainder of the second kind.
#[test]
fn proofs_of_non_elementarity() {
    let mut s = Sys::new();
    for (f, why) in [
        ("sqrt(1 + x^3)", "Chebyshev"),
        ("(1 + x^2)^(1/3)", "Chebyshev"),
        ("sqrt(x)*(1 + x)^(1/3)", "Chebyshev"),
        ("1/sqrt(1 - x^4)", "Chebyshev"),
        ("x^2/sqrt(x^3 + x + 1)", "second kind"),
        ("(x + 2)/sqrt(x^3 - x + 1)", "second kind"),
    ] {
        let r = s.run(&format!("risch_integrate({f}, x)"));
        assert!(r.starts_with("failure(nonelementary") && r.contains(why), "{f}: {r}");
    }
    // an elementary one is not refuted: x/sqrt(1 + x^4) = asinh(x^2)/2
    let r = s.run("risch_integrate(x/sqrt(1 + x^4), x)");
    assert!(!r.starts_with("failure"), "{r}");
}

/// Printed forms of the documented example and a few classics.
#[test]
fn printed_forms() {
    let mut s = Sys::new();
    for (f, want) in [
        ("1/(x^2*sqrt(x^2 - 1))", "(-1 + x^2)^(1/2)/x"),
        ("1/(1 + x^2)^(3/2)", "x/(1 + x^2)^(1/2)"),
        ("(5*x^3 + 2)/sqrt(x^3 + 1)", "2*x*(1 + x^3)^(1/2)"),
        ("sqrt(x)/(1 + x)", "2*(-atan(x^(1/2)) + x^(1/2))"),
        // the trigonometric substitution's form is the smallest
        ("1/((x^2 + 1)*sqrt(x^2 + 2))", "atan(x/(2 + x^2)^(1/2))"),
        ("x^2*sqrt(x^2 + 1)", "(-1/8)*log(x + (1 + x^2)^(1/2)) + ((1/8)*x + (1/4)*x^3)*(1 + x^2)^(1/2)"),
    ] {
        assert_eq!(s.run(&format!("integrate({f}, x)")), s.run(want), "{f}");
    }
    let r = s.run("integrate(sqrt(x + sqrt(x)), x)");
    assert!(!r.contains("integrate(") && r.contains("(x + x^(1/2))^(1/2)"), "{r}");
}

/// Definite integrals through the driver.
#[test]
fn definite() {
    let mut s = Sys::new();
    for (f, range, want) in [
        ("sqrt(x)/(1 + x)", "[x, 0, 1]", 2.0 - std::f64::consts::FRAC_PI_2),
        ("1/(x*sqrt(x^2 + 1))", "[x, 1, 2]", 0.4001617619599396),
        ("1/sqrt(1 - x^4)", "[x, 0, 1/2]", 0.5032094431773309),
        ("x^2/sqrt(1 - x^2)", "[x, 0, 1/2]", 0.0452930368530398),
        ("x^2*sqrt(x^2 + 1)", "[x, 0, 1]", 0.42015838751246775),
    ] {
        let out = s.run(&format!("n(integrate({f}, {range}), 20)"));
        let v: f64 = out.trim_end_matches('.').parse().unwrap_or(f64::NAN);
        assert!((v - want).abs() < 1e-8, "{f} over {range}: {out}");
    }
}
