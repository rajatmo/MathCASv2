//! Times the compiled Mandelbrot escape-count grid (`grid(n)`), the numeric compiler's benchmark for powers and sums
//! or products of machine reals, and checks it against the interpreter at a size the interpreter runs quickly.
//!
//! ```text
//! cargo run --release -p mc-jit --example mandelbrot -- [n] [reps]      (defaults: 300, 5)
//! ```
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

/// The program: the escape count of one point, and the total over an n x n grid of [-2, 1] x [-1.5, 1.5].
pub const PROGRAM: &str = "\
@compiled fn escape(cr: Real, ci: Real) = { var x = 0.0; var y = 0.0; var k = 0; while k < 100 and x*x + y*y <= 4.0 { let t = x*x - y*y + cr; y = 2.0*x*y + ci; x = t; k = k + 1 }; k }
@compiled fn grid(n: Int) = { var total = 0; for i in range(n) { for j in range(n) { total = total + escape(-2.0 + 3.0*i/n, -1.5 + 3.0*j/n) } }; total }";

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(300);
    let reps: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(5).max(1);
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    for def in PROGRAM.lines() {
        assert_eq!(eval(&mut e, def), "null", "{def}");
    }
    println!("escape: {}", eval(&mut e, "compiled_info(escape)"));
    // every repetition runs the compiled loop instead of finding the result among the memoised normal forms
    e.configure("memo", "off").expect("the memo setting");

    // the interpreter (the same definitions, never compiled) agrees at a small size
    let mut plain = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    for def in PROGRAM.lines() {
        eval(&mut plain, def.trim_start_matches("@compiled "));
    }
    let (want, got) = (eval(&mut plain, "grid(20)"), eval(&mut e, "grid(20)"));
    assert_eq!(got, want, "grid(20): compiled {got}, interpreted {want}");
    println!("grid(20) = {got} (interpreter agrees)");

    let mut times: Vec<Duration> = Vec::with_capacity(reps);
    let mut value = String::new();
    for k in 0..reps {
        let t0 = Instant::now();
        value = eval(&mut e, &format!("grid({n})"));
        times.push(t0.elapsed());
        println!("[{}/{reps} {}%] grid({n}) = {value} in {:.3} s", k + 1, 100 * (k + 1) / reps, times[k].as_secs_f64());
    }
    times.sort();
    println!("grid({n}) = {value}: median {:.3} s, best {:.3} s over {reps} runs", times[reps / 2].as_secs_f64(), times[0].as_secs_f64());
}
