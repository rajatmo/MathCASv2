//! I2's cases on `mc-core`: each case in each configuration, checked against the sequential reference (one thread,
//! no splitting, no memo, no collection). Prints a Markdown table on stdout and `[k/N pct%]` progress on stderr;
//! exits with status 1 on a mismatch.
//!
//! Options (lists take `+`, since the guard script splits arguments on commas): `--quick` (test sizes),
//! `--cases R4+R5+R13+R14+S7+R14e+R14ev+MR+MRF`, `--threads 1+2+4`, `--stress` (collect at every safe point and verify),
//! `--runs N` (median of N, default 3).
use mc_core::Config;
use mc_eval::workloads::{self, CondKind, Out, Run};
use mc_eval::{EvalConfig, Split};
use std::process::ExitCode;
use std::time::Duration;

type Case = (&'static str, [usize; 2], fn(Run, usize) -> Out);

fn cases() -> Vec<Case> {
    vec![
        ("R4", [1000, 10_000], |r, n| workloads::r4(r, 10_000, n)),
        ("R5", [10_000, 1_000_000], |r, n| workloads::r5(r, 50_000, n)),
        ("R13", [10_000, 1_000_000], |r, n| workloads::r13(r, 1000, n)),
        ("R14", [100_000, 1_000_000], workloads::r14),
        ("S7", [100_000, 1_000_000], workloads::s7),
        ("R14e", [20_000, 200_000], |r, n| workloads::r14e(r, n, CondKind::Native)),
        ("R14ev", [20_000, 200_000], |r, n| workloads::r14e(r, n, CondKind::Eval)),
        ("MR", [20_000, 200_000], |r, n| workloads::mr(r, n, None)),
        ("MRF", [500, 3000], |r, n| workloads::mr_fuzz(r, n, 20)),
    ]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut quick = false;
    let mut stress = false;
    let mut runs = 3;
    let mut threads = vec![1usize, 2, 4];
    let mut only: Option<Vec<String>> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--quick" => quick = true,
            "--stress" => stress = true,
            "--runs" => runs = it.next().and_then(|v| v.parse().ok()).expect("--runs needs a number"),
            "--threads" => {
                threads = it.next().expect("--threads needs a list").split('+').map(|s| s.parse::<usize>().expect("threads").clamp(1, 4)).collect()
            }
            "--cases" => only = Some(it.next().expect("--cases needs a list").split('+').map(str::to_string).collect()),
            other => panic!("unknown option {other}"),
        }
    }
    let heap = if stress { Config::stress() } else { Config::default() };
    let cs: Vec<Case> = cases().into_iter().filter(|c| only.as_ref().is_none_or(|o| o.iter().any(|n| n == c.0))).collect();
    // (name, run) per configuration; the first is the reference
    let mut cfgs: Vec<(String, Run)> = vec![("ref".into(), Run::reference())];
    cfgs.push(("seq".into(), Run { heap, threads: 1, eval: EvalConfig { split: Split::Never, ..EvalConfig::default() } }));
    for &t in &threads {
        cfgs.push((format!("lazy {t}"), Run { heap, threads: t, eval: EvalConfig::default() }));
    }
    let total = cs.len() * cfgs.len();
    let mut done = 0;
    let mut bad = 0;
    println!("| case | n | config | ms | speedup vs seq | splits | redos | holes | fuel | check | note |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for (name, sizes, f) in &cs {
        let n = sizes[(!quick) as usize];
        let mut reference = None;
        let mut t_seq = None;
        for (cname, run) in &cfgs {
            done += 1;
            eprintln!("[{done}/{total} {}%] {name} n={n} {cname}", 100 * done / total);
            let mut times = vec![];
            let mut last: Option<Out> = None;
            for _ in 0..runs {
                let o = f(*run, n);
                times.push(o.time);
                if let Some(p) = &last {
                    if p.check != o.check {
                        bad += 1;
                        eprintln!("{name} {cname}: two runs disagree");
                    }
                }
                last = Some(o);
            }
            times.sort();
            let t = times[times.len() / 2];
            let o = last.expect("a run");
            let want = *reference.get_or_insert(o.check);
            if cname == "seq" {
                t_seq = Some(t);
            }
            let verdict = if o.check == want {
                "ok"
            } else {
                bad += 1;
                "MISMATCH"
            };
            let speedup = t_seq.map_or(String::new(), |s| format!("{:.2}", s.as_secs_f64() / t.as_secs_f64()));
            let st = o.stats;
            println!(
                "| {name} | {n} | {cname} | {:.1} | {speedup} | {} | {} | {} | {} | {verdict} | {} |",
                ms(t),
                st.splits,
                st.redos,
                st.holes,
                st.fuel,
                o.note
            );
        }
    }
    if bad > 0 {
        eprintln!("{bad} mismatches");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
