//! Round 31: factoring in the kernel exp(w) for exp(c w), polynomial_mod in several variables,
//! factor_terms modulo an integer.
use mc_api::{Engine, StopFlag};
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
fn exp_multiples_are_powers_of_the_kernel() {
    check(&[
        ("factor_list(exp(2*x) - 2*exp(x) + 1)", "[[1, 1], [-1 + exp(x), 2]]"),
        ("factor_square_free_list(exp(3*x) - 3*exp(2*x) + 3*exp(x) - 1)", "[[1, 1], [-1 + exp(x), 3]]"),
        ("factor(exp(2*x) - 1)", "(-1 + exp(x))*(1 + exp(x))"),
        // the writer puts the canonical power back
        ("expand((exp(x) + 1)^2)", "1 + 2*exp(x) + exp(2*x)"),
        ("expand(exp(2*x)*exp(3*x))", "exp(5*x)"),
    ]);
}

#[test]
fn polynomial_mod_several_variables() {
    check(&[
        ("polynomial_mod(x^2 + a^2, a*x - 1)", "a^2 + x^2"),
        ("polynomial_mod(x^2*y, x*y - 1)", "x"),
        ("polynomial_mod(x^5, x^2 + 1)", "x"),
        ("polynomial_mod(x^3 + y^2, [x^2 - y])", "x^3 + x^4"),
        ("polynomial_mod(x^2, x + a)", "a^2"),
    ]);
}

#[test]
fn factor_terms_modulus() {
    check(&[
        ("factor_terms(5*x^2 + 2, modulus: 3)", "2*(1 + x^2)"),
        ("factor_terms(3*x + 10, modulus: 7)", "3*(1 + x)"),
        ("factor_terms(4*x + 6)", "2*(3 + 2*x)"),
    ]);
}
