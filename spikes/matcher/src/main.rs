//! Matcher spike harness: runs every case at its sizes in each variant, on a fresh evaluator each run, and prints the
//! median time per phase, the peak heap above baseline, whether the variants' results agree, and a note.
//!
//! usage: matcher [--quick | --full] [--runs N] [--limit SECONDS] [case filter...]
//!
//! `--quick` runs the smallest size once with a 10 s limit (seconds in total); `--full` (the default) runs the three
//! sizes of the benchmark cases, `--runs` times each (default 3), with a 60 s limit per run (benchmarks section 1). A
//! run stopped by the limit is reported as `limit`, with the progress it made, and is not repeated.
use matcher::work::{self, Opts, Out};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct Counting;
static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let c = CUR.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(c, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        CUR.fetch_sub(l.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            if n > l.size() {
                let c = CUR.fetch_add(n - l.size(), Ordering::Relaxed) + (n - l.size());
                PEAK.fetch_max(c, Ordering::Relaxed);
            } else {
                CUR.fetch_sub(l.size() - n, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static A: Counting = Counting;

struct Res {
    variant: &'static str,
    out: Out,
    peak: usize,
}

fn run_one(c: &work::Case, variant: &'static str, n: usize, runs: usize, opts: &Opts) -> Res {
    let mut outs: Vec<Out> = vec![];
    let mut peak = 0;
    for _ in 0..runs {
        let base = CUR.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let out = (c.run)(variant, n, opts);
        peak = peak.max(PEAK.load(Ordering::Relaxed) - base);
        let stop = out.limited;
        outs.push(out);
        if stop {
            break;
        }
    }
    // median per phase
    let mut out = outs.pop().unwrap();
    if !outs.is_empty() {
        outs.push(Out { phases: out.phases.clone(), ..Out::default() });
        for (i, ph) in out.phases.iter_mut().enumerate() {
            let mut v: Vec<Duration> = outs.iter().map(|o| o.phases[i].1).collect();
            v.sort();
            ph.1 = v[v.len() / 2];
        }
    }
    Res { variant, out, peak }
}

fn fmt(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1e3;
    if ms < 1.0 {
        format!("{:.3} ms", ms)
    } else if ms < 100.0 {
        format!("{:.2} ms", ms)
    } else {
        format!("{:.0} ms", ms)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let quick = args.iter().any(|a| a == "--quick");
    let num = |flag: &str| -> Option<u64> { args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()) };
    let runs = if quick { 1 } else { num("--runs").unwrap_or(3) as usize };
    let secs: u64 = num("--limit").unwrap_or(if quick { 10 } else { 60 });
    let filters: Vec<String> =
        args.iter().filter(|a| !a.starts_with("--") && a.parse::<u64>().is_err()).map(|a| a.to_lowercase()).collect();
    let opts = Opts { quick, limit: Duration::from_secs(secs) };
    // the matcher recurses in pattern depth and the evaluator in term depth: give the work thread room
    let h = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            println!("| case | size | variant | phase | median | peak heap | check | note |");
            println!("|---|---|---|---|---|---|---|---|");
            let mut bad = 0;
            for c in work::cases() {
                if !filters.is_empty() && !filters.iter().any(|f| c.id.to_lowercase().contains(f)) {
                    continue;
                }
                let sizes: Vec<usize> = if quick { vec![c.quick] } else { c.sizes.to_vec() };
                for n in sizes {
                    let rs: Vec<Res> = c.variants.iter().map(|v| run_one(&c, v, n, runs, &opts)).collect();
                    let checked: Vec<u64> =
                        rs.iter().filter(|r| !r.out.limited && !r.out.exclude).map(|r| r.out.check).collect();
                    let agree = checked.iter().all(|&h| h == checked[0]);
                    if !agree {
                        bad += 1;
                    }
                    for r in &rs {
                        let check = if r.out.limited {
                            "limit"
                        } else if r.out.exclude {
                            "excluded"
                        } else if agree {
                            "ok"
                        } else {
                            "MISMATCH"
                        };
                        for (i, (p, d)) in r.out.phases.iter().enumerate() {
                            println!(
                                "| {} | {} | {} | {} | {} | {:.1} MB | {} | {} |",
                                c.id,
                                n,
                                r.variant,
                                p,
                                fmt(*d),
                                r.peak as f64 / 1e6,
                                check,
                                if i + 1 == r.out.phases.len() { r.out.note.as_str() } else { "" }
                            );
                        }
                    }
                }
            }
            if bad > 0 {
                eprintln!("{bad} case sizes with MISMATCH");
                std::process::exit(1);
            }
        })
        .unwrap();
    h.join().unwrap();
}
