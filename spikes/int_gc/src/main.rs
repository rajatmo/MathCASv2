//! Driver of integration spike I1. For each workload and size: the sequential non-collecting baseline (`ref`: no
//! collection, one thread, `never` split), then each heap mode (`none`: spike 4's store, no collection; `D1`:
//! generational with topological promotion; `D2`: generational with per-thread chunk order) at each thread count,
//! with spike 4's lazy granularity rule; `D1s`, `D2s`: the same with a sequential collector (no parallel copy).
//! Every result is compared with that baseline. Then the other baselines: spike 1b's single-threaded collector C1 on
//! S3/S8/LS and spike 4's original single-evaluation M6a/M8/M10, unchanged. `--full` runs once per configuration
//! (about two minutes); `--runs N` repeats and reports medians.
//!
//! usage: int_gc (--quick | --full) [--stress] [--runs N] [--threads 1+2+4] [--modes none+D1+D2] [--no-base] [case...]
//!   (lists take '+' or ',': the guard script splits its argument list on commas)
//!   --stress   collect at every safe point (major every fourth), verify every invariant before and after each
//!              collection, count cross-thread hits (not a measurement)
use int_gc::eval::{Config as EvalConfig, Split};
use int_gc::heap::{Config, GcEvent, Order};
use int_gc::work::{self, Out, Session};
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::{Duration, Instant};

struct Counting;
static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: forwarded to the system allocator with the caller's layout.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let c = CUR.fetch_add(l.size(), Relaxed) + l.size();
            PEAK.fetch_max(c, Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        // SAFETY: as `alloc`.
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            let c = CUR.fetch_add(l.size(), Relaxed) + l.size();
            PEAK.fetch_max(c, Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: as `alloc`.
        unsafe { System.dealloc(p, l) };
        CUR.fetch_sub(l.size(), Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        // SAFETY: as `alloc`.
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            if n > l.size() {
                let c = CUR.fetch_add(n - l.size(), Relaxed) + (n - l.size());
                PEAK.fetch_max(c, Relaxed);
            } else {
                CUR.fetch_sub(l.size() - n, Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static A: Counting = Counting;

type Run = fn(&mut Session, EvalConfig, usize) -> Out;

struct Case {
    id: &'static str,
    /// quick, full, stress sizes
    sizes: [usize; 3],
    run: Run,
}

fn cases() -> Vec<Case> {
    vec![
        Case { id: "M6a deriv x8", sizes: [1_000, 5_000, 500], run: |s, c, n| work::m6a(s, c, n, 8) },
        Case { id: "M8 intern", sizes: [400_000, 4_000_000, 100_000], run: |s, _, n| work::m8(s, n) },
        Case { id: "M10 ptable x4", sizes: [10_000, 100_000, 2_000], run: |s, c, n| work::m10(s, c, n, 4) },
        Case { id: "S3 parts", sizes: [100_000, 1_000_000, 10_000], run: |s, _, n| work::s3(s, n) },
        Case { id: "S8 alloc", sizes: [100_000, 1_000_000, 50_000], run: |s, _, n| work::s8(s, n) },
        Case { id: "LS session", sizes: [2_000, 10_000, 500], run: work::long_session },
    ]
}

struct Res {
    total: Duration,
    work: Duration,
    safe: Duration,
    peak: usize,
    after: usize,
    live: usize,
    check: u64,
    log: Vec<GcEvent>,
    cross_hits: u64,
    evals: usize,
}

fn pool(threads: usize) -> ThreadPool {
    ThreadPoolBuilder::new().num_threads(threads).stack_size(256 << 20).build().unwrap()
}

fn run_once(c: &Case, cfg: Config, split: Split, pool: &ThreadPool, n: usize) -> Res {
    let base = CUR.load(Relaxed);
    PEAK.store(base, Relaxed);
    let t0 = Instant::now();
    let mut s = Session::new(cfg, 1);
    // the whole session runs on a worker of the pool: evaluations fork from there, safe points run there
    let (out, total, peak) = pool.install(|| {
        let out = (c.run)(&mut s, EvalConfig::new(split), n);
        let total = t0.elapsed();
        let peak = PEAK.load(Relaxed).saturating_sub(base);
        s.finish();
        (out, total, peak)
    });
    let after = CUR.load(Relaxed).saturating_sub(base);
    let r = Res {
        total,
        work: s.t_work,
        safe: s.t_safe,
        peak,
        after,
        live: s.heap.old_nodes(),
        check: out.check,
        log: s.heap.log().to_vec(),
        cross_hits: s.heap.cross_hits(),
        evals: s.evals,
    };
    drop(s);
    r
}

fn ms(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1e3;
    if ms < 1.0 {
        format!("{ms:.3}")
    } else if ms < 100.0 {
        format!("{ms:.2}")
    } else {
        format!("{ms:.0}")
    }
}

fn mb(b: usize) -> String {
    format!("{:.1}", b as f64 / 1e6)
}

fn pauses(log: &[GcEvent], kind: &str) -> String {
    let p: Vec<Duration> = log.iter().filter(|e| e.kind == kind).map(|e| e.pause).collect();
    if p.is_empty() {
        return "-".into();
    }
    let total: Duration = p.iter().sum();
    format!("{} / {} / {}", p.len(), ms(*p.iter().max().unwrap()), ms(total / p.len() as u32))
}

/// Mean time per collection of each phase, in microseconds.
fn phases(log: &[GcEvent], kind: &str) -> String {
    let p: Vec<&GcEvent> = log.iter().filter(|e| e.kind == kind).collect();
    if p.is_empty() {
        return "-".into();
    }
    let mean = |i: usize| p.iter().map(|e| e.phases[i].as_secs_f64()).sum::<f64>() * 1e6 / p.len() as f64;
    format!("{:.0}/{:.0}/{:.0}/{:.0}", mean(0), mean(1), mean(2), mean(3))
}

fn median_by<X>(mut v: Vec<X>, key: impl Fn(&X) -> Duration) -> X {
    v.sort_by_key(|x| key(x));
    let m = v.len() / 2;
    v.swap_remove(m)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let full = has("--full");
    let stress = has("--stress");
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let runs: usize = opt("--runs").and_then(|v| v.parse().ok()).unwrap_or(1);
    let threads: Vec<usize> =
        opt("--threads").map(|v| v.split([',', '+']).filter_map(|x| x.parse().ok()).collect()).unwrap_or(vec![1, 2, 4]);
    let modes: Vec<String> =
        opt("--modes").map(|v| v.split([',', '+']).map(String::from).collect()).unwrap_or(vec!["none".into(), "D1".into(), "D2".into(), "D1s".into(), "D2s".into()]);
    let valued = ["--runs", "--threads", "--modes"];
    let filters: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !valued.contains(&args[i - 1].as_str())))
        .map(|(_, a)| a.to_lowercase())
        .collect();
    let base = !has("--no-base") && !stress;
    let h = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            let pools: Vec<(usize, ThreadPool)> = threads.iter().map(|&t| (t, pool(t))).collect();
            let p1 = pool(1);
            let sel: Vec<Case> = cases()
                .into_iter()
                .filter(|c| filters.is_empty() || filters.iter().any(|f| c.id.to_lowercase().contains(f)))
                .collect();
            let total_steps = sel.len() * (1 + modes.len() * pools.len());
            let mut step = 0;
            let mut failed = false;
            println!(
                "| case | size | mode | thr | total ms | in evals | in safe points | evals | minor: n / max / mean ms | major: n / max / mean ms | \
                 minor phases us mark/copy/memo/reset | major phases us mark/slide/tables/roots | peak MB | after GC MB | live nodes | promoted | out-of-order edges | cross-worker edges | cross-worker hits | vs none | check |"
            );
            println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
            for c in &sel {
                let n = c.sizes[if stress { 2 } else if full { 1 } else { 0 }];
                step += 1;
                eprintln!("[{step}/{total_steps} {}%] {} {n} ref", 100 * step / total_steps, c.id);
                let reference = run_once(c, Config::none(), Split::Never, &p1, n);
                let mut none_t: Vec<(usize, Duration)> = vec![];
                for m in &modes {
                    for (t, p) in &pools {
                        step += 1;
                        eprintln!("[{step}/{total_steps} {}%] {} {n} {m} {t}", 100 * step / total_steps, c.id);
                        let mut cfg = match m.as_str() {
                            "D1" => Config::gen(Order::Topo),
                            "D2" => Config::gen(Order::Chunk),
                            "D1s" => Config { par_gc: false, ..Config::gen(Order::Topo) },
                            "D2s" => Config { par_gc: false, ..Config::gen(Order::Chunk) },
                            _ => Config::none(),
                        };
                        cfg.stress = stress;
                        cfg.instrument = stress;
                        let rs: Vec<Res> = (0..runs).map(|_| run_once(c, cfg, Split::Lazy, p, n)).collect();
                        let peak = rs.iter().map(|r| r.peak).max().unwrap();
                        let ok = rs.iter().all(|r| r.check == reference.check);
                        failed |= !ok;
                        let r = median_by(rs, |r| r.total);
                        if m == "none" {
                            none_t.push((*t, r.total));
                        }
                        let vs = none_t
                            .iter()
                            .find(|x| x.0 == *t)
                            .map(|x| format!("{:.2}", r.total.as_secs_f64() / x.1.as_secs_f64()))
                            .unwrap_or_else(|| "-".into());
                        let minors: Vec<&GcEvent> = r.log.iter().filter(|e| e.kind == "minor").collect();
                        let promoted: usize = minors.iter().map(|e| e.survivors).sum();
                        let ooo: usize = r.log.iter().map(|e| e.out_of_order).sum();
                        let cross: usize = minors.iter().map(|e| e.cross_owner).sum();
                        println!(
                            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                            c.id,
                            n,
                            m,
                            t,
                            ms(r.total),
                            ms(r.work),
                            ms(r.safe),
                            r.evals,
                            pauses(&r.log, "minor"),
                            pauses(&r.log, "major"),
                            phases(&r.log, "minor"),
                            phases(&r.log, "major"),
                            mb(peak),
                            mb(r.after),
                            r.live,
                            promoted,
                            ooo,
                            cross,
                            if stress { r.cross_hits.to_string() } else { "-".into() },
                            vs,
                            if ok { "ok" } else { "MISMATCH" }
                        );
                    }
                }
                println!(
                    "| {} | {} | ref | 1 | {} | {} | {} | {} | - | - | - | - | {} | {} | - | - | - | - | - | - | (reference) |",
                    c.id,
                    n,
                    ms(reference.total),
                    ms(reference.work),
                    ms(reference.safe),
                    reference.evals,
                    mb(reference.peak),
                    mb(reference.after)
                );
            }
            if base {
                baselines(full, &pools);
            }
            failed
        })
        .unwrap();
    let failed = h.join().unwrap();
    if failed {
        eprintln!("MISMATCH: some configuration differs from the baseline");
        std::process::exit(1);
    }
}

/// Spike 1b's C1 collector (single-threaded) on S3/S8/LS and spike 4's store C without collection on its original
/// single-evaluation M6a/M8/M10, both run unchanged from their crates.
type Baseline<'a> = (&'a str, usize, Box<dyn Fn() -> parallel::work::Out + 'a>);

fn baselines(full: bool, pools: &[(usize, ThreadPool)]) {
    use collect::gc::{GcStore, Mode};
    println!("\nBaselines (unchanged spike code; medians of 3):\n");
    println!("| baseline | case | size | thr | total ms | peak MB |");
    println!("|---|---|---|---|---|---|");
    type W = fn(&mut GcStore, usize) -> collect::work::Out;
    let ws: [(&str, W, usize); 3] = [
        ("S3 parts", collect::work::s3_parts, if full { 1_000_000 } else { 100_000 }),
        ("S8 alloc", collect::work::s8_alloc, if full { 1_000_000 } else { 100_000 }),
        ("LS session", collect::work::long_session, if full { 10_000 } else { 2_000 }),
    ];
    for (name, w, n) in ws {
        let mut v = vec![];
        for _ in 0..3 {
            let base = CUR.load(Relaxed);
            PEAK.store(base, Relaxed);
            let t0 = Instant::now();
            let mut st = GcStore::new("C1", Mode::Gen { nursery: 1 << 20 }, false);
            w(&mut st, n);
            v.push((t0.elapsed(), PEAK.load(Relaxed).saturating_sub(base)));
            drop(st);
        }
        let peak = v.iter().map(|x| x.1).max().unwrap();
        let r = median_by(v, |x| x.0);
        println!("| spike 1b C1 | {name} | {n} | 1 | {} | {} |", ms(r.0), mb(peak));
    }
    use parallel::eval::{Config as PConfig, Split as PSplit};
    use parallel::table::Optimistic;
    for (t, p) in pools {
        let cfg = PConfig::new(PSplit::Lazy);
        let rows: [Baseline; 3] = [
            ("M6a deriv (one evaluation)", if full { 5_000 } else { 1_000 }, Box::new(move || parallel::work::m6_deriv::<Optimistic>(p, cfg, if full { 5_000 } else { 1_000 }))),
            ("M8 intern", if full { 4_000_000 } else { 400_000 }, Box::new(move || parallel::work::m8_intern::<Optimistic>(p, if full { 4_000_000 } else { 400_000 }))),
            ("M10 ptable (two seeds)", if full { 100_000 } else { 10_000 }, Box::new(move || parallel::work::m10_random::<Optimistic>(p, cfg, if full { 100_000 } else { 10_000 }))),
        ];
        for (name, n, f) in rows.iter() {
            let mut v = vec![];
            for _ in 0..3 {
                let base = CUR.load(Relaxed);
                PEAK.store(base, Relaxed);
                let o = f();
                v.push((o.time, PEAK.load(Relaxed).saturating_sub(base)));
            }
            let peak = v.iter().map(|x| x.1).max().unwrap();
            let r = median_by(v, |x| x.0);
            println!("| spike 4 C, no collection | {name} | {n} | {t} | {} | {} |", ms(r.0), mb(peak));
        }
    }
}
