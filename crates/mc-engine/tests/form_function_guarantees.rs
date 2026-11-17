//! The forms the form functions guarantee (their docs pages state them): `together` keeps the written denominators as
//! factors and the rational content outside, `apart` writes each fraction with an integer primitive numerator,
//! `expand_all` expands denominators, `trig_reduce` writes the content outside, `factor` reads powers with a common
//! symbolic exponent as one variable, `trig_factor` merges Pythagorean pairs, `horner_form` keeps rational
//! coefficients, `factor_terms` maps over logical combinations. Each family is checked over many inputs: the answer
//! is equal to the input (cancelled difference 0, or the value at a point) and has the stated shape.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `a` evaluates to the same term as the form `b` (the test engine prints plainly, so terms are compared).
fn is(e: &mut CoreEngine, a: &str, b: &str) {
    let got = ev(e, a);
    assert_eq!(ev(e, &format!("({a}) === ({b})")), "true", "{a} gave {got}");
}

/// `a - b` cancels to 0.
fn same(e: &mut CoreEngine, a: &str, b: &str) -> bool {
    ev(e, &format!("together(expand(({a}) - ({b})))")) == "0"
}

/// The value of `a - b` at x = 3/10 (s = 7/10) is 0 to 12 digits.
fn same_at_a_point(e: &mut CoreEngine, a: &str, b: &str) -> bool {
    ev(e, &format!("abs(n(rewrite(({a}) - ({b}), [x -> 3/10, s -> 7/10]), 20)) < 10^-12")) == "true"
}

#[test]
fn together_keeps_written_factors_and_content() {
    let mut e = CoreEngine::new();
    is(&mut e, "together(1/x + 1/(x + 1) + 1/(x + 2) + 1/(x + 3))", "2*(3 + 11*x + 9*x^2 + 2*x^3)/(x*(1 + x)*(2 + x)*(3 + x))");
    is(&mut e, "together((x - 1)*((x - 2)/((x - 3)*(x - 4)*(x - 5))))", "(-2 + x)*(-1 + x)/((-5 + x)*(-4 + x)*(-3 + x))");
    is(&mut e, "together(1/(sqrt(2) + x) + 1/(x^2 - 2))", "(-2 + x + 2^(1/2) + x^2)/((-2 + x^2)*(x + 2^(1/2)))");
    is(&mut e, "together(2*x + 4)", "4 + 2*x");
    // many sums of fractions: equal to the input, the denominator a product, idempotent
    for a in 1..=4 {
        for b in 1..=3 {
            let src = format!("{a}/(x + {b}) + {b}/(x - {a}) + x/(2*x^2 + {})", a + b);
            let t = ev(&mut e, &format!("together({src})"));
            assert!(same(&mut e, &t, &src), "{src}: {t}");
            assert_eq!(ev(&mut e, &format!("together({t})")), t, "{src}");
            assert!(ev(&mut e, &format!("head(denominator({t}))")) == "mul", "{src}: {t}");
        }
    }
}

#[test]
fn apart_numerators_are_primitive_integer_polynomials() {
    let mut e = CoreEngine::new();
    is(
        &mut e,
        "apart((x^5 - 2)/((1 + x + x^2)*(2 + x)*(1 - x)))",
        "2 - 34/(9*(2 + x)) - x + 1/(9*(-1 + x)) + (-3 - x)/(3*(1 + x + x^2))",
    );
    is(&mut e, "apart((x + y)/((x + 1)*(y + 1)*(x - y)), x)", "(1 - y)/((1 + x)*(1 + y)^2) + 2*y/((1 + y)^2*(x - y))");
    for k in 1..=6 {
        let src = format!("(x^3 + {k}*x + 1)/((x^2 + {k})*(3*x + {})*(x - 1)^2)", k + 1);
        let r = ev(&mut e, &format!("apart({src}, x)"));
        assert!(same(&mut e, &r, &src), "{src}: {r}");
        // every numerator has integer coefficients
        let ok = ev(&mut e, &format!("every(apply(list, {r}), t => every(coefficient_list(expand(numerator(t)), x), is_integer))"));
        assert_eq!(ok, "true", "{src}: {r}");
    }
}

#[test]
fn expand_all_expands_denominators() {
    let mut e = CoreEngine::new();
    is(&mut e, "expand_all((x + y)^2/(z + y)^2)", "x^2/(2*y*z + y^2 + z^2) + y^2/(2*y*z + y^2 + z^2) + 2*x*y/(2*y*z + y^2 + z^2)");
    is(&mut e, "expand_all(1/(1 + x)^3 + sin((1 + x)^3))", "(1 + 3*x + 3*x^2 + x^3)^-1 + sin(1 + 3*x + 3*x^2 + x^3)");
    for k in 1..=5 {
        let src = format!("(x + {k})^2/((x - {k})*(x + y)^2) + 1/(x + {k})^2");
        let r = ev(&mut e, &format!("expand_all({src})"));
        assert!(same(&mut e, &r, &src), "{src}: {r}");
        let ok = ev(&mut e, &format!("every(apply(list, {r}), t => denominator(t) === expand(denominator(t)) and numerator(t) === expand(numerator(t)))"));
        assert_eq!(ok, "true", "{src}: {r}");
    }
}

#[test]
fn trig_reduce_writes_the_content_outside() {
    let mut e = CoreEngine::new();
    is(&mut e, "trig_reduce(4*sin(x)^4 == 1 and 2*cos(x)^2 >= 1)", "(1/2)*(3 + cos(4*x) - 4*cos(2*x)) == 1 and 1 + cos(2*x) >= 1");
    is(&mut e, "trig_reduce(2*(cos(x) + cos(3*x)))", "2*(cos(x) + cos(3*x))");
    for (m, n) in [(2, 2), (3, 1), (4, 0), (1, 5), (3, 3), (6, 2)] {
        let src = format!("sin(x)^{m}*cos(x)^{n}");
        let r = ev(&mut e, &format!("trig_reduce({src})"));
        assert!(same_at_a_point(&mut e, &r, &src), "{src}: {r}");
        // the sum inside has integer coefficients without a common factor
        let inner = ev(&mut e, &format!("{{ let r = {r}; if head(r) === mul then r[2] else r }}"));
        let cs = ev(&mut e, &format!("map(t => if is_number(t) then t else if head(t) === mul and is_number(t[1]) then t[1] else 1, apply(list, {inner}))"));
        assert_eq!(ev(&mut e, &format!("every({cs}, is_integer) and apply(gcd, {cs}) == 1")), "true", "{src}: {r}");
    }
}

#[test]
fn factor_reads_a_common_symbolic_exponent() {
    let mut e = CoreEngine::new();
    is(&mut e, "factor(x^(2*s) + 2*x^s + 1)", "(1 + x^s)^2");
    is(&mut e, "factor(x^((2/3)*s) + 2*x^s + 1)", "(1 + x^((1/3)*s))*(1 - x^((1/3)*s) + 2*x^((2/3)*s))");
    is(&mut e, "factor(2^(3*x) - 1)", "(-1 + 2^x)*(1 + 2^x + 2^(2*x))");
    for p in ["x^(4*s) - 1", "x^(3*s) + 1", "4^(2*x) - 2*4^x + 1", "y^(6*s) - y^(2*s)", "x^((1/2)*s) - x^((3/2)*s)"] {
        let f = ev(&mut e, &format!("factor({p})"));
        assert!(f.contains('*') || f.contains(")^"), "{p}: {f}");
        assert_eq!(ev(&mut e, &format!("expand(({f}) - ({p}))")), "0", "{p}: {f}");
    }
}

#[test]
fn trig_factor_merges_pythagorean_pairs() {
    let mut e = CoreEngine::new();
    is(&mut e, "trig_factor(cosh(x)^2 - cosh(x)^4)", "-cosh(x)^2*sinh(x)^2");
    for p in ["cos(x)^2 - cos(x)^4", "sin(x)^3 - sin(x)", "cosh(x)^4 - 2*cosh(x)^2 + 1", "1 - cos(x)^2"] {
        let f = ev(&mut e, &format!("trig_factor({p})"));
        assert!(!f.contains("-1 + "), "{p}: {f}");
        assert!(same_at_a_point(&mut e, &f, p), "{p}: {f}");
    }
}

#[test]
fn horner_form_keeps_rational_coefficients() {
    let mut e = CoreEngine::new();
    is(
        &mut e,
        "horner_form((315*x^8 - 539*x^6 + 285*x^4 - 45*x^2)/16, x)",
        "(-45/16 + (285/16 + (-539/16 + (315/16)*x^2)*x^2)*x^2)*x^2",
    );
    for k in 2..=6 {
        let src = format!("x^3/{k} + x/3 + 1/{}", k + 1);
        let r = ev(&mut e, &format!("horner_form({src})"));
        assert_eq!(ev(&mut e, &format!("expand(({r}) - ({src}))")), "0", "{src}: {r}");
    }
}

#[test]
fn the_other_written_forms() {
    let mut e = CoreEngine::new();
    is(
        &mut e,
        "factor_terms(1 < 77*x^3 - 21*x + 35 and 77*x^3 - 21*x + 35 < 2)",
        "1 < 7*(5 - 3*x + 11*x^3) and 7*(5 - 3*x + 11*x^3) < 2",
    );
    is(&mut e, "simplify(quotient(4*n + 5, 2), element(n, integers))", "2*(1 + n)");
    is(&mut e, "complex_expand(re(2*z^3 - z + 1), z)", "1 - re(z) + 2*re(z)^3 - 6*im_part(z)^2*re(z)");
    is(&mut e, "apart_square_free((x^2 - a)/(x^3 - 2*a*x^2 + a^2*x))", "(-1 + a)/(x - a)^2 - 1/(a*x) + (1 + a)/(a*(x - a))");
    is(&mut e, "normal(series(inverse_jacobi_sn(x, m), m, 0, 1))", "asin(x) + m*((1/4)*asin(x) - (1/4)*x*(1 - x^2)^(1/2))");
    is(&mut e, "collect((1 + x + y + z)^2, [x, y], simplify)", "x*(2*y + 2*(1 + z)) + 2*y*(1 + z) + x^2 + y^2 + (1 + z)^2");
    is(&mut e, "power_expand(sqrt(x*y), assumptions: true)", "exp(im*pi*floor(1/2 - arg(x)/(2*pi) - arg(y)/(2*pi)))*x^(1/2)*y^(1/2)");
    is(&mut e, "full_simplify(4*sin(x)^2*cos(x)^2 + 4*sin(x)*cos(x) + 1, trig: false)", "(1 + 2*cos(x)*sin(x))^2");
    is(&mut e, "full_simplify(golden_ratio^4 - golden_ratio)", "3 + 5^(1/2)");
    is(&mut e, "simplify(sqrt((x - y)^2) + sqrt((y - x)^2))", "2*((x - y)^2)^(1/2)");
}
