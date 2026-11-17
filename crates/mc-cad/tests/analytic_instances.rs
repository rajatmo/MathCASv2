//! `find_instance` over the complex numbers for equations with analytic functions: every point is checked here by
//! substituting it at 40 digits (residual below 10^-30) and the inequations by the distance of the two sides.
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

/// `find_instance(cond, vars)` gives one point; `lhs - rhs` (in the forms given, with Bessel functions) vanishes
/// there and `x - y` does not when `distinct`.
fn instance(e: &mut CoreEngine, call: &str, residual: &str, distinct: bool) {
    ev(e, &format!("p = find_instance({call})"));
    let r = ev(e, "p");
    assert!(r.starts_with("[[x -> "), "{call}: {r}");
    let v = ev(e, &format!("n(rewrite({residual}, p[1]), 40)"));
    // a value too near 0 for 40 digits comes as its enclosure inside failure(precision_not_reached, ...): every
    // number printed there must be below 10^-30
    let tiny = if v.starts_with("failure(precision_not_reached") || v.starts_with("ball(0, ") {
        let v = v.replace("ball(0, ", "ball(0e-999, ");
        let exps: Vec<i32> = v.split("e-").skip(1).filter_map(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()).collect();
        !exps.is_empty() && exps.iter().all(|&k| k > 30) && v.matches("ball(").count() * 2 == exps.len()
    } else {
        ev(e, &format!("abs({v}) < 10^-30")) == "true"
    };
    assert!(tiny, "{call}: {r}: {v}");
    if distinct {
        assert_eq!(ev(e, "abs(n(rewrite(x - y, p[1]), 30)) > 10^-10"), "true", "{call}: {r}");
    }
}

#[test]
fn analytic_instances() {
    timed(120, |e| {
        // H1_1(x) = J_1 + i Y_1; h1_1(y) = -exp(i y) (y + i)/y^2 (DLMF 10.49.6)
        instance(
            e,
            "hankel_h1(1, x) == spherical_hankel_h1(1, y) and x != y, [x, y]",
            "bessel_j(1, x) + im*bessel_y(1, x) + exp(im*y)*(y + im)/y^2",
            true,
        );
        instance(
            e,
            "spherical_hankel_h2(1, x) == spherical_hankel_h2(1, y) and x != y, [x, y]",
            "exp(-im*x)*(x - im)/x^2 - exp(-im*y)*(y - im)/y^2",
            true,
        );
        instance(e, "sin(x + y) == 1/2 and exp(x) - y == 1, [x, y]", "sin(x + y) - 1/2", false);
        instance(e, "sin(x + y) == 1/2 and exp(x) - y == 1, [x, y]", "exp(x) - y - 1", false);
        // sin(x) = 3 has no real solution but complex ones (pi/2 + i acosh(3) and others)
        instance(e, "sin(x) == 3, x", "sin(x) - 3", false);
        instance(e, "x^2 == -1, x", "x^2 + 1", false);
        // a condition that no point meets stays without a point
        assert_eq!(ev(e, "find_instance(x^2 == -1 and x > 0, x)"), "[]");
    });
}
