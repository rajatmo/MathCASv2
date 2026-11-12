//! The parallel portfolios of `integrate` and `d_solve` (roadmap item 16A): the same answers at 1, 2, 4 and 8 threads.
use mc_api::{Engine, StopFlag};

const CASES: &[&str] = &[
    "integrate(exp(x^2) * x^3 + sin(x) / x + exp(-x^2), x)",
    "integrate(sqrt(1 + x^4), x)",
    "integrate(1 / (x^5 + x + 1), x)",
    "integrate(x * log(1 + x^2) * atan(x), x)",
    "integrate(sin(x)^3 * cos(x)^4 / (1 + sin(x)^2), x)",
    "integrate(x^2 * exp(-a * x^2) * erf(x), x)",
    "integrate(exp(x^2), x)",
    "integrate(1 / sqrt(x^3 + 1), x)",
    "integrate(e^(a * x), [x, 0, inf], generate_conditions: false)",
    "d_solve(z * w'''(z) + 2 * w''(z) + z * w'(z) == 0, w(z), z)",
    "d_solve((b + a * x) * y(x) + (-1 + 2 * x) * y'(x) + 2 * (-1 + x) * x * y''(x) == 0, y(x), x)",
    "d_solve(y''(x) - x * y'(x) + y(x) == 0, y, x)",
    "d_solve(y'(x) == x^2 + y(x)^2, y(x), x)",
    "d_solve(y'(x) == y(x)^3 + x, y(x), x)",
    "d_solve(2 * x * y(x) + (x^2 + 3 * y(x)^2) * y'(x) == 0, y(x), x)",
    "d_solve(x^2 * y''(x) + x * y'(x) + (x^2 - 4) * y(x) == 0, y(x), x)",
    "d_solve([y''(x) + y(x) == 0, y(0) == 1, y'(0) == 0], y(x), x)",
];

fn answers(threads: usize) -> Vec<String> {
    let mut e = mc_engine::CoreEngine::with_config(mc_engine::EngineConfig { threads, ..Default::default() });
    let stop = StopFlag::new();
    CASES.iter().map(|s| e.eval(s, &stop).map(|v| v.printed).unwrap_or_else(|err| format!("{err:?}"))).collect()
}

#[test]
fn the_same_answers_at_every_thread_count() {
    let one = answers(1);
    for (s, a) in CASES.iter().zip(&one) {
        assert!(!a.contains("failure"), "{s}: {a}");
    }
    for t in [2, 4, 8] {
        let other = answers(t);
        for (k, s) in CASES.iter().enumerate() {
            assert_eq!(other[k], one[k], "{s} at {t} threads");
        }
    }
}
