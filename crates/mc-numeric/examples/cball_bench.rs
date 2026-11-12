//! Measurements of the complex ball functions (item N0C): time per call of `cball::{exp, log, sin, ...}` at 30,
//! 100, 1000 and 10^4 digits on z = 1.3 + 2.7i (exact parts with 64 random-looking lower bits), against the real ball
//! function of the real part at the same precision (in parentheses; `-` outside its real domain), and `n(f(1 + 2im), d)` through the evaluator.
//!
//! `cargo run --release --example cball_bench`
use mc_arith::num::{Ball, Float, Int, Nat};
use mc_core::Session;
use mc_eval::{EvalConfig, Kernel};
use mc_numeric::cball::{self, CBall};
use mc_numeric::numeval::first_precision;
use mc_numeric::Fun;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

fn part(v: f64, low: u64) -> Ball {
    let (neg, man, exp) = mc_arith::num::float::f64_to_dyadic(v).unwrap();
    let m = Float::new(neg, man, exp);
    Ball::exact(m.add_exact(&Float::new(false, Nat::from_u64(low), m.top() - 53 - 64)))
}

/// Seconds per call of `f`, repeated until 0.2 s have passed.
fn per_call(mut f: impl FnMut()) -> f64 {
    let t0 = Instant::now();
    let mut n = 0u32;
    while n < 3 || t0.elapsed().as_secs_f64() < 0.2 {
        f();
        n += 1;
    }
    t0.elapsed().as_secs_f64() / n as f64
}

fn fmt(s: f64) -> String {
    if s < 1e-3 {
        format!("{:.1} µs", s * 1e6)
    } else {
        format!("{:.2} ms", s * 1e3)
    }
}

fn main() {
    let z = CBall::new(part(1.3, 0x9e37_79b9_7f4a_7c15), part(2.7, 0xbf58_476d_1ce4_e5b9));
    let names = ["exp", "log", "sin", "cos", "tan", "sqrt", "atan", "asin", "acos", "sinh", "cosh", "tanh"];
    let digits = [30u64, 100, 1000, 10_000];
    println!("| function | {} |", digits.map(|d| format!("{d} digits")).join(" | "));
    println!("|---|{}", "---|".repeat(digits.len()));
    for name in names {
        let f = Fun::from_name(name).unwrap();
        let cells: Vec<String> = digits
            .iter()
            .map(|&d| {
                let p = first_precision(d);
                let c = per_call(|| drop(f.cball(&z, p).expect("a value")));
                let real =
                    if f.ball(&z.re, p).is_some() { fmt(per_call(|| drop(f.ball(&z.re, p)))) } else { "-".to_string() };
                format!("{} ({real})", fmt(c))
            })
            .collect();
        println!("| {name} | {} |", cells.join(" | "));
    }
    let p = first_precision(100);
    let w = CBall::new(part(0.5, 1), part(-0.25, 3));
    println!("pow(z, w) at 100 digits: {}", fmt(per_call(|| drop(cball::pow(&z, &w, p)))));
    println!(
        "z^(1/3) at 100 digits: {}",
        fmt(per_call(|| drop(cball::pow_rat(&z, &mc_arith::num::Rat::new(Int::from_i64(1), Int::from_i64(3)), p))))
    );

    // through the evaluator, printing excluded: one first call each (the session may keep results)
    let stop = Arc::new(AtomicBool::new(false));
    let mut s = Session::with_state(mc_core::Config::default(), 1, |h| {
        let mut k = Kernel::new(h, EvalConfig::default());
        mc_engine::arith::install(&mut k, h, &stop);
        mc_numeric::install_with_stop(&mut k, h, &stop);
        k
    });
    for name in ["exp", "log", "sin"] {
        for d in digits {
            let src = format!("n({name}(1 + 2im), {d})");
            let secs = s.step(|h, k| {
                let t = mc_syntax::read(h, &src).expect("valid source")[0];
                let t0 = Instant::now();
                let _ = k.eval(h, t);
                t0.elapsed().as_secs_f64()
            });
            println!("{src}: {}", fmt(secs));
        }
    }
}
