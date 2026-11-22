//! Constrained extrema of trigonometric objectives on boxes (polar coordinates, angles over a period or an arc,
//! several angles): every answer is checked independently of the method. The value is not above (below, for a
//! maximum) any sample of a machine grid over the box, the objective at the returned point equals the value at 30
//! digits, and the point lies in the box. Problems whose faces are not all solved fail with a reason: never a value
//! taken from the corners alone (the distance from the unit disk to (3, 4) was given as the corner value 20).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

/// `f` over the box `lo_i <= v_i <= hi_i`: `min_value` (or `max_value`) and `arg_min` (or `arg_max`) against a
/// grid of `k` steps per side and against each other; the exact value when `want` is given.
fn check(e: &mut CoreEngine, f: &str, vars: &[(&str, &str, &str)], max: bool, k: usize, want: Option<&str>) {
    let cond = vars.iter().map(|(v, lo, hi)| format!("{lo} <= {v} and {v} <= {hi}")).collect::<Vec<_>>().join(" and ");
    let names = vars.iter().map(|(v, _, _)| *v).collect::<Vec<_>>().join(", ");
    let (val, arg, pick, cmp, slack) = if max { ("max_value", "arg_max", "max", "<=", "+") } else { ("min_value", "arg_min", "min", ">=", "-") };
    let call = |what: &str| format!("{what}([{f}, {cond}], [{names}])");
    if let Some(w) = want {
        assert_eq!(ev(e, &call(val)), w, "{}", call(val));
    }
    let iters = vars.iter().map(|(v, lo, hi)| format!("{v}, range({lo}, {hi}, ({hi} - ({lo}))/{k})")).collect::<Vec<_>>().join(", ");
    holds(e, &format!("{{ let v = n({}, 30); let s = {pick}(flatten(table(n({f}), {iters}))); s {cmp} v {slack} 10^-12 * (1 + abs(v)) }}", call(val)));
    let rules = vars.iter().enumerate().map(|(i, (v, _, _))| format!("{v} -> p[{}]", i + 1)).collect::<Vec<_>>().join(", ");
    let inside = vars
        .iter()
        .enumerate()
        .map(|(i, (_, lo, hi))| format!("n({lo}) - 10^-20 <= n(p[{0}], 30) and n(p[{0}], 30) <= n({hi}) + 10^-20", i + 1))
        .collect::<Vec<_>>()
        .join(" and ");
    holds(e, &format!("{{ let p = {}; let v = {}; abs(n(rewrite({f}, [{rules}]) - v, 30)) < 10^-25 and {inside} }}", call(arg), call(val)));
}

#[test]
fn distance_from_a_disk_in_polar_coordinates() {
    let mut e = CoreEngine::new();
    let f = "(r*cos(t) - 3)^2 + (r*sin(t) - 4)^2";
    let polar = [("r", "0", "1"), ("t", "0", "2*pi")];
    check(&mut e, f, &polar, false, 40, Some("16"));
    check(&mut e, f, &polar, true, 40, Some("36"));
    assert_eq!(ev(&mut e, &format!("minimize([{f}, 0 <= r and r <= 1 and 0 <= t and t <= 2*pi], [r, t])")), "[16, [r -> 1, t -> atan(4/3)]]");
    // arcs: the nearest direction outside the range, a range around it, a range shifted by a period
    check(&mut e, f, &[("r", "0", "1"), ("t", "pi/2", "pi")], false, 40, Some("18"));
    check(&mut e, f, &[("r", "0", "2"), ("t", "-pi/4", "pi/4")], false, 40, None);
    check(&mut e, f, &[("r", "0", "1"), ("t", "2*pi", "4*pi")], false, 40, Some("16"));
    check(&mut e, "(r*cos(t) + 3)^2 + (r*sin(t) - 4)^2", &[("r", "0", "2"), ("t", "0", "2*pi")], false, 40, Some("9"));
}

#[test]
fn trigonometric_polynomials_on_boxes() {
    let mut e = CoreEngine::new();
    let full = [("x", "0", "2*pi"), ("y", "0", "2*pi")];
    check(&mut e, "sin(x) + sin(y) + sin(x + y)", &full, true, 60, Some("(3/2)*3^(1/2)"));
    check(&mut e, "sin(x) + sin(y) + sin(x + y)", &full, false, 60, Some("(-3/2)*3^(1/2)"));
    check(&mut e, "sin(x)*cos(y)", &[("x", "0", "pi"), ("y", "0", "pi")], false, 40, Some("-1"));
    check(&mut e, "x*sin(y)", &[("x", "1", "2"), ("y", "0", "2*pi")], false, 40, Some("-2"));
    check(&mut e, "r^2*sin(2*t)", &[("r", "0", "1"), ("t", "0", "pi")], false, 40, Some("-1"));
    check(&mut e, "r*(cos(t) - 2*sin(t))", &[("r", "1", "2"), ("t", "0", "2*pi")], false, 40, Some("-2*5^(1/2)"));
    check(&mut e, "sin(x)^2 + cos(y)", &[("x", "0", "1"), ("y", "0", "1")], false, 20, Some("cos(1)"));
    // three and four angles
    check(&mut e, "sin(x)*sin(y)*sin(z)", &[("x", "0", "pi"), ("y", "0", "pi"), ("z", "0", "2*pi")], false, 12, Some("-1"));
    check(&mut e, "cos(a) + cos(b) + cos(c) + cos(d)", &[("a", "0", "4"), ("b", "0", "4"), ("c", "0", "4"), ("d", "0", "4")], false, 6, Some("-4"));
    // one variable
    assert_eq!(ev(&mut e, "max_value([sin(t) + cos(2*t), 0 <= t and t <= 2*pi], t)"), "9/8");
    assert_eq!(ev(&mut e, "arg_min([(cos(t) - 3)^2 + (sin(t) - 4)^2, 0 <= t and t <= 2*pi], t)"), "atan(4/3)");
}

#[test]
fn curves_of_critical_points_and_unsolved_faces() {
    let mut e = CoreEngine::new();
    // sin(x + y) is -1 along the line x + y = 3 pi/2: a curve of critical points, decided by its value
    check(&mut e, "sin(x + y)", &[("x", "0", "pi"), ("y", "0", "pi")], false, 40, Some("-1"));
    check(&mut e, "cos(x - y)", &[("x", "0", "pi"), ("y", "0", "pi/2")], true, 40, Some("1"));
    // faces that solve does not solve: a failure with the reason, not the smallest value at the corners
    let fail = |what: &str| format!("failure(unsolved, {what}, \"the critical points on a face of the constraints are not found\")");
    assert_eq!(ev(&mut e, "max_value([sin(x*y), 0 <= x and x <= 2 and 0 <= y and y <= 2], [x, y])"), fail("max_value"));
    assert_eq!(ev(&mut e, "min_value([cos(x) + cos(y), x^2 + y^2 <= 4], [x, y])"), fail("min_value"));
}
