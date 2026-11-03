//! Parallel-evaluation spike harness. For each class-M case, size, store design and granularity rule, runs the
//! evaluation at each thread count (median of the runs, each on a fresh store) and checks every result against the
//! sequential reference (`never` split, one thread). Then the single-thread regression: the concurrent stores at one
//! thread against spike 1's sequential `hc+imm` on R10 and R11.
//!
//! usage: parallel [--quick | --full] [--runs N] [--threads 1,2,4] [--stores ABC] [--grain G] [--poll P] [case...]
//!   --quick  smallest sizes, one run, 1/2/4 threads (seconds)
//!   (none)   two smallest sizes, three runs, 1/2/4 threads
//!   --full   all sizes, five runs, 1/2/4/8/16 threads (for an idle machine)
use parallel::eval::{Abort, Config, Eval, Split};
use parallel::seq;
use parallel::store::{Shared, Store};
use parallel::table::{Locked, Optimistic, Sharded, Table};
use parallel::work::{self, Out, M11_SHAPES};
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::time::Duration;

type Run = Box<dyn Fn(char, &ThreadPool, Config, usize) -> Out>;

macro_rules! by_store {
    ($f:ident, $s:expr, $($a:expr),*) => {
        match $s {
            'A' => work::$f::<Locked>($($a),*),
            'B' => work::$f::<Sharded>($($a),*),
            _ => work::$f::<Optimistic>($($a),*),
        }
    };
}

struct Case {
    id: String,
    sizes: [usize; 3],
    /// (store, split, largest size to run it at)
    configs: Vec<(char, Split, usize)>,
    /// Interning only: no evaluator, so no split rule and no sequential-evaluator reference.
    store_only: bool,
    run: Run,
}

fn cases() -> Vec<Case> {
    use Split::*;
    let all = usize::MAX;
    let mut v = vec![
        Case {
            id: "M6a deriv".into(),
            sizes: [1_000, 5_000, 20_000],
            configs: vec![('C', Lazy, all), ('C', Static, all), ('C', Always, all), ('B', Lazy, all), ('A', Lazy, all)],
            store_only: false,
            run: Box::new(|s, p, c, n| by_store!(m6_deriv, s, p, c, n)),
        },
        Case {
            id: "M6b loop".into(),
            sizes: [1_000, 10_000, 50_000],
            configs: vec![('C', Lazy, all), ('C', Static, all), ('C', Always, all), ('A', Lazy, all)],
            store_only: false,
            run: Box::new(|s, p, c, n| by_store!(m6_loop, s, p, c, n)),
        },
        Case {
            id: "M7 map".into(),
            sizes: [100_000, 1_000_000, 10_000_000],
            configs: vec![('C', Lazy, all), ('C', Static, all), ('C', Always, 1_000_000)],
            store_only: false,
            run: Box::new(|s, p, c, n| by_store!(m7_map, s, p, c, n)),
        },
        Case {
            id: "M8 intern".into(),
            sizes: [1_000_000, 10_000_000, 100_000_000],
            // the one-lock baseline only collapses further at the largest size: not worth minutes of the full run
            configs: vec![('A', Never, 10_000_000), ('B', Never, all), ('C', Never, all)],
            store_only: true,
            run: Box::new(|s, p, _, n| by_store!(m8_intern, s, p, n)),
        },
        Case {
            id: "M10 ptable".into(),
            sizes: [10_000, 100_000, 1_000_000],
            configs: vec![('C', Lazy, all), ('C', Always, 100_000), ('A', Lazy, 100_000)],
            store_only: false,
            run: Box::new(|s, p, c, n| by_store!(m10_random, s, p, c, n)),
        },
    ];
    // 20 more random nested trees with cheap leaves: many effect placements, checked at every thread count
    v.push(Case {
        id: "M11 fuzz".into(),
        sizes: [100, 300, 1_000],
        configs: vec![('C', Lazy, all), ('C', Always, all), ('B', Always, all)],
        store_only: false,
        run: Box::new(|s, p, c, n| {
            let mut out = by_store!(m11_effects, s, p, c, 3, n);
            for shape in 4..23 {
                let o = by_store!(m11_effects, s, p, c, shape, n);
                out.time += o.time;
                out.check = parallel::mix(out.check, o.check);
                out.stats.redos += o.stats.redos;
                out.stats.splits += o.stats.splits;
            }
            out.note = String::new();
            out
        }),
    });
    for (i, name) in M11_SHAPES.iter().enumerate() {
        v.push(Case {
            id: format!("M11 {name}"),
            sizes: [1_000, 5_000, 20_000],
            configs: vec![('C', Lazy, all), ('C', LazyStop, all), ('C', Always, all), ('A', Always, all)],
            store_only: false,
            run: Box::new(move |s, p, c, n| by_store!(m11_effects, s, p, c, i, n)),
        });
    }
    v
}

fn pool(threads: usize) -> ThreadPool {
    // deep terms recurse in the evaluator and the derivative kernel
    ThreadPoolBuilder::new().num_threads(threads).stack_size(256 << 20).build().unwrap()
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn store_name(s: char) -> &'static str {
    match s {
        'A' => Locked::NAME,
        'B' => Sharded::NAME,
        _ => Optimistic::NAME,
    }
}

struct Opts {
    sizes: Vec<usize>,
    runs: usize,
    threads: Vec<usize>,
    stores: String,
    filters: Vec<String>,
    /// Overrides of the granularity constants (for exploring the rule).
    grain: Option<u64>,
    poll: Option<u64>,
}

impl Opts {
    fn cfg(&self, split: Split) -> Config {
        let mut c = Config::new(split);
        c.grain = self.grain.unwrap_or(c.grain);
        c.poll = self.poll.unwrap_or(c.poll);
        c
    }
}

fn run_case(c: &Case, o: &Opts, pools: &[(usize, ThreadPool)], failures: &mut u32) {
    for &si in &o.sizes {
        let n = c.sizes[si];
        // the reference: the sequential evaluator, or (interning only) store C at one thread
        let one = &pools.iter().find(|(t, _)| *t == 1).expect("thread count 1 is required").1;
        let (rs, rsplit) = ('C', Split::Never);
        let mut ref_times = vec![];
        let mut reference = 0;
        let mut seq_note = String::new();
        // long cases (a sequential run over 2 s) take at most three runs
        let mut runs = o.runs;
        let mut r = 0;
        while r < runs {
            r += 1;
            let out = (c.run)(rs, one, o.cfg(rsplit), n);
            if out.time.as_secs_f64() > 2.0 {
                runs = runs.min(3);
            }
            ref_times.push(out.time);
            reference = out.check;
            seq_note = out.note;
        }
        let t_seq = median(ref_times);
        if !c.store_only {
            println!(
                "| {} | {} | {} | {} | 1 | {:.2} | | | 1.00 | 0 | 0 | ref {} |",
                c.id,
                n,
                store_name(rs),
                rsplit.name(),
                ms(t_seq),
                seq_note
            );
        }
        for &(s, split, max) in &c.configs {
            if !o.stores.contains(s) || n > max {
                continue;
            }
            let mut t1 = None;
            for (threads, p) in pools {
                let mut times = vec![];
                let (mut ok, mut splits, mut redos, mut note) = (true, 0, 0, String::new());
                for _ in 0..runs {
                    let out = (c.run)(s, p, o.cfg(split), n);
                    ok &= out.check == reference;
                    times.push(out.time);
                    (splits, redos, note) = (out.stats.splits, out.stats.redos, out.note);
                }
                let t = median(times);
                let base = *t1.get_or_insert(t);
                let speedup = ms(base) / ms(t);
                if !ok {
                    *failures += 1;
                }
                println!(
                    "| {} | {} | {} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {} | {} | {} {} |",
                    c.id,
                    n,
                    store_name(s),
                    if c.store_only { "-" } else { split.name() },
                    threads,
                    ms(t),
                    speedup,
                    speedup / *threads as f64,
                    ms(t) / ms(t_seq),
                    splits,
                    redos,
                    if ok { "ok" } else { "MISMATCH" },
                    note
                );
            }
        }
    }
}

/// A fuel limit stops every task (semantics 18.4): a 64-argument computation of about 3 * 10^6 fuel under a budget
/// of 10^5 must fail with `Limit` at every thread count and split rule.
fn limit_check(pools: &[(usize, ThreadPool)], failures: &mut u32) {
    for (threads, p) in pools {
        for split in [Split::Never, Split::Lazy, Split::Always] {
            let st = Store::<Optimistic>::new();
            let mut cfg = Config::new(split);
            cfg.fuel_limit = 100_000;
            let ev = Eval::new(&st, cfg, 1);
            let plus = st.sym("plus");
            let args: Vec<_> = (0..64).map(|i| st.app(ev.s.lp, &[st.int(50_000), st.int(i)])).collect();
            let t = st.app(plus, &args);
            let (r, stats) = p.install(|| ev.run(t));
            // an interrupt requested while tasks run (here: before the next evaluation) stops it the same way
            let ev2 = Eval::new(&st, Config::new(split), 1);
            ev2.interrupt();
            let (r2, _) = p.install(|| ev2.run(t));
            let ok = r == Err(Abort::Limit) && stats.fuel < 1_000_000 && r2 == Err(Abort::Limit);
            if !ok {
                *failures += 1;
            }
            println!(
                "| LIMIT | fuel 10^5, interrupt | {} | {} | {} | fuel used by the root task {} | {} |",
                Optimistic::NAME,
                split.name(),
                threads,
                stats.fuel,
                if ok { "ok" } else { "FAIL" }
            );
        }
    }
}

/// A store's name, its phases' median times, and its fingerprint.
type Row = (&'static str, Vec<(&'static str, Duration)>, u64);

fn on_seq(r10: bool, n: usize) -> seq::work::Out {
    let mut st = seq::hc::HcStore::with(false, true);
    if r10 { seq::work::r10_replace(&mut st, n) } else { seq::work::r11_reeval(&mut st, n) }
}

fn on_conc<Tb: Table>(r10: bool, n: usize) -> seq::work::Out {
    let st = Store::<Tb>::new();
    let mut sh = Shared(&st);
    if r10 { seq::work::r10_replace(&mut sh, n) } else { seq::work::r11_reeval(&mut sh, n) }
}

/// Spike 1's R10/R11 on its sequential `hc+imm` and on each concurrent store at one thread.
fn regression(o: &Opts, failures: &mut u32) {
    println!("\n| case | size | store | phase | median ms | vs hc+imm | check |");
    println!("|---|---|---|---|---|---|---|");
    let sizes = [10_000, 100_000, 1_000_000];
    for (id, r10) in [("R10 replace", true), ("R11 eval", false)] {
        for &si in &o.sizes {
            let n = sizes[si];
            let mut rows: Vec<Row> = vec![];
            let med = |runs: Vec<seq::work::Out>| {
                let phases = (0..runs[0].phases.len())
                    .map(|i| (runs[0].phases[i].0, median(runs.iter().map(|r| r.phases[i].1).collect())))
                    .collect();
                (phases, runs[0].check)
            };
            let (p, c) = med((0..o.runs).map(|_| on_seq(r10, n)).collect());
            rows.push(("hc+imm (spike 1)", p, c));
            for s in ['A', 'B', 'C'] {
                if !o.stores.contains(s) {
                    continue;
                }
                let runs = (0..o.runs)
                    .map(|_| match s {
                        'A' => on_conc::<Locked>(r10, n),
                        'B' => on_conc::<Sharded>(r10, n),
                        _ => on_conc::<Optimistic>(r10, n),
                    })
                    .collect();
                let (p, c) = med(runs);
                rows.push((store_name(s), p, c));
            }
            let agree = rows.iter().all(|r| r.2 == rows[0].2);
            if !agree {
                *failures += 1;
            }
            for (name, phases, _) in &rows {
                for (i, (ph, d)) in phases.iter().enumerate() {
                    println!(
                        "| {} | {} | {} | {} | {:.2} | {:.2} | {} |",
                        id,
                        n,
                        name,
                        ph,
                        ms(*d),
                        ms(*d) / ms(rows[0].1[i].1),
                        if agree { "ok" } else { "MISMATCH" }
                    );
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let (quick, full) = (flag("--quick"), flag("--full"));
    let mut o = Opts {
        sizes: if quick { vec![0] } else if full { vec![0, 1, 2] } else { vec![0, 1] },
        runs: value("--runs").and_then(|v| v.parse().ok()).unwrap_or(if quick { 1 } else if full { 5 } else { 3 }),
        threads: if full { vec![1, 2, 4, 8, 16] } else { vec![1, 2, 4] },
        stores: value("--stores").unwrap_or("ABC".into()),
        filters: vec![],
        grain: value("--grain").and_then(|v| v.parse().ok()),
        poll: value("--poll").and_then(|v| v.parse().ok()),
    };
    if let Some(t) = value("--threads") {
        o.threads = t.split(',').filter_map(|x| x.parse().ok()).collect();
    }
    let skip_values: Vec<String> = ["--runs", "--threads", "--stores", "--grain", "--poll"].iter().filter_map(|f| value(f)).collect();
    o.filters = args.iter().filter(|a| !a.starts_with("--") && !skip_values.contains(a)).map(|a| a.to_lowercase()).collect();
    if !o.threads.contains(&1) {
        o.threads.insert(0, 1);
    }
    // setup recursion (random expression trees) needs a large stack on the main thread too
    let h = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            let wanted = |id: &str| o.filters.is_empty() || o.filters.iter().any(|f| id.to_lowercase().contains(f));
            let pools: Vec<(usize, ThreadPool)> = o.threads.iter().map(|&t| (t, pool(t))).collect();
            let mut failures = 0;
            println!("| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |");
            println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
            for c in cases() {
                if wanted(&c.id) {
                    run_case(&c, &o, &pools, &mut failures);
                }
            }
            if wanted("limit") {
                limit_check(&pools, &mut failures);
            }
            if wanted("r10") || wanted("r11") || wanted("regression") {
                regression(&o, &mut failures);
            }
            if failures > 0 {
                println!("\n{failures} FAILURES");
                std::process::exit(1);
            }
            println!("\nall checks ok");
        })
        .unwrap();
    h.join().unwrap();
}
