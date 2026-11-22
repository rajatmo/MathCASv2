//! trig_reduce of sums with tan, cot, sec, csc (and the hyperbolic ones) over one denominator, complex_expand of
//! equations, power_expand in given variables, the sign of a symbolic modulus, real quotients that cancel, and
//! function_expand of sum_k k^m x^k. Each answer is checked against an independent computation: values at sample
//! points to 30 digits, or the identity itself.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() })
}

fn check(cases: &[(&str, &str)]) {
    let mut e = engine();
    let stop = StopFlag::new();
    for (src, want) in cases {
        let got = e.eval(src, &stop).map(|v| v.printed).unwrap_or_else(|err| panic!("{src}: {err}"));
        assert_eq!(got, *want, "{src}");
    }
}

/// `|a - b| < 10^-25` at 30 digits after substituting the rules.
fn same_value(a: &str, b: &str, at: &str) {
    let mut e = engine();
    let stop = StopFlag::new();
    let src = format!("abs(n(rewrite(({a}) - ({b}), {at}), 30)) < 10^(-25)");
    let got = e.eval(&src, &stop).map(|v| v.printed).unwrap_or_else(|err| panic!("{src}: {err}"));
    assert_eq!(got, "true", "{src}");
}

#[test]
fn trig_reduce_of_tangent_sums() {
    check(&[
        ("trig_reduce(tan(x) + tan(y))", "sec(x)*sec(y)*sin(x + y)"),
        ("trig_reduce(coth(x) + coth(y))", "csch(x)*csch(y)*sinh(x + y)"),
        ("trig_reduce([tan(x) + cot(y), tanh(x) - coth(y)])", "[cos(x - y)*csc(y)*sec(x), -1*cosh(x - y)*csch(y)*sech(x)]"),
        ("trig_reduce(sin(x)^2 + tan(x)^2)", "(1/8)*(5*sec(x)^2 + -4*cos(2*x)*sec(x)^2 + -1*cos(4*x)*sec(x)^2)"),
        ("trig_reduce(1 + tan(x)^2)", "sec(x)^2"),
        // a single tangent and products stay as they were
        ("trig_reduce(tan(x))", "tan(x)"),
        ("trig_reduce(sin(x)*cos(x))", "(1/2)*sin(2*x)"),
    ]);
    let at = "[x -> 3/10, y -> 7/10]";
    for e in ["tan(x) + tan(y)", "coth(x) + coth(y)", "tan(x) + cot(y)", "tanh(x) - coth(y)", "sin(x)^2 + tan(x)^2", "sec(x) + csc(y)", "sech(x)^2 + tanh(y)"] {
        same_value(&format!("trig_reduce({e})"), e, at);
    }
}

#[test]
fn complex_expand_of_equations() {
    check(&[
        ("complex_expand(re(z) == (z + conjugate(z))/2, z)", "true"),
        ("complex_expand(exp(im*z) == cos(z) + im*sin(z))", "true"),
        // a relation that is not an identity stays one between the expanded sides
        ("complex_expand(x + im*y == 2, [])", "x + 1im*y == 2"),
    ]);
}

#[test]
fn power_expand_in_given_variables() {
    check(&[
        ("power_expand(sqrt(a*b) + sqrt(c*d), [a, b])", "a^(1/2)*b^(1/2) + (c*d)^(1/2)"),
        ("power_expand(log(a*b) + log(c*d), a)", "log(a) + log(b) + log(c*d)"),
    ]);
}

#[test]
fn modulus_sign_and_real_quotients() {
    check(&[
        ("full_simplify(mod(a, -b) + mod(-a, b) == 0)", "true"),
        ("full_simplify(quotient(a^2 - b^2, a - b), element([a, b], reals))", "floor(a + b)"),
    ]);
    // mod(a, -b) = -mod(-a, b) at numbers of both signs
    for (a, b) in [("7.3", "2.1"), ("-7.3", "2.1"), ("7.3", "-2.1"), ("5", "3"), ("-5", "3")] {
        same_value(&format!("mod({a}, -({b}))"), &format!("-mod(-({a}), {b})"), "[]");
    }
    // quotient(a^2 - b^2, a - b) = floor(a + b) for a != b
    for (a, b) in [("5/2", "7/10"), ("-3/2", "1/3"), ("2", "-2/7")] {
        same_value(&format!("quotient(({a})^2 - ({b})^2, ({a}) - ({b}))"), &format!("floor(({a}) + ({b}))"), "[]");
    }
}

#[test]
fn function_expand_of_weighted_geometric_sums() {
    // sum_(k = 1..n) k x^k at x = 1/2 is 2 - (n + 2)/2^n; checked by the defining sum for n = 1..12
    let mut e = engine();
    let stop = StopFlag::new();
    for m in 0..=3 {
        let src = format!(
            "{{ f = function_expand(harmonic_number(n, -{m}, 1/2)); table(rewrite(f, n -> j) - sum(k^{m}/2^k, [k, 1, j]), j, 12) }}"
        );
        let got = e.eval(&src, &stop).map(|v| v.printed).unwrap_or_else(|err| panic!("{src}: {err}"));
        assert_eq!(got, "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]", "{src}");
    }
}
