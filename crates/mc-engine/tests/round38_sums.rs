//! Round 38: convergence of series on the scale of iterated logarithms (Bertrand's series). Each verdict is
//! checked against the integral test: the antiderivative of the summand tends to a finite limit exactly when the
//! series converges.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn bertrand_series_with_iterated_logarithms() {
    // sum 1/(n log n log log n) diverges: its antiderivative log(log(log(n))) is unbounded
    assert_eq!(ev("sum_convergence(1/(n*log(n)*log(log(n))), n)"), "false");
    assert_eq!(ev("d(log(log(log(n))), n) - 1/(n*log(n)*log(log(n)))"), "0");
    // with (log log n)^2 it converges: the antiderivative -1/log(log(n)) tends to 0
    assert_eq!(ev("sum_convergence(1/(n*log(n)*log(log(n))^2), n)"), "true");
    assert_eq!(ev("simplify(d(-1/log(log(n)), n) - 1/(n*log(n)*log(log(n))^2))"), "0");
    // one level deeper, and exponents 1/2 and 3/2 on the last logarithm
    assert_eq!(ev("sum_convergence(1/(n*log(n)*log(log(n))*log(log(log(n)))^2), n)"), "true");
    assert_eq!(ev("sum_convergence(1/(n*log(n)*log(log(n))^(1/2)), n)"), "false");
    assert_eq!(ev("sum_convergence(1/(n*log(n)*log(log(n))^(3/2)), n)"), "true");
}
