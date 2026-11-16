//! The value of kei_2 at 0 and the limit of the regularized confluent hypergeometric function at infinity.


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
/// kei_2(x) as x -> 0. ker_2 + i kei_2 = e^(-i pi) K_2(x e^(i pi/4)) and K_2(w) = 2/w^2 - 1/2 + O(w^2 log w) (DLMF
/// 10.31.1, 10.65), so with w^2 = i x^2 the leading term is -2 i/x^2 and kei_2(x) = 2/x^2 + O(log x): positive, and
/// the same on both sides of 0, as w^2 = i x^2 holds for x < 0 too (the values at -0.1 and 0.1 are 199.996 plus an
/// imaginary part of 0.004 at -0.1). The limit is +inf from each side: a signed infinity, not complex infinity. For
/// an odd order the leading term is odd in x and the real domain x > 0 decides (the infinity of the right side).
#[test]
fn kei_of_order_two_at_zero_is_plus_infinity() {
    assert_eq!(ev("kelvin_kei(2, 0)"), "inf");
    let right: f64 = ev("kelvin_kei(2, 0.1)").parse().unwrap();
    assert!((right - 199.996).abs() < 1e-3, "{right}");
    let left = ev("kelvin_kei(2, -0.1)");
    assert!(left.starts_with("complex(199.99"), "{left}");
    let tiny: f64 = ev("n(kelvin_kei(2, 10^-6), 20)").parse().unwrap();
    assert!((tiny * 1e-12 - 2.0).abs() < 1e-6, "{tiny}");
}

/// 1F1(1; b; x)/Gamma(b) ~ e^x x^(1 - b) as x -> inf (DLMF 13.7.2 with Gamma(1) = 1): for real b the factor x^(1 - b)
/// is positive and e^x beats every power of x, so the limit is +inf; for a nonreal b the modulus e^x x^(1 - Re b)
/// still grows without bound but the factor x^(-i Im b) turns forever, so there is no signed infinity; for a symbol b
/// the answer depends on whether b is real, and the call stays unevaluated.
#[test]
fn regularized_1f1_limit_depends_on_the_parameter() {
    assert_eq!(ev("limit(hypergeometric_1f1_regularized(1, 3, x), x -> inf)"), "inf");
    assert_eq!(ev("limit(hypergeometric_1f1_regularized(1, 3/2, x), x -> inf)"), "inf");
    assert_eq!(ev("limit(hypergeometric_1f1_regularized(1, -1/2, x), x -> inf)"), "inf");
    for src in ["limit(hypergeometric_1f1_regularized(1, b, x), x -> inf)", "limit(hypergeometric_1f1_regularized(1, 1 + im, x), x -> inf)"] {
        let v = ev(src);
        assert!(v.starts_with("limit("), "{src}: {v}");
    }
}
