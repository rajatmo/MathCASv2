//! Answers with free parameters and step functions. A property of a function with symbolic parameters is
//! `true` or `false` only when it holds for every value of them (otherwise a condition on them, or no answer); a
//! condition solved over the integers or rationals with a free parameter keeps the membership of each value as a
//! condition; step functions on an unbounded line give the parametrised answer of a periodic condition; circular
//! and hyperbolic functions shifted by quarter periods simplify. Each answer is checked by another route: the
//! property at particular parameter values, the condition at particular values against the solution of the
//! instance, the bounded algorithm on a large interval, a filter over integers, or numeric values.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn analytic_with_parameters_is_conditional() {
    let mut e = CoreEngine::new();
    // 1F1(a; b; z) is entire in z unless b is a nonpositive integer
    assert_eq!(
        ev(&mut e, "function_analytic(hypergeometric_1f1(a, b, z), z)"),
        "conditional_expression(true, element(a | b, reals) and (b > 0 or not_element(b, integers)))"
    );
    assert_eq!(ev(&mut e, "[function_analytic(hypergeometric_1f1(a, 2, z), z), function_analytic(hypergeometric_1f1(a, -1, z), z)]"), "[true, false]");
    // variations: Legendre P is a polynomial for integer degree, with a cut at -1 otherwise
    assert_eq!(
        ev(&mut e, "function_analytic(legendre_p(n, z), z)"),
        "conditional_expression(true, element(n, reals) and (element(-n, integers) and -n <= 0 or element(1 + n, integers) and 1 + n <= 0))"
    );
    assert_eq!(ev(&mut e, "[function_analytic(legendre_p(3, z), z), function_analytic(legendre_p(1/2, z), z)]"), "[true, false]");
    // the incomplete gamma function is entire for a positive integer order: (2 + 2 z + z^2) e^(-z) for order 3
    assert_eq!(ev(&mut e, "function_analytic(gamma(a, z), z)"), "conditional_expression(true, element(a, integers) and a > 0)");
    assert_eq!(ev(&mut e, "[function_analytic(gamma(3, z), z), function_analytic(gamma(1/2, z), z)]"), "[true, false]");
    assert_eq!(ev(&mut e, "function_expand(gamma(3, z))"), "(2 + 2*z + z^2)*exp(-z)");
    // the polylogarithm: a cut or a pole on the line for every order, rational for the nonpositive integers
    assert_eq!(ev(&mut e, "function_analytic(polylog(a, z), z)"), "false");
    assert_eq!(ev(&mut e, "function_meromorphic(polylog(a, z), z)"), "conditional_expression(true, element(a, integers) and a <= 0)");
}

#[test]
fn generic_domains_decide_nothing() {
    let mut e = CoreEngine::new();
    // x^a has the domain x > 0 only for generic a: neither false nor indeterminate
    assert_eq!(ev(&mut e, "function_analytic(z^a, z)"), "function_analytic(z^a, z)");
    assert_eq!(ev(&mut e, "function_sign(z^a, z)"), "function_sign(z^a, z)");
    // variations: the particular exponents are decided (z^2 analytic, z^0 = 1 positive, sqrt not defined for z < 0)
    assert_eq!(ev(&mut e, "[function_analytic(z^2, z), function_analytic(z^(1/2), z)]"), "[true, false]");
    assert_eq!(ev(&mut e, "[function_sign(z^0, z), function_sign(z^(1/2), z)]"), "[1, indeterminate]");
    // a domain that leaves out points for every value of the parameter still decides
    assert_eq!(ev(&mut e, "[function_sign(log(a*z), z), function_monotonicity(sqrt(z - a), z)]"), "[indeterminate, indeterminate]");
}

#[test]
fn discrete_domains_with_a_free_parameter() {
    let mut e = CoreEngine::new();
    let r = "element(-(-y)^(1/2), rationals) and x == -(-y)^(1/2) or element((-y)^(1/2), rationals) and x == (-y)^(1/2)";
    assert_eq!(ev(&mut e, "reduce(x^2 + y == 0, x, rationals)"), r);
    // the condition at y = -4 and y = -2 is the answer of the instance
    assert_eq!(ev(&mut e, "rewrite(reduce(x^2 + y == 0, x, rationals), y -> -4)"), "x == -2 or x == 2");
    assert_eq!(ev(&mut e, "reduce(x^2 - 4 == 0, x, rationals)"), "x == -2 or x == 2");
    assert_eq!(ev(&mut e, "rewrite(reduce(x^2 + y == 0, x, rationals), y -> -2)"), "false");
    assert_eq!(ev(&mut e, "reduce(x^2 - 2 == 0, x, rationals)"), "false");
    // variations over the integers: y x == 1 and 2 x == y
    assert_eq!(ev(&mut e, "reduce(y*x == 1, x, integers)"), "element(y^-1, integers) and x == y^-1");
    assert_eq!(ev(&mut e, "[rewrite(reduce(y*x == 1, x, integers), y -> 1/3), reduce(x/3 == 1, x, integers)]"), "[x == 3, x == 3]");
    assert_eq!(ev(&mut e, "[rewrite(reduce(2*x == y, x, integers), y -> 7), reduce(2*x == 7, x, integers)]"), "[false, false]");
    assert_eq!(
        ev(&mut e, "solve(x^2 + y == 0, x, rationals)"),
        "[[x -> conditional_expression(-(-y)^(1/2), element(-(-y)^(1/2), rationals))], [x -> conditional_expression((-y)^(1/2), element((-y)^(1/2), rationals))]]"
    );
}

#[test]
fn step_functions_on_the_whole_line() {
    let mut e = CoreEngine::new();
    // bounded by the errors of the steps: the same as the bounded algorithm on a large interval
    for (c, want) in [
        ("floor(x) + floor(2*x - 1) == 0", "1/2 <= x and x < 1"),
        ("ceiling(x) + ceiling(2*x + 1) == 0", "-1 < x and x <= -1/2"),
        ("round(x) + round(2*x + 1) == 0", "-1/2 <= x and x <= -1/4"),
        ("floor(x/2) == floor(x/3)", "-4 <= x and x < -3 or -2 <= x and x < 2 or 3 <= x and x < 4"),
    ] {
        assert_eq!(ev(&mut e, &format!("reduce({c}, x, reals)")), want, "{c}");
        assert_eq!(ev(&mut e, &format!("reduce({c} and -20 < x and x < 20, x, reals)")), want, "{c} bounded");
    }
    // a tail where the condition always holds
    assert_eq!(ev(&mut e, "reduce(floor(x) > 0, x, reals)"), "1 <= x");
    // periodic: the residues of the Chinese remainder theorem, a parity, the integers
    assert_eq!(ev(&mut e, "reduce([mod(x, 3) == 1, mod(x, 5) == 2], x, integers)"), "element(c(1), integers) and x == 7 + 15*c(1)");
    assert_eq!(ev(&mut e, "chinese_remainder([1, 2], [3, 5])"), "7");
    assert_eq!(ev(&mut e, "reduce(mod(x, 2) == 1, x, reals)"), "element(c(1), integers) and x == 1 + 2*c(1)");
    assert_eq!(ev(&mut e, "reduce(mod(x, 2) == 1 and -5 < x and x < 5, x, reals)"), "x == -3 or x == -1 or x == 1 or x == 3");
    assert_eq!(ev(&mut e, "reduce(floor(x) == x, x, reals)"), "element(c(1), integers) and x == c(1)");
    assert_eq!(
        ev(&mut e, "reduce(mod(x, 2) == mod(x, 3), x, integers)"),
        "element(c(1), integers) and (x == 6*c(1) or x == 1 + 6*c(1))"
    );
    assert_eq!(ev(&mut e, "select(range(-6, 12), k => mod(k, 2) == mod(k, 3))"), "[-6, -5, 0, 1, 6, 7, 12]");
    // round takes halves to the even neighbour: it moves with its argument by even integers only
    assert_eq!(ev(&mut e, "reduce(round(x) == x, x, reals)"), "element(c(1), integers) and (x == 2*c(1) or x == 1 + 2*c(1))");
}

#[test]
fn quarter_period_shifts() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "simplify(cos(x - pi/2))"), "sin(x)");
    // variations, each against numeric values
    assert_eq!(
        ev(&mut e, "[simplify(tan(x + pi/2)) == -cot(x), simplify(sin(x + 3*pi/2)) == -cos(x), simplify(sinh(x + im*pi/2)) == im*cosh(x), simplify(sech(x - im*pi/2)) == im*csch(x)]"),
        "[true, true, true, true]"
    );
    for (shifted, reduced) in [("tan(0.4 + pi/2)", "-cot(0.4)"), ("sin(0.9 + 3*pi/2)", "-cos(0.9)"), ("sinh(0.7 + im*pi/2)", "im*cosh(0.7)"), ("sech(0.3 - im*pi/2)", "im*csch(0.3)")] {
        let d = ev(&mut e, &format!("abs(n({shifted} - {reduced})) < 10^-12"));
        assert_eq!(d, "true", "{shifted}");
    }
    // Mathieu's equation with a sine: the basis at t = x/2 - pi/4, its residual 0
    assert_eq!(
        ev(&mut e, "d_solve(y''(x) + (2 + 3*sin(x))*y(x) == 0, y(x), x)"),
        "[[y(x) -> c(1)*mathieu_c(8, -6, (-1/4)*pi + (1/2)*x) + c(2)*mathieu_s(8, -6, (-1/4)*pi + (1/2)*x)]]"
    );
    for f in ["mathieu_c(8, -6, x/2 - pi/4)", "mathieu_s(8, -6, x/2 - pi/4)"] {
        let r = ev(&mut e, &format!("f = {f}; abs(n(rewrite(d(f, x, x) + (2 + 3*sin(x))*f, x -> 0.37))) < 10^-10"));
        assert_eq!(r, "true", "{f}");
    }
}
