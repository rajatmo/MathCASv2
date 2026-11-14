//! Round 40: equations between products of powers of positive bases, solved over the reals by taking logarithms.
//! Each solution is substituted back and checked numerically to 30 digits.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Every solution of `sols` makes `lhs - rhs` vanish to 30 digits.
fn check(e: &mut CoreEngine, lhs: &str, rhs: &str) {
    let r = ev(e, &format!("s = solve({lhs} == {rhs}, x, reals); s"));
    assert!(r.starts_with("[[x -> "), "{lhs} == {rhs}: {r}");
    let k: usize = ev(e, "length(s)").parse().unwrap();
    for i in 1..=k {
        let v = ev(e, &format!("abs(n(rewrite({lhs} - ({rhs}), s[{i}]), 30)) < 10^-25"));
        assert_eq!(v, "true", "{lhs} == {rhs}, solution {i} of {r}");
    }
}

#[test]
fn powers_of_positive_bases() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "solve((27^(2*x - 1))^(1/x) == sqrt(9^(2*x - 1)), x, reals)"), "[[x -> 1/2], [x -> 3]]");
    assert_eq!(ev(&mut e, "n_solve_values((27^(2*x - 1))^(1/x) == sqrt(9^(2*x - 1)), x, reals)"), "[0.5, 3.0]");
    // variations: another pair of bases with a quadratic exponent, an exponential against a power of 3, a
    // constant base pi
    check(&mut e, "5*2^x", "3^(x^2)");
    check(&mut e, "exp(x)", "3^(x + 1)");
    check(&mut e, "pi^(x/2)", "(2^(x - 1))^3");
    // two solutions of the quadratic exponent equation
    assert_eq!(ev(&mut e, "length(solve(5*2^x == 3^(x^2), x, reals))"), "2");
}

#[test]
fn opposite_signs_have_no_real_solution() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "solve(2^x == -3^x, x, reals)"), "[]");
    assert_eq!(ev(&mut e, "solve(-5*exp(x^2) == 7^x, x, reals)"), "[]");
    // a base that is not positive is not taken: x^2 can be 0
    assert!(ev(&mut e, "solve((x^2)^x == 2^x, x, reals)") != "[]");
}
