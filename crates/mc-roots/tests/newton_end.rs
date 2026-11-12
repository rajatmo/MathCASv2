//! Newton's method stops at a root once a step no longer moves x: no backtracking through dozens of halvings at
//! the noise level of f. Observed through the compiled-code cache: a solve that evaluates its residual only a few
//! times never reaches the evaluation count at which the default policy compiles (a process of its own, since the
//! cache's counters are process-wide).
mod common;
use common::Sys;

#[test]
fn a_converged_newton_solve_takes_few_evaluations() {
    let mut s = Sys::new();
    let before = mc_numfn::cache_stats().compiles;
    for (src, want) in [
        ("find_root(x^2 - 2, [x, 1])", 2f64.sqrt()),
        ("find_root(x^3 - 3, [x, 1.0e3])", 3f64.cbrt()),
        ("find_root(cos(x) == x, [x, 1])", 0.739_085_133_215_160_7),
        ("find_root(exp(x) - 3, [x, 0.5])", 3f64.ln()),
        ("find_root(1.0e20*(x - 2)^3 + 1.0e20*(x - 2), [x, 3])", 2.0),
    ] {
        let x = s.reals(src);
        assert!((x[0] - want).abs() <= 4.0 * f64::EPSILON * want.abs(), "{src}: {x:?}");
    }
    assert_eq!(mc_numfn::cache_stats().compiles, before, "a solve evaluated its residual more than 32 times");
}
