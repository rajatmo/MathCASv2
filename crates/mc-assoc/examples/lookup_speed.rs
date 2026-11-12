//! Association speed on the assembled engine: building an association of n keys from a list of rules, then n
//! lookups in it, for each size of `--sizes` (default `1000+10000+100000`), a fresh engine per run, median of the
//! runs. The lookups use three forms: `lookup(a, i)` with integer keys, `a[k(i)]` (part access) with compound keys,
//! and `key_exists(a, i)` for absent keys; a loop without lookups is the baseline. A per-lookup time that stays
//! flat as n grows shows that lookups are not linear. Prints one line per size and form on stdout and `[k/N pct%]`
//! progress on stderr; exits with status 1 when a result is wrong.
//!
//! Options (lists take `+`, since the guard script splits arguments on commas): `--sizes 1000+100000`, `--runs N`
//! (default 3), `--threads N` (default 2).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// One measured statement: its name, its source for size n, and its expected result.
struct Form {
    name: &'static str,
    src: fn(u64) -> String,
    expect: fn(u64) -> String,
}

const FORMS: &[Form] = &[
    Form {
        name: "build assoc(map(i => i -> 2i, range(n)))",
        src: |_| "a = assoc(map(i => i -> 2*i, range(n)))\nlength(a)".into(),
        expect: |n| n.to_string(),
    },
    Form {
        name: "n x lookup(a, i)",
        src: |_| "{ var s = 0; for i in range(n) { s = s + lookup(a, i) }; s }".into(),
        expect: |n| (n * (n + 1)).to_string(),
    },
    Form {
        name: "build assoc(map(i => k(i) -> i, range(n)))",
        src: |_| "b = assoc(map(i => k(i) -> i, range(n)))\nlength(b)".into(),
        expect: |n| n.to_string(),
    },
    Form {
        name: "n x b[k(i)]",
        src: |_| "{ var s = 0; for i in range(n) { s = s + b[k(i)] }; s }".into(),
        expect: |n| (n * (n + 1) / 2).to_string(),
    },
    Form {
        name: "n x key_exists(a, -i) (absent)",
        src: |_| "{ var c = 0; for i in range(n) { if key_exists(a, -i) then c = c + 1 }; c }".into(),
        expect: |_| "0".into(),
    },
    Form {
        name: "n x the same loop without a lookup",
        src: |_| "{ var s = 0; for i in range(n) { s = s + 2*i }; s }".into(),
        expect: |n| (n * (n + 1)).to_string(),
    },
];

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let sizes: Vec<u64> = arg(&args, "--sizes")
        .unwrap_or("1000+10000+100000")
        .split('+')
        .map(|s| s.parse().expect("a size"))
        .collect();
    let runs: usize = arg(&args, "--runs").map_or(3, |s| s.parse().expect("a run count"));
    let threads: usize = arg(&args, "--threads").map_or(2, |s| s.parse().expect("a thread count"));
    let total = sizes.len() * runs;
    let mut done = 0;
    let mut ok = true;
    for &n in &sizes {
        let mut times: Vec<Vec<Duration>> = vec![Vec::new(); FORMS.len()];
        for _ in 0..runs {
            let mut e = CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
            let stop = StopFlag::new();
            e.eval(&format!("n = {n}"), &stop).expect("the size is set");
            for (f, t) in FORMS.iter().zip(&mut times) {
                let start = Instant::now();
                let r = e.eval(&(f.src)(n), &stop).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"));
                t.push(start.elapsed());
                if r != (f.expect)(n) {
                    eprintln!("{}, n = {n}: got {r}, expected {}", f.name, (f.expect)(n));
                    ok = false;
                }
            }
            done += 1;
            eprintln!("[{done}/{total} {}%]", done * 100 / total);
        }
        for (f, t) in FORMS.iter().zip(&mut times) {
            t.sort();
            let median = t[t.len() / 2];
            println!(
                "n = {n:>7}  {:<44} {:>9.2} ms  {:>8.0} ns per key",
                f.name,
                median.as_secs_f64() * 1e3,
                median.as_secs_f64() * 1e9 / n as f64
            );
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
