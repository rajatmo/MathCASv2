//! `function_domain` of order relations with circular functions (a family over one period, or the conjunction with
//! a polynomial bound reduced), and the singular sets on the line of the inverse Jacobi functions of a numeric
//! parameter (the complement of the open interior of the real domain, and the jump at 0 of arccs for m < 1 and
//! arcds for m > 1). Each answer is checked independently: the family against the original relation at grid points
//! (the integer c(1) taken from the point), the singular sets against machine values (real and continuous off the
//! set, not real on it, the jump at 0). Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

#[test]
fn domains_of_circular_relations() {
    timed("periodic domains", 60, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "function_domain(log(sin(x)), x)"), "element(c(1), integers) and 2*pi*c(1) < x and x < pi + 2*pi*c(1)");
        // (domain, the original relation, the integer of the period holding x)
        for (f, rel, k) in [
            ("log(sin(x))", "sin(x) > 0", "floor(x/(2*pi))"),
            ("log(cos(x))", "cos(x) > 0", "floor((x + pi/2)/(2*pi))"),
            ("sqrt(sin(x))", "sin(x) >= 0", "floor(x/(2*pi))"),
            ("log(tan(x) - 1)", "tan(x) > 1", "floor(x/pi)"),
        ] {
            holds(&mut e, &format!(
                "{{ let d = function_domain({f}, x); let pts = range(-12.31, 12.3, 0.173); all_true(table(rewrite(rewrite(d, c(1) -> {k}), x -> t) == rewrite({rel}, x -> t), t, pts), b => b) }}"
            ));
            holds(&mut e, &format!("not free_of(function_domain({f}, x), c(1))"));
        }
        // with a polynomial bound: one interval
        assert_eq!(ev(&mut e, "function_domain([log(tan(x) - 1), x^2 < 1], x)"), "not_element(1/2 + x/pi, integers) and (1/4)*pi < x and x < 1");
        holds(&mut e, "{ let d = function_domain([log(tan(x) - 1), x^2 < 1], x); all_true(table(rewrite(d, x -> t) == (tan(t) > 1 and t^2 < 1), t, range(-1.5, 1.5, 0.0137)), b => b) }");
    });
}

/// The singular set `s` of `f(x)` (a function name and its parameter) against machine values on a grid: off the
/// set real and continuous, on the set (away from 0) not real.
fn check_cuts(e: &mut CoreEngine, f: &str, m: &str) {
    let s = format!("function_singularities({f}(x, {m}), x)");
    let set = ev(e, &s);
    assert!(!set.starts_with("function_singularities"), "{s} stays unevaluated");
    holds(e, &format!(
        "{{ let s = {s}; all_true(table(if rewrite(s, x -> t) then true else (abs(im_part({f}(t, {m}))) < 10^-12 and abs({f}(t + 10^-7, {m}) - {f}(t - 10^-7, {m})) < 10^-4), t, range(-3.013, 3.0, 0.0371)), b => b) }}"
    ));
    holds(e, &format!(
        "{{ let s = {s}; all_true(table({{ let v = {f}(t, {m}); if rewrite(s, x -> t) then not (is_number(v) and abs(im_part(v)) < 10^-12) else true }}, t, range(-3.013, 3.0, 0.0371)), b => b) }}"
    ));
}

#[test]
fn inverse_jacobi_singular_sets_on_the_line() {
    timed("inverse Jacobi cuts", 120, || {
        let mut e = CoreEngine::new();
        for f in ["sn", "cn", "dn", "cd", "sd", "nd", "dc", "nc", "ns", "sc", "cs", "ds"] {
            for m in ["1/3", "3", "-2"] {
                check_cuts(&mut e, &format!("inverse_jacobi_{f}"), m);
            }
        }
        assert_eq!(ev(&mut e, "function_singularities(inverse_jacobi_sn(x, 1/3), x)"), "x <= -1 or x >= 1");
        assert_eq!(ev(&mut e, "function_singularities(inverse_jacobi_cs(x, 3), x)"), "-2^(1/2) <= x and x <= 2^(1/2)");
        assert_eq!(ev(&mut e, "function_discontinuities(inverse_jacobi_ds(x, 3), x)"), "x == 0");
        // the jump at 0: K(m) on the right, -K(m) on the left
        holds(&mut e, "abs(inverse_jacobi_cs(10.0^-12, 1/3) - n(elliptic_k(1/3))) < 10^-9 and abs(inverse_jacobi_cs(-10.0^-12, 1/3) + n(elliptic_k(1/3))) < 10^-9");
        holds(&mut e, "abs(inverse_jacobi_ds(10.0^-12, 3) - inverse_jacobi_ds(-10.0^-12, 3)) > 1");
    });
}
