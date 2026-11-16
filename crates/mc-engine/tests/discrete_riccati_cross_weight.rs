//! `discrete_riccati_solve([a, b], [q, r, p])` with a cross weight p: the solution is checked by the residual of the
//! equation with the cross term and by the stability of the closed loop a - b k, k = (r + b^T x b)^-1 (b^T x a +
//! p^T) (spectral radius below 1). Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

#[test]
fn cross_weighted_equation() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    holds(&mut e, "a = [[0, 1, 0], [0, -0.01, 0.3], [0, -0.003, -10]]; b = [[0, 0], [0, -1], [0.1, 0]]; q = identity_matrix(3); r = identity_matrix(2); p = [[0.1, 0], [-0.2, 0], [0.15, 0.2]]; true");
    holds(&mut e, "x = discrete_riccati_solve([a, b], [q, r, p]); is_matrix(x)");
    holds(&mut e, "norm(dot(transpose(a), x, a) - x - dot(dot(transpose(a), x, b) + p, inverse(r + dot(transpose(b), x, b)), dot(transpose(b), x, a) + transpose(p)) + q) < 10^-9");
    holds(&mut e, "k = dot(inverse(r + dot(transpose(b), x, b)), dot(transpose(b), x, a) + transpose(p)); max(map(abs, eigenvalues(a - dot(b, k)))) < 1");
    // symmetric, and p = 0 gives the equation without the cross term
    holds(&mut e, "norm(x - transpose(x)) < 10^-12");
    holds(&mut e, "norm(discrete_riccati_solve([a, b], [q, r, 0*p]) - discrete_riccati_solve([a, b], [q, r])) < 10^-9");
    assert!(t0.elapsed() < Duration::from_secs(30), "took {:?}", t0.elapsed());
}
