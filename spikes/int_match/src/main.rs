//! I2 driver: every case in every configuration, each run on a fresh heap and evaluator; every configuration's
//! check must equal the baseline (the sequential evaluator without memo, one thread); spike 2's cases are also run
//! with spike 2's own sequential evaluator in this process, whose check must equal ours and whose time is the
//! single-thread baseline.
//!
//! usage: int_match [--quick | --full] [--runs N] [case filter...]
//!   --quick  small sizes, one run (seconds)
//!   --full   benchmark sizes, three runs (median), under 3 minutes on a busy machine
use int_match::eval::{Config, Split};
use int_match::index::IndexMode;
use int_match::memo::MemoKind;
use int_match::work::{self, CondKind, Out};
use int_match::combine;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::io::Write;
use std::time::Duration;

type RunFn = Box<dyn Fn(&ThreadPool, Config, usize, bool) -> Out>;

struct Case {
    id: &'static str,
    /// (quick, full)
    sizes: [usize; 2],
    /// Memo setting for the non-reference configurations.
    memo: bool,
    /// Condition reads untracked: memoised configurations are expected to go stale.
    untracked: bool,
    run: RunFn,
    /// Spike 2's case id and variant, for the baseline and cross-check.
    s2: Option<(&'static str, &'static str)>,
}

fn cases() -> Vec<Case> {
    let fuzz: RunFn = Box::new(|p, c, n, _| {
        let mut o = work::mr(p, c, n, Some(1));
        for s in 2..=20 {
            let x = work::mr(p, c, n, Some(s));
            o.check = combine(o.check, x.check);
            o.phases.extend(x.phases);
            o.stats.splits += x.stats.splits;
            o.stats.redos += x.stats.redos;
        }
        o.note = "20 random layouts".into();
        o
    });
    vec![
        Case { id: "R4 AC rules", sizes: [1000, 10_000], memo: true, untracked: false, run: Box::new(work::r4), s2: Some(("R4 AC rules", "indexed/smart")) },
        Case { id: "R5 literal rules", sizes: [5000, 50_000], memo: false, untracked: false, run: Box::new(work::r5), s2: Some(("R5 literal rules", "indexed")) },
        Case { id: "R13 arg-head rules", sizes: [100, 1000], memo: false, untracked: false, run: Box::new(work::r13), s2: Some(("R13 arg-head rules", "arg-head")) },
        Case { id: "R14 read sets", sizes: [100_000, 1_000_000], memo: true, untracked: false, run: Box::new(work::r14), s2: Some(("R14 read sets", "memo/mask")) },
        Case { id: "S7 cases", sizes: [100_000, 1_000_000], memo: false, untracked: false, run: Box::new(work::s7), s2: Some(("S7 cases", "smart")) },
        Case {
            id: "R14e native cond",
            sizes: [20_000, 200_000],
            memo: true,
            untracked: false,
            run: Box::new(|p, c, n, _| work::r14e(p, c, n, CondKind::Native)),
            s2: None,
        },
        Case {
            id: "R14e eval cond",
            sizes: [20_000, 200_000],
            memo: true,
            untracked: false,
            run: Box::new(|p, c, n, _| work::r14e(p, c, n, CondKind::Eval)),
            s2: None,
        },
        Case {
            id: "R14e eval cond untracked",
            sizes: [20_000, 20_000],
            memo: true,
            untracked: true,
            run: Box::new(|p, c, n, _| work::r14e(p, c, n, CondKind::Eval)),
            s2: None,
        },
        Case { id: "MR map + redefinitions", sizes: [20_000, 200_000], memo: true, untracked: false, run: Box::new(|p, c, n, _| work::mr(p, c, n, None)), s2: None },
        Case { id: "MR fuzz", sizes: [500, 3000], memo: true, untracked: false, run: fuzz, s2: None },
    ]
}

#[derive(Clone, Copy)]
struct Variant {
    name: &'static str,
    split: Split,
    threads: usize,
    /// None: the case's memo setting.
    memo: Option<bool>,
    bits: Option<u32>,
    kind: MemoKind,
    index: IndexMode,
}

const fn v(name: &'static str, split: Split, threads: usize) -> Variant {
    Variant { name, split, threads, memo: None, bits: None, kind: MemoKind::Dense, index: IndexMode::LockFree }
}

fn variants() -> Vec<Variant> {
    vec![
        Variant { memo: Some(false), ..v("ref", Split::Never, 1) },
        v("seq", Split::Never, 1),
        v("lazy", Split::Lazy, 1),
        v("lazy", Split::Lazy, 2),
        v("lazy", Split::Lazy, 4),
        v("always", Split::Always, 4),
        Variant { kind: MemoKind::Hashed, ..v("seq hashed-memo", Split::Never, 1) },
        Variant { kind: MemoKind::Hashed, ..v("lazy hashed-memo", Split::Lazy, 4) },
        Variant { bits: Some(4), kind: MemoKind::Hashed, ..v("lazy tiny-memo", Split::Lazy, 4) },
        Variant { index: IndexMode::Locked, ..v("lazy locked-index", Split::Lazy, 4) },
    ]
}

fn pool(threads: usize) -> ThreadPool {
    ThreadPoolBuilder::new().num_threads(threads).stack_size(64 << 20).build().unwrap()
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let quick = args.iter().any(|a| a == "--quick");
    let runs: usize = args
        .iter()
        .position(|a| a == "--runs")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(if quick { 1 } else { 3 });
    let filters: Vec<String> =
        args.iter().filter(|a| !a.starts_with("--") && a.parse::<u64>().is_err()).map(|a| a.to_lowercase()).collect();
    let h = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || run_all(quick, runs, &filters))
        .unwrap();
    let bad = h.join().unwrap();
    if bad > 0 {
        eprintln!("{bad} MISMATCH rows");
        std::process::exit(1);
    }
}

fn run_all(quick: bool, runs: usize, filters: &[String]) -> u32 {
    let pools: Vec<(usize, ThreadPool)> = [1, 2, 4].iter().map(|&t| (t, pool(t))).collect();
    let pool_of = |t: usize| &pools.iter().find(|(n, _)| *n == t).unwrap().1;
    let cs: Vec<Case> = cases()
        .into_iter()
        .filter(|c| filters.is_empty() || filters.iter().any(|f| c.id.to_lowercase().contains(f)))
        .collect();
    let vs = variants();
    let total = cs.iter().map(|c| vs.iter().filter(|v| (v.bits.is_none() && v.kind == MemoKind::Dense) || c.memo).count() + c.s2.is_some() as usize).sum::<usize>();
    let mut done = 0;
    let mut bad = 0;
    println!("| case | n | config | threads | ms | speedup vs seq | vs spike 2 | splits | redos | check | note |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for c in &cs {
        let n = c.sizes[quick as usize ^ 1];
        let mut progress = |what: &str| {
            done += 1;
            eprintln!("[{done}/{total} {:.0}%] {} n={n} {what}", 100.0 * done as f64 / total as f64, c.id);
            let _ = std::io::stderr().flush();
        };
        // spike 2's sequential evaluator, same process
        let s2 = c.s2.map(|(id, variant)| {
            progress("spike 2");
            let case = matcher::work::cases().into_iter().find(|k| k.id == id).expect("spike 2 case");
            let opts = matcher::work::Opts { quick, limit: Duration::from_secs(60) };
            let mut times = vec![];
            let mut check = 0;
            for _ in 0..runs {
                let o = (case.run)(variant, n, &opts);
                times.push(o.phases.iter().filter(|(p, _)| *p != "define").map(|(_, d)| *d).sum::<Duration>());
                check = o.check;
            }
            let t = median(times);
            println!("| {} | {n} | spike 2 {variant} | 1 | {:.2} | | 1.00 | | | | spike 2's sequential evaluator |", c.id, ms(t));
            (t, check)
        });
        let mut reference = None;
        let mut t_seq = None;
        for var in &vs {
            if (var.bits.is_some() || var.kind == MemoKind::Hashed) && !c.memo {
                continue;
            }
            progress(&format!("{} t={}", var.name, var.threads));
            let mut cfg = Config::new(var.split);
            cfg.memo = var.memo.unwrap_or(c.memo);
            cfg.memo_bits = var.bits.unwrap_or(cfg.memo_bits);
            cfg.memo_kind = var.kind;
            cfg.index = var.index;
            cfg.track_conds = !c.untracked;
            let p = pool_of(var.threads);
            let mut times = vec![];
            let mut last = None;
            for _ in 0..runs {
                let o = (c.run)(p, cfg, n, quick);
                times.push(o.time());
                if let Some(prev) = &last {
                    let prev: &Out = prev;
                    assert_eq!(prev.check, o.check, "{} {}: two runs of one configuration disagree", c.id, var.name);
                }
                last = Some(o);
            }
            let o = last.unwrap();
            let t = median(times);
            let reference = *reference.get_or_insert(o.check);
            if var.name == "seq" {
                t_seq = Some(t);
            }
            let verdict = if o.check == reference {
                "ok".to_string()
            } else if c.untracked && cfg.memo {
                "stale (expected)".to_string()
            } else {
                bad += 1;
                "MISMATCH".to_string()
            };
            let vs2 = match s2 {
                Some((ts2, ck)) if var.name == "ref" || var.name == "seq" => {
                    if o.s2 != ck {
                        bad += 1;
                        format!("{:.2} S2 MISMATCH", t.as_secs_f64() / ts2.as_secs_f64())
                    } else {
                        format!("{:.2} (s2 check ok)", t.as_secs_f64() / ts2.as_secs_f64())
                    }
                }
                _ => String::new(),
            };
            let speedup = t_seq.map_or(String::new(), |s| format!("{:.2}", s.as_secs_f64() / t.as_secs_f64()));
            let phases = if o.phases.iter().filter(|(p, _)| *p != "define").count() > 1 && o.phases.len() <= 6 {
                let p: Vec<String> = o.phases.iter().filter(|(p, _)| *p != "define").map(|(_, d)| format!("{:.2}", ms(*d))).collect();
                format!("phases {} ms; ", p.join(" / "))
            } else {
                String::new()
            };
            println!(
                "| {} | {n} | {} | {} | {:.2} | {} | {} | {} | {} | {} | {}{} |",
                c.id,
                var.name,
                var.threads,
                ms(t),
                speedup,
                vs2,
                o.stats.splits,
                o.stats.redos,
                verdict,
                phases,
                o.note
            );
            let _ = std::io::stdout().flush();
        }
        if c.untracked {
            println!("| {} | {n} | | | | | | | | | memoised configurations must be stale: condition reads untracked |", c.id);
        }
    }
    bad
}
