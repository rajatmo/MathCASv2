//! Round 19: evaluator and library fixes found while documenting round 18, checked through
//! the whole engine.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// Every case in one session, in order: the value printed must be `want`.
fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let mut bad = vec![];
    for (src, want) in cases {
        let got = eval(&mut e, src);
        if got != *want {
            bad.push(format!("{src}\n   got  {got}\n   want {want}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn rest_parameter_clauses_with_conditions_match_numbers() {
    check(&[
        ("fn g(xs...) when length(xs) == 2 = 1", "null"),
        ("fn g(xs...) when length(xs) == 1 = 2", "null"),
        ("[g(1, 2), g(a, b), g(1), g(true, false)]", "[1, 1, 2, 1]"),
        ("g(1, 2, 3)", "failure(no_match, g, [1, 2, 3])"),
        // equally general rules keep definition order (conditions on the right side)
        ("rule f(x_) := ppp(x) when x > 0", "null"),
        ("rule f(x_) := { u^2 when { u = x - 1; u } > 0 }", "null"),
        ("[f(6), f(0)]", "[ppp(6), f(0)]"),
    ]);
}

#[test]
fn truth_values_compare_and_destructuring_assigns() {
    check(&[
        ("true == false", "false"),
        ("false != true", "true"),
        ("equal(true, true, false)", "false"),
        ("[q, r] = quotient_remainder(7, 2)", "[3, 1]"),
        ("[q, r]", "[3, 1]"),
        ("[[a1, a2], a3] = [[1, 2], 3]", "[[1, 2], 3]"),
        ("a1 + a2 + a3", "6"),
    ]);
}

#[test]
fn map_heads_and_activate_patterns() {
    check(&[
        ("map(f, [a, b, c], heads: true)", "f(list)(f(a), f(b), f(c))"),
        ("map(f, [[[[a]]]], 2, heads: true)", "f(list)(f(f(list)(f([[a]]))))"),
        ("map(f, [a, b], heads: false)", "[f(a), f(b)]"),
        ("activate(inactive(add)(2, 2, inactive(pow)(3, 2)), add)", "4 + inactive(pow)(3, 2)"),
        ("activate(inactive(add)(2, 2, inactive(pow)(3, 2)), pow)", "inactive(add)(2, 2, 9)"),
        (
            "activate(inactive(add)(inactive(cos)(pi), inactive(integrate)(x, x)), f_ when not match_q(f, integrate))",
            "-1 + inactive(integrate)(x, x)",
        ),
        ("activate(inactive(derivative)(1)(cos)(x), heads: false)", "inactive(derivative)(1)(cos)(x)"),
    ]);
}

#[test]
fn unevaluated_arguments() {
    check(&[
        ("length(unevaluated(5 + 6 + 7 + 8))", "4"),
        ("rule f(x_) := g(x)", "null"),
        ("f(unevaluated(1 + 1))", "g(2)"),
        ("h(unevaluated(1 + 2))", "h(unevaluated(1 + 2))"),
        ("hold(unevaluated(1 + 2))", "hold(unevaluated(1 + 2))"),
        ("hold(eval(unevaluated(1 + 2)))", "hold(unevaluated(1 + 2))"),
        ("length(1 + x + y)", "3"),
    ]);
}

#[test]
fn time_and_memory_limits() {
    check(&[
        ("time_constrained(pause(10), 0.3, \"timed out\")", "\"timed out\""),
        ("time_constrained(pause(10), 0.2)", "failure(limit, time)"),
        ("time_constrained(2 + 3, 5)", "5"),
        // the session goes on normally after a limit fired
        ("[time_constrained(pause(5), 0.2, 1), 2 + 3]", "[1, 5]"),
        ("memory_constrained(range(100), 100, \"too much\")", "\"too much\""),
        ("memory_constrained(range(5), 10^6)", "[1, 2, 3, 4, 5]"),
        ("memory_constrained(map(&(&1^2 + 1), range(10^5)), 1000, \"x\")", "\"x\""),
        ("1 + 1", "2"),
    ]);
}

#[test]
fn sums_cancel_and_roots_of_unity() {
    check(&[
        ("(x + 1) - (x + 1)", "0"),
        ("a = x + y; a - a", "0"),
        ("-(x + 1)", "-1 - x"),
        ("(x + 1) - (x + 2)", "-1"),
        ("1im*(-1)^(1/6)", "(-1)^(2/3)"),
        ("-1im*(-1)^(1/6)", "-(-1)^(2/3)"),
        ("(-1)^(-1/3)", "-(-1)^(2/3)"),
        ("1im*(-1)^(1/3)", "(-1)^(5/6)"),
        ("(-1)^(4/3)", "-(-1)^(1/3)"),
    ]);
}

#[test]
fn root_objects_canonical() {
    check(&[
        ("root(&(&1^2 - 3*&1 - 1), 2)", "(1/2)*(3 + 13^(1/2))"),
        ("root(&(&1^5 - 3*&1), 2)", "0"),
        ("root(&(&1^5 - 3*&1), 3)", "root(&(-3 + &1^4), 2)"),
        ("root(&(&1^5 + 2*&1 + 1), 1, 0)", "root(&(1 + 2*&1 + &1^5), 1)"),
        ("root(y => y^5 + 2*y + 1, 1) == root(x => x^5 + 2*x + 1, 1)", "true"),
        ("root(x => x^2 + 1, 1)", "-1im"),
        ("solve(x^5 + 2*x + 1 == 0, x)[1]", "[x -> root(&(1 + 2*&1 + &1^5), 1)]"),
    ]);
}

#[test]
fn special_function_values() {
    let close = |got: &str, want: f64| got.parse::<f64>().is_ok_and(|g| (g - want).abs() <= 1e-9 * (1.0 + want.abs()));
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    for (src, want) in [
        ("hypergeometric_u(-2, 1, 1.5)", -1.75),
        ("whittaker_w(1, 0.5, 1.0)", 0.6065306597126334),
        ("whittaker_w(2, 1.5, 3.0)", 2.008171441335868),
        ("kelvin_ker(2.5)", -0.06968797258904535),
        ("kelvin_ker(2, 2.5)", 0.18880430952581462),
        ("kelvin_ker(-1, 0.5)", 1.52240340653209),
        ("parabolic_cylinder_d(7., 5)", 48.454398819315514),
        ("parabolic_cylinder_d(0.51, 0.87)", 0.8481137305149469),
        ("fresnel_f(10.0)", 0.03183002141511776),
        ("log_gamma(1.0)", 0.0),
    ] {
        let got = eval(&mut e, src);
        assert!(close(&got, want), "{src}: {got} vs {want}");
    }
    check(&[
        ("hypergeometric_u(-2, b, z)", "b + -2*z + -2*b*z + b^2 + z^2"),
        ("erf(-a)", "-erf(a)"),
        // round 37: z^(-1/2), not (1/(pi z))^(1/2), which has the other sign on the negative axis
        ("struve_h(1/2, x)", "2^(1/2)*pi^(-1/2)*x^(-1/2) + -1*cos(x)*2^(1/2)*pi^(-1/2)*x^(-1/2)"),
        ("logistic_sigmoid(2*im*pi)", "1/2"),
        ("logistic_sigmoid(im*pi/2)", "complex(1/2, 1/2)"),
        ("series(logistic_sigmoid(x), x, 0, 5)", "1/2 + x/4 - x^3/48 + x^5/480 + O(x^6)"),
        ("d(hypergeometric_u(a, b, x), x)", "-1*a*hypergeometric_u(1 + a, 1 + b, x)"),
        ("parabolic_cylinder_d(n, 0)", "2^((1/2)*n)*pi^(1/2)/gamma((1/2)*(1 - n))"),
        ("partitions_p(100)", "190569292"),
        ("harmonic_number(10)", "7381/2520"),
        ("log_gamma(1.0)", "0.0"),
    ]);
}

#[test]
fn slow_numeric_sums_and_round19_quality_cases() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    for (src, tol) in [
        ("abs(n_sum(1/n^(51/50), [n, 1, inf]) - n(zeta(51/50)))", 1e-9),
        ("abs(n_sum(1/n^(11/10), [n, 1, inf]) - n(zeta(11/10)))", 1e-9),
    ] {
        let got = eval(&mut e, src);
        assert!(got.parse::<f64>().is_ok_and(|v| v < tol), "{src}: {got}");
    }
    check(&[
        ("q_factorial(3, q)", "(1 + q)*(1 + q + q^2)"),
        ("dimensions(f(f(1, 2), f(3, 4)))", "[2, 2]"),
        ("dimensions(f(g(1, 2), g(3, 4)), allowed_heads: all)", "[2, 2]"),
        ("dimensions(f(g(1, 2), g(3, 4)))", "[2]"),
        // round 20: the periodic closed form, verified by differencing (it stayed unevaluated in round 19)
        ("sum(mod(k^2, 2), k)", "floor((1/2)*k)"),
        ("free_of(a + b + c, a + d)", "true"),
        ("member_of([a + b + c], a + c)", "false"),
        ("sqrt(2) + sqrt(3) == root(&(&1^4 - 10*&1^2 + 1), 4) + 10^(-100)", "false"),
        ("minimal_polynomial(cos(pi/9), x)", "-1 + -6*x + 8*x^3"),
        ("complex_infinity + 1", "complex_infinity"),
        ("0*complex_infinity", "indeterminate"),
    ]);
}

#[test]
fn association_prepend_and_huge_fraction_powers() {
    check(&[
        ("prepend(assoc(1 -> a, 2 -> b), 2 -> c)", "assoc(2 -> c, 1 -> a)"),
        ("prepend(assoc(1 -> a), 2 -> c)", "assoc(2 -> c, 1 -> a)"),
        ("(3/2)^10", "59049/1024"),
        ("head((1 + 10^-12)^(10^12))", "pow"),
    ]);
}

#[test]
fn argument_conditions_order_and_shifted_sums() {
    check(&[
        ("rule f(1) = 2", "null"),
        ("rule f(x_) := x", "null"),
        // a condition on an argument pattern is more specific than the bare pattern
        ("rule f(x_ when x > -2) := g1(x)", "null"),
        ("[f(0), f(1), f(-5)]", "[g1(0), 2, -5]"),
        ("discrete_shift(sum(u(k), [k, i + a, i + b]), i)", "sum(u(1 + k), [k, a + i, b + i])"),
        ("discrete_shift(sum(u(i), i), i)", "sum(u(1 + i), i)"),
        ("discrete_shift(sum(u(i), [i, a, b]), i)", "sum(u(i), [i, a, b])"),
        ("discrete_shift(product(u(i), [i, m, n]), n)", "product(u(i), [i, m, 1 + n])"),
    ]);
}
