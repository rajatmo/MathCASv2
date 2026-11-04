//! Time and memory of the list producers at 10^7 draws on the assembled engine (package P):
//! `cargo run --release -p mc-random --example producers -- [case] [n]`, under the guard with a memory cap (it
//! reports the peak of the whole process, so run one case per process). Cases: `real` (`random_real(1.0, n)`),
//! `range` (`random_real([-1, 1], n)`), `normal` (`random_normal(0, 1, n)`), `integer` (`random_integer([1, 6], n)`),
//! `matrix` (`random_real(1.0, [n/100, 100])`), `all` (each in turn). Each statement prints its time and a
//! `[k/N pct%]` progress mark.
use mc_bench::{Engine, StopFlag};
use std::time::Instant;

fn main() {
    let case = std::env::args().nth(1).unwrap_or_else(|| "all".to_string());
    let n: u64 = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(10_000_000);
    let all = [
        ("real", format!("x = random_real(1.0, {n}); length(x)")),
        ("range", format!("x = random_real([-1, 1], {n}); length(x)")),
        ("normal", format!("x = random_normal(0, 1, {n}); length(x)")),
        ("integer", format!("x = random_integer([1, 6], {n}); length(x)")),
        ("matrix", format!("x = random_real(1.0, [{}, 100]); length(x)", n / 100)),
    ];
    let steps: Vec<_> = all.iter().filter(|(c, _)| case == "all" || *c == case).collect();
    assert!(!steps.is_empty(), "unknown case {case}");
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    e.eval("seed(2026)", &stop).expect("seed");
    for (k, (what, src)) in steps.iter().enumerate() {
        let t0 = Instant::now();
        let out = match e.eval(src, &stop) {
            Ok(r) => r.printed,
            Err(err) => format!("error: {err:?}"),
        };
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let shown: String = out.chars().take(40).collect();
        println!("[{}/{} {}%] {what:<8} {ms:>10.1} ms   {shown}", k + 1, steps.len(), (k + 1) * 100 / steps.len());
    }
}
