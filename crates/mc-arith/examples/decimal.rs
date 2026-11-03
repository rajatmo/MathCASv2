//! Timing of the decimal conversions of huge integers (benchmark N9 prints numbers of tens of millions of digits).
//!
//! `cargo run --release --example decimal [digits ...]` (default 10^6 and 10^7): for each size, an integer of about
//! that many digits (a power of 7) is converted to decimal and back, with the median of a few runs. Run it through
//! the guard: 10^7 digits needs about 1 GB.
use mc_arith::num::Nat;
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let sizes: Vec<u64> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let sizes = if sizes.is_empty() { vec![1_000_000, 10_000_000] } else { sizes };
    let steps = sizes.len();
    for (i, &d) in sizes.iter().enumerate() {
        // 7^e has about e log10(7) digits
        let e = (d as f64 / 7f64.log10()) as u64;
        let x = Nat::from_u64(7).pow_u64(e);
        let reps = if d >= 5_000_000 { 3 } else { 5 };
        let (mut to, mut from) = (vec![], vec![]);
        let mut text = String::new();
        for _ in 0..reps {
            let t = Instant::now();
            text = x.to_decimal();
            to.push(t.elapsed().as_secs_f64());
            let t = Instant::now();
            let back = Nat::from_decimal(&text).expect("digits");
            from.push(t.elapsed().as_secs_f64());
            assert!(back == x, "round trip at {d} digits");
        }
        println!(
            "[{}/{steps} {}%] {} digits: to_decimal {:.3} s, from_decimal {:.3} s (median of {reps})",
            i + 1,
            100 * (i + 1) / steps,
            text.len(),
            median(to),
            median(from)
        );
    }
}
