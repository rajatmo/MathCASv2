//! The package through the evaluator: source text in, printed values out.
mod common;
use common::Engine;

fn check(cases: &[(&str, &str)]) {
    let mut e = Engine::new();
    let mut bad = vec![];
    for (src, want) in cases {
        let got = e.eval(src);
        if got != *want {
            bad.push(format!("{src}\n    got  {got}\n    want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn exact_values() {
    check(&[
        ("sin(0)", "0"),
        ("sin(pi)", "0"),
        ("cos(pi)", "-1"),
        ("cos(0)", "1"),
        ("sin(pi/6)", "1/2"),
        ("cos(pi/3)", "1/2"),
        ("sin(pi/4)", "(1/2)*2^(1/2)"),
        ("cos(5*pi/6)", "(-1/2)*3^(1/2)"),
        ("sin(-pi/2)", "-1"),
        ("sin(7*pi)", "0"),
        ("cos(-x)", "cos(x)"),
        ("sin(-x)", "-sin(x)"),
        ("tan(pi/4)", "1"),
        ("tan(pi/3)", "3^(1/2)"),
        ("tan(pi/2)", "tan((1/2)*pi)"),
        ("log(1)", "0"),
        ("log(e)", "1"),
        ("log(e^3)", "3"),
        ("exp(0)", "1"),
        ("exp(1)", "e"),
        ("exp(log(x))", "x"),
        ("log(exp(2))", "2"),
        ("exp(im*pi)", "-1"),
        ("sqrt(4)", "2"),
        ("sqrt(8)", "2*2^(1/2)"),
        ("sqrt(x)", "x^(1/2)"),
        ("asin(1/2)", "(1/6)*pi"),
        ("asin(-1)", "(-1/2)*pi"),
        ("acos(0)", "(1/2)*pi"),
        ("acos(-1)", "pi"),
        ("acos(1)", "0"),
        ("atan(1)", "(1/4)*pi"),
        ("atan(sqrt(3))", "(1/3)*pi"),
        ("atan(-1/sqrt(3))", "(-1/6)*pi"),
        ("sin(asin(x))", "x"),
        ("sinh(0)", "0"),
        ("cosh(0)", "1"),
        ("tanh(0)", "0"),
        ("sin(1)", "sin(1)"),
        ("log(2)", "log(2)"),
        ("sin([0, pi/2, x])", "[0, 1, sin(x)]"),
    ]);
}

#[test]
fn machine_values() {
    check(&[
        ("sin(1.0)", "0.8414709848078965"),
        ("exp(1.0)", "2.718281828459045"),
        ("log(2.0)", "0.6931471805599453"),
        ("sqrt(2.0)", "1.4142135623730951"),
        ("atan(1.0)", "0.7853981633974483"),
        ("n(pi)", "3.141592653589793"),
        ("n(e)", "2.718281828459045"),
        ("n(1/3)", "0.3333333333333333"),
        ("n(sin(1) + x)", "0.8414709848078965 + x"),
        ("n([pi, 1/2, y])", "[3.141592653589793, 0.5, y]"),
        ("n(sqrt(2))", "1.4142135623730951"),
    ]);
}

#[test]
fn balls() {
    check(&[
        ("n(pi, 30)", "3.1415926535897932384626433832795029"),
        ("n(e, 20)", "2.71828182845904523536029"),
        ("n(sqrt(2), 20)", "1.414213562373095048801689"),
        ("n(euler_gamma, 20)", "0.577215664901532860606512"),
        ("n(catalan, 20)", "0.91596559417721901505460"),
        ("sin(1.0~30)", "0.841470984807896506652502321630"),
        ("n(exp(pi*sqrt(163)), 40)", "262537412640768743.999999999999250072597198"),
    ]);
}

#[test]
fn refined_comparisons() {
    check(&[
        (
            "[exp(pi*sqrt(163)) < 640320^3 + 744, pi^4 + pi^5 < exp(6), sqrt(1000001) - 1000 < 1/2000]",
            "[true, true, true]",
        ),
        ("pi < 22/7", "true"),
        ("pi > 3", "true"),
        ("e == 2", "false"),
        ("pi != 3", "true"),
        ("sin(1)^2 + cos(1)^2 == 1", "cos(1)^2 + sin(1)^2 == 1"),
        ("x < pi", "x < pi"),
        ("is_positive(pi)", "true"),
        ("is_negative(log(1/2))", "true"),
        ("is_real(pi + e)", "true"),
        ("is_positive(x)", "is_positive(x)"),
    ]);
}
