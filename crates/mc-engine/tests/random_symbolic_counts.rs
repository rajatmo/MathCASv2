//! Random draws with a count that is not a number: the call stays unevaluated or fails with an error, quickly and
//! without allocating.

use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Duration;

/// Evaluate the statements in one fresh session on a worker thread; `TIMEOUT` after 60 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    let stop = StopFlag::new();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn random_polygon_with_a_symbolic_count_stays_unevaluated() {
    for src in [
        "random_polygon([convex, i])",
        "random_polygon([convex, n_u])",
        "random_polygon([convex_hull, automatic, n_u])",
        "random_polygon(3, [2, n_u])",
        "random_polygon(3, n_u)",
        "random_polygon([simple, -2])",
        "random_polygon([star_shaped, 1.5])",
        "random_polygon(n_u -> 4)",
    ] {
        let v = ev(src);
        assert!(v.starts_with("random_polygon("), "{src}: {v}");
    }
    assert!(ev("random_polygon([convex, 5])").starts_with("polygon("));
}

#[test]
fn table_of_random_polygons_with_a_symbolic_bound() {
    assert_eq!(ev("polygons = table(random_polygon([convex, i]), i, n_u); polygons"), "table(random_polygon([convex, i]), i, n_u)");
    assert_eq!(ev("table(f(i), i, n_u)"), "table(f(i), i, n_u)");
}

#[test]
fn random_functions_with_a_symbolic_count_fail_with_an_error() {
    for src in [
        "random_real([0, 1], n_u)",
        "random_integer(10, [2, n_u])",
        "random_normal(0, 1, n_u)",
        "random_choice([1, 2, 3], n_u)",
        "random_sample([1, 2, 3], n_u)",
        "random_variate(normal_distribution(0, 1), n_u)",
    ] {
        let v = ev(src);
        assert!(v.starts_with("failure(argument"), "{src}: {v}");
    }
}
