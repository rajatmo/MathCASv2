//! Root objects of analytic functions, `root([f, x0])`: every object is checked against an independent value at
//! 60 digits (a closed form, or `find_root` at 60 digits for a root without one); roots that are not simple and
//! isolated are refused (the call stays unevaluated); a root on a branch cut and a root next to a pole; the
//! producers `solve`, `reduce`, `n_solve` in a region and the extrema on a closed interval.
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

/// `a` and `b` agree to 55 digits at 60 digits.
fn agree(e: &mut CoreEngine, a: &str, b: &str) {
    let src = format!("abs(n({a}, 60) - n({b}, 60)) < 10^-55");
    assert_eq!(ev(e, &src), "true", "{a} against {b}: {} vs {}", ev(e, &format!("n({a}, 60)")), ev(e, &format!("n({b}, 60)")));
}

#[test]
fn objects_against_closed_forms() {
    timed(60, |e| {
        for (obj, exact) in [
            ("root([&(exp(&1) - 2), 0.7])", "log(2)"),
            ("root([x => sin(x) - 1/2, 0.5])", "pi/6"),
            ("root([&(exp(&1) - 1 - im), 0.3 + 0.8im])", "log(1 + im)"),
            ("root([&(exp(&1) + 1), 3.1im])", "pi*im"),
            // a pole of tan at pi/2, 0.01 away
            ("root([&(tan(&1) - 100), 1.56])", "atan(100)"),
            ("root([&(cos(&1) - 1/3), 1.2])", "acos(1/3)"),
        ] {
            // a root object, or the closed form when solve gives it exactly on a short interval around the root
            let r = ev(e, obj);
            assert!(!r.contains("failure") && !r.starts_with("root([x =>"), "{obj}: {r}");
            agree(e, obj, exact);
        }
    });
}

#[test]
fn canonical_form() {
    timed(60, |e| {
        // the factor that vanishes, coprime integer coefficients, the first positive, a machine starting value
        let r = ev(e, "root([&(-1/2 + dawson_f(&1)*&1), 0.92])");
        assert!(r.ends_with(", 0.9241388730045917])"), "{r}");
        assert_eq!(r, ev(e, "root([&(1 - 2*&1*dawson_f(&1)), 0.9241388730045917])"));
        assert_eq!(ev(e, "root([&(sinh(dawson_f(&1))*(1 - 2*&1*dawson_f(&1))/dawson_f(&1)), 0.9])"), ev(e, "root([&(2*&1*dawson_f(&1) - 1), 0.93])"));
        assert_eq!(ev(e, "root([&(&1/2 - sin(&1)), 1.9])"), ev(e, "root([&(2*sin(&1) - &1), 1.895494267033981])"));
        assert!(ev(e, "root([&(&1/2 - sin(&1)), 1.9])").ends_with(", 1.895494267033981])"));
        // a root on the imaginary axis: the real part of the starting value is 0
        assert!(ev(e, "root([&(exp(&1) + 1), 3.1im])").contains("3.141592653589793"));
        // b^(k + e) and b^e/b^-k give one numerator
        let r = ev(e, "root([&((1 + &1)^(-1 + sin(&1)) - 1/2), 2.4])");
        assert!(r.ends_with(", 2.6574194313793456])"), "{r}");
        assert_eq!(r, ev(e, "root([&(4*(1 + &1)^sin(&1) - 2 - 2*&1), 2.7])"));
        // printing again gives the same object
        let r = ev(e, "root([&(&1 - exp(1/&1)), -0.07 - 0.2im])");
        assert_eq!(ev(e, &r), r);
    });
}

#[test]
fn roots_without_closed_form() {
    timed(90, |e| {
        for (obj, f, x0) in [
            ("root([&(&1 - 2*sin(&1)), 1.9])", "x - 2*sin(x)", "1.9"),
            ("root([&(1 - 2*&1*dawson_f(&1)), 0.92])", "1 - 2*x*dawson_f(x)", "0.92"),
            ("root([&(bessel_j(0, &1)), 2.4])", "bessel_j(0, x)", "2.4"),
            ("root([&(gamma(&1) - log(&1) - im/2), 1.5 - im])", "gamma(x) - log(x) - im/2", "1.5 - im"),
        ] {
            let fr = format!("rewrite(x, find_root({f} == 0, [x, {x0}], working_precision: 70))");
            let v = ev(e, &fr);
            assert!(!v.contains("find_root"), "{fr}: {v}");
            agree(e, obj, &format!("({v})"));
        }
    });
}

#[test]
fn refused() {
    timed(60, |e| {
        for src in [
            // a double root
            "root([&((&1 - 1)^2*exp(&1)), 1.1])",
            // no root
            "root([&(sin(&1) - 2), 1.5])",
            // a root on the cut of log (at -2)
            "root([&(log(&1) - log(2) - pi*im), -2.1])",
        ] {
            let r = ev(e, src);
            assert!(r.starts_with("root([&("), "{src}: {r}");
            assert!(ev(e, &format!("n({src})")).starts_with("root("), "{src}");
        }
    });
}

#[test]
fn comparisons_and_values() {
    timed(60, |e| {
        // the root 1.8954942670339809... of x = 2 sin(x)
        let r = "root([&(&1 - 2*sin(&1)), 1.9])";
        assert_eq!(ev(e, &format!("{r} < 18955/10000")), "true");
        assert_eq!(ev(e, &format!("{r} > 18954/10000")), "true");
        assert_eq!(ev(e, &format!("n({r})")), "1.895494267033981");
        // arithmetic keeps the object
        assert!(ev(e, &format!("2*{r}")).starts_with("2*root("));
        agree(e, &format!("2*sin({r})"), r);
        // a root that solve gives exactly near the start is that number
        assert_eq!(ev(e, "root([&(sin(&1)), 3.1])"), "pi");
    });
}

#[test]
fn region_solvers() {
    timed(120, |e| {
        let s = ev(e, "solve(exp(x) == -1 and abs(x - 3*im) < 1, x)");
        assert!(s.starts_with("[[x -> root(") && !s.contains("], ["), "{s}");
        agree(e, "solve_values(exp(x) == -1 and abs(x - 3*im) < 1, x)[1]", "pi*im");
        // two complex roots in the square, conjugate
        let sq = "-1 <= re(x) and re(x) <= 1 and -1 <= im_part(x) and im_part(x) <= 1";
        let s = ev(e, &format!("solve_values(sin(e^x) - cos(2*x) == 1 and {sq}, x)"));
        assert_eq!(s.matches("root(").count(), 2, "{s}");
        for k in 1..=2 {
            let v = format!("solve_values(sin(e^x) - cos(2*x) == 1 and {sq}, x)[{k}]");
            let fr = ev(e, &format!("rewrite(x, find_root(sin(e^x) - cos(2*x) == 1, [x, n({v})], working_precision: 70))"));
            agree(e, &v, &format!("({fr})"));
        }
        // three roots of sin(x) - x == 2 in the disk |x| < 5: one real, two complex
        let r = ev(e, "reduce(sin(x) - x == 2 and abs(x) < 5, x)");
        assert_eq!(r.matches("root(").count(), 3, "{r}");
        assert!(r.contains("-2.554195952837043])"), "{r}");
        // n_solve gives the numbers
        let r = ev(e, "n_solve_values(exp(x) == -1 and abs(x - 3*im) < 1, x)");
        assert!(r.contains("3.141592653589793"), "{r}");
        agree(e, "n_solve_values(exp(x) == -1 and abs(x - 3*im) < 1, x)[1]", "pi*im");
        // nothing in the region
        assert_eq!(ev(e, "solve(exp(x) == -1 and abs(x - 1) < 1/2, x)"), "[]");
    });
}

#[test]
fn extrema_on_an_interval() {
    timed(120, |e| {
        agree(e, "max_value([sin(x) + cos(x), 0 <= x and x <= 1], x)", "sqrt(2)");
        let p = ev(e, "arg_max([airy_ai(x + sin(x)) + cos(x^2), -5 <= x and x <= 5], x)");
        assert!(p.starts_with("root([&("), "{p}");
        let fr = ev(e, "rewrite(x, find_root(d(airy_ai(x + sin(x)) + cos(x^2), x) == 0, [x, -0.43], working_precision: 70))");
        agree(e, &p, &format!("({fr})"));
        let q = ev(e, "arg_min([airy_ai(x + sin(x)) + cos(x^2), -5 <= x and x <= 5], x)");
        let fr = ev(e, "rewrite(x, find_root(d(airy_ai(x + sin(x)) + cos(x^2), x) == 0, [x, -3.96], working_precision: 70))");
        agree(e, &q, &format!("({fr})"));
        // the value is the function at the point, and maximize gives both
        let m = ev(e, "maximize([airy_ai(x + sin(x)) + cos(x^2), -5 <= x and x <= 5], x)");
        assert!(m.starts_with("[airy_ai(root("), "{m}");
        // an end wins: x on [0, 1] with a transcendental term
        assert_eq!(ev(e, "arg_max([x + sin(x)/10, 0 <= x and x <= 1], x)"), "1");
    });
}
