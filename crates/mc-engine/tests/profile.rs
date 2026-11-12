//! `profile(e)` and `timing(e, breakdown: true)` (decision sp-core.5): the value is the evaluation's, the rows name
//! the functions called with their counts, and the breakdown has its parts.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn profile_rows() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    eval(&mut e, "fn work(k) = n_integrate(sin(k*x)^2, [x, 0, 1])");
    eval(&mut e, "fn twice(k) = 2*work(k)");
    eval(&mut e, "p = profile(map(twice, range(1, 6)))");
    for (src, want) in [
        ("p[2] == map(twice, range(1, 6))", "true"),
        ("p[1] > 0", "true"),
        // one row per function and kind; the last row is the evaluation outside every measured call
        ("select(p[3], r => r[1] == twice)[1][[2, 3]]", "[\"definitions\", 6]"),
        ("select(p[3], r => r[1] == work)[1][[2, 3]]", "[\"definitions\", 6]"),
        ("select(p[3], r => r[1] == n_integrate)[1][[2, 3]]", "[\"library\", 6]"),
        ("last(p[3])[[1, 2]]", "[evaluation, \"interpreter\"]"),
        // the inclusive time of a caller covers its callees; own times are at most the inclusive ones
        ("select(p[3], r => r[1] == twice)[1][5] >= select(p[3], r => r[1] == n_integrate)[1][5]", "true"),
        ("count(map(r => r[4] <= r[5] + 1.0e-9, p[3]), false)", "0"),
        // sorted by own time
        ("q = most(map(r => r[4], p[3])); q == reverse(sort(q))", "true"),
        ("length(profile(1 + 1, top: 1)[3]) <= 2", "true"),
        // a profile inside a profile measures nothing of its own
        ("profile(profile(1)[3])[2]", "[]"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}

#[test]
fn timing_breakdown() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    eval(&mut e, "t = timing(n_integrate(exp(-x^2), [x, 0, 3]), breakdown: true)");
    for (src, want) in [
        ("length(t)", "3"),
        ("keys(t[3])", "[\"compiled\", \"library\", \"interpreter\", \"gc\", \"allocated_mb\"]"),
        ("t[3][\"library\"] > 0 and t[3][\"library\"] <= t[1] + 1.0e-9", "true"),
        ("abs(t[2] - 0.886207348259521) < 1.0e-12", "true"),
        ("length(timing(1, breakdown: false))", "2"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}
