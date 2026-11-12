//! Parallel lifting and parallel optimisation cases (roadmap item 16A): the same answers at 1, 2, 4 and 8 threads.
use mc_api::{Engine, StopFlag};

const CASES: &[&str] = &[
    "minimize([max(x - y, abs(y)), min(x - 2, x - y^2) >= abs(x * y - 1)], [x, y])",
    "maximize([-max(x - y, abs(y)), min(x - 2, x - y^2) >= abs(x * y - 1)], [x, y])",
    "minimize([x - y, -y^2 + 4 * x^2 - 4 * x^4 > 0], [x, y])",
    "arg_min([abs(x - 1) + abs(y + 2), x^2 + y^2 <= 9], [x, y])",
    "resolve(for_all([a, b, c, d], sqrt((a + c)^2 + (b + d)^2) <= sqrt(a^2 + b^2) + sqrt(c^2 + d^2)))",
    "resolve(for_all(x, a x^2 + b x + c > 0), reals)",
    "resolve(exists(x, a x^2 + b x + c < 0), reals)",
    "reduce(x^2 + y^2 < 2 and x - y >= 1, [x, y])",
    "reduce(exists([x, y], a x + b y >= 1 and (a^2 - b) x - (a - b^2) y == 2), [a, b])",
    "cylindrical_decomposition(x^2 + y^2 + z^2 < 1 and x y z > 1/100, [x, y, z])",
    "cylindrical_decomposition(y^2 == x*(-1 + x^4), [x, y])",
    "resolve(exists(y, x y^2 + y + z == 0 and x^2 + z^2 <= 4), reals)",
    "resolve(exists([x, y, z, t], 1221 x + 3434 y + 5566 z + 8778 t == 90909 and x >= 0 and y >= 0 and z >= 0 and t >= 0), integers)",
    "reduce([12 x1 + 16 x2 + 20 x3 + 27 x4 == 123, x1 >= 0, x2 >= 0, x3 >= 0, x4 >= 0], [x1, x2, x3, x4], integers)",
    "resolve(exists([x, y], a x^2 + b x + c == 0 and a y^2 + b y + c == 0 and x != y))",
    "resolve(for_all(x, exists(y, x^2 + a y^2 == b)))",
    "reduce(x^2 + y^2 == 1 and x y == a, [x, y])",
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
        assert!(!a.starts_with("minimize(") && !a.contains("failure"), "{s}: {a}");
    }
    for t in [2, 4, 8] {
        let other = answers(t);
        for (k, s) in CASES.iter().enumerate() {
            assert_eq!(other[k], one[k], "{s} at {t} threads");
        }
    }
}
