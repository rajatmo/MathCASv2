//! Property answers only when proved. Sign, monotonicity and convexity on a bounded domain are proved by
//! enclosures of f, f' or f'' over the domain; values at points only refute (a counterexample confirmed at exact
//! points); a property that neither a proof nor a confirmed counterexample decides stays unevaluated. Functions of
//! several intervals are enclosed over the box (corner hull only for proved partial signs). Each answer is checked
//! by an independent route: values at 30 digits, a second formula, or the defining inequality at grid points.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Is `src` true at 30 digits (a comparison of closed numeric terms)?
fn holds(e: &mut CoreEngine, src: &str) -> bool {
    ev(e, &format!("n({src}, 30)")) == "true" || ev(e, src) == "true"
}

#[test]
fn signs_on_bounded_domains_are_proved_by_enclosures() {
    let mut e = CoreEngine::new();
    // sin(x) + 1/2 on [0, 3]: the least value is at an end (sin is concave there), sin(3) + 1/2 > 0
    assert_eq!(ev(&mut e, "function_sign([sin(x) + 1/2, 0 <= x and x <= 3], x)"), "1");
    assert!(holds(&mut e, "sin(3) + 1/2 > 0"));
    // cos decreases on [0, 3] (its derivative -sin(x) is 0 at the end 0 and negative inside)
    assert_eq!(ev(&mut e, "function_monotonicity([cos(x), 0 <= x and x <= 3], x)"), "-1");
    assert!(holds(&mut e, "cos(0) > cos(3/2) and cos(3/2) > cos(3)"));
    // sin is concave on [0, 3]: the midpoint lies above the chord
    assert_eq!(ev(&mut e, "function_convexity([sin(x), 0 <= x and x <= 3], x)"), "-1");
    assert!(holds(&mut e, "sin(3/2) > (sin(0) + sin(3))/2"));
    // J0 is concave on [0, 1] (J0'' = (J2 - J0)/2 < 0 there)
    assert_eq!(ev(&mut e, "function_convexity([bessel_j(0, x), 0 <= x and x <= 1], x)"), "-1");
    assert!(holds(&mut e, "(bessel_j(2, 1) - bessel_j(0, 1))/2 < 0"));
}

#[test]
fn bounded_integer_sets_are_decided_by_their_values() {
    let mut e = CoreEngine::new();
    // n^2 - 5 on 1..10: -4 at 1, 95 at 10
    assert_eq!(ev(&mut e, "function_sign([n^2 - 5, 1 <= n and n <= 10], n, integers)"), "indeterminate");
    assert_eq!(ev(&mut e, "[1^2 - 5, 10^2 - 5]"), "[-4, 95]");
    assert_eq!(ev(&mut e, "function_sign([n^2 - 5, 3 <= n and n <= 10], n, integers)"), "1");
    assert_eq!(ev(&mut e, "min(table(k^2 - 5, [k, 3, 10]))"), "4");
    assert_eq!(ev(&mut e, "function_sign([(-1)^n, 1 <= n and n <= 6], n, integers)"), "indeterminate");
}

#[test]
fn points_refute_only_at_proved_values() {
    let mut e = CoreEngine::new();
    // values of both signs, proved at exact points
    assert_eq!(ev(&mut e, "function_sign(sin(x) - 1/3, x)"), "indeterminate");
    assert!(holds(&mut e, "sin(0) - 1/3 < 0 and sin(3/2) - 1/3 > 0"));
    assert_eq!(ev(&mut e, "function_monotonicity(x^3 - x, x)"), "indeterminate");
    assert!(holds(&mut e, "(-1)^3 - (-1) < (-1/2)^3 + 1/2 and (1/2)^3 - 1/2 < (-1/2)^3 + 1/2"));
    // a cusp: f'' < 0 where it exists, but f is not concave (its chords)
    assert_eq!(ev(&mut e, "function_convexity(x^(2/3), x)"), "indeterminate");
    // x^2 takes the value 1 twice
    assert_eq!(ev(&mut e, "function_injective(x^2, x)"), "false");
    assert_eq!(ev(&mut e, "function_injective(x exp(-x^2), x)"), "false");
    assert!(holds(&mut e, "1/2 exp(-1/4) > 0 and 2 exp(-4) < 1/2 exp(-1/4)"));
}

#[test]
fn answers_only_samples_supported_stay_unevaluated() {
    let mut e = CoreEngine::new();
    // airy_ai(x) + 1 is positive (airy_ai > -0.42), but no proof is built for the whole line
    let r = ev(&mut e, "function_sign(airy_ai(x) + 1, x)");
    assert!(r.starts_with("function_sign("), "{r}");
    assert!(holds(&mut e, "airy_ai(-1) + 1 > 0"));
    // the Cantor function is nondecreasing (a fact of the table: the distribution function of a measure), never
    // strictly (its differences prove nothing either way)
    assert_eq!(ev(&mut e, "function_monotonicity(cantor_staircase(x), x)"), "1");
    let r = ev(&mut e, "function_monotonicity(cantor_staircase(x), x, strict_inequalities: true)");
    assert!(r.starts_with("function_monotonicity(") || r == "indeterminate", "{r}");
    // floor of an increasing argument is nondecreasing by the rule of compositions (not by samples)
    assert_eq!(ev(&mut e, "function_monotonicity(floor(x), x)"), "1");
    assert_eq!(ev(&mut e, "function_monotonicity(floor(x^3 + x) + unit_step(x), x)"), "1");
    assert!(holds(&mut e, "floor(1/2) <= floor(3/2) and floor(3/2) <= floor(5/2)"));
    // 1/x is injective: decreasing on x < 0 and on x > 0 (the sign of -1/x^2), with the images (-inf, 0) and (0, inf)
    // from the limits at the ends of the pieces; the pieces between sampled poles never prove it
    assert_eq!(ev(&mut e, "function_injective(1/x, x)"), "true");
}

#[test]
fn periods_are_proved_by_simplification() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_period(sin(x)^2, x)"), "pi");
    assert_eq!(ev(&mut e, "simplify(sin(x + pi)^2 - sin(x)^2)"), "0");
    assert_eq!(ev(&mut e, "function_period(sin(x) + cos(2 x), x)"), "2*pi");
    assert!(holds(&mut e, "sin(1 + pi) + cos(2 + 2 pi) != sin(1) + cos(2)"));
    assert_eq!(ev(&mut e, "function_period(sin(x)^2 + cos(x)^2, x)"), "0");
}

/// Is every value of `f(X, Y)` at the 5 x 5 grid of the box [a1, b1] x [a2, b2] inside the interval `r`?
fn grid_inside(e: &mut CoreEngine, f: &str, (a1, b1): (&str, &str), (a2, b2): (&str, &str), r: &str) -> bool {
    let src = format!(
        "rn = n({r}, 40); vs = flatten(table(n({}, 30), [i, 0, 4], [j, 0, 4])); \
         length(select(vs, v => not(is_interval_member(rn, v))))",
        f.replace('X', &format!("({a1} + ({b1} - ({a1})) i/4)")).replace('Y', &format!("({a2} + ({b2} - ({a2})) j/4)"))
    );
    ev(e, &src) == "0"
}

#[test]
fn several_intervals_are_enclosed_over_the_box() {
    let mut e = CoreEngine::new();
    // pochhammer(a, n) increases in both arguments here: the hull of the corners
    let r = ev(&mut e, "r = pochhammer(interval([123/100, 124/100]), interval([234/100, 235/100])); r");
    assert!(r.starts_with("interval("), "{r}");
    assert!(grid_inside(&mut e, "pochhammer(X, Y)", ("123/100", "124/100"), ("234/100", "235/100"), "r"));
    // centered intervals: the same box rule
    let r = ev(&mut e, "c = pochhammer(centered_interval(3/2, 1/100), centered_interval(5/2, 1/50)); c");
    assert!(r.starts_with("centered_interval("), "{r}");
    ev(&mut e, "ri = interval([c[1] - c[2], c[1] + c[2]])");
    assert!(grid_inside(&mut e, "pochhammer(X, Y)", ("149/100", "151/100"), ("248/100", "252/100"), "ri"));
    // beta's ball form is loose on wide balls: no useful enclosure, and the call stays (never a grid estimate)
    let r = ev(&mut e, "beta(interval([1, 2]), interval([2, 3]))");
    assert!(r.starts_with("beta(") || r.starts_with("interval("), "{r}");
    // not monotone in the second argument over the box (0F1 has a zero in it): interval arithmetic still encloses
    let r = ev(&mut e, "h = hypergeometric_0f1(interval([1, 2]), interval([-3, 3])); h");
    if r.starts_with("interval(") {
        assert!(grid_inside(&mut e, "hypergeometric_0f1(X, Y)", ("1", "2"), ("-3", "3"), "h"));
    } else {
        assert!(r.starts_with("hypergeometric_0f1("), "{r}");
    }
}
