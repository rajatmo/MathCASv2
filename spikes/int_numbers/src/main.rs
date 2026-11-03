//! I3 harness: runs every workload on the bare numbers and on each store configuration (fresh store per run),
//! prints the median phase times, the collections (pauses, payload words moved, boxes freed, memory reserved after),
//! the peak heap above baseline from a counting allocator, the time against the bare numbers, and whether every
//! fingerprint agrees.
//!
//! usage: int_numbers (--quick | --full) [--runs N] [--stress] [--configs a,b] [--limit SECS] [--out FILE] [case-filter...]
//! - `--out FILE`: also write the final table there (in order; the guard's captured output may interleave lines);
//! - `--configs`: run only these, separated by `,` or `+` (the threshold sweep `hyb16`, `hyb256`, `hyb4k` runs only when named here);
//! - `--stress`: collect at every safe point (minor each time, major every fourth) and verify the store's invariants
//!   after every collection (correctness, not a measurement);
//! - `--runs N`: median of N runs (default 3 in --full, 1 in --quick); a run longer than 1 s is not repeated;
//! - `--limit`: a watchdog raises the stop flag after this many seconds (default 170); stopped runs are marked.
use int_numbers::store::{Config, Gc, Hashing, Heap, Placement};
use int_numbers::work::{self, Big, Churn, Keep, Out};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

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

const NURSERY: usize = 1 << 20;
const HYBRID: usize = 64;

fn configs(stress: bool) -> Vec<Config> {
    let gen = Gc::Gen { nursery: NURSERY };
    let mk = |name, place, hash, gc| Config { name, place, hash, gc, stress, count_boxed: true };
    vec![
        mk("arena", Placement::Arena, Hashing::Eager, gen),
        mk("boxed", Placement::Boxed, Hashing::Eager, gen),
        mk("hybrid", Placement::Hybrid(HYBRID), Hashing::Eager, gen),
        mk("hyb-sampled", Placement::Hybrid(HYBRID), Hashing::Sampled, gen),
        mk("hyb-lazy", Placement::Hybrid(HYBRID), Hashing::Lazy(HYBRID), gen),
        mk("hyb-nogc", Placement::Hybrid(HYBRID), Hashing::Eager, Gc::None),
        Config { count_boxed: false, ..mk("boxed-nocount", Placement::Boxed, Hashing::Eager, gen) },
        // threshold sweep, only with --configs
        mk("hyb16", Placement::Hybrid(16), Hashing::Eager, gen),
        mk("hyb256", Placement::Hybrid(256), Hashing::Eager, gen),
        mk("hyb4k", Placement::Hybrid(4096), Hashing::Eager, gen),
    ]
}

/// A case at one size: the store workload and the bare one.
struct Case {
    id: &'static str,
    size: String,
    store: Box<dyn Fn(&mut Heap) -> Out>,
    bare: Box<dyn Fn() -> Out>,
}

fn cases(full: bool) -> Vec<Case> {
    let mut v: Vec<Case> = vec![];
    let pick = |q: &[usize], f: &[usize]| if full { f.to_vec() } else { q.to_vec() };
    for n in pick(&[10_000], &[10_000, 100_000, 1_000_000]) {
        v.push(Case {
            id: "N5 tree",
            size: format!("{n}!"),
            store: Box::new(move |h| work::n5_tree_store(h, n)),
            bare: Box::new(move || work::n5_tree_bare(n)),
        });
    }
    for n in pick(&[10_000], &[10_000, 30_000]) {
        v.push(Case {
            id: "N5 seq",
            size: format!("{n}!"),
            store: Box::new(move |h| work::n5_seq_store(h, n)),
            bare: Box::new(move || work::n5_seq_bare(n)),
        });
    }
    for n in pick(&[10_000], &[10_000, 100_000]) {
        v.push(Case {
            id: "N10 split",
            size: format!("H({n})"),
            store: Box::new(move |h| work::n10_split_store(h, n)),
            bare: Box::new(move || work::n10_split_bare(n)),
        });
    }
    for n in pick(&[10_000], &[10_000, 100_000]) {
        v.push(Case {
            id: "N10 seq",
            size: format!("H({n})"),
            store: Box::new(move |h| work::n10_seq_store(h, n)),
            bare: Box::new(move || work::n10_seq_bare(n)),
        });
    }
    for n in pick(&[100], &[100, 400]) {
        v.push(Case {
            id: "EXP plain",
            size: format!("n={n}"),
            store: Box::new(move |h| work::expand_plain_store(h, n)),
            bare: Box::new(move || work::expand_bare(n)),
        });
        v.push(Case {
            id: "EXP packed",
            size: format!("n={n}"),
            store: Box::new(move |h| work::expand_packed_store(h, n)),
            bare: Box::new(move || work::expand_bare(n)),
        });
    }
    for n in pick(&[2_000], &[2_000, 20_000]) {
        let p = 3400;
        v.push(Case {
            id: "ZETA balls",
            size: format!("n={n} p={p}"),
            store: Box::new(move |h| work::zeta_store(h, n, p)),
            bare: Box::new(move || work::zeta_bare(n, p)),
        });
    }
    let d = if full { 1_000_000 } else { 10_000 };
    v.push(Case {
        id: "PI/E balls",
        size: format!("{d} digits"),
        store: Box::new(move |h| work::consts_store(h, d)),
        bare: Box::new(move || work::consts_bare(d)),
    });
    let c = if full {
        Churn { evals: 300, lo_digits: 1e3, hi_digits: 1e5 }
    } else {
        Churn { evals: 60, lo_digits: 1e3, hi_digits: 1e4 }
    };
    v.push(Case {
        id: "LS churn",
        size: format!("{} evals, {:.0e}..{:.0e} digits", c.evals, c.lo_digits, c.hi_digits),
        store: Box::new(move |h| work::churn_store(h, c)),
        bare: Box::new(move || work::churn_bare(c)),
    });
    let k = if full {
        Keep { evals: 300, big_digits: 1e6, terms: 5000 }
    } else {
        Keep { evals: 100, big_digits: 1e5, terms: 2000 }
    };
    let b = if full {
        Big { evals: 200, limbs: 52_000, ring: 8, terms: 200 }
    } else {
        Big { evals: 50, limbs: 5_200, ring: 8, terms: 200 }
    };
    v.push(Case {
        id: "LS big",
        size: format!("{} evals, {} limbs, ring {}", b.evals, b.limbs, b.ring),
        store: Box::new(move |h| work::big_store(h, b)),
        bare: Box::new(move || work::big_bare(b)),
    });
    let (hk, hl) = if full { (2000, 1000) } else { (500, 1000) };
    v.push(Case {
        id: "HASH hazard",
        size: format!("{hk} x {hl} limbs"),
        store: Box::new(move |h| work::hash_hazard_store(h, hk, hl)),
        bare: Box::new(move || work::hash_hazard_bare(hk, hl)),
    });
    v.push(Case {
        id: "LS keep",
        size: format!("{} evals, {:.0e}-digit kept, {} terms", k.evals, k.big_digits, k.terms),
        store: Box::new(move |h| work::keep_store(h, k)),
        bare: Box::new(move || work::keep_bare(k)),
    });
    v
}

struct Res {
    name: &'static str,
    total: Duration,
    phases: Vec<(&'static str, Duration)>,
    peak: usize,
    check: u64,
    notes: Vec<(&'static str, String)>,
    log: Vec<int_numbers::store::GcEvent>,
    hashed: u64,
    stopped: bool,
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn run(name: &'static str, runs: usize, mut one: impl FnMut() -> (Out, Vec<int_numbers::store::GcEvent>, u64)) -> Res {
    let mut times = vec![];
    let (mut peak, mut check, mut notes, mut log, mut hashed) = (0, 0, vec![], vec![], 0);
    for _ in 0..runs {
        let base = CUR.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let (o, l, hw) = one();
        peak = peak.max(PEAK.load(Ordering::Relaxed).saturating_sub(base));
        check = o.check;
        notes = o.notes;
        log = l;
        hashed = hw;
        let took: Duration = o.phases.iter().map(|p| p.1).sum();
        times.push(o.phases);
        // runs longer than a second are not repeated (keeps --full within the time budget)
        if int_numbers::stopped() || took > Duration::from_secs(1) {
            break;
        }
    }
    let phases = (0..times[0].len()).map(|i| (times[0][i].0, median(times.iter().map(|t| t[i].1).collect()))).collect();
    let total = median(times.iter().map(|t| t.iter().map(|p| p.1).sum()).collect());
    Res { name, total, phases, peak, check, notes, log, hashed, stopped: int_numbers::stopped() }
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
    format!("{:.2}", b as f64 / 1e6)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let full = has("--full");
    let stress = has("--stress");
    let val = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let runs: usize = val("--runs").and_then(|v| v.parse().ok()).unwrap_or(if full { 3 } else { 1 });
    let limit: u64 = val("--limit").and_then(|v| v.parse().ok()).unwrap_or(170);
    let wanted: Option<Vec<String>> = val("--configs").map(|v| v.split([',', '+']).map(|s| s.to_string()).collect());
    let out_file = val("--out");
    let valued = ["--runs", "--limit", "--configs", "--out"];
    let filters: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !valued.contains(&args[i - 1].as_str())))
        .map(|(_, a)| a.to_lowercase())
        .collect();
    std::thread::spawn(move || {
        let t0 = Instant::now();
        // bounded by the limit; parks in slices of a second
        while t0.elapsed() < Duration::from_secs(limit) {
            std::thread::park_timeout(Duration::from_millis(1000));
        }
        int_numbers::STOP.store(true, Ordering::Relaxed);
        eprintln!("watchdog: stop flag raised after {limit} s");
    });
    let cfgs: Vec<Config> = configs(stress)
        .into_iter()
        .filter(|c| match &wanted {
            Some(w) => w.iter().any(|x| x == c.name),
            None => !c.name.starts_with("hyb") || c.name.contains('-') || c.name == "hybrid",
        })
        .collect();
    let cs: Vec<Case> = cases(full)
        .into_iter()
        .filter(|c| filters.is_empty() || filters.iter().any(|f| c.id.to_lowercase().contains(f)))
        .collect();
    let total_runs = cs.len() * (cfgs.len() + 1);
    let mut k = 0;
    let t_start = Instant::now();
    println!(
        "mode: {}{}, runs {runs}, nursery {} MB, hybrid threshold {HYBRID} words",
        if full { "full" } else { "quick" },
        if stress { " stress" } else { "" },
        NURSERY >> 20
    );
    let mut all_ok = true;
    let mut table = vec![
        "| case | size | config | total ms | phases (ms) | GCs (major) | pause max | pause mean | major max | moved MB | \
         boxes freed | reserved after last GC MB | peak heap MB | vs bare | hashed Mwords | check |"
            .to_string(),
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|".to_string(),
    ];
    for c in &cs {
        let mut rs = vec![];
        k += 1;
        eprintln!("[{k}/{total_runs} {:.0}%] {} {} bare ({:.1} s)", 100.0 * k as f64 / total_runs as f64, c.id, c.size, t_start.elapsed().as_secs_f64());
        rs.push(run("bare", runs, || ((c.bare)(), vec![], 0)));
        for cfg in &cfgs {
            k += 1;
            eprintln!(
                "[{k}/{total_runs} {:.0}%] {} {} {} ({:.1} s)",
                100.0 * k as f64 / total_runs as f64,
                c.id,
                c.size,
                cfg.name,
                t_start.elapsed().as_secs_f64()
            );
            rs.push(run(cfg.name, runs, || {
                let mut h = Heap::new(*cfg);
                let o = (c.store)(&mut h);
                let l = h.gc_log().to_vec();
                let hw = h.hashed_words;
                (o, l, hw)
            }));
        }
        let agree = rs.iter().all(|r| r.check == rs[0].check);
        let fp2: Vec<&String> =
            rs.iter().filter_map(|r| r.notes.iter().find(|n| n.0 == "fp2").map(|n| &n.1)).collect();
        let agree2 = fp2.iter().all(|x| *x == fp2[0]);
        let ok = agree && agree2 && !rs.iter().any(|r| r.stopped);
        all_ok &= ok;
        let bare = rs[0].total;
        for r in &rs {
            let phases: Vec<String> = r.phases.iter().map(|(p, d)| format!("{p} {}", ms(*d))).collect();
            let pauses: Vec<Duration> = r.log.iter().map(|e| e.pause).collect();
            let ptotal: Duration = pauses.iter().sum();
            let pmax = pauses.iter().max().copied().unwrap_or_default();
            let pmean = if pauses.is_empty() { Duration::ZERO } else { ptotal / pauses.len() as u32 };
            let majors: Vec<&int_numbers::store::GcEvent> = r.log.iter().filter(|e| e.kind == "major").collect();
            let mmax = majors.iter().map(|e| e.pause).max().unwrap_or_default();
            let moved: usize = r.log.iter().map(|e| e.moved_words * 8).sum();
            let freed: usize = r.log.iter().map(|e| e.freed_boxes).sum();
            let last = r.log.last().map(|e| mb(e.after)).unwrap_or_else(|| "-".into());
            let status = if r.stopped {
                "STOPPED"
            } else if ok {
                "ok"
            } else {
                "MISMATCH"
            };
            table.push(format!(
                "| {} | {} | {} | {} | {} | {} ({}) | {} | {} | {} | {} | {} | {} | {} | {:.2} | {:.2} | {} |",
                c.id,
                c.size,
                r.name,
                ms(r.total),
                phases.join(", "),
                r.log.len(),
                majors.len(),
                ms(pmax),
                ms(pmean),
                ms(mmax),
                mb(moved),
                freed,
                last,
                mb(r.peak),
                r.total.as_secs_f64() / bare.as_secs_f64(),
                r.hashed as f64 / 1e6,
                status
            ));
        }
        println!("{}", table[table.len() - rs.len()..].join("\n"));
    }
    let summary = format!(
        "mode: {}{}, runs {runs}, nursery {} MB, hybrid threshold {HYBRID} words\n\n{}\n\nall fingerprints agree: {}; elapsed {:.1} s\n",
        if full { "full" } else { "quick" },
        if stress { " stress" } else { "" },
        NURSERY >> 20,
        table.join("\n"),
        if all_ok { "yes" } else { "NO" },
        t_start.elapsed().as_secs_f64()
    );
    if let Some(f) = out_file {
        std::fs::write(&f, &summary).expect("writing the results file");
    }
    println!("\n{summary}");
    if !all_ok {
        std::process::exit(1);
    }
}
