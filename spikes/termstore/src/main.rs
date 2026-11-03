//! Term-store spike harness: runs every workload at its sizes on each store, three times on a fresh store each,
//! and prints the median time per phase, the peak heap above baseline, and whether the fingerprints agree.
//!
//! usage: termstore [--quick] [--runs N] [workload...]
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use termstore::hc::HcStore;
use termstore::tree::TreeStore;
use termstore::work::{self, Out};
use termstore::Store;

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
    tree: Workload<TreeStore>,
    hc: Workload<HcStore>,
    /// run the stamped hash-consed store too (only where evaluation memoises)
    stamps: bool,
}

fn cases() -> Vec<Case> {
    vec![
        Case { id: "R3 derivative", sizes: [10_000, 100_000, 300_000], tree: work::r3_derivative, hc: work::r3_derivative, stamps: false },
        Case { id: "R8 expand", sizes: [10, 20, 30], tree: work::r8_expand, hc: work::r8_expand, stamps: false },
        Case { id: "R9 deep", sizes: [10_000, 100_000, 1_000_000], tree: work::r9_deep, hc: work::r9_deep, stamps: false },
        Case { id: "R10 replace", sizes: [10_000, 100_000, 1_000_000], tree: work::r10_replace, hc: work::r10_replace, stamps: false },
        Case { id: "R11 re-eval", sizes: [10_000, 100_000, 1_000_000], tree: work::r11_reeval, hc: work::r11_reeval, stamps: true },
        Case { id: "S3 parts", sizes: [10_000, 100_000, 1_000_000], tree: work::s3_parts, hc: work::s3_parts, stamps: false },
        Case { id: "S6 transpose", sizes: [10_000, 100_000, 1_000_000], tree: work::s6_transpose, hc: work::s6_transpose, stamps: false },
        Case { id: "S8 alloc", sizes: [100_000, 1_000_000, 10_000_000], tree: work::s8_alloc, hc: work::s8_alloc, stamps: false },
        Case { id: "EQ same/hash", sizes: [10_000, 100_000, 1_000_000], tree: work::eq_compare, hc: work::eq_compare, stamps: false },
    ]
}

struct Result {
    store: &'static str,
    phases: Vec<(&'static str, Duration)>,
    peak: usize,
    check: u64,
}

fn run_one<St: Store>(mk: impl Fn() -> St, w: Workload<St>, n: usize, runs: usize) -> Result {
    let mut times: Vec<Vec<(&'static str, Duration)>> = vec![];
    let (mut peak, mut check, mut name) = (0, 0, "");
    for _ in 0..runs {
        let base = CUR.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let mut st = mk();
        name = st.name();
        let out = w(&mut st, n);
        drop(st);
        peak = peak.max(PEAK.load(Ordering::Relaxed) - base);
        check = out.check;
        times.push(out.phases);
    }
    // median per phase
    let phases = (0..times[0].len())
        .map(|i| {
            let mut v: Vec<Duration> = times.iter().map(|t| t[i].1).collect();
            v.sort();
            (times[0][i].0, v[v.len() / 2])
        })
        .collect();
    Result { store: name, phases, peak, check }
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
    let runs = args
        .iter()
        .position(|a| a == "--runs")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let filters: Vec<String> = args.iter().filter(|a| !a.starts_with("--") && a.parse::<usize>().is_err()).cloned().collect();
    // deep reference-counted trees drop recursively: give the work thread a large stack
    let h = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            println!("| case | size | store | phase | median | peak heap | check |");
            println!("|---|---|---|---|---|---|---|");
            for c in cases() {
                if !filters.is_empty() && !filters.iter().any(|f| c.id.to_lowercase().contains(&f.to_lowercase())) {
                    continue;
                }
                let sizes: &[usize] = if quick { &c.sizes[..1] } else { &c.sizes };
                for &n in sizes {
                    let mut rs = vec![
                        run_one(TreeStore::default, c.tree, n, runs),
                        run_one(|| HcStore::with(false, false), c.hc, n, runs),
                        run_one(|| HcStore::with(false, true), c.hc, n, runs),
                    ];
                    if c.stamps {
                        rs.push(run_one(|| HcStore::with(true, true), c.hc, n, runs));
                    }
                    let agree = rs.iter().all(|r| r.check == rs[0].check);
                    for r in &rs {
                        for (p, d) in &r.phases {
                            println!(
                                "| {} | {} | {} | {} | {} | {:.1} MB | {} |",
                                c.id,
                                n,
                                r.store,
                                p,
                                fmt(*d),
                                r.peak as f64 / 1e6,
                                if agree { "ok" } else { "MISMATCH" }
                            );
                        }
                    }
                }
            }
        })
        .unwrap();
    h.join().unwrap();
}
