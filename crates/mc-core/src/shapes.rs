//! Session shapes for stress tests and measurements: I1's S3, S8, LS and M8 and an I3-style number workload, as
//! sessions of evaluations with a safe point after each. Each returns a check computed from structure only
//! ([`fingerprint`]), so a collecting run must agree with a non-collecting, single-threaded reference.
//!
//! - **S3**: a 1000-row integer matrix (rows built in parallel), then single-element replacements, one per
//!   evaluation; every replaced row and outer list becomes garbage.
//! - **S8**: distinct `f(i, g(i))` in parallel, all kept (nothing dies; promotion-heavy).
//! - **LS**: a long session. Each evaluation builds a random temporary tree in parallel, normalises it with a small
//!   memoised rewriter (`inc`, `dbl`, `add` on integers; five symbols with own values), and keeps
//!   `out(k, piece of the normal form, result k/2)`. Every 50th evaluation redefines an own value, which invalidates
//!   the memo by epoch.
//! - **M8**: concurrent construction from a shared pool that every worker rebuilds; a final evaluation rebuilds
//!   everything and checks it gets the kept handles; after a full collection the heap holds exactly the distinct
//!   live terms.
//! - **NUM**: big integers of 1 to 130 limbs (arena and boxed payloads), one-limb words, rationals, balls, strings and
//!   reals inside compounds; a third are kept, and a final evaluation rebuilds the kept ones and checks identity.
//!
//! Every loop is bounded by the shape's size parameters.
use crate::hash::{bytes_hash, finalize, mix, K};
use crate::{Config, Heap, Roots, Session, Store, Sym, View, T};
use rayon::prelude::*;
use rustc_hash::FxHashMap;

/// A store-independent structural fingerprint: equal for equal terms in any [`Store`]. Iterative, memoised per
/// handle (terms are DAGs).
pub fn fingerprint<'h, S: Store>(st: &'h S, t: T<'h>) -> u64 {
    let mut done: FxHashMap<u32, u64> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    while let Some((u, expanded)) = stack.pop() {
        if done.contains_key(&u.raw()) {
            continue;
        }
        let v = st.view(u);
        let kids: Vec<T<'h>> = match v {
            View::App { head, args } => std::iter::once(head).chain(args.iter().copied()).collect(),
            View::Rational { num, den } => vec![num, den],
            _ => vec![],
        };
        if !expanded && !kids.is_empty() {
            stack.push((u, true));
            stack.extend(kids.iter().map(|&k| (k, false)));
            continue;
        }
        let words = |seed: u64, w: &[u64]| w.iter().fold(seed, |h, &x| mix(h, x));
        let fp = match v {
            View::Int(i) => mix(1, i as u64),
            View::BigInt { neg, mag } => words(mix(2, neg as u64), mag),
            View::Rational { .. } => mix(mix(3, done[&kids[0].raw()]), done[&kids[1].raw()]),
            View::Real(r) => mix(4, r.to_bits()),
            View::Ball { neg, words: w } => words(mix(5, neg as u64), w),
            View::Str(s) => mix(6, bytes_hash(s.as_bytes())),
            View::Sym(s) => mix(7, bytes_hash(st.sym_name(s).as_bytes())),
            View::App { .. } => kids.iter().fold(mix(8, kids.len() as u64), |h, k| mix(h, done[&k.raw()])),
        };
        done.insert(u.raw(), finalize(fp));
    }
    done[&t.raw()]
}

/// Deterministic, splittable pseudo-random numbers (SplitMix64, spike 4's decision 4.7).
#[derive(Clone, Copy, Debug)]
pub struct Rng(u64);

impl Rng {
    /// The stream of a seed.
    pub fn new(seed: u64) -> Rng {
        Rng(finalize(seed ^ K))
    }
    /// The `i`-th independent stream of `seed`.
    pub fn split(seed: u64, i: u64) -> Rng {
        Rng(finalize(seed ^ finalize(i.wrapping_add(K))))
    }
    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(K);
        finalize(self.0)
    }
    /// A number below `n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// The shapes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// matrix element replacements
    S3,
    /// promotion-heavy: nothing dies
    S8,
    /// long session with a memoised rewriter
    Ls,
    /// canonical construction across threads
    M8,
    /// number payloads
    Num,
}

impl Shape {
    /// Every shape.
    pub const ALL: [Shape; 5] = [Shape::S3, Shape::S8, Shape::Ls, Shape::M8, Shape::Num];

    /// The shape's name in reports.
    pub fn name(self) -> &'static str {
        match self {
            Shape::S3 => "S3",
            Shape::S8 => "S8",
            Shape::Ls => "LS",
            Shape::M8 => "M8",
            Shape::Num => "NUM",
        }
    }

    /// The shape of a name (any case).
    pub fn from_name(s: &str) -> Option<Shape> {
        Shape::ALL.into_iter().find(|x| x.name().eq_ignore_ascii_case(s))
    }

    /// Run the shape on a session; returns its structural check.
    pub fn run(self, s: &mut Session<Roots>, scale: Scale) -> u64 {
        let big = scale == Scale::Bench;
        match self {
            Shape::S3 => s3(s, if big { 1_000_000 } else { 50_000 }, if big { 2000 } else { 300 }),
            Shape::S8 => s8(s, if big { 1_000_000 } else { 100_000 }),
            Shape::Ls => long_session(s, if big { 10_000 } else { 1000 }, if big { 300 } else { 150 }),
            Shape::M8 => m8(s, if big { 400_000 } else { 100_000 }),
            Shape::Num => numbers(s, if big { 4000 } else { 1000 }, if big { 20 } else { 8 }),
        }
    }
}

/// Sizes: `Quick` for tests (well under a second each), `Bench` for the benchmark (seconds).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scale {
    /// test sizes
    Quick,
    /// benchmark sizes
    Bench,
}

/// The outcome of one run.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// the shape's structural check
    pub check: u64,
    /// time of the whole shape, with its final check
    pub total: std::time::Duration,
    /// time inside evaluations
    pub work: std::time::Duration,
    /// time in safe points
    pub safe: std::time::Duration,
    /// the collections
    pub gc: crate::GcStats,
    /// bytes reserved by the heap at the end
    pub reserved_end: usize,
    /// bytes reserved after the full collection
    pub reserved_after_full: usize,
}

/// Run a shape on a fresh session with this configuration and thread count, then collect fully and verify.
pub fn run(shape: Shape, cfg: Config, threads: usize, scale: Scale) -> Outcome {
    let mut s = Session::new(cfg, threads, Roots::new());
    let t0 = std::time::Instant::now();
    let check = shape.run(&mut s, scale);
    let total = t0.elapsed();
    let reserved_end = s.heap.reserved_bytes();
    s.collect_full();
    if cfg.stress {
        s.verify();
    }
    let reserved_after_full = s.heap.reserved_bytes();
    Outcome {
        check,
        total,
        work: s.work_time,
        safe: s.safe_time,
        gc: s.heap.gc_stats().clone(),
        reserved_end,
        reserved_after_full,
    }
}

fn args_of<'h>(h: &'h Heap, t: T<'h>) -> &'h [T<'h>] {
    match h.view(t) {
        View::App { args, .. } => args,
        _ => &[],
    }
}

/// A fingerprint of every root.
fn check_roots(s: &Session<Roots>) -> u64 {
    s.read(|h, keep| (0..keep.len()).fold(keep.len() as u64, |c, i| mix(c, fingerprint(h, keep.get(h, i)))))
}

/// The canonical integer term of an `i128`.
fn int_of<'h>(h: &'h Heap, v: i128) -> T<'h> {
    let m = v.unsigned_abs();
    h.big_int(v < 0, &[m as u64, (m >> 64) as u64])
}

// ---------------------------------------------------------------------------------------------------------------

/// S3: an `n`-element matrix of 1000 rows, then `steps` replacement evaluations.
pub fn s3(s: &mut Session<Roots>, n: usize, steps: usize) -> u64 {
    let list = s.heap.sym("list");
    let (rows, cols) = (1000usize, (n / 1000).max(1));
    s.step(|h: &Heap, keep: &mut Roots| {
        let rs: Vec<T> = (0..rows)
            .into_par_iter()
            .map(|i| {
                let row: Vec<T> = (0..cols).map(|j| h.int((i * cols + j) as i64)).collect();
                h.call(list, &row)
            })
            .collect();
        keep.push(h.call(list, &rs));
    });
    let mut rng = Rng::new(33);
    for k in 0..steps {
        let (i, j) = (rng.below(rows as u64) as usize, rng.below(cols as u64) as usize);
        s.step(|h: &Heap, keep: &mut Roots| {
            let m = keep.get(h, 0);
            let mut rs = args_of(h, m).to_vec();
            let mut row = args_of(h, rs[i]).to_vec();
            row[j] = h.int(-(k as i64));
            rs[i] = h.call(list, &row);
            keep.set(0, h.call(list, &rs));
        });
    }
    check_roots(s)
}

/// S8: `n` distinct `f(i, g(i))`, all kept, built in 100 parallel evaluations.
pub fn s8(s: &mut Session<Roots>, n: usize) -> u64 {
    let (f, g, list) = (s.heap.sym("f"), s.heap.sym("g"), s.heap.sym("list"));
    let chunk = n / 100;
    for c in 0..100 {
        s.step(|h: &Heap, keep: &mut Roots| {
            let v: Vec<T> = (0..chunk)
                .into_par_iter()
                .with_min_len(1024)
                .map(|k| {
                    let a = h.int((c * chunk + k) as i64);
                    h.call(f, &[a, h.call(g, &[a])])
                })
                .collect();
            keep.push(h.call(list, &v));
        });
    }
    check_roots(s)
}

// ---------------------------------------------------------------------------------------------------------------
// LS

/// A random tree of `size` nodes over `heads` (arity 1 to 3) and `leaves` (symbols and small integers), built
/// bottom-up with a work list (no recursion on the term).
fn gen_tree<'h>(h: &'h Heap, heads: &[Sym], leaves: &[Sym], rng: &mut Rng, size: usize) -> T<'h> {
    let mut pool: Vec<T<'h>> = vec![];
    for _ in 0..size {
        let arity = 1 + rng.below(3) as usize;
        if pool.len() < arity || rng.below(3) == 0 {
            let leaf = if rng.below(2) == 0 {
                h.int(rng.below(20) as i64)
            } else {
                h.sym_term(leaves[rng.below(leaves.len() as u64) as usize])
            };
            pool.push(leaf);
        } else {
            let args = pool.split_off(pool.len() - arity);
            pool.push(h.call(heads[rng.below(heads.len() as u64) as usize], &args));
        }
    }
    while pool.len() > 1 {
        let args = pool.split_off(pool.len() - pool.len().min(3));
        pool.push(h.call(heads[rng.below(heads.len() as u64) as usize], &args));
    }
    pool.pop().unwrap_or_else(|| h.int(0))
}

/// The rewriter's view of the session: its symbols, the own values in scope, and the memo epochs.
struct Env<'h, 'a> {
    inc: Sym,
    dbl: Sym,
    add: Sym,
    defined: &'a [Sym],
    values: &'a [T<'h>],
    /// stamp of entries made now, and the oldest stamp still valid (the last redefinition)
    epoch: u32,
    valid_from: u32,
}

impl<'h> Env<'h, '_> {
    fn rewrite(&self, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        let int = |t: T<'h>| match h.view(t) {
            View::Int(v) => Some(v as i128),
            _ => None,
        };
        if let View::Sym(s) = h.view(head) {
            if s == self.inc && args.len() == 1 {
                if let Some(v) = int(args[0]) {
                    return int_of(h, v + 1);
                }
            } else if s == self.dbl && args.len() == 1 {
                if let Some(v) = int(args[0]) {
                    return int_of(h, 2 * v);
                }
            } else if s == self.add && args.len() == 2 {
                if let (Some(a), Some(b)) = (int(args[0]), int(args[1])) {
                    return int_of(h, a + b);
                }
            }
        }
        h.app(head, args)
    }

    /// The normal form of `t`: own values substituted, then rewritten bottom-up, memoised per compound.
    fn normalize(&self, h: &'h Heap, t: T<'h>) -> T<'h> {
        enum F<'h> {
            Enter(T<'h>),
            Exit(T<'h>, T<'h>, usize),
        }
        let mut work = vec![F::Enter(t)];
        let mut vals: Vec<T<'h>> = vec![];
        while let Some(f) = work.pop() {
            match f {
                F::Enter(u) => match h.view(u) {
                    View::Sym(s) => {
                        let v = self.defined.iter().position(|&d| d == s).map_or(u, |i| self.values[i]);
                        vals.push(v);
                    }
                    View::App { head, args } => {
                        if let Some(hit) = h.memo_get(u).filter(|m| m.epoch >= self.valid_from) {
                            vals.push(hit.value);
                            continue;
                        }
                        work.push(F::Exit(u, head, args.len()));
                        work.extend(args.iter().rev().map(|&a| F::Enter(a)));
                    }
                    _ => vals.push(u),
                },
                F::Exit(u, head, n) => {
                    let args = vals.split_off(vals.len() - n);
                    let r = self.rewrite(h, head, &args);
                    h.memo_put(u, r, self.epoch, 0);
                    vals.push(r);
                }
            }
        }
        vals.pop().expect("a normal form")
    }

    /// `normalize`, with the top-level arguments in parallel.
    fn normalize_par(&self, h: &'h Heap, t: T<'h>) -> T<'h> {
        match h.view(t) {
            View::App { head, args } => {
                let a: Vec<T<'h>> = args.par_iter().map(|&x| self.normalize(h, x)).collect();
                self.rewrite(h, head, &a)
            }
            _ => self.normalize(h, t),
        }
    }
}

/// LS: `evals` evaluations of temporary trees of `size` nodes (spike 1b's long session on the parallel heap).
pub fn long_session(s: &mut Session<Roots>, size: usize, evals: usize) -> u64 {
    let hp = &s.heap;
    let defined: Vec<Sym> = (0..5).map(|i| hp.sym(&format!("c{i}"))).collect();
    let heads = [hp.sym("inc"), hp.sym("dbl"), hp.sym("add"), hp.sym("wrap"), hp.sym("pair")];
    let plain: Vec<Sym> = (0..10).map(|i| hp.sym(&format!("s{i}"))).collect();
    let out = hp.sym("out");
    let mut leaves = plain.clone();
    leaves.extend(defined.iter().copied());
    let (inc, dbl, add) = (heads[0], heads[1], heads[2]);
    let base = defined.len();
    let mut rng = Rng::new(12);
    let mut valid_from = 0u32;
    // roots: the own values of `defined`, then the kept results
    // (own values do not mention the defined symbols, so they need no rewriting against each other)
    for _ in 0..base {
        let seed = rng.next_u64();
        s.step(|h: &Heap, keep: &mut Roots| keep.push(gen_tree(h, &heads, &plain, &mut Rng::new(seed), 200)));
    }
    for k in 0..evals {
        let epoch = s.steps as u32;
        if k % 50 == 49 {
            let (seed, j) = (rng.next_u64(), (k / 50) % defined.len());
            s.step(|h: &Heap, keep: &mut Roots| keep.set(j, gen_tree(h, &heads, &plain, &mut Rng::new(seed), 200)));
            valid_from = epoch + 1;
        }
        let epoch = s.steps as u32;
        let seed = rng.next_u64();
        let (defined, heads, leaves) = (&defined, &heads, &leaves);
        s.step(|h: &Heap, keep: &mut Roots| {
            let values: Vec<T> = (0..base).map(|i| keep.get(h, i)).collect();
            let env = Env { inc, dbl, add, defined, values: &values, epoch, valid_from };
            let parts: Vec<T> = (0..8u64)
                .into_par_iter()
                .map(|p| gen_tree(h, heads, leaves, &mut Rng::split(seed, p), size / 8))
                .collect();
            let temp = h.call(heads[4], &parts);
            let nf = env.normalize_par(h, temp);
            let mut piece = nf;
            for d in 0..4 {
                let a = args_of(h, piece);
                if a.is_empty() {
                    break;
                }
                piece = a[(k + d) % a.len()];
            }
            let prev = if k > 0 { keep.get(h, base + k / 2) } else { h.int(0) };
            keep.push(h.call(out, &[h.int(k as i64), piece, prev]));
        });
    }
    // each result's own parts by fingerprint, its back pointer by identity
    s.read(|h, keep| {
        let mut check = 0u64;
        for i in 0..base {
            check = mix(check, fingerprint(h, keep.get(h, i)));
        }
        for k in 0..keep.len() - base {
            let a = args_of(h, keep.get(h, base + k));
            assert!(matches!(h.view(a[0]), View::Int(v) if v == k as i64), "LS: result {k} lost its index");
            assert!(k == 0 || a[2] == keep.get(h, base + k / 2), "LS: back pointer of result {k} lost");
            check = mix(check, fingerprint(h, a[1]));
        }
        check
    })
}

// ---------------------------------------------------------------------------------------------------------------

/// M8: `n/2` constructions of `g(i)` and `f(i, g(i))` in 10 parallel evaluations, half from a shared pool, then a
/// parallel rebuild that must find the kept handles; after a full collection only the distinct live terms remain.
pub fn m8(s: &mut Session<Roots>, n: usize) -> u64 {
    let (f, g, list) = (s.heap.sym("f"), s.heap.sym("g"), s.heap.sym("list"));
    let k = n / 2;
    let shared = (k / 16).max(1);
    let item = move |j: usize| if j.is_multiple_of(2) { ((j / 2) % shared) as i64 } else { (shared + j) as i64 };
    fn build<'h>(h: &'h Heap, f: Sym, g: Sym, v: i64) -> T<'h> {
        let i = h.int(v);
        h.call(f, &[i, h.call(g, &[i])])
    }
    let rounds = 10;
    for r in 0..rounds {
        let (lo, hi) = (r * k / rounds, (r + 1) * k / rounds);
        s.step(|h: &Heap, keep: &mut Roots| {
            let ids: Vec<T> = (lo..hi).into_par_iter().with_min_len(1024).map(|j| build(h, f, g, item(j))).collect();
            keep.push(h.call(list, &ids));
        });
    }
    let ok = s.step(|h: &Heap, keep: &mut Roots| {
        (0..rounds).all(|r| {
            let lo = r * k / rounds;
            let a = args_of(h, keep.get(h, r));
            a.par_iter().enumerate().with_min_len(1024).all(|(j, &t)| build(h, f, g, item(lo + j)) == t)
        })
    });
    assert!(ok, "M8: an item rebuilt later has a different handle");
    if s.heap.config().collect {
        s.collect_full();
    }
    let distinct = (0..k).map(item).collect::<rustc_hash::FxHashSet<i64>>().len();
    let stored = s.heap.stored();
    assert_eq!(stored, 2 * distinct + rounds, "M8: stored terms are not exactly the distinct live terms");
    check_roots(s)
}

// ---------------------------------------------------------------------------------------------------------------
// NUM

/// The `i`-th number record of a seed: `num(i, big integer, rational, ball, string, real, word)`.
fn number_record<'h>(h: &'h Heap, syms: &[Sym; 2], seed: u64, i: u64) -> T<'h> {
    let mut r = Rng::split(seed, i);
    let limbs: Vec<u64> = (0..1 + r.below(130)).map(|_| r.next_u64()).collect();
    let neg = r.below(2) == 0;
    // trailing zero limbs must be trimmed by the canonicalising constructor
    let mut padded = limbs.clone();
    padded.extend([0, 0]);
    let big = h.big_int(neg, &padded);
    debug_assert_eq!(big, h.big_int(neg, &limbs));
    // a reduced rational: an odd numerator over a power of two (a word or, past 63 bits, a payload)
    let mut num: Vec<u64> = (0..1 + r.below(70)).map(|_| r.next_u64()).collect();
    num[0] |= 1;
    let e = 1 + r.below(100) as usize;
    let mut den = vec![0u64; e / 64 + 1];
    den[e / 64] = 1 << (e % 64);
    let rat = h.rational(h.big_int(!neg, &num), h.big_int(false, &den));
    let ball_words: Vec<u64> = (0..3 + r.below(100)).map(|_| r.next_u64()).collect();
    let ball = h.ball(neg, &ball_words);
    let len = r.below(700) as usize;
    let text: String = (0..len).map(|k| ['a', 'b', 'é', '∑', 'z'][(k + i as usize) % 5]).collect();
    let real = h.real(f64::from_bits(r.next_u64()));
    let word = h.int((r.next_u64() | 1 << 62) as i64);
    h.call(syms[0], &[h.int(i as i64), big, rat, ball, h.string(&text), real, word, h.sym_term(syms[1])])
}

/// NUM: `rounds` evaluations of `per` number records built in parallel; a third are kept, the rest die.
pub fn numbers(s: &mut Session<Roots>, per: usize, rounds: usize) -> u64 {
    let syms = [s.heap.sym("num"), s.heap.sym("tag")];
    let list = s.heap.sym("list");
    let kept = |i: u64| i.is_multiple_of(3);
    for r in 0..rounds as u64 {
        s.step(|h: &Heap, keep: &mut Roots| {
            let recs: Vec<T> = (0..per).into_par_iter().map(|i| number_record(h, &syms, r, i as u64)).collect();
            let ks: Vec<T> = recs.iter().enumerate().filter(|(i, _)| kept(*i as u64)).map(|(_, &t)| t).collect();
            keep.push(h.call(list, &ks));
        });
    }
    let ok = s.step(|h: &Heap, keep: &mut Roots| {
        (0..rounds).all(|r| {
            let a = args_of(h, keep.get(h, r));
            a.par_iter().enumerate().all(|(j, &t)| number_record(h, &syms, r as u64, 3 * j as u64) == t)
        })
    });
    assert!(ok, "NUM: a number rebuilt later has a different handle");
    check_roots(s)
}
