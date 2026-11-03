//! Evaluator speed and depth on the assembled engine: the cases package 1a measures (R1 with the memo off, the
//! interpreted `while` loop, R12, R2, a deep linear recursion), each a fresh engine per run, median of the runs.
//! Prints one line per case on stdout and `[k/N pct%]` progress on stderr; exits with status 1 when a result is
//! wrong.
//!
//! Options (lists take `+`, since the guard script splits arguments on commas): `--cases R1+LOOP+R12+R2+DEEP`,
//! `--runs N` (default 3), `--threads N` (default 2), `--size CASE=N` (repeatable through `+`, e.g.
//! `--size R2=100000+DEEP=1000000`).
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::EvalConfig;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// One case: its name, default size, the setup source, the measured source for a size, the expected result for a
/// size, the unit of work per size (for the per-call figure) and whether the memo is on.
struct Case {
    name: &'static str,
    size: u64,
    setup: &'static str,
    measure: fn(u64) -> String,
    expect: fn(u64) -> Option<String>,
    work: fn(u64) -> u64,
    unit: &'static str,
    memo: bool,
}

/// fib(n) by the doubling-free loop, as text (for the expected values of R1 and R2).
fn fib(n: u64) -> String {
    let (mut a, mut b) = (0u128, 1u128);
    for _ in 0..n {
        (a, b) = (b, a + b);
    }
    a.to_string()
}

/// Calls of the naive recursive fib(n): 2 fib(n + 1) - 1.
fn fib_calls(n: u64) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n + 1 {
        (a, b) = (b, a + b);
    }
    2 * a - 1
}

/// The longest Collatz chain below `m` (R12's expected values).
fn collatz(m: u64) -> Option<String> {
    let best = (1..=m)
        .map(|k| {
            let (mut c, mut s) = (k, 0u64);
            while c != 1 {
                c = if c % 2 == 0 { c / 2 } else { 3 * c + 1 };
                s += 1;
            }
            s
        })
        .max()?;
    Some(best.to_string())
}

const R12: &str = "fn collatz_steps(k: Int) = {
  var c = k
  var s = 0
  while c != 1 {
    c = if mod(c, 2) == 0 then c/2 else 3c + 1
    s = s + 1
  }
  s
}";

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "R1",
            size: 25,
            setup: "rule fib(0) = 0\nrule fib(1) = 1\nrule fib(k_: Int) := fib(k - 1) + fib(k - 2)",
            measure: |n| format!("fib({n})"),
            expect: |n| (n <= 90).then(|| fib(n)),
            work: fib_calls,
            unit: "call",
            memo: false,
        },
        Case {
            name: "LOOP",
            size: 1_000_000,
            setup: "",
            measure: |n| format!("{{ var i = 0; while i < {n} {{ i = i + 1 }}; i }}"),
            expect: |n| Some(n.to_string()),
            work: |n| n,
            unit: "iteration",
            memo: true,
        },
        Case {
            name: "R12",
            size: 100_000,
            setup: R12,
            measure: |m| format!("range({m}) |> map(collatz_steps, _) |> max"),
            expect: |m| if m <= 1_000_000 { collatz(m) } else { None },
            work: |m| m,
            unit: "element",
            memo: true,
        },
        Case {
            name: "R2",
            size: 10_000,
            setup: "rule fib(0) = 0\nrule fib(1) = 1\nrule fib(k_: Int) := { let v = fib(k - 1) + fib(k - 2); fib(k) = v; v }",
            // the property of the case: Cassini's identity
            measure: |m| format!("fib({m})^2 - fib({m} - 1)*fib({m} + 1) == (-1)^({m} - 1)"),
            expect: |_| Some("true".into()),
            work: |m| m,
            unit: "level",
            memo: true,
        },
        Case {
            name: "DEEP",
            size: 1_000_000,
            setup: "rule deep(0) = 0\nrule deep(n_: Int) := 1 + deep(n - 1)",
            measure: |n| format!("deep({n})"),
            expect: |n| Some(n.to_string()),
            work: |n| n,
            unit: "level",
            memo: true,
        },
    ]
}

/// One run of a case in a fresh engine: the time of the measured statement and its printed value.
fn run(c: &Case, size: u64, threads: usize) -> Result<(Duration, String), String> {
    let d = EngineConfig::default();
    let mut e = CoreEngine::with_config(EngineConfig { threads, eval: EvalConfig::default(), ..d });
    let stop = StopFlag::new();
    if !c.memo {
        e.configure("memo", "off").map_err(|e| e.to_string())?;
    }
    if !c.setup.is_empty() {
        e.eval(c.setup, &stop).map_err(|e| format!("setup: {e}"))?;
    }
    let src = (c.measure)(size);
    let t0 = Instant::now();
    let v = e.eval(&src, &stop).map_err(|e| e.to_string())?;
    Ok((t0.elapsed(), v.printed))
}

fn main() -> ExitCode {
    let mut names: Vec<String> = vec![];
    let mut runs = 3usize;
    let mut threads = 2usize;
    let mut sizes: Vec<(String, u64)> = vec![];
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let v = args.next().unwrap_or_default();
        match a.as_str() {
            "--cases" => names = v.split('+').map(str::to_string).collect(),
            "--runs" => runs = v.parse().expect("--runs N"),
            "--threads" => threads = v.parse().expect("--threads N"),
            "--size" => {
                for kv in v.split('+') {
                    let (k, n) = kv.split_once('=').expect("--size CASE=N");
                    sizes.push((k.to_string(), n.parse().expect("--size CASE=N")));
                }
            }
            _ => panic!("unknown option {a}"),
        }
    }
    let all = cases();
    let chosen: Vec<&Case> = all.iter().filter(|c| names.is_empty() || names.iter().any(|n| n == c.name)).collect();
    let total = chosen.len() * runs;
    let mut done = 0;
    let mut ok = true;
    for c in chosen {
        let size = sizes.iter().find(|(k, _)| k == c.name).map_or(c.size, |&(_, n)| n);
        let mut times = vec![];
        for _ in 0..runs {
            match run(c, size, threads) {
                Ok((t, v)) => {
                    if let Some(want) = (c.expect)(size) {
                        if v != want {
                            println!("{} {size}: wrong result {v} (expected {want})", c.name);
                            ok = false;
                        }
                    }
                    times.push(t);
                }
                Err(e) => {
                    println!("{} {size}: error {e}", c.name);
                    ok = false;
                }
            }
            done += 1;
            eprintln!("[{done}/{total} {}%]", done * 100 / total);
        }
        times.sort();
        if let Some(&m) = times.get(times.len() / 2) {
            let per = m.as_secs_f64() * 1e9 / (c.work)(size) as f64;
            println!(
                "{} {size}: median {:.1} ms of {} runs, {per:.0} ns per {} ({threads} threads)",
                c.name,
                m.as_secs_f64() * 1e3,
                times.len(),
                c.unit
            );
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
