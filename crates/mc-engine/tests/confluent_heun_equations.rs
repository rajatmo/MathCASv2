//! `d_solve` of the biconfluent, double confluent and triconfluent Heun equations with numeric parameters: every
//! returned solution is substituted into the equation and the residual is checked numerically at two points.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Solve `y'' + p y' + q y == 0`, then check that both basis functions satisfy it at two points (30 digits, residual
/// below 1e-15).
fn check(p: &str, q: &str, points: [&str; 2]) {
    let mut e = CoreEngine::new();
    let sol = ev(&mut e, &format!("sol = d_solve(y''(z) + ({p})*y'(z) + ({q})*y(z) == 0, y(z), z); sol"));
    assert!(!sol.starts_with("d_solve"), "{p} | {q}: left unevaluated: {sol}");
    for k in 1..=2 {
        for pt in points {
            let src = format!(
                "f = rewrite(y(z), first(sol)); g = rewrite(f, [c({k}) -> 1, c({}) -> 0]); \
                 r = d(g, [z, 2]) + ({p})*d(g, z) + ({q})*g; abs(n(rewrite(r, z -> {pt}), 30)) < 10^-15",
                3 - k
            );
            assert_eq!(ev(&mut e, &src), "true", "{p} | {q}: basis {k} at {pt}: {sol}");
        }
    }
}

#[test]
fn biconfluent() {
    check("1/10/z + 1/10 + 3/2*z", "(9/10*z - 1/31)/z", ["1/3", "7/5"]);
    check("2/7/z - 1/3 + z", "(z/5 + 2)/z", ["1/2", "6/5"]);
    check("5/3/z + 2 - z/4", "(3*z - 1/2)/z", ["2/3", "11/10"]);
}

#[test]
fn double_confluent() {
    check("1/10/z^2 + 1/10/z + 3/2", "(9/10*z - 2)/z^2", ["1/2", "5/4"]);
    check("1/3/z^2 - 1/z + 1", "(z/2 + 1)/z^2", ["3/4", "3/2"]);
    check("2/z^2 + 1/5/z - 1/2", "(2*z - 3)/z^2", ["1/2", "9/8"]);
}

#[test]
fn triconfluent() {
    check("1/10 + 4/7*z + 3/2*z^2", "8 - 9/10*z", ["1/2", "6/5"]);
    check("-1/2 + z - z^2/3", "z + 1/4", ["1/3", "1"]);
    check("2 + z^2", "3*z/2 - 1", ["2/5", "4/3"]);
}

#[test]
fn integer_exponent_is_not_given_a_second_solution() {
    // g = 1: the second solution would repeat the first, so the call is left unevaluated
    let mut e = CoreEngine::new();
    let s = ev(&mut e, "d_solve(y''(z) + (1/z + 1/10 + 3/2*z)*y'(z) + (9/10*z - 1/31)/z*y(z) == 0, y(z), z)");
    assert!(s.starts_with("d_solve("), "{s}");
}
