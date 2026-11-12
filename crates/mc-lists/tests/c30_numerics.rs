//! Round 30: the optimizers with integer and complex variables and inactive forms. Expected values are
//! identities or hand derivations.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (one worker thread, fresh session); `TIMEOUT` after 30 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn optimizers_with_integer_and_complex_variables() {
    assert_eq!(ev("conic_optimization(x + y, [x + 2*y >= 3, x >= -2], [element(x, integers), element(y, integers)])"), "[x -> -2, y -> 3]");
    assert_eq!(ev("conic_optimization(x + y, norm([x, y]) <= 1, [element(x, integers), element(y, reals)])"), "[x -> -1, y -> 0]");
    assert_eq!(ev("conic_optimization(-x + y, [x + 2*y >= 3, element(x, nonpositive_integers)], [x, y])"), "[x -> 0, y -> 3/2]");
    assert_eq!(ev("linear_optimization(x + y, [x + 2*y == 3, x >= 1/2, y >= 0], [element(x, integers), y])"), "[x -> 1, y -> 1]");
    assert_eq!(ev("convex_optimization(x^2 + y^2, [x + 2*y >= 2], [element(x, integers), element(y, integers)])"), "[x -> 0, y -> 1]");
    assert_eq!(ev("conic_optimization(re(z) + im_part(z), abs(z) <= 2, element(z, complexes))"), "[z -> complex(-1, -1)*2^(1/2)]");
    assert_eq!(ev("second_order_cone_optimization(re(z) + im_part(z), vector_greater_equal([z, im]), element(z, complexes))"), "[z -> 1im]");
    assert_eq!(ev("conic_optimization(dot([1, 1], x), vector_greater_equal([inactive(add)(dot([[1, 2], [1, 0]], x), [-3, 1]), 0]), x)"), "[x -> [-1, 2]]");
}

#[test]
fn gaussian_prime_omega() {
    assert_eq!(ev("prime_omega(5 + 9*im)"), "2");
    assert_eq!(ev("prime_omega(12, gaussian_integers: true)"), "5");
    assert_eq!(ev("prime_omega(3, gaussian_integers: true)"), "1");
    assert_eq!(ev("prime_omega(12)"), "3");
}

#[test]
fn digits_and_tau() {
    assert_eq!(ev("expand(from_digits([a, b, c], x))"), "c + a*x^2 + b*x");
    assert_eq!(ev("from_digits([1, 0], x)"), "x");
    assert_eq!(ev("integer_digits([-3, 12])"), "[[3], [1, 2]]");
    assert_eq!(ev("ramanujan_tau(63001)"), "-80561663527802406257321747");
    assert_eq!(ev("is_prime_power(1/25)"), "true");
    assert_eq!(ev("is_prime_power(2/25)"), "false");
}

#[test]
fn linear_algebra_forms() {
    assert_eq!(ev("cross([1, 2, 3, 4], [1, 4, 9, 16], [1, 8, 27, 81])"), "[-150, 174, -82, 12]");
    assert_eq!(ev("permanent([[1, 2], [3, 4]], method: \"Glynn\")"), "10");
    assert_eq!(ev("euclidean_distance(v, [0, 0, 0])"), "abs(v)*3^(1/2)");
    assert_eq!(ev("manhattan_distance([a, b], v)"), "abs(a - v) + abs(b - v)");
    assert_eq!(ev("distance_matrix([[0, 0], [3, 4]], distance_function: euclidean_distance)"), "[[0, 5], [5, 0]]");
    assert_eq!(ev("total(table(0.1, 100000), method: \"CompensatedSummation\") - 10^4"), "0.0");
}

#[test]
fn flatten_level_groups() {
    assert_eq!(ev("flatten([[[[a, b], [c, d]], [[0, 0], [0, 0]]], [[[0, 0], [0, 0]], [[a, b], [c, d]]]], [[1, 3], [2, 4]])"), "[[a, b, 0, 0], [c, d, 0, 0], [0, 0, a, b], [0, 0, c, d]]");
    assert_eq!(ev("flatten([[1, 2], [3, 4]], [[2], [1]])"), "[[1, 3], [2, 4]]");
    assert_eq!(ev("real_valued_numeric_q(pi)"), "true");
}
