//! `solve` with a domain or conditions, and `reduce` with several unknowns or over the integers.
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
fn solve_with_domains_and_conditions() {
    check(&[
        ("solve((x^4 - 1)*(x^4 - 4) == 0, x, reals)", "[[x -> -2^(1/2)], [x -> -1], [x -> 1], [x -> 2^(1/2)]]"),
        ("solve((x^4 - 1)*(x^4 - 4) == 0, x, integers)", "[[x -> -1], [x -> 1]]"),
        ("solve(x^2 == 4 and x > 0, x)", "[[x -> 2]]"),
        ("solve(abs(x) == 2, x, reals)", "[[x -> 2], [x -> -2]]"),
        ("solve(divisor_sigma(2, n) == n^2 + 1 and (0 < n and n < 10), n, integers)", "[[n -> 2], [n -> 3], [n -> 5], [n -> 7]]"),
        ("solve(x^2 == -1, x, reals)", "[]"),
        // radical equations: the roots squaring adds are checked against the equation and dropped
        ("solve(sqrt(x + 2) == x, x)", "[[x -> 2]]"),
        ("solve(sqrt(x) == -1, x)", "[]"),
        ("solve(sqrt(a*x) == b, x)", "[[x -> a^-1*b^2]]"),
        // the facts of assuming about the unknowns
        ("assuming(x > 0, solve(x^2 == 4, x))", "[[x -> 2]]"),
        ("assuming(element(n, integers), solve(x^2 == 4, x))", "[[x -> -2], [x -> 2]]"),
    ]);
}

#[test]
fn reduce_through_solutions() {
    check(&[
        ("reduce(2*x + 3*y - 5*z == 1 and 3*x - 4*y + 7*z == 3, [x, y, z])", "x == (1/17)*(13 - z) and y == (1/17)*(-3 + 29*z)"),
        ("reduce((x^4 - 1)*(x^4 - 4) == 0, x, integers)", "x == -1 or x == 1"),
        ("reduce(x^2 < 1, [x])", "-1 < x and x < 1"),
    ]);
}
