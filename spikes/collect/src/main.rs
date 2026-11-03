//! Arena-collection spike harness: runs every workload on each store, `--runs` times on a fresh store each, and
//! prints the median total and phase times, the collections (count, pauses, memory after), the peak heap above
//! baseline, the throughput overhead against the same store with collection off, and whether the fingerprints agree.
//!
//! usage: collect (--quick | --medium | --full | --size N) [--runs N] [--stores a,b] [--stress] [--trace] [workload...]
//! - `--quick`: smallest size of each case (seconds); `--medium`: two smallest; `--full`: all sizes;
//! - `--stress`: collect at every safe point (correctness check of the collectors, not a measurement);
//! - `--trace`: also print the memory after each collection (every 20th) for the long session;
//! - `--size N`: only that size; `--stores a,b`: only these stores (`hc`, `A`, `A-fl`, `B`, `C`, `C1`, `C0`; `none`
//!   always runs as the baseline).
use collect::gc::{ArgPolicy, GcStore, Mode};
use collect::hc::HcStore;
use collect::work::{self, Out};
use collect::{GcEvent, Store};
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

type Workload<St> = fn(&mut St, usize) -> Out;

struct Case {
    id: &'static str,
    sizes: [usize; 3],
    hc: Workload<HcStore>,
    gc: Workload<GcStore>,
}

fn cases() -> Vec<Case> {
    vec![
        Case { id: "S3 parts", sizes: [10_000, 100_000, 1_000_000], hc: work::s3_parts, gc: work::s3_parts },
        Case { id: "S8 alloc", sizes: [100_000, 1_000_000, 10_000_000], hc: work::s8_alloc, gc: work::s8_alloc },
        Case { id: "R10 replace", sizes: [10_000, 100_000, 1_000_000], hc: work::r10_replace, gc: work::r10_replace },
        Case { id: "R11 re-eval", sizes: [10_000, 100_000, 1_000_000], hc: work::r11_reeval, gc: work::r11_reeval },
        Case { id: "LS long session", sizes: [2_000, 10_000, 50_000], hc: work::long_session, gc: work::long_session },
    ]
}

/// The collected stores: (name, mode).
fn gc_modes() -> Vec<(&'static str, Mode)> {
    vec![
        ("none", Mode::None),
        ("A", Mode::MarkSweep(ArgPolicy::Compact)),
        ("A-fl", Mode::MarkSweep(ArgPolicy::FreeLists)),
        ("B", Mode::Compact),
        ("C", Mode::Gen { nursery: 8 << 20 }),
        ("C1", Mode::Gen { nursery: 1 << 20 }),
        ("C0", Mode::Gen { nursery: 0 }),
    ]
}

struct Result {
    store: &'static str,
    total: Duration,
    phases: Vec<(&'static str, Duration)>,
    peak: usize,
    check: u64,
    notes: Vec<(&'static str, u64)>,
    log: Vec<GcEvent>,
}

fn run_one<St: Store>(mk: impl Fn() -> St, w: Workload<St>, n: usize, runs: usize) -> Result {
    let mut times: Vec<Vec<(&'static str, Duration)>> = vec![];
    let (mut peak, mut check, mut name, mut notes, mut log) = (0, 0, "", vec![], vec![]);
    for _ in 0..runs {
        let base = CUR.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let mut st = mk();
        name = st.name();
        let out = w(&mut st, n);
        log = st.gc_log().to_vec();
        drop(st);
        peak = peak.max(PEAK.load(Ordering::Relaxed) - base);
        check = out.check;
        notes = out.notes;
        times.push(out.phases);
    }
    let median = |mut v: Vec<Duration>| {
        v.sort();
        v[v.len() / 2]
    };
    let phases = (0..times[0].len()).map(|i| (times[0][i].0, median(times.iter().map(|t| t[i].1).collect()))).collect();
    let total = median(times.iter().map(|t| t.iter().map(|p| p.1).sum()).collect());
    Result { store: name, total, phases, peak, check, notes, log }
}

fn ms(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1e3;
    if ms < 1.0 {
        format!("{:.3}", ms)
    } else if ms < 100.0 {
        format!("{:.2}", ms)
    } else {
        format!("{:.0}", ms)
    }
}

fn mb(b: usize) -> String {
    format!("{:.1}", b as f64 / 1e6)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let nsizes = if has("--full") {
        3
    } else if has("--medium") {
        2
    } else {
        1
    };
    let (stress, trace) = (has("--stress"), has("--trace"));
    // `--size N`: run only that size (for scaling checks)
    let size: Option<usize> =
        args.iter().position(|a| a == "--size").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok());
    // `--stores a,b`: run only these stores (`hc` is spike 1's store); `none` always runs, as the overhead baseline
    let stores: Option<Vec<String>> = args
        .iter()
        .position(|a| a == "--stores")
        .and_then(|i| args.get(i + 1))
        .map(|v| v.split(',').map(|s| s.to_string()).collect());
    let wanted = move |s: &str| s == "none" || stores.as_ref().is_none_or(|v| v.iter().any(|w| w == s));
    let runs: usize =
        args.iter().position(|a| a == "--runs").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(3);
    // workload filters: the arguments that are neither options nor the value of an option
    let valued = ["--runs", "--size", "--stores"];
    let filters: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !valued.contains(&args[i - 1].as_str())))
        .map(|(_, a)| a.clone())
        .collect();
    // the workloads recurse over random trees: give the work thread a large stack, as spike 1 does
    let h = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            println!(
                "| case | size | store | total ms | phases (ms) | GCs | pause total | pause max | pause mean | \
                 live nodes after last GC | reserved after last GC MB | max reserved after GC MB | peak heap MB | overhead | check | notes |"
            );
            println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
            let mut traces = vec![];
            for c in cases() {
                if !filters.is_empty() && !filters.iter().any(|f| c.id.to_lowercase().contains(&f.to_lowercase())) {
                    continue;
                }
                let sizes = size.map(|n| vec![n]).unwrap_or_else(|| c.sizes[..nsizes].to_vec());
                for n in sizes {
                    let mut rs = vec![];
                    if wanted("hc") {
                        rs.push(run_one(|| HcStore::with(true, true), c.hc, n, runs));
                    }
                    for (name, mode) in gc_modes().into_iter().filter(|m| wanted(m.0)) {
                        rs.push(run_one(|| GcStore::new(name, mode, stress), c.gc, n, runs));
                    }
                    let agree = rs.iter().all(|r| r.check == rs[0].check);
                    let none = rs.iter().find(|r| r.store == "none").map(|r| r.total).unwrap();
                    for r in &rs {
                        let phases: Vec<String> = r.phases.iter().map(|(p, d)| format!("{p} {}", ms(*d))).collect();
                        let pauses: Vec<Duration> = r.log.iter().map(|e| e.pause).collect();
                        let ptotal: Duration = pauses.iter().sum();
                        let pmax = pauses.iter().max().copied().unwrap_or_default();
                        let pmean = if pauses.is_empty() { Duration::ZERO } else { ptotal / pauses.len() as u32 };
                        let last = r.log.last().map(|e| mb(e.after)).unwrap_or_else(|| "-".into());
                        let maxa = r.log.iter().map(|e| e.after).max().map(mb).unwrap_or_else(|| "-".into());
                        let notes: Vec<String> = r.notes.iter().map(|(k, v)| format!("{k} {v}")).collect();
                        let over = (r.total.as_secs_f64() / none.as_secs_f64() - 1.0) * 100.0;
                        let minors = r.log.iter().filter(|e| e.kind == "minor").count();
                        let gcs = if minors > 0 {
                            format!("{} ({} minor)", r.log.len(), minors)
                        } else {
                            r.log.len().to_string()
                        };
                        println!(
                            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:+.0}% | {} | {} |",
                            c.id,
                            n,
                            r.store,
                            ms(r.total),
                            phases.join(", "),
                            gcs,
                            ms(ptotal),
                            ms(pmax),
                            ms(pmean),
                            r.log.last().map(|e| e.live.to_string()).unwrap_or_else(|| "-".into()),
                            last,
                            maxa,
                            mb(r.peak),
                            over,
                            if agree { "ok" } else { "MISMATCH" },
                            notes.join(", ")
                        );
                        if trace && c.id.starts_with("LS") && !r.log.is_empty() {
                            let step = (r.log.len() / 20).max(1);
                            let s: Vec<String> = r.log.iter().step_by(step).map(|e| mb(e.after)).collect();
                            traces.push(format!("{} {} {}: {}", c.id, n, r.store, s.join(" ")));
                        }
                    }
                }
            }
            for t in traces {
                println!("\ntrace (MB reserved after collection): {t}");
            }
        })
        .unwrap();
    h.join().unwrap();
}
