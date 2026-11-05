//! Timings of the machine and ball paths (run under the guard: `cargo run --release --example bench`).
//! Prints one line per function and precision with `[k/N pct%]` progress.
use mc_arith::num::{Int, Rat};
use mc_arith::Real;
use mc_special::functions::Special;
use std::time::Instant;

/// A function, its machine arguments and its exact arguments (as numerator, denominator).
type Case = (Special, [f64; 2], [(i64, i64); 2]);

fn main() {
    let cases: [Case; 8] = [
        (Special::Gamma, [7.3, 0.0], [(22, 3), (0, 1)]),
        (Special::LogGamma, [7.3, 0.0], [(22, 3), (0, 1)]),
        (Special::Digamma, [7.3, 0.0], [(22, 3), (0, 1)]),
        (Special::Polygamma, [2.0, 7.3], [(2, 1), (22, 3)]),
        (Special::Beta, [2.5, 3.7], [(5, 2), (37, 10)]),
        (Special::Zeta, [3.3, 0.0], [(33, 10), (0, 1)]),
        (Special::BesselK, [1.0, 2.5], [(1, 1), (5, 2)]),
        (Special::BesselI, [1.0 / 3.0, 2.5], [(1, 3), (5, 2)]),
    ];
    let digits = [15u64, 50, 300, 1000];
    let total = cases.len();
    for (i, (f, m, q)) in cases.iter().enumerate() {
        let args = &m[..f.arity()];
        let reps = 20_000;
        let t = Instant::now();
        let mut s = 0.0;
        for k in 0..reps {
            let mut a = args.to_vec();
            let last = a.len() - 1;
            a[last] += k as f64 * 1e-9;
            if let mc_numeric::fun::Mach::Real(v) = f.machine_value(&a) {
                s += v;
            }
        }
        let mach = t.elapsed().as_secs_f64() / reps as f64;
        let mut line = format!("{:<10} machine {:>7.2} us", f.name(), mach * 1e6);
        let exact: Vec<Real> =
            q[..f.arity()].iter().map(|&(n, d)| Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))).collect();
        for d in digits {
            let p = (d as f64 * std::f64::consts::LOG2_10).ceil() as u64;
            let n = if d >= 1000 { 1 } else { 5 };
            let t = Instant::now();
            for _ in 0..n {
                assert!(f.ball_value(&exact, p).is_some());
            }
            line += &format!("  {d} digits {:>9.3} ms", t.elapsed().as_secs_f64() * 1e3 / n as f64);
        }
        println!("[{}/{total} {}%] {line}  (checksum {s:.3e})", i + 1, 100 * (i + 1) / total);
    }
}
