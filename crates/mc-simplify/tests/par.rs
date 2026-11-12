//! The parallel search and spot checks of `simplify` and `full_simplify` (roadmap item 16A): the same answers at 1, 2,
//! 4 and 8 threads.
use mc_api::{Engine, StopFlag};

const CASES: &[&str] = &[
    "full_simplify(e^elliptic_f(x, 1), -pi / 2 < x and x < pi / 2)",
    "full_simplify(product_log(x * log(x)), x > 1 / e)",
    "full_simplify(product_log(x * e^x), x >= -1)",
    "full_simplify(function_expand(catalan_number(n) == binomial(2 * n, n) - binomial(2 * n, n + 1)))",
    "full_simplify(exp_integral_e(1 - n, x) * x^n)",
    "simplify((x^4 - 1)/(x^2 - 1) + sin(x)^2 + cos(x)^2)",
    "simplify(sqrt(x^2), x < 0)",
    "simplify((x^10 - y^10)/(x - y), x > 0)",
    "full_simplify(gamma(x + 3)/gamma(x) - x (x + 1) (x + 2))",
    "simplify(tan(x) cos(x) + log(8) - 3 log(2))",
    "full_simplify(sinh(x) + cosh(x) - exp(x))",
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
