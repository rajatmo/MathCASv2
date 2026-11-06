//! Series of functions the series kernels do not know, through their derivatives (the Taylor table of the native),
//! with the whole engine (the derivative rules and special values live in other crates).
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let stop = StopFlag::new();
    for (src, want) in cases {
        let got = e.eval(src, &stop).map(|v| v.printed).unwrap_or_else(|err| panic!("{src}: {err}"));
        assert_eq!(got, *want, "{src}");
    }
}

#[test]
fn taylor_through_derivatives() {
    check(&[
        ("series(f(x), x, 0, 3)", "f(0) + f'(0)*x + (1/2)*f''(0)*x^2 + (1/6)*f'''(0)*x^3 + O(x^4)"),
        ("series(asin(x), x, 0, 5)", "x + x^3/6 + 3*x^5/40 + O(x^6)"),
        ("series(acos(x), x, 0, 3)", "(1/2)*pi - x - x^3/6 + O(x^4)"),
        ("series(erf(x), x, 0, 3)", "2*pi^(-1/2)*x + (-2/3)*pi^(-1/2)*x^3 + O(x^4)"),
        ("series(bessel_j(0, x), x, 0, 4)", "1 - x^2/4 + x^4/64 + O(x^5)"),
        ("series(airy_ai(x), x, 0, 1)", "(1/3)*3^(1/3)/gamma(2/3) + (-1/3)*3^(2/3)/gamma(1/3)*x + O(x^2)"),
        ("series(erf(sin(x)), x, 0, 3)", "2*pi^(-1/2)*x - pi^(-1/2)*x^3 + O(x^4)"),
        ("series(f(g(x)), x, 0, 1)", "f(g(0)) + f'(g(0))*g'(0)*x + O(x^2)"),
        // a pole or a branch point of the function at the point: not by derivatives
        ("series(gamma(x), x, 0, 1)", "series(gamma(x), x, 0, 1)"),
        ("series(asin(x), x, -1, 2)", "series(asin(x), x, -1, 2)"),
    ]);
}

#[test]
fn symbolic_powers_through_logarithms() {
    check(&[
        ("series(tanh(y)^k, y, 0, 3)", "y^k + (-1/3)*k*y^k*y^2 + O(y^4)"),
        ("series(x^x, x, 0, 2)", "1 + log(x)*x + (1/2)*log(x)^2*x^2 + O(x^3)"),
    ]);
}
