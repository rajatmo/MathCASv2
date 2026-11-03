//! Measurements of the package (busy machine, release build): N1 (`n(pi, d)` through the evaluator, printing
//! included), N12, and the elementary functions at 10^3 to 10^5 digits. Prints `[k/N pct%]` progress.
//!
//! `cargo run --release --example bench [-- quick]`
use mc_arith::num::{Ball, Float, Nat};
use mc_core::Session;
use mc_eval::{EvalConfig, Kernel};
use mc_numeric::consts::Const;
use mc_numeric::Fun;
use mc_syntax::Printer;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

fn session() -> Session<Kernel> {
    let stop = Arc::new(AtomicBool::new(false));
    Session::with_state(mc_core::Config::default(), 1, |h| {
        let mut k = Kernel::new(h, EvalConfig::default());
        mc_engine::arith::install(&mut k, h, &stop);
        mc_numeric::install_with_stop(&mut k, h, &stop);
        k
    })
}

/// Evaluate `src` (one statement) and print it; the seconds for evaluation and for printing, and the text.
fn run(s: &mut Session<Kernel>, src: &str) -> (f64, f64, String) {
    s.step(|h, k| {
        let t = mc_syntax::read(h, src).expect("valid source")[0];
        let t0 = Instant::now();
        let v = k.eval(h, t).value;
        let t1 = Instant::now();
        let text = Printer::with_numbers(h, Box::new(mc_engine::format::ArithNumbers)).expr(v);
        (t1.duration_since(t0).as_secs_f64(), t1.elapsed().as_secs_f64(), text)
    })
}

fn main() {
    let quick = std::env::args().any(|a| a == "quick");
    let pi_sizes: &[u64] = if quick { &[10_000, 100_000] } else { &[10_000, 100_000, 1_000_000] };
    let fn_digits: &[u64] = if quick { &[1_000, 10_000] } else { &[1_000, 10_000, 100_000] };
    let total = pi_sizes.len() + 1 + fn_digits.len() * Fun::ALL.len();
    let mut k = 0;
    let mut progress = |what: &str| {
        k += 1;
        eprintln!("[{k}/{total} {}%] {what}", 100 * k / total);
    };

    println!("| case | digits | evaluate (s) | print (s) | check |");
    println!("|---|---|---|---|---|");
    for &d in pi_sizes {
        // a fresh session: the constant cache is per process, so the larger sizes come after the smaller ones
        let mut s = session();
        let (te, tp, text) = run(&mut s, &format!("n(pi, {d})"));
        let ok = text.starts_with("3.14159265358979323846") && text.len() as u64 > d;
        println!(
            "| N1 n(pi, d) | {d} | {te:.3} | {tp:.3} | {} printed digits, prefix {} |",
            text.len() - 1,
            if ok { "ok" } else { "WRONG" }
        );
        progress(&format!("N1 at {d}"));
    }
    let mut s = session();
    let n12 = "[exp(pi*sqrt(163)) < 640320^3 + 744, pi^4 + pi^5 < exp(6), sqrt(1000001) - 1000 < 1/2000]";
    let (te, _, text) = run(&mut s, n12);
    println!("| N12 | - | {te:.5} | - | {text} |");
    progress("N12");

    println!();
    println!("Ball functions at exact argument 0.7 (log at 2.7, acos/asin at 0.3), seconds per call:");
    println!();
    print!("| digits |");
    for f in Fun::ALL {
        print!(" {} |", f.name());
    }
    println!();
    println!("|---|{}", "---|".repeat(Fun::ALL.len()));
    for &d in fn_digits {
        let p = mc_arith::text::digits_to_bits(d) + 16;
        // warm the constant caches so that the times are the functions' own
        Const::Pi.ball(4 * p);
        Const::E.ball(4 * p);
        print!("| {d} |");
        for f in Fun::ALL {
            let x = match f {
                Fun::Log => {
                    Ball::exact(Float::new(false, Nat::from_u64(27), 0)).div(&Ball::from_i64(10), p).expect("nonzero")
                }
                Fun::Asin | Fun::Acos => Ball::from_ratio(3, 10, p),
                _ => Ball::from_ratio(7, 10, p),
            };
            let t0 = Instant::now();
            let r = f.ball(&x, p).expect("in the domain");
            let t = t0.elapsed().as_secs_f64();
            assert!(r.rel_bits() > p as f64 - 16.0, "{}: {} bits", f.name(), r.rel_bits());
            print!(" {t:.4} |");
            progress(&format!("{} at {d}", f.name()));
        }
        println!();
    }
}
