//! The form `piecewise_expand` states (its docs page): the answer equals the input at many points of every piece
//! (exact arithmetic, jump points included), its values are distinct, none equals the default, the default is 0
//! when 0 is a value, and no piecewise function is left inside a condition.
mod common;
use common::Sys;
use std::time::{Duration, Instant};

/// The sample points: multiples of 1/8 (the jump points of the waves among them) and of 1/7 in `[lo, hi]`.
fn points(lo: i64, hi: i64) -> Vec<String> {
    let mut v = vec![];
    for k in lo * 8..=hi * 8 {
        v.push(format!("{k}/8"));
    }
    for k in lo * 7..=hi * 7 {
        v.push(format!("{k}/7"));
    }
    v
}

/// `piecewise_expand(f, cond)` (or of `f` alone) has the stated form and equals `f` at the points of `[lo, hi]`
/// that satisfy `cond`.
fn check(s: &mut Sys, f: &str, cond: Option<&str>, lo: i64, hi: i64) {
    let call = match cond {
        Some(c) => format!("piecewise_expand({f}, {c})"),
        None => format!("piecewise_expand({f})"),
    };
    let r = s.run(&call);
    if s.run(&format!("head({r}) === piecewise")) == "true" {
        let vs = format!("map(p => p[1], ({r})[1])");
        assert_eq!(s.run(&format!("length(union({vs})) == length({vs})")), "true", "{call}: equal values not merged: {r}");
        assert_eq!(s.run(&format!("member({vs}, ({r})[2])")), "false", "{call}: a piece has the default's value: {r}");
        assert_eq!(s.run(&format!("({r})[2] === 0 or not member({vs}, 0)")), "true", "{call}: 0 is not the default: {r}");
        let cs = format!("map(p => p[2], ({r})[1])");
        for g in ["max", "min", "abs", "piecewise", "floor", "unit_step"] {
            assert_eq!(s.run(&format!("free_of({cs}, {g})")), "true", "{call}: {g} left in a condition: {r}");
        }
    }
    for p in points(lo, hi) {
        let inside = match cond {
            Some(c) => s.run(&format!("rewrite({c}, x -> {p})")),
            None => "true".to_string(),
        };
        if inside != "true" {
            continue;
        }
        let d = s.run(&format!("rewrite(({f}) - ({r}), x -> {p})"));
        assert_eq!(d, "0", "{call} = {r} differs at x = {p}");
    }
}

#[test]
fn waves_and_steps_in_intervals() {
    let start = Instant::now();
    let mut s = Sys::new();
    for w in ["sawtooth_wave", "square_wave", "triangle_wave"] {
        for u in ["x", "2*x", "x + 1/3"] {
            for (cond, lo, hi) in [("-1 <= x and x <= 1", -1, 1), ("0 < x and x < 3", 0, 3), ("-1 < x and x < 1", -1, 1)] {
                check(&mut s, &format!("{w}({u})"), Some(cond), lo, hi);
            }
        }
    }
    for f in ["fractional_part(x)", "floor(x) + x", "ceiling(2*x)", "round(x)", "mod(x, 2)", "fractional_part(x)^2"] {
        check(&mut s, f, Some("0 < x and x < 4"), 0, 4);
    }
    assert!(start.elapsed() < Duration::from_secs(240), "took {:?}", start.elapsed());
}

#[test]
fn functions_of_the_real_line() {
    let start = Instant::now();
    let mut s = Sys::new();
    for f in [
        "unit_triangle(x)",
        "x*unit_box(x)",
        "unit_triangle(2*x - 1)",
        "max(x, 1 - x, x^2 - 3)",
        "min(x^2 + 2*x - 2, max(2*x^2 - 3*x + 4, x^2 - 3))",
        "max(x, min(1 - x, 2*x))",
        "clip(x, [0, 4])",
        "abs(x - 1) + abs(x + 1)",
        "unit_step(x + 1) - unit_step(x - 1) + 1",
        "ramp(x - 1) - ramp(x + 1)",
        "if x < -pi/2 then -1 else (if x <= pi/2 then sin(x) else 1)",
        "if max(x, 1 - x) > 2 then x else 1",
        "if abs(x) <= 1 then 1 - x^2 else 0",
    ] {
        check(&mut s, f, None, -4, 4);
    }
    assert!(start.elapsed() < Duration::from_secs(240), "took {:?}", start.elapsed());
}

/// The written forms of the docs page (compared as terms: the test engine prints plainly).
#[test]
fn stated_forms() {
    let mut s = Sys::new();
    for (a, want) in [
        ("piecewise_expand(fractional_part(x), 0 < x and x < 4)", "piecewise([[-3 + x, x >= 3], [-2 + x, 2 <= x and x < 3], [-1 + x, 1 <= x and x < 2]], x)"),
        ("piecewise_expand(square_wave(x), -1 <= x and x <= 1)", "piecewise([[-1, -1/2 <= x and x < 0 or 1/2 <= x and x < 1]], 1)"),
        ("piecewise_expand(triangle_wave(x), 0 < x and x < 1)", "piecewise([[4*(-1 + x), x >= 3/4], [4*x, x < 1/4]], -2*(-1 + 2*x))"),
        ("piecewise_expand(unit_triangle(x))", "piecewise([[1 - x, 0 <= x and x <= 1], [1 + x, -1 <= x and x < 0]], 0)"),
        ("piecewise_expand(unit_triangle(a, a, a, a), 0 < a and a < 1)", "(-1 + a)^4"),
        ("piecewise_expand(if x < -pi/2 then -1 else (if x <= pi/2 then sin(x) else 1))", "piecewise([[-1, x < (-1/2)*pi], [1, x > (1/2)*pi]], sin(x))"),
        ("piecewise_expand(min(x^2 + 2*x - 2, max(2*x^2 - 3*x + 4, x^2 - 3)))", "piecewise([[-2 + 2*x + x^2, -5*x + x^2 >= -6]], 4 - 3*x + 2*x^2)"),
        (
            "piecewise_expand(unit_step(x + 1) - unit_step(x - 1)*(sin(x)^2 + cos(x)^2) + 1, method: [value_simplifier -> simplify])",
            "piecewise([[1, x >= 1 or x < -1]], 2)",
        ),
    ] {
        let got = s.run(a);
        assert_eq!(s.run(&format!("({a}) === ({want})")), "true", "{a} gave {got}");
    }
}
