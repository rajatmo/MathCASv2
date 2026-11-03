//! Class-M workloads (02-benchmarks.md section 5). Each builds its input in a fresh store, then times one evaluation
//! on the given pool and returns a store-independent check of everything observable (value, output, definitions).
use crate::eval::{Config, Eval, RunStats, Syms};
use crate::seq::Rng;
use crate::store::Store;
use crate::table::Table;
use crate::{mix, Term};
use rayon::prelude::*;
use rayon::ThreadPool;
use std::time::{Duration, Instant};

pub struct Out {
    pub time: Duration,
    pub check: u64,
    pub stats: RunStats,
    pub note: String,
}

fn timed<R: Send>(pool: &ThreadPool, f: impl FnOnce() -> R + Send) -> (R, Duration) {
    pool.install(|| {
        let t = Instant::now();
        let r = f();
        (r, t.elapsed())
    })
}

/// Run `t` on `ev` in `pool`; the check covers the value, the output log and the final value of `c`.
fn eval_out<Tb: Table>(pool: &ThreadPool, ev: &Eval<Tb>, t: Term) -> Out {
    let ((r, stats), time) = timed(pool, || ev.run(t));
    let st = ev.st;
    let v = r.expect("evaluation failed");
    let out = st.app(ev.s.list, &ev.output());
    let mut check = mix(st.fingerprint(v), st.fingerprint(out));
    if let Some(c) = ev.value_of(ev.s.c) {
        check = mix(mix(check, st.fingerprint(c)), ev.version_of(ev.s.c) as u64);
    }
    Out { time, check, stats, note: String::new() }
}

/// A random expression in add, mul, pow, sin, cos, exp, log over x, y and small integers (spike 1's R3 generator).
fn gen_calc<Tb: Table>(st: &Store<Tb>, c: &Syms, rng: &mut Rng, size: usize) -> Term {
    if size <= 1 {
        return match rng.below(3) {
            0 => c.x,
            1 => c.y,
            _ => st.int(rng.below(9) as i64 + 1),
        };
    }
    match rng.below(10) {
        0..=2 => {
            let k = 1 + rng.below(size as u64 - 1) as usize;
            let a = gen_calc(st, c, rng, k);
            let b = gen_calc(st, c, rng, size - k);
            let h = if rng.below(2) == 0 { c.add } else { c.mul };
            st.app(h, &[a, b])
        }
        3 => {
            let a = gen_calc(st, c, rng, size - 1);
            let n = st.int(rng.below(4) as i64 + 2);
            st.app(c.pow, &[a, n])
        }
        k => {
            let a = gen_calc(st, c, rng, size - 1);
            let h = [c.sin, c.cos, c.exp, c.log, c.sin, c.cos][(k - 4) as usize];
            st.app(h, &[a])
        }
    }
}

fn int_list<Tb: Table>(st: &Store<Tb>, list: Term, n: usize) -> Term {
    let xs: Vec<Term> = (0..n as i64).map(|i| st.int(i)).collect();
    st.app(list, &xs)
}

/// M6a: `plus(d(e_1), ..., d(e_64))`, each `e_i` a random expression of `n` nodes: cost visible in the term size.
pub fn m6_deriv<Tb: Table>(pool: &ThreadPool, cfg: Config, n: usize) -> Out {
    let st = Store::<Tb>::new();
    let ev = Eval::new(&st, cfg, 1);
    let (d, plus) = (st.sym("d"), st.sym("plus"));
    let args: Vec<Term> = (0..64).map(|i| st.app(d, &[gen_calc(&st, &ev.s, &mut Rng::new(1000 + i), n)])).collect();
    let t = st.app(plus, &args);
    eval_out(pool, &ev, t)
}

/// M6b: `plus(loop(k, 0), ..., loop(k, 63))`: each argument is a chain of `k` rewrites with a 3-node term, so its
/// cost is invisible in the size and only measurement finds it.
pub fn m6_loop<Tb: Table>(pool: &ThreadPool, cfg: Config, k: usize) -> Out {
    let st = Store::<Tb>::new();
    let ev = Eval::new(&st, cfg, 1);
    let plus = st.sym("plus");
    let args: Vec<Term> = (0..64).map(|i| st.app(ev.s.lp, &[st.int(k as i64), st.int(i)])).collect();
    let t = st.app(plus, &args);
    eval_out(pool, &ev, t)
}

/// M7: `map(inc, list(0, ..., n-1))`: tiny tasks.
pub fn m7_map<Tb: Table>(pool: &ThreadPool, cfg: Config, n: usize) -> Out {
    let st = Store::<Tb>::new();
    let ev = Eval::new(&st, cfg, 1);
    let (map, inc) = (st.sym("map"), st.sym("inc"));
    let t = st.app(map, &[inc, int_list(&st, ev.s.list, n)]);
    eval_out(pool, &ev, t)
}

/// M10: `list(stats(ptable(rwalk, n)), stats(pmap(rwalk, list(0..n/4))))`, under two session seeds. Every element
/// draws `WALK_LEN` times from its own stream.
pub fn m10_random<Tb: Table>(pool: &ThreadPool, cfg: Config, n: usize) -> Out {
    let mut total = Duration::ZERO;
    let mut checks = vec![];
    let mut stats = RunStats::default();
    for seed in [1, 2] {
        let st = Store::<Tb>::new();
        let ev = Eval::new(&st, cfg, seed);
        let (ptable, pmap, rwalk, stats_h) = (st.sym("ptable"), st.sym("pmap"), st.sym("rwalk"), st.sym("stats"));
        let a = st.app(stats_h, &[st.app(ptable, &[rwalk, st.int(n as i64)])]);
        let b = st.app(stats_h, &[st.app(pmap, &[rwalk, int_list(&st, ev.s.list, n / 4)])]);
        let t = st.app(ev.s.list, &[a, b]);
        let o = eval_out(pool, &ev, t);
        total += o.time;
        stats.splits += o.stats.splits;
        stats.redos += o.stats.redos;
        checks.push(o.check);
    }
    // the two seeds must give different draws (the streams are really used)
    assert_ne!(checks[0], checks[1], "M10: seeds 1 and 2 agree");
    Out { time: total, check: mix(checks[0], checks[1]), stats, note: String::new() }
}

pub const M11_SHAPES: [&str; 3] = ["prints", "defines", "nested"];

/// M11: arguments that print or define in the middle. `k` is the cost of one heavy argument.
/// - prints: `plus` of 64 heavy pure arguments, five of them wrapped in `print`;
/// - defines: 64 arguments, the odd ones reading the global `c` (`loopc`), with `define(c, ...)` at positions 20 and
///   45 and `print(c)` at 46: arguments right of a definition that read `c` must be re-evaluated, the others kept;
/// - nested: a random tree of `plus` nodes (depth 3) over heavy, reading, printing, defining and random-drawing leaves,
///   including `ptable` with printing elements and `table` drawing from the session stream. Shapes 3 and up are
///   further random trees of the same kind (the fuzz case).
pub fn m11_effects<Tb: Table>(pool: &ThreadPool, cfg: Config, shape: usize, k: usize) -> Out {
    let st = Store::<Tb>::new();
    let ev = Eval::new(&st, cfg, 5);
    let s = &ev.s;
    let (plus, define) = (st.sym("plus"), st.sym("define"));
    let int = |v: usize| st.int(v as i64);
    let heavy = |k: usize, i: usize| st.app(s.lp, &[int(k), int(i)]);
    let reading = |k: usize, i: usize| st.app(s.loopc, &[int(k), int(i)]);
    let print = |t: Term| st.app(s.print, &[t]);
    let def = |v: usize| st.app(define, &[s.c, int(v)]);
    // c starts with a value, set by an earlier top-level evaluation
    pool.install(|| ev.run(def(1)).0.unwrap());
    let t = match shape {
        0 => {
            let a: Vec<Term> =
                (0..64).map(|i| if [7, 23, 40, 41, 58].contains(&i) { print(heavy(k, i)) } else { heavy(k, i) }).collect();
            st.app(plus, &a)
        }
        1 => {
            let a: Vec<Term> = (0..64)
                .map(|i| match i {
                    20 => def(2),
                    45 => def(3),
                    46 => print(s.c),
                    i if i % 2 == 1 => reading(k / 4, i),
                    i => heavy(k, i),
                })
                .collect();
            st.app(plus, &a)
        }
        _ => {
            let (ptable, table, rwalk, pr, stats, random) =
                (st.sym("ptable"), st.sym("table"), st.sym("rwalk"), st.sym("pr"), st.sym("stats"), s.random);
            let mut rng = Rng::new(9 + shape as u64);
            let mut leaf = |rng: &mut Rng| {
                let i = rng.below(1000) as usize;
                match rng.below(9) {
                    0 => heavy(k, i),
                    1 => reading(k / 4, i),
                    2 => print(heavy(k / 4, i)),
                    3 => def(i),
                    4 => print(s.c),
                    5 => st.app(random, &[int(100)]),
                    6 => st.app(stats, &[st.app(ptable, &[rwalk, int(16)])]),
                    7 => st.app(ptable, &[pr, int(3)]),
                    _ => st.app(table, &[rwalk, int(4)]),
                }
            };
            fn tree<Tb: Table>(st: &Store<Tb>, plus: Term, rng: &mut Rng, depth: u32, leaf: &mut impl FnMut(&mut Rng) -> Term) -> Term {
                if depth == 0 {
                    return leaf(rng);
                }
                let n = 3 + rng.below(4) as usize;
                let a: Vec<Term> = (0..n).map(|_| tree(st, plus, rng, depth - 1, leaf)).collect();
                st.app(plus, &a)
            }
            tree(&st, plus, &mut rng, 3, &mut leaf)
        }
    };
    let mut o = eval_out(pool, &ev, t);
    o.note = format!("{} outputs", ev.output().len());
    o
}

/// M8: concurrent interning. `n` constructions of `g(i)` and `f(i, g(i))`: half of the items come from a shared
/// pool of `n/32` values that every worker rebuilds (overlapping), half are distinct. Checked: rebuilding every item
/// afterwards returns the id first returned (canonical ids, whichever thread built the term), and the number of
/// stored terms is exactly the number of distinct terms.
pub fn m8_intern<Tb: Table>(pool: &ThreadPool, n: usize) -> Out {
    let st = Store::<Tb>::new();
    let (f, g) = (st.sym("f"), st.sym("g"));
    let k = n / 2;
    let shared = (k / 16).max(1);
    let item = |j: usize| if j.is_multiple_of(2) { ((j / 2) % shared) as i64 } else { (shared + j) as i64 };
    let build = |j: usize| {
        let i = st.int(item(j));
        let gi = st.app(g, &[i]);
        st.app(f, &[i, gi])
    };
    let (ids, time) = timed(pool, || (0..k).into_par_iter().with_min_len(1024).map(build).collect::<Vec<Term>>());
    let canonical = pool.install(|| ids.par_iter().enumerate().with_min_len(1024).all(|(j, &t)| build(j) == t));
    assert!(canonical, "M8: an item rebuilt later has a different id");
    let distinct = shared.min(k.div_ceil(2)) + k / 2;
    assert_eq!(st.len(), 2 * distinct + 2, "M8: stored terms are not exactly the distinct terms");
    let check = ids.iter().fold(st.len() as u64, |h, &t| mix(h, st.meta(t).0));
    Out { time, check, stats: RunStats::default(), note: format!("{:.0} MB reserved", st.reserved_bytes() as f64 / 1e6) }
}
