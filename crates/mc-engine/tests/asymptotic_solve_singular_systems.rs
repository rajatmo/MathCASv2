//! asymptotic_solve for two polynomial equations whose Jacobian is singular at the start: Puiseux branches of the
//! resultants paired by the residual. Each branch is checked here by its residual at a small t, which must be of a
//! higher order than t^k. Time-bounded.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// every branch of `bs` (a list of rule lists) makes the equations `fs` smaller than t^k at t = t0
fn residuals_small(e: &mut CoreEngine, bs: &str, fs: &str, t0: &str, k: u32) {
    let r = ev(e, &format!("every({bs}, b => every({fs}, f => abs(n(rewrite(rewrite(f, b), t -> {t0}))) < n({t0})^{k}/100))"));
    assert_eq!(r, "true", "{}", ev(e, &format!("map(b => map(f => n(rewrite(rewrite(f, b), t -> {t0})), {fs}), {bs})")));
}

#[test]
fn branches_through_a_singular_start() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    let fs = "[y^2 - t*x + t, x^2 + t*y]";
    let v = ev(&mut e, "bb = asymptotic_solve(y^2 - t*x + t == 0 and x^2 + t*y == 0, [[x, y], [0, 0]], [t, 0, 2]); bb");
    assert!(v.starts_with("[[x -> "), "{v}");
    assert_eq!(ev(&mut e, "length(bb)"), "4");
    // x = c t^(3/4) - c^2 t^(3/2)/4 with c^4 = -1, y = -x^2/t to order 2
    residuals_small(&mut e, "bb", fs, "1/10^6", 2);
    assert_eq!(ev(&mut e, "sort(map(b => expand(rewrite(x, b)), bb)) === sort(map(c => expand(c*t^(3/4) - c^2*t^(3/2)/4), [(-1)^(1/4), -(-1)^(1/4), (-1)^(3/4), -(-1)^(3/4)]))"), "true");
    // reals: none for t > 0 (x^4 = -t^3 to leading order), two for t < 0
    assert_eq!(ev(&mut e, "asymptotic_solve(y^2 - t*x + t == 0 and x^2 + t*y == 0, [[x, y], [0, 0]], [t, 0, 2], reals)"), "[]");
    let w = ev(&mut e, "ww = asymptotic_solve(y^2 - t*x + t == 0 and x^2 + t*y == 0, [[x, y], [0, 0]], [t, 0, 2], reals, direction: from_below); ww");
    assert!(!w.contains("im"), "{w}");
    assert_eq!(ev(&mut e, "length(ww)"), "2");
    residuals_small(&mut e, "ww", fs, "-1/10^6", 2);
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}

#[test]
fn every_branch_with_repeated_roots() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    // y (y^2 - t x) = 0, x^2 - t^2 y = 1: x = +-1, y = 0 exactly, and four branches x = +-1 + O(t^(5/2)) found by a
    // second Newton polygon at the double roots
    let v = ev(&mut e, "bb = asymptotic_solve(y^3 - t*x*y == 0 and x^2 - t^2*y == 1, [[x, y]], [t, 0, 3]); bb");
    assert!(v.starts_with("[[x -> -1, y -> 0], [x -> 1, y -> 0], "), "{v}");
    assert_eq!(ev(&mut e, "length(bb)"), "6");
    residuals_small(&mut e, "bb", "[y^3 - t*x*y, x^2 - t^2*y - 1]", "1/10^6", 3);
    // orders at and below the bounds, a machine order, a regular start: they end
    for src in [
        "asymptotic_solve(y^3 - t*x*y == 0 and x^2 - t^2*y == 1, [[x, y]], [t, 0, 0])",
        "asymptotic_solve(y^3 - t*x*y == 0 and x^2 - t^2*y == 1, [[x, y]], [t, 0, 1])",
        "asymptotic_solve(y^3 - t*x*y == 0 and x^2 - t^2*y == 1, [[x, y]], [t, 0, 3.5])",
        "asymptotic_solve(y^3 - t*x*y == 0 and x^2 - t^2*y == 1, [[x, y]], [t, 0.0, 2])",
        "asymptotic_solve(y^2 - x^2 - t == 0 and y - x == 0, [[x, y]], [t, 0, 2])",
    ] {
        let r = ev(&mut e, src);
        assert!(!r.is_empty(), "{src}");
    }
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}
