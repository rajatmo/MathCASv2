//! Monotonicity of the inverse Jacobi functions with a cut on the real line.


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
/// arccs(x|m) with m < 1 and arcds(x|m) with m > 1 are real on the whole line and odd, with the cut at 0 (DLMF
/// 22.15.11, 22.15.12): arccs(x|m) is the integral of ((1 + t^2)(1 - m + t^2))^(-1/2) from x to inf for x > 0, which
/// decreases to K(m) at 0+, and arccs(-x|m) = -arccs(x|m). The values at -0.3 and 0.3 (-1.38, 1.38) show an increase
/// across 0, those at 0.1 and 0.3 (1.61, 1.38) a decrease on x > 0: not monotonic on the line. The cut in the table
/// must not turn the answer into an unevaluated call.
#[test]
fn functions_with_a_cut_at_zero_are_not_monotonic() {
    for src in [
        "function_monotonicity(inverse_jacobi_cs(x, 1/3), x)",
        "function_monotonicity(inverse_jacobi_ds(x, 3), x)",
        "function_monotonicity(inverse_jacobi_cs(x, 1/2), x)",
        "function_monotonicity(inverse_jacobi_ds(x, 2), x)",
    ] {
        assert_eq!(ev(src), "indeterminate", "{src}");
    }
}

/// Functions without a cut keep their proved monotonicity.
#[test]
fn functions_without_a_cut_keep_their_answer() {
    assert_eq!(ev("function_monotonicity(inverse_jacobi_sc(x, 1/3), x)"), "1");
    assert_eq!(ev("function_monotonicity(atan(x), x)"), "1");
}
