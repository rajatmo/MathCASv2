//! Timing of list workloads at 10^7 elements on the assembled engine, statement by statement, for the packed
//! array measurements (item N4a): `cargo run --release -p mc-lists --example packed_measure [n]`.
//! Run it under the guard with a memory cap; each statement prints its time and a `[k/N pct%]` progress mark.
use mc_api::{Engine, StopFlag};
use std::time::Instant;

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(10_000_000);
    let steps = [
        ("range(n) of integers", format!("u = range({n}); length(u)")),
        ("range(1.0, n) of reals", format!("v = range(1.0, {n}.0); length(v)")),
        ("a second real range", format!("w = range(2.0, {}.0); length(w)", n + 1)),
        ("total(v)", "total(v)".to_string()),
        ("sum(v)", "sum(v)".to_string()),
        ("v[n/2]", format!("v[{}]", n / 2)),
        ("v + w", "length(v + w)".to_string()),
        ("2.0 * v", "length(2.0 * v)".to_string()),
        ("random_real(1.0, n)", format!("x = random_real(1.0, {n}); length(x)")),
    ];
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    for (k, (what, src)) in steps.iter().enumerate() {
        let t0 = Instant::now();
        let out = match e.eval(src, &stop) {
            Ok(r) => r.printed,
            Err(err) => format!("error: {err:?}"),
        };
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let shown: String = out.chars().take(40).collect();
        println!("[{}/{} {}%] {what:<24} {ms:>10.1} ms   {shown}", k + 1, steps.len(), (k + 1) * 100 / steps.len());
    }
}
