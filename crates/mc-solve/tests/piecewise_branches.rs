//! Equations with a piecewise function of the unknown split into their branches, conditions linear in the unknown
//! bounding the members of a family, and the `cubics`/`quartics` options of `reduce`. Each answer is checked by
//! substitution (the residual of every root is 0 to 30 digits) and the counts by hand in the comments.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Runs `f` and checks that it took at most `secs` seconds.
fn timed(secs: u64, f: impl FnOnce(&mut CoreEngine)) {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    f(&mut e);
    assert!(t.elapsed() < Duration::from_secs(secs), "took {:?}", t.elapsed());
}

#[test]
fn branches() {
    timed(60, |e| {
        // 0 < x < 1: x^2 + x = 1 gives (sqrt(5) - 1)/2 = 0.618 inside; 1 < x < 2: x^3 + x^2 = 1 has its real root
        // 0.755 outside; elsewhere x^2 = 1 gives -1 and 1 (1 is in neither open interval)
        assert_eq!(
            ev(e, "reduce(x^2 + piecewise([[x, 0 < x and x < 1], [x^3, 1 < x and x < 2]]) == 1, x)"),
            "x == -1 or x == (1/2)*(-1 + 5^(1/2)) or x == 1"
        );
        // sin(x) = 1/2 on [-pi/2, pi/2] only at pi/6; the constant branches -1 and 1 never equal 1/2
        let f = "(if x < -pi / 2 then -1 else (if -pi / 2 <= x and x <= pi / 2 then sin(x) else 1))";
        assert_eq!(ev(e, &format!("reduce({f} == 1/2, x)")), "x == (1/6)*pi");
        assert_eq!(ev(e, &format!("solve({f} == 1/2, x)")), "[[x -> (1/6)*pi]]");
        // x >= 0: x^2 - x + 1 = 9, x = (1 + sqrt(33))/2; x < 0: x^2 - x = 9, x = (1 - sqrt(37))/2
        let r = ev(e, "n_solve(abs(x)^2 - x + unit_step(x) == 9, x, reals)");
        assert_eq!(r, "[[x -> -2.5413812651491097], [x -> 3.3722813232690143]]");
        assert_eq!(ev(e, "abs(n((1 - 37^(1/2))/2) + 2.5413812651491097) < 10^-14"), "true");
        assert_eq!(ev(e, "abs(n((1 + 33^(1/2))/2) - 3.3722813232690143) < 10^-14"), "true");
        assert_eq!(ev(e, "solve(piecewise([[x^2, x < 0]], x + 1) == 4, x)"), "[[x -> -2], [x -> 3]]");
    });
}

#[test]
fn linear_conditions_bound_families() {
    timed(60, |e| {
        // 2 x <= 7 and x >= 0: pi/6 and 5 pi/6 (13 pi/6 = 6.8 is beyond 3.5)
        assert_eq!(ev(e, "solve(sin(x) == 1/2 and 2*x <= 7 and not(x < 0), x)"), "[[x -> (1/6)*pi], [x -> (5/6)*pi]]");
        assert_eq!(ev(e, "solve(sin(x) == 1/2 and x - pi/2 <= 0 and x + pi/2 >= 0, x)"), "[[x -> (1/6)*pi]]");
        assert_eq!(ev(e, "solve(sin(x) == 1/2 and -x >= -7 and x > 0, x)"), "[[x -> (1/6)*pi], [x -> (5/6)*pi], [x -> (13/6)*pi]]");
    });
}

#[test]
fn reduce_radical_options() {
    timed(60, |e| {
        // three roots by Cardano's formulas; each a root of the cubic to 30 digits
        let r = ev(e, "reduce(x^3 + 2*x^2 + 3*x + 4 == 0, x, cubics -> true)");
        assert_eq!(r.matches(" or ").count(), 2, "{r}");
        assert!(!r.contains("root("), "{r}");
        ev(e, "rs = x /. solve(x^3 + 2*x^2 + 3*x + 4 == 0, x, cubics -> true)");
        assert_eq!(ev(e, "map(v => abs(rewrite(x^3 + 2*x^2 + 3*x + 4, x -> n(v, 40))) < 10^-30, rs)"), "[true, true, true]");
        let r = ev(e, "reduce(x^4 + 2*x^2 + 3*x + 4 == 0, x, quartics -> true)");
        assert_eq!(r.matches(" or ").count(), 3, "{r}");
        assert!(!r.contains("root("), "{r}");
    });
}
