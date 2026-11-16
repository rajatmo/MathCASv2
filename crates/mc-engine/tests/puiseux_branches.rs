//! Branches of algebraic equations through singular points and to infinity in `asymptotic_solve` (Newton's
//! polygon). Each branch is checked against a numeric root of the equation at a small x.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Every branch of `asymptotic_solve(f == 0, ..., [x, 0, k])` lies within `tol` of a root of f(x0, y) at x = x0.
fn near_roots(call: &str, f: &str, x0: &str, tol: &str) {
    let src = format!(
        "bs = {call}; rs = map(r => y /. r, n_solve(rewrite({f}, x -> {x0}) == 0, y)); \
         length(bs) > 0 and every(bs, b => some(rs, r => abs(n(rewrite(y /. b[1], x -> {x0})) - r) < {tol}))"
    );
    assert_eq!(ev(&src), "true", "{call}");
}

#[test]
fn branches_to_infinity() {
    assert_eq!(
        ev("asymptotic_solve(x*y^4 - (x + 1)*y^2 + x == 1, [y], [x, 0, 3]) === [[y -> -im - (27*im/8)*x^2 + (3*im/2)*x + (207*im/16)*x^3], [y -> im - (207*im/16)*x^3 - (3*im/2)*x + (27*im/8)*x^2], [y -> -1/x^(1/2) - x^(1/2) + 2*x^(3/2)], [y -> -2*x^(3/2) + x^(-1/2) + x^(1/2)]]"),
        "true"
    );
    assert_eq!(
        ev("asymptotic_solve(x*y^4 - (x + 1)*y^2 + x == 1, [y], [x, 0, 3], reals) === [[y -> -1/x^(1/2) - x^(1/2) + 2*x^(3/2)], [y -> -2*x^(3/2) + x^(-1/2) + x^(1/2)]]"),
        "true"
    );
    near_roots("asymptotic_solve(x*y^4 - (x + 1)*y^2 + x == 1, [y], [x, 0, 3])", "x*y^4 - (x + 1)*y^2 + x - 1", "1/1000", "10^-6");
    near_roots("asymptotic_solve(x*y^3 + y - 1 == 0, [y], [x, 0, 4])", "x*y^3 + y - 1", "1/1000", "10^-6");
}

#[test]
fn branches_through_a_singular_point() {
    assert_eq!(
        ev("take(asymptotic_solve(y^5 - x*y + x^2 == 0, [y, 0], [x, 0, 5]), 2) === [[y -> x + x^4], [y -> -x^(1/4) - (1/4)*x + (5/32)*x^(7/4) - (5/32)*x^(5/2) + (385/2048)*x^(13/4) - (1/4)*x^4 + (23205/65536)*x^(19/4)]]"),
        "true"
    );
    assert_eq!(ev("length(asymptotic_solve(y^5 - x*y + x^2 == 0, [y, 0], [x, 0, 5]))"), "5");
    assert_eq!(ev("length(asymptotic_solve(y^5 - x*y + x^2 == 0, [y, 0], [x, 0, 5], reals))"), "3");
    near_roots("asymptotic_solve(y^5 - x*y + x^2 == 0, [y, 0], [x, 0, 5])", "y^5 - x*y + x^2", "1/10000", "10^-7");
    // y^2 = x^3 + x^2 y: the cusp-like branches y ~ +-x^(3/2)
    near_roots("asymptotic_solve(y^2 - x^2*y - x^3 == 0, [y, 0], [x, 0, 4])", "y^2 - x^2*y - x^3", "1/10000", "10^-12");
    // a repeated root of a characteristic polynomial (a second polygon) is not done: no answer rather than a wrong one
    assert!(ev("asymptotic_solve((y - x)^2 - x^3 == 0, [y, 0], [x, 0, 3])").starts_with("failure(unevaluated, asymptotic_solve"));
}
