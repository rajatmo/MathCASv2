//! Workloads as sessions of evaluations. Each evaluation runs inside the thread pool with shared borrows of the heap
//! and the session state ([`Session::step`]); a safe point follows it. A workload keeps handles between evaluations
//! only in its [`Roots`]; inside an evaluation it uses branded handles freely, on any number of threads.
//!
//! - spike 4's M6a (`plus` of 64 derivatives), M8 (concurrent interning with a shared pool and canonical-id checks)
//!   and M10 (`ptable`/`pmap` of random walks), each repeated over several evaluations of a session;
//! - spike 1b's S3 (matrix element replacements, one per evaluation), S8 (allocation, nothing dies) and LS (long
//!   session: 1000 evaluations of temporary expressions, kept results pointing backwards, redefinitions), with
//!   construction and evaluation going through spike 4's parallel evaluator.
//!
//! Every workload returns a check computed from structure only (fingerprints, structural hashes), which must agree
//! with the sequential, non-collecting reference.
use crate::eval::{Config, Defs, Eval, Syms};
use crate::heap::{self, Heap, View};
use crate::{mix, Rng, Roots, Sym, T};
use rayon::prelude::*;
use std::time::{Duration, Instant};

pub struct Session {
    pub heap: Heap,
    pub defs: Defs,
    pub keep: Roots,
    /// time inside evaluations, and in safe points
    pub t_work: Duration,
    pub t_safe: Duration,
    pub evals: usize,
}

impl Session {
    pub fn new(cfg: heap::Config, seed: u64) -> Session {
        let heap = Heap::new(cfg);
        let defs = Defs::new(&heap, seed);
        Session { heap, defs, keep: Roots::new(), t_work: Duration::ZERO, t_safe: Duration::ZERO, evals: 0 }
    }

    /// One evaluation, then a safe point. Call it on a thread of the pool the evaluation should use (the driver
    /// runs the whole session inside `ThreadPool::install`). `f` gets shared borrows for a scope `'h` it cannot name
    /// outside, so no branded handle can be returned (the result type `R` is fixed outside the scope) or stored
    /// anywhere but in the roots; and since rayon's `join` and parallel iterators return only once every task they
    /// spawned has finished (detached `spawn` needs `'static` closures, which a branded handle is not), every task
    /// of the evaluation has stopped when `f` returns: the safe point is a global barrier.
    pub fn step<R>(&mut self, f: impl for<'h> FnOnce(&'h Heap, &'h Defs, &mut Roots) -> R) -> R {
        let Session { heap, defs, keep, .. } = self;
        let t0 = Instant::now();
        let r = f(&*heap, &*defs, &mut *keep);
        let t1 = Instant::now();
        heap.safe_point(&mut [defs, keep], false);
        self.t_work += t1 - t0;
        self.t_safe += t1.elapsed();
        self.evals += 1;
        r
    }

    /// A fingerprint of every kept handle and of the output log.
    pub fn check_all(&self) -> u64 {
        let h = &self.heap;
        let mut c = self.keep.len() as u64;
        for i in 0..self.keep.len() {
            c = mix(c, h.fingerprint(self.keep.get(h, i)));
        }
        for t in self.defs.output(h) {
            c = mix(c, h.fingerprint(t));
        }
        c
    }

    /// End of a workload: collect everything but the live set (for "memory after collection").
    pub fn finish(&mut self) {
        let Session { heap, defs, keep, .. } = self;
        heap.collect_full(&mut [defs, keep]);
    }
}

pub struct Out {
    pub check: u64,
    pub notes: Vec<(&'static str, u64)>,
}

/// A random expression in add, mul, pow, sin, cos, exp, log over x, y and small integers (spike 1's R3 generator).
fn gen_calc<'h>(st: &'h Heap, c: &Syms, rng: &mut Rng, size: usize) -> T<'h> {
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

fn int_list<'h>(st: &'h Heap, list: T<'h>, n: usize) -> T<'h> {
    let xs: Vec<T<'h>> = (0..n as i64).map(|i| st.int(i)).collect();
    st.app(list, &xs)
}

fn args_of<'h>(st: &'h Heap, t: T<'h>) -> &'h [T<'h>] {
    match st.view(t) {
        View::App(_, a) => a,
        _ => &[],
    }
}

// ---------------------------------------------------------------------------------------------------------------
// M6a: `rounds` times, an evaluation that builds `plus(d(e_1), ..., d(e_64))` (the 64 random expressions of `n`
// nodes built in parallel), then an evaluation of it; the result replaces the input in the session.

pub fn m6a(s: &mut Session, cfg: Config, n: usize, rounds: usize) -> Out {
    let (d, plus) = (s.heap.sym("d"), s.heap.sym("plus"));
    for r in 0..rounds {
        s.step(|h: &Heap, defs: &Defs, keep: &mut Roots| {
            let args: Vec<T> = (0..64u64)
                .into_par_iter()
                .map(|i| h.app(d.t(), &[gen_calc(h, &defs.s, &mut Rng::new(1000 + 64 * r as u64 + i), n)]))
                .collect();
            keep.push(h.app(plus.t(), &args));
        });
        s.step(|h: &Heap, defs: &Defs, keep: &mut Roots| {
            let t = keep.get(h, r);
            let v = Eval::new(h, defs, cfg).run(t).0.expect("M6a evaluation failed");
            keep.set(r, v);
        });
    }
    Out { check: s.check_all(), notes: vec![] }
}

// ---------------------------------------------------------------------------------------------------------------
// M8: `n/2` constructions of `g(i)` and `f(i, g(i))` in 10 evaluations (parallel), half from a shared pool that
// every worker rebuilds, half distinct; each evaluation's items are kept as one list. A final evaluation rebuilds
// every item in parallel and checks it gets the id it has in the kept list (canonical across threads and across
// promotions); after a full collection the heap holds exactly the distinct terms.

pub fn m8(s: &mut Session, n: usize) -> Out {
    let (f, g, list) = (s.heap.sym("f"), s.heap.sym("g"), s.heap.sym("list"));
    let k = n / 2;
    let shared = (k / 16).max(1);
    let item = move |j: usize| if j.is_multiple_of(2) { ((j / 2) % shared) as i64 } else { (shared + j) as i64 };
    fn build<'h>(h: &'h Heap, f: Sym, g: Sym, v: i64) -> T<'h> {
        let i = h.int(v);
        let gi = h.app(g.t(), &[i]);
        h.app(f.t(), &[i, gi])
    }
    let rounds = 10;
    for r in 0..rounds {
        let (lo, hi) = (r * k / rounds, (r + 1) * k / rounds);
        s.step(|h: &Heap, _: &Defs, keep: &mut Roots| {
            let ids: Vec<T> = (lo..hi).into_par_iter().with_min_len(1024).map(|j| build(h, f, g, item(j))).collect();
            keep.push(h.app(list.t(), &ids));
        });
    }
    let ok = s.step(|h: &Heap, _: &Defs, keep: &mut Roots| {
        (0..rounds).all(|r| {
            let lo = r * k / rounds;
            let a = args_of(h, keep.get(h, r));
            a.par_iter().enumerate().with_min_len(1024).all(|(j, &t)| build(h, f, g, item(lo + j)) == t)
        })
    });
    assert!(ok, "M8: an item rebuilt later has a different id");
    s.finish();
    let distinct = shared.min(k.div_ceil(2)) + k / 2;
    let stored = s.heap.stored();
    assert_eq!(stored, 2 * distinct + rounds, "M8: stored terms are not exactly the distinct live terms");
    let h = &s.heap;
    let check = (0..rounds).fold(stored as u64, |c, r| mix(c, h.meta(s.keep.get(h, r)).0));
    Out { check, notes: vec![("stored", stored as u64)] }
}

// ---------------------------------------------------------------------------------------------------------------
// M10: `rounds` evaluations of `list(stats(ptable(rwalk, n)), stats(pmap(rwalk, list(0..n/4))))`; each draws its
// seeds from the session stream, so successive rounds differ.

pub fn m10(s: &mut Session, cfg: Config, n: usize, rounds: usize) -> Out {
    let (ptable, pmap, rwalk, stats) = (s.heap.sym("ptable"), s.heap.sym("pmap"), s.heap.sym("rwalk"), s.heap.sym("stats"));
    for _ in 0..rounds {
        s.step(|h: &Heap, defs: &Defs, keep: &mut Roots| {
            let list = defs.s.list;
            let a = h.app(stats.t(), &[h.app(ptable.t(), &[rwalk.t(), h.int(n as i64)])]);
            let b = h.app(stats.t(), &[h.app(pmap.t(), &[rwalk.t(), int_list(h, list, n / 4)])]);
            let t = h.app(list, &[a, b]);
            keep.push(Eval::new(h, defs, cfg).run(t).0.expect("M10 evaluation failed"));
        });
    }
    let h = &s.heap;
    assert!(rounds < 2 || s.keep.get(h, 0) != s.keep.get(h, 1), "M10: successive rounds agree");
    Out { check: s.check_all(), notes: vec![] }
}

// ---------------------------------------------------------------------------------------------------------------
// S3: a 1000 x (n/1000) integer matrix (rows built in parallel), then 10^4 element replacements, one evaluation
// each; every old row and outer list becomes garbage.

pub fn s3(s: &mut Session, n: usize) -> Out {
    s3_steps(s, n, 10_000)
}

/// S3 with `steps` replacements.
pub fn s3_steps(s: &mut Session, n: usize, steps: i64) -> Out {
    let list = s.heap.sym("list");
    let (rows, cols) = (1000usize, (n / 1000).max(1));
    s.step(|h: &Heap, _: &Defs, keep: &mut Roots| {
        let rs: Vec<T> = (0..rows)
            .into_par_iter()
            .map(|i| {
                let row: Vec<T> = (0..cols).map(|j| h.int((i * cols + j) as i64)).collect();
                h.app(list.t(), &row)
            })
            .collect();
        keep.push(h.app(list.t(), &rs));
    });
    let mut rng = Rng::new(33);
    for k in 0..steps {
        let (i, j) = (rng.below(rows as u64) as usize, rng.below(cols as u64) as usize);
        s.step(|h: &Heap, _: &Defs, keep: &mut Roots| {
            let m = keep.get(h, 0);
            let mut rs = args_of(h, m).to_vec();
            let mut row = args_of(h, rs[i]).to_vec();
            row[j] = h.int(-k);
            rs[i] = h.app(list.t(), &row);
            keep.set(0, h.app(list.t(), &rs));
        });
    }
    Out { check: s.check_all(), notes: vec![] }
}

// ---------------------------------------------------------------------------------------------------------------
// S8: n distinct `f(i, g(i))`, all kept: 100 evaluations each build n/100 of them in parallel as one list.

pub fn s8(s: &mut Session, n: usize) -> Out {
    let (f, g, list) = (s.heap.sym("f"), s.heap.sym("g"), s.heap.sym("list"));
    let chunk = n / 100;
    for c in 0..100 {
        s.step(|h: &Heap, _: &Defs, keep: &mut Roots| {
            let v: Vec<T> = (0..chunk)
                .into_par_iter()
                .with_min_len(1024)
                .map(|k| {
                    let a = h.int((c * chunk + k) as i64);
                    let gb = h.app(g.t(), &[a]);
                    h.app(f.t(), &[a, gb])
                })
                .collect();
            keep.push(h.app(list.t(), &v));
        });
    }
    Out { check: s.check_all(), notes: vec![] }
}

// ---------------------------------------------------------------------------------------------------------------
// LS: spike 1b's long session on the parallel evaluator. 1000 evaluations; each builds a random temporary
// expression of `size` nodes over `inc`, `dbl`, `add`, `wrap`, `pair`, small integers, ten plain symbols and five
// symbols with own values, evaluates it (in parallel), and keeps `out(k, piece of the normal form, result k/2)`.
// Every 50th evaluation is a redefinition of one of the five symbols.

fn gen_tree<'h>(st: &'h Heap, heads: &[Sym], leaves: &[Sym], rng: &mut Rng, size: usize) -> T<'h> {
    if size <= 1 {
        return if rng.below(2) == 0 { st.int(rng.below(20) as i64) } else { leaves[rng.below(leaves.len() as u64) as usize].t() };
    }
    let arity = (1 + rng.below(3) as usize).min(size - 1);
    let mut rest = size - 1;
    let mut args = vec![];
    for i in 0..arity {
        let k = if i + 1 == arity { rest } else { 1 + rng.below((rest - (arity - i - 1)) as u64) as usize };
        let k = k.max(1);
        rest -= k.min(rest);
        args.push(gen_tree(st, heads, leaves, rng, k));
    }
    let h = heads[rng.below(heads.len() as u64) as usize];
    st.app(h.t(), &args)
}

pub fn long_session(s: &mut Session, cfg: Config, size: usize) -> Out {
    long_session_n(s, cfg, size, 1000)
}

/// The long session with `evals` evaluations.
pub fn long_session_n(s: &mut Session, cfg: Config, size: usize, evals: usize) -> Out {
    let hp = &s.heap;
    let sy = |n: &str| hp.sym(n);
    let defined: Vec<Sym> = (0..5).map(|i| sy(&format!("c{i}"))).collect();
    let heads = vec![sy("inc"), sy("dbl"), sy("add"), sy("wrap"), sy("pair")];
    let plain: Vec<Sym> = (0..10).map(|i| sy(&format!("s{i}"))).collect();
    let (out, define) = (sy("out"), sy("define"));
    let mut leaves = plain.clone();
    leaves.extend(defined.iter().copied());
    let mut rng = Rng::new(12);
    // own values do not mention the defined symbols, so they cannot loop
    let redefine = |s: &mut Session, rng: &mut Rng, j: usize| {
        let seed = rng.next_u64();
        s.step(|h: &Heap, defs: &Defs, _: &mut Roots| {
            let rhs = gen_tree(h, &heads, &plain, &mut Rng::new(seed), 200);
            Eval::new(h, defs, cfg).run(h.app(define.t(), &[defined[j].t(), rhs])).0.expect("definition failed");
        });
    };
    for j in 0..5 {
        redefine(s, &mut rng, j);
    }
    for k in 0..evals {
        if k % 50 == 49 {
            redefine(s, &mut rng, (k / 50) % 5);
        }
        let seed = rng.next_u64();
        s.step(|h: &Heap, defs: &Defs, keep: &mut Roots| {
            let temp = gen_tree(h, &heads, &leaves, &mut Rng::new(seed), size);
            let nf = Eval::new(h, defs, cfg).run(temp).0.expect("LS evaluation failed");
            let mut piece = nf;
            for d in 0..4 {
                let a = args_of(h, piece);
                if a.is_empty() {
                    break;
                }
                piece = a[(k + d) % a.len()];
            }
            let prev = if k > 0 { keep.get(h, k / 2) } else { h.int(0) };
            keep.push(h.app(out.t(), &[h.int(k as i64), piece, prev]));
        });
    }
    // fingerprint each result's own parts and check its back pointer by identity (spike 1b)
    let h = &s.heap;
    let mut check = 0u64;
    for &c in &defined {
        check = mix(check, h.fingerprint(s.defs.value_of(h, c).expect("own value lost")));
    }
    for k in 0..s.keep.len() {
        let a = args_of(h, s.keep.get(h, k));
        assert_eq!(h.as_int(a[0]), Some(k as i64));
        assert!(k == 0 || a[2] == s.keep.get(h, k / 2), "back pointer of result {k} lost");
        check = mix(check, h.fingerprint(a[1]));
    }
    Out { check, notes: vec![] }
}
