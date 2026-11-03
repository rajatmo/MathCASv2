//! The benchmark cases at their full sizes, with `[k/N pct%]` progress and timings. Ignored by default: run under
//! the guard (`cargo test --release --test large_cases -- --ignored --nocapture`, at most 4 GB).
mod cases;
mod common;
use common::Sys;
use std::time::Instant;

#[test]
#[ignore]
fn full_sizes() {
    type Step = Box<dyn Fn(&mut Sys)>;
    let steps: Vec<(&str, Step)> = vec![
        ("P1 m=30", Box::new(|s| cases::dense_product(s, &["x", "y", "z"], 30))),
        ("P2 m=15", Box::new(|s| cases::dense_product(s, &["x", "y", "z", "t"], 15))),
        ("P2 m=20", Box::new(|s| cases::dense_product(s, &["x", "y", "z", "t"], 20))),
        ("R8 m=30", Box::new(|s| cases::r8(s, 30))),
        ("P4 m=15", Box::new(|s| cases::p4(s, 15))),
        ("P5 d=128", Box::new(|s| cases::p5(s, 128))),
        ("P11 m=10^4", Box::new(|s| cases::p11(s, 10_000))),
        ("P11 m=10^5", Box::new(|s| cases::p11(s, 100_000))),
        ("P11 m=10^6", Box::new(|s| cases::p11(s, 1_000_000))),
        ("P12 m=1000", Box::new(|s| cases::p12(s, 1000))),
    ];
    let n = steps.len();
    for (k, (name, f)) in steps.iter().enumerate() {
        let t = Instant::now();
        f(&mut Sys::new());
        println!("[{}/{n} {}%] {name}: {:?}", k + 1, (k + 1) * 100 / n, t.elapsed());
    }
}
