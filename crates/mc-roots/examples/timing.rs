//! Timing of representative problems through the engine (release build, busy machine): median wall time of 5 runs
//! per problem, each in a fresh engine (the memo would answer a repeated evaluation), start-up excluded. Prints
//! `[k/N pct%]` progress.
//!
//! `cargo run --release --example timing`
use mc_api::{Engine, StopFlag};
use std::time::Instant;

const PROBLEMS: &[(&str, &str)] = &[
    ("Newton, one variable", "find_root(cos(x) == x, [x, 1])"),
    ("secant", "find_root(x^3 - 2*x - 5, [x, 2, 3])"),
    ("Brent", "find_root(x^3 - 2*x - 5, [x, 2, 2, 3])"),
    ("complex Newton", "find_root(x^3 - 1, [x, -1 + im])"),
    ("system of 3 (Newton)", "find_root([3*x - cos(y*z) - 1/2, x^2 - 81*(y + 0.1)^2 + sin(z) + 1.06, exp(-x*y) + 20*z + (10*pi - 3)/3], [[x, 0.1], [y, 0.1], [z, -0.1]])"),
    ("system of 2 (Broyden)", "find_root([x^2 + y^2 - 4, x*y - 1], [[x, 2], [y, 0.5]], method: broyden)"),
    ("Rosenbrock (BFGS)", "find_minimum((1 - x)^2 + 100*(y - x^2)^2, [[x, -1.2], [y, 1]])"),
    ("Brent minimum", "find_minimum(x^4 - 3*x^2 + x, [x, 1])"),
    ("verified, 100 digits", "find_root(x*exp(x) == 1, [x, 1], digits: 100, verify: true)"),
    ("verified, 1000 digits", "find_root(cos(x) == x, [x, 1], digits: 1000, verify: true)"),
    ("verified, 10000 digits", "find_root(x^2 - 2, [x, 1], digits: 10000, verify: true)"),
];

const RUNS: usize = 5;

fn main() {
    let stop = StopFlag::new();
    let total = PROBLEMS.len() * RUNS;
    let mut done = 0;
    let mut rows = Vec::new();
    for (name, src) in PROBLEMS {
        let mut times = Vec::with_capacity(RUNS);
        for _ in 0..RUNS {
            let mut e = mc_engine::CoreEngine::new();
            let t = Instant::now();
            let v = e.eval(src, &stop).expect("evaluates");
            times.push(t.elapsed().as_secs_f64());
            assert!(!v.printed.starts_with("failure"), "{src}: {}", v.printed);
            done += 1;
            eprintln!("[{done}/{total} {}%] {name}", done * 100 / total);
        }
        times.sort_by(f64::total_cmp);
        rows.push((name, times[RUNS / 2]));
    }
    println!("| problem | median |");
    println!("|---|---|");
    for (name, t) in rows {
        println!("| {name} | {:.2} ms |", t * 1e3);
    }
}
