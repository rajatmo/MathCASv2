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

// `piecewise_expand`: pieces in the order of their values (polynomials of one variable by degree, then by their
// coefficients from the leading one), equal values merged, the default 0 or the last value, the waves' values
// factored, `max` inside a condition expanded; each answer also equal to the input at points of every piece.
#[test]
fn piecewise_expand_orders_merges_and_factors() {
    let mut e = CoreEngine::new();
    is(&mut e, "piecewise_expand(sawtooth_wave(x), -1 <= x and x <= 1)", "piecewise([[-1 + x, x >= 1], [x, 0 <= x and x < 1]], 1 + x)");
    is(&mut e, "piecewise_expand(square_wave(x), 0 < x and x < 1)", "piecewise([[-1, x >= 1/2]], 1)");
    is(
        &mut e,
        "piecewise_expand(triangle_wave(x), -1 <= x and x <= 1)",
        "piecewise([[4*(-1 + x), x >= 3/4], [4*x, -1/4 <= x and x < 1/4], [4*(1 + x), x < -3/4], [-2*(-1 + 2*x), 1/4 <= x and x < 3/4]], -2*(1 + 2*x))",
    );
    is(&mut e, "piecewise_expand(min(x^2 + 2*x - 2, max(2*x^2 - 3*x + 4, x^2 - 3)))", "piecewise([[-2 + 2*x + x^2, -5*x + x^2 >= -6]], 4 - 3*x + 2*x^2)");
    for (f, c) in [("triangle_wave(3*x)", "0 < x and x < 2"), ("sawtooth_wave(x/2) + square_wave(x)", "-2 < x and x < 2"), ("max(x, 2 - x^2)", "-3 < x and x < 3")] {
        let r = ev(&mut e, &format!("piecewise_expand({f}, {c})"));
        for k in -24..=24 {
            let p = format!("{k}/8");
            if ev(&mut e, &format!("rewrite({c}, x -> {p})")) == "true" {
                assert_eq!(ev(&mut e, &format!("rewrite(({f}) - ({r}), x -> {p})")), "0", "{f} = {r} at x = {p}");
            }
        }
    }
}

// The forms of exp_to_trig, trig_to_exp, cancel, together, factor_terms, apart in a kernel and to_radicals: each
// family runs over many inputs within a time budget (it stops early, after at least a few inputs).

/// The time each family below may take.
const FAMILY_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);

/// `a - b` vanishes at the points `xs` of x (t, y and a set to numbers too): |value| < 10^-15 at 20 digits (the 1
/// added keeps an exact zero away from zero, where `n` cannot reach a relative precision).
fn zero_at_points(e: &mut CoreEngine, a: &str, b: &str, xs: &[&str]) -> bool {
    xs.iter().all(|x| {
        let d = format!("rewrite(({a}) - ({b}), [x -> {x}, t -> {x} + 1/3, y -> 2/7, a -> 5/3])");
        let src = format!("abs(n(1 + 10^12*{d}, 20) - 1) < 10^-3");
        ev(e, &src) == "true"
    })
}

#[test]
fn exp_to_trig_leaves_no_exponential_of_a_variable() {
    let mut e = CoreEngine::new();
    is(&mut e, "exp_to_trig(3^x - 3^(-x))", "2*sinh(x*log(3))");
    is(&mut e, "exp_to_trig([exp(im*x) == -1, (-1)^(1/11)])", "[cos(x) + im*sin(x) == -1, cos((1/11)*pi) + im*sin((1/11)*pi)]");
    is(&mut e, "exp_to_trig((-1)^(5/7))", "-sin((3/14)*pi) + im*cos((3/14)*pi)");
    is(&mut e, "exp_to_trig([y(t) -> a*exp(t) + b*exp(-t)])", "[y(t) -> a*cosh(t) + a*sinh(t) + b*cosh(t) - b*sinh(t)]");
    is(&mut e, "exp_to_trig(exp(x) + exp(2 + im*x), hyperbolic: false)", "exp(x) + cos(x)*exp(2) + im*exp(2)*sin(x)");
    let start = std::time::Instant::now();
    let mut n = 0;
    'outer: for c in 1..=4 {
        for k in 2..=5 {
            if start.elapsed() > FAMILY_BUDGET && n >= 4 {
                break 'outer;
            }
            let src = format!("{c}*exp({k}*x) - exp(-x)/{c} + {k}^(x/{c}) + exp(im*{k}*x + {c}*t)");
            let r = ev(&mut e, &format!("exp_to_trig({src})"));
            assert!(zero_at_points(&mut e, &r, &src, &["3/10", "-7/5"]), "{src}: {r}");
            assert_eq!(ev(&mut e, &format!("free_of({r}, exp) and free_of({r}, e)")), "true", "{src}: {r}");
            let r = ev(&mut e, &format!("exp_to_trig({src}, hyperbolic: false)"));
            assert!(zero_at_points(&mut e, &r, &src, &["3/10"]), "{src}: {r}");
            assert_eq!(ev(&mut e, &format!("free_of({r}, cosh) and free_of({r}, sinh)")), "true", "{src}: {r}");
            n += 1;
        }
    }
    // powers of -1: the value kept, no power of -1 left
    for q in 2..=13 {
        for p in [1, q - 1, q + 1, 3 * q - 1] {
            let src = format!("(-1)^({p}/{q})");
            let r = ev(&mut e, &format!("exp_to_trig({src})"));
            assert!(zero_at_points(&mut e, &r, &src, &["0"]), "{src}: {r}");
            assert_eq!(ev(&mut e, &format!("length(cases([{r}], (-1)^_, inf))")), "0", "{src}: {r}");
        }
    }
}

#[test]
fn trig_to_exp_keeps_tan_one_fraction_and_powers_whole() {
    let mut e = CoreEngine::new();
    is(&mut e, "trig_to_exp(tan(x))", "im*(-exp(im*x) + exp(-im*x))/(exp(-im*x) + exp(im*x))");
    is(&mut e, "trig_to_exp(sin(5*asin(x)))", "(im/2)*(-(im*x + (1 - x^2)^(1/2))^5 + (im*x + (1 - x^2)^(1/2))^-5)");
    let start = std::time::Instant::now();
    for k in 1..=6 {
        if start.elapsed() > FAMILY_BUDGET && k > 3 {
            break;
        }
        for f in ["tan", "cot"] {
            let r = ev(&mut e, &format!("trig_to_exp({f}({k}*x))"));
            assert!(zero_at_points(&mut e, &r, &format!("{f}({k}*x)"), &["3/10", "-7/5"]), "{f}({k} x): {r}");
            assert_eq!(ev(&mut e, &format!("head({r}) === mul")), "true", "{f}({k} x): {r}");
        }
        let src = format!("sin({k}*x) + cos(x)^2 + tan(x)");
        let r = ev(&mut e, &format!("trig_to_exp({src})"));
        assert!(zero_at_points(&mut e, &r, &src, &["3/10", "-7/5"]), "{src}: {r}");
        assert_eq!(ev(&mut e, &format!("free_of({r}, sin) and free_of({r}, cos) and free_of({r}, tan)")), "true", "{src}: {r}");
        let src = format!("sin({}*asin(x))", k + 1);
        let r = ev(&mut e, &format!("trig_to_exp({src})"));
        assert!(zero_at_points(&mut e, &r, &src, &["3/10", "-2/5"]), "{src}: {r}");
        assert_eq!(ev(&mut e, &format!("free_of({r}, asin) and head({r}) === mul")), "true", "{src}: {r}");
    }
}

#[test]
fn cancel_and_together_written_forms() {
    let mut e = CoreEngine::new();
    is(&mut e, "cancel((3*x^3 + 21*x^2 - 7*x + 55 - (257/2 + (7/2)*x))/(2*x^2 - 7))", "(3/2)*(7 + x)");
    is(&mut e, "together((x - 1)/(x^2 - 1) + (x - 2)/(x^2 - 4))", "(3 + 2*x)/((1 + x)*(2 + x))");
    is(&mut e, "together(1/x + 1/(1 + x), modulus: 2)", "1/(x*(1 + x))");
    let start = std::time::Instant::now();
    'outer: for a in 1..=6 {
        for d in 2..=5 {
            if start.elapsed() > FAMILY_BUDGET && a > 2 {
                break 'outer;
            }
            // the content of the numerator is a, written outside with the denominator d (when d divides a the
            // evaluator has divided already)
            if a % d != 0 {
                let src = format!("({a}*{d}*x^2 + {}*x + {})/{d}", 2 * a, 3 * a);
                is(&mut e, &format!("cancel({src})"), &format!("({a}/{d})*({d}*x^2 + 2*x + 3)"));
            }
            // a term whose written fraction cancels keeps its reduced denominator as a factor
            let src = format!("(x - {a})/(x^2 - {}) + 1/(x + {})", a * a, a + d);
            let r = ev(&mut e, &format!("together({src})"));
            assert!(same(&mut e, &r, &src), "{src}: {r}");
            assert_eq!(ev(&mut e, &format!("head(denominator({r})) === mul")), "true", "{src}: {r}");
        }
    }
}

#[test]
fn factor_terms_keeps_the_factor_free_of_the_variable_whole() {
    let mut e = CoreEngine::new();
    is(&mut e, "factor_terms(5*im*x^2 + 20*x*im + 10)", "5im*(-2im + 4*x + x^2)");
    is(
        &mut e,
        "factor_terms(-6*y - 6*a*y + 2*x^2*y + 2*a*x^2*y + 4*a*y^2 + 4*a^2*y^2, x)",
        "2*(y + a*y)*(-3 + 2*a*y + x^2)",
    );
    let start = std::time::Instant::now();
    for k in 1..=4 {
        if start.elapsed() > FAMILY_BUDGET && k > 2 {
            break;
        }
        for g in ["5im", "-3im", "2 + 2im", "4*im"] {
            let src = format!("expand(({g})*(x^2 + {k}*x + {}*im))", k + 1);
            is(&mut e, &format!("factor_terms({src})"), &format!("({g})*(x^2 + {k}*x + {}*im)", k + 1));
        }
        let src = format!("expand(2*(y + {k}*a*y)*(x^2 - 3 + a*y))");
        let r = ev(&mut e, &format!("factor_terms({src}, x)"));
        assert!(same(&mut e, &r, &src), "{src}: {r}");
        // three factors: the content, the factor free of x as one sum, the rest
        assert_eq!(ev(&mut e, &format!("length({r}) == 3 and ({r})[1] == 2 and head(({r})[2]) === add")), "true", "{src}: {r}");
    }
}

#[test]
fn apart_in_one_kernel() {
    let mut e = CoreEngine::new();
    is(&mut e, "apart_square_free((exp(x)^2 - 2)/(exp(4*x) - 2*exp(2*x) + 1))", "-1/(-1 + exp(2*x))^2 + (-1 + exp(2*x))^-1");
    is(&mut e, "apart(3*sin(x)/((sin(x) + 1)*(sin(x) + 4)))", "-1/(1 + sin(x)) + 4/(4 + sin(x))");
    let start = std::time::Instant::now();
    for k in 1..=5 {
        if start.elapsed() > FAMILY_BUDGET && k > 2 {
            break;
        }
        for src in [
            format!("(exp(x) + {k})/((exp(2*x) - {})*(exp(x) + {}))", (k + 1) * (k + 1), k + 2),
            format!("1/((sin(x) - {})*(sin(x) + {k}))", k + 1),
            format!("exp(-x)/(exp(x) + {k})"),
        ] {
            let r = ev(&mut e, &format!("apart({src})"));
            assert!(zero_at_points(&mut e, &r, &src, &["3/10", "-7/5"]), "{src}: {r}");
            assert_eq!(ev(&mut e, &format!("head({r}) === add and free_of({r}, apart)")), "true", "{src}: {r}");
            let r = ev(&mut e, &format!("apart_square_free({src})"));
            assert!(zero_at_points(&mut e, &r, &src, &["3/10", "-7/5"]), "{src}: {r}");
            assert_eq!(ev(&mut e, &format!("free_of({r}, apart_square_free)")), "true", "{src}: {r}");
        }
    }
}

#[test]
fn to_radicals_distributes_rational_factors() {
    let mut e = CoreEngine::new();
    is(
        &mut e,
        "to_radicals(root(&(&1^3 + &1 + 11), 1))",
        "(-1/3)*2^(1/3)*3^(2/3)*((-99 + 9813^(1/2))^-1)^(1/3) + (1/6)*2^(2/3)*3^(1/3)*(-99 + 9813^(1/2))^(1/3)",
    );
    let start = std::time::Instant::now();
    'outer: for k in 1..=4 {
        for m in [3, 7, 11] {
            if start.elapsed() > FAMILY_BUDGET && k > 1 {
                break 'outer;
            }
            let rt = format!("root(&(&1^3 + {k}*&1 + {m}), 1)");
            let r = ev(&mut e, &format!("to_radicals({rt})"));
            assert_eq!(ev(&mut e, &format!("abs(n(({r}) - ({rt}), 30)) < 10^-20")), "true", "{rt}: {r}");
            // no term is a rational number times a sum
            let terms = format!("if head({r}) === add then apply(list, {r}) else [{r}]");
            let ok = ev(&mut e, &format!("every({terms}, u => not (head(u) === mul and length(u) == 2 and is_rational(u[1]) and head(u[2]) === add))"));
            assert_eq!(ok, "true", "{rt}: {r}");
        }
    }
}

// `simplify`: the logarithms of positive rationals over their primes in the zero test (the power series of log2
// and log10 against coefficients written with log(4), log(8)), and the arguments of max and min that another
// argument passes by a positive number dropped.
#[test]
fn simplify_log_primes_zero_and_dominated_max_min() {
    let mut e = CoreEngine::new();
    for (k, n) in [(2, 2), (3, 2), (4, 2), (5, 2), (2, 10), (3, 10), (2, 3), (4, 6)] {
        let pk = (n as i64).pow(k);
        is(&mut e, &format!("simplify(expand(x^{k}/log({pk}) - x^{k}/({k}*log({n}))))"), "0");
        is(&mut e, &format!("simplify((1 + x)^2*(1/log({pk}) - 1/({k}*log({n}))))"), "0");
    }
    is(&mut e, "simplify(1/log(4/9) - 1/(2*log(2) - 2*log(3)))", "0");
    is(&mut e, "simplify(max(1 - x, x, 1 + x))", "max(1 + x, 1 - x)");
    is(&mut e, "simplify(min(1 - x, x, 1 + x))", "min(x, 1 - x)");
    is(&mut e, "simplify(max(x, x + 1/2))", "1/2 + x");
    is(&mut e, "simplify(min(a, b))", "min(a, b)");
    is(&mut e, "simplify(max(x, y, 2))", "max(2, x, y)");
}
