//! Real solutions with square roots of parameter expressions keep the boundary of their conditions, where the
//! radicand is 0 and two roots meet: every parameter region is checked by substitution against the root count.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let _ = tx.send(e.eval(&s, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}")));
    });
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// The distinct real values the solution gives at x = x0, their number, and the largest residual at 30 digits.
fn region(eq: &str, x0: &str) -> String {
    ev(&format!(
        "{{ let s = solve({eq}, y, reals); \
           let vs = union(select(map(r => rewrite(y, rewrite(r, x -> {x0})), s), v => not (v === undefined))); \
           let p = rewrite(first(apply(list, {eq})) - last(apply(list, {eq})), x -> {x0}); \
           [length(vs), max(join([0], map(v => abs(n(rewrite(p, y -> v), 30)), vs))) < 10^-25] }}"
    ))
}

#[test]
fn biquadratic_regions() {
    // y^4 - x y^2 + 1: y^2 = (x +- sqrt(x^2 - 4))/2, real and positive for x >= 2; no real root for x < 2, the two
    // double roots +-1 at x = 2, four simple roots for x > 2
    let eq = "y^4 - x*y^2 + 1 == 0";
    for (x0, count) in [("-3", 0), ("0", 0), ("2", 2), ("5/2", 4), ("3", 4), ("10", 4)] {
        assert_eq!(region(eq, x0), format!("[{count}, true]"), "x = {x0}");
    }
}

#[test]
fn quadratic_regions() {
    // y^2 - x y + 1: discriminant x^2 - 4; the double root x/2 at x = -2 and x = 2
    let eq = "y^2 - x*y + 1 == 0";
    for (x0, count) in [("-3", 2), ("-2", 1), ("0", 0), ("2", 1), ("3", 2)] {
        assert_eq!(region(eq, x0), format!("[{count}, true]"), "x = {x0}");
    }
}

#[test]
fn reciprocal_square_root_stays_strict() {
    // 1/sqrt(x) has no value at x = 0: the condition keeps x > 0
    assert_eq!(ev("solve(y*sqrt(x) == 1, y, reals)"), "[[y -> conditional_expression(x^(-1/2), 0 < x)]]");
}
