//! Workloads. Spike 2's R4, R5, R13, R14 and S7 are ported with spike 2's generators (same random streams, same
//! symbol creation order, so the same symbol indices), so their results compare bit for bit with spike 2's
//! sequential evaluator run in the same process. New cases:
//!
//! - **R14e**: R14 with a condition that *evaluates*: `f(x_ : integer) /; is_prime(g(x)) := hi(x)`, where `g` is
//!   defined by a rule whose right side reads the global `c`. Phases redefine an unrelated symbol, then `c` (read
//!   only inside the evaluation the condition starts), then add a rule to `g` (a rule-set lookup inside the
//!   condition). Variants: `native` (the same predicate computed in the closure, reading `c` directly), `eval`, and
//!   `untracked` (the condition's reads are dropped: expected to go stale).
//! - **MR**: `map(F, xs)` for a rule-heavy `F` (literal rules, typed conditional rules whose conditions evaluate,
//!   argument-head rules), where some elements redefine a global the conditions read, add rules to `F`'s index or to
//!   argument-head rule sets, replace a rule, or print, in the middle of the parallel map (ordered effects).
use crate::eval::{Config, Eval, RuleDef, RunStats};
use crate::heap::Heap;
use crate::index::Rhs;
use crate::matcher::{Binds, Val};
use crate::pat::{alt, blank, cond, lit, papp, seq0, seq1, x, xt, Pat, Ty};
use crate::{combine, fingerprint, fold_fps, Rng, Term};
use rayon::ThreadPool;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Out {
    pub phases: Vec<(&'static str, Duration)>,
    /// Everything observable, compared across configurations.
    pub check: u64,
    /// The check spike 2's driver computes for the same case (0: none).
    pub s2: u64,
    pub stats: RunStats,
    pub note: String,
}

impl Out {
    /// Total time of the evaluation phases.
    pub fn time(&self) -> Duration {
        self.phases.iter().filter(|(p, _)| *p != "define").map(|(_, d)| *d).sum()
    }
}

fn run(pool: &ThreadPool, ev: &Eval, t: Term, o: &mut Out, phase: &'static str) -> Term {
    let (r, d, s) = pool.install(|| {
        let t0 = Instant::now();
        let (r, s) = ev.run(t);
        (r, t0.elapsed(), s)
    });
    o.phases.push((phase, d));
    o.stats.splits += s.splits;
    o.stats.redos += s.redos;
    o.stats.fuel += s.fuel;
    r.expect("evaluation failed")
}

fn timed<R>(o: &mut Out, phase: &'static str, f: impl FnOnce() -> R) -> R {
    let t0 = Instant::now();
    let r = f();
    o.phases.push((phase, t0.elapsed()));
    r
}

fn pick(rng: &mut Rng, v: &[Term]) -> Term {
    v[rng.below(v.len() as u64) as usize]
}

/// Spike 2's `hit(k, b0, b1, ...)` right side.
fn hit_rhs(hit: Term, list: Term, k: i64, nvars: usize) -> Rhs {
    Arc::new(move |b: &Binds, st: &Heap| {
        let mut items = vec![st.int(k)];
        for v in 0..nvars as u16 {
            let t = match b.get(v) {
                Val::One(t) => t,
                Val::Seq(..) => st.app(list, b.seq(v)),
                Val::Unbound => st.int(-1),
            };
            items.push(t);
        }
        st.app(hit, &items)
    })
}

fn rhs(f: impl Fn(&Binds, &Heap) -> Term + Send + Sync + 'static) -> Rhs {
    Arc::new(f)
}

// ---------------------------------------------------------------------------------------------------------------
// R4: AC matching, many rules on add/mul applied once at the top of each subject (`top` holds its argument and
// dispatches once, like spike 2's `rewrite_top`); the subjects are the elements of one list, evaluated in parallel.

struct AcWorld {
    add: Term,
    mul: Term,
    cs: Vec<Term>,
    fs: Vec<Term>,
    hit: Term,
    list: Term,
}

fn ac_world(st: &Heap) -> AcWorld {
    let add = st.sym("add");
    let mul = st.sym("mul");
    let cs = (0..100).map(|i| st.sym(&format!("c{i:02}"))).collect();
    let fs = (0..10).map(|i| st.sym(&format!("f{i}"))).collect();
    let hit = st.sym("hit");
    let list = st.sym("list");
    AcWorld { add, mul, cs, fs, hit, list }
}

fn ac_subject(st: &Heap, w: &AcWorld, rng: &mut Rng) -> Term {
    let (h, g) = if rng.pct(50) { (w.add, w.mul) } else { (w.mul, w.add) };
    let n = 4 + rng.below(13) as usize;
    let mut args = Vec::with_capacity(n);
    for _ in 0..n {
        let r = rng.below(100);
        let a = if r < 50 {
            pick(rng, &w.cs)
        } else if r < 60 {
            st.int(1 + rng.below(4) as i64)
        } else if r < 85 {
            let f = pick(rng, &w.fs);
            let c = pick(rng, &w.cs[..10]);
            st.app(f, &[c])
        } else {
            let c = pick(rng, &w.cs);
            let d = if rng.pct(50) { pick(rng, &w.cs) } else { st.int(1 + rng.below(4) as i64) };
            st.app_ac(g, &[c, d])
        };
        args.push(a);
    }
    st.app_ac(h, &args)
}

fn ac_rule(w: &AcWorld, rng: &mut Rng, k: i64) -> (Pat, Rhs) {
    let (h, g) = if rng.pct(50) { (w.add, w.mul) } else { (w.mul, w.add) };
    let c = |rng: &mut Rng| lit(pick(rng, &w.cs));
    let f = |rng: &mut Rng| pick(rng, &w.fs);
    let fx = |rng: &mut Rng, v: u16| papp(pick(rng, &w.fs), vec![x(v)]);
    let lhs = if k % 1000 == 999 {
        let p = papp(h, vec![xt(0, Ty::Symbol), xt(1, Ty::Symbol), xt(2, Ty::Symbol), seq0(3)]);
        cond(p, |b, cc| {
            let st = cc.heap();
            let s: u64 = (0..3).map(|v| st.sym_index(b.one(v)).unwrap() as u64).sum();
            Ok(s.is_multiple_of(997))
        })
    } else {
        match rng.below(8) {
            0 => papp(h, vec![c(rng), c(rng), seq0(0)]),
            1 => papp(h, vec![c(rng), fx(rng, 0), seq0(1)]),
            2 => papp(h, vec![fx(rng, 0), fx(rng, 0), c(rng), seq0(1)]),
            3 => papp(h, vec![xt(0, Ty::Integer), c(rng), c(rng), seq1(1)]),
            4 => papp(h, vec![papp(g, vec![c(rng), x(0)]), c(rng), x(1), seq0(2)]),
            5 => papp(h, vec![fx(rng, 0), x(0), c(rng), seq0(1)]),
            6 => {
                let f2 = f(rng);
                papp(h, vec![fx(rng, 0), xt(1, Ty::Head(f2)), c(rng), seq0(2)])
            }
            _ => papp(h, vec![c(rng), x(0), x(1), x(2)]),
        }
    };
    let nv = lhs.n_vars();
    (lhs, hit_rhs(w.hit, w.list, k, nv))
}

pub fn r4(pool: &ThreadPool, cfg: Config, n_rules: usize, quick: bool) -> Out {
    let n_sub = if quick { 1000 } else { 10_000 };
    let st = Heap::new();
    let w = ac_world(&st);
    let mut ev = Eval::new(&st, cfg);
    ev.set_ac(w.add);
    ev.set_ac(w.mul);
    let mut o = Out::default();
    let mut rng = Rng::new(4);
    let rules: Vec<_> = (0..n_rules).map(|k| ac_rule(&w, &mut rng, k as i64)).collect();
    let mut rng = Rng::new(44);
    let subs: Vec<Term> = (0..n_sub).map(|_| ac_subject(&st, &w, &mut rng)).collect();
    timed(&mut o, "define", || rules.into_iter().for_each(|(l, r)| ev.define(l, r, None)));
    let top = st.sym("top");
    let tops: Vec<Term> = subs.iter().map(|&s| st.app(top, &[s])).collect();
    let t = st.app(w.list, &tops);
    let r = run(pool, &ev, t, &mut o, "apply");
    let res = st.args(r);
    let hits = res.iter().zip(&subs).filter(|(r, s)| r != s).count();
    o.check = fold_fps(&st, res);
    o.s2 = o.check;
    o.note = format!("{hits} of {n_sub} subjects rewritten");
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R5: many rules with literal arguments; the calls are the elements of one list.

pub fn r5(pool: &ThreadPool, cfg: Config, n_rules: usize, quick: bool) -> Out {
    let n_calls = if quick { 10_000 } else { 1_000_000 };
    let st = Heap::new();
    let g = st.sym("g");
    let miss = st.sym("miss");
    let mut ev = Eval::new(&st, cfg);
    let mut o = Out::default();
    timed(&mut o, "define", || {
        ev.define(papp(g, vec![x(0)]), rhs(move |b, st| st.app(miss, &[b.one(0)])), None);
        for k in 0..n_rules as i64 {
            ev.define(papp(g, vec![lit(st.int(k))]), rhs(move |_, st| st.int(1_000_000 + 3 * k)), None);
        }
    });
    let mut rng = Rng::new(5);
    let calls: Vec<Term> = (0..n_calls).map(|_| st.app(g, &[st.int(rng.below(2 * n_rules as u64) as i64)])).collect();
    let t = st.app(ev.s.list, &calls);
    let r = run(pool, &ev, t, &mut o, "calls");
    let res = st.args(r);
    let hits = res.iter().filter(|&&r| st.as_int(r).is_some()).count();
    o.check = fold_fps(&st, res);
    o.s2 = o.check;
    o.note = format!("{hits} of {n_calls} calls hit a literal rule");
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R13: a generic `area` extended by many kinds through argument-head rules.

pub fn r13(pool: &ThreadPool, cfg: Config, kinds: usize, quick: bool) -> Out {
    let n_calls = if quick { 10_000 } else { 1_000_000 };
    let st = Heap::new();
    let area = st.sym("area");
    let unknown = st.sym("unknown_area");
    let unk = st.sym("blob");
    let ks: Vec<Term> = (0..kinds).map(|j| st.sym(&format!("kind{j}"))).collect();
    let mut ev = Eval::new(&st, cfg);
    let mut o = Out::default();
    timed(&mut o, "define", || {
        ev.define(papp(area, vec![x(0)]), rhs(move |b, st| st.app(unknown, &[b.one(0)])), None);
        for (j, &kj) in ks.iter().enumerate() {
            let j = j as i64;
            let r = rhs(move |b, st| {
                let a = st.as_int(b.one(0)).unwrap();
                st.int((a * a + j) % 1_000_000_007)
            });
            ev.define(papp(area, vec![papp(kj, vec![xt(0, Ty::Integer)])]), r, Some(kj));
        }
    });
    let mut rng = Rng::new(13);
    let calls: Vec<Term> = (0..n_calls)
        .map(|i| {
            let k = if rng.pct(10) { unk } else { pick(&mut rng, &ks) };
            let obj = st.app(k, &[st.int(i as i64 % 30_000)]);
            st.app(area, &[obj])
        })
        .collect();
    let t = st.app(ev.s.list, &calls);
    let r = run(pool, &ev, t, &mut o, "calls");
    let res = st.args(r);
    let generic = res.iter().filter(|&&r| st.head(r) == Some(unknown)).count();
    o.check = fold_fps(&st, res);
    o.s2 = o.check;
    o.note = format!("{generic} calls fell through to the generic rule");
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R14: conditions that read global values; phases with top-level redefinitions between evaluations.

pub fn r14(pool: &ThreadPool, cfg: Config, n: usize, _quick: bool) -> Out {
    let st = Heap::new();
    let (f, hi, lo, zz, list) = (st.sym("f"), st.sym("hi"), st.sym("lo"), st.sym("zz"), st.sym("list"));
    let gs: Arc<Vec<Term>> = Arc::new((0..100).map(|k| st.sym(&format!("g{k}"))).collect());
    let mut ev = Eval::new(&st, Config { memo_bits: 22, ..cfg });
    for (k, &g) in gs.iter().enumerate() {
        ev.set_own(g, st.int(k as i64 % 7));
    }
    ev.define(papp(f, vec![x(0)]), rhs(move |b, st| st.app(lo, &[b.one(0)])), None);
    let gs2 = gs.clone();
    let p = cond(papp(f, vec![xt(0, Ty::Integer)]), move |b, cc| {
        let xv = cc.int(b.one(0)).unwrap();
        let g = cc.value(gs2[(xv % 100) as usize]).and_then(|v| cc.int(v)).unwrap_or(0);
        Ok(xv % 7 > g)
    });
    ev.define(p, rhs(move |b, st| st.app(hi, &[b.one(0)])), None);
    let calls: Vec<Term> = (0..n as i64).map(|i| st.app(f, &[st.int(i)])).collect();
    let t = st.app(list, &calls);
    let mut o = Out::default();
    let r1 = run(pool, &ev, t, &mut o, "eval");
    let r2 = run(pool, &ev, t, &mut o, "re-eval, no change");
    ev.set_own(zz, st.int(1));
    ev.safe_point();
    let r3 = run(pool, &ev, t, &mut o, "re-eval, unrelated change");
    ev.set_own(gs[3], st.int(6));
    let r4 = run(pool, &ev, t, &mut o, "re-eval, g3 changed");
    if !(r1 == r2 && r2 == r3) {
        for (i, ((&a, &b), &c)) in st.args(r1).iter().zip(st.args(r2)).zip(st.args(r3)).enumerate() {
            if a != b || b != c {
                eprintln!("element {i}: {} / {} / {}", st.show(a), st.show(b), st.show(c));
                break;
            }
        }
        let d = |r: Term| format!("{:?} head {:?} len {} fp {:x}", r, st.head(r), st.args(r).len(), fingerprint(&st, r));
        panic!("R14: results changed without a relevant redefinition: {} | {} | {}", d(r1), d(r2), d(r3));
    }
    o.s2 = fingerprint(&st, r4);
    o.check = combine(fingerprint(&st, r1), o.s2);
    o.note = format!("result {}", if r1 == r4 { "unchanged" } else { "changed" });
    o
}

// ---------------------------------------------------------------------------------------------------------------
// S7: `cases` (pattern selection without rule sets), five patterns over one held list, elements in parallel.

pub fn s7(pool: &ThreadPool, cfg: Config, n: usize, _quick: bool) -> Out {
    let st = Heap::new();
    let (f, g, h, add, list) = (st.sym("f"), st.sym("g"), st.sym("h"), st.sym("add"), st.sym("list"));
    let cs: Vec<Term> = (0..20).map(|i| st.sym(&format!("c{i:02}"))).collect();
    let mut rng = Rng::new(7);
    let elems: Vec<Term> = (0..n)
        .map(|_| {
            let r = rng.below(100);
            let i = |rng: &mut Rng| st.int(rng.below(50) as i64);
            if r < 30 {
                let a = i(&mut rng);
                let b = i(&mut rng);
                st.app(f, &[a, b])
            } else if r < 35 {
                let a = i(&mut rng);
                st.app(f, &[a, a])
            } else if r < 50 {
                let c = pick(&mut rng, &cs);
                let a = i(&mut rng);
                st.app(g, &[c, a])
            } else if r < 60 {
                let c = pick(&mut rng, &cs);
                st.app(h, &[c])
            } else if r < 70 {
                i(&mut rng)
            } else if r < 80 {
                pick(&mut rng, &cs)
            } else {
                let k = 2 + rng.below(5) as usize;
                let args: Vec<Term> =
                    (0..k).map(|_| if rng.pct(70) { pick(&mut rng, &cs) } else { st.int(rng.below(9) as i64) }).collect();
                st.app_ac(add, &args)
            }
        })
        .collect();
    let mut ev = Eval::new(&st, cfg);
    ev.set_ac(add);
    let c1 = cs[1];
    let pats: Vec<Pat> = vec![
        cond(papp(f, vec![xt(0, Ty::Integer), xt(1, Ty::Integer)]), |b, cc| Ok(cc.int(b.one(0)) > cc.int(b.one(1)))),
        alt(vec![papp(g, vec![blank(), blank()]), papp(h, vec![blank()])]),
        papp(f, vec![x(0), x(0)]),
        papp(add, vec![lit(c1), seq1(0)]),
        papp(add, vec![xt(0, Ty::Symbol), xt(1, Ty::Integer), seq0(2)]),
    ];
    ev.pats = pats.into_iter().map(|p| ev.compile(p)).map(|p| { let n = p.n_vars(); (p, n) }).collect();
    let cases = st.sym("cases");
    let l = st.app(list, &elems);
    let calls: Vec<Term> = (0..5).map(|k| st.app(cases, &[l, st.int(k)])).collect();
    let t = st.app(list, &calls);
    let mut o = Out::default();
    let r = run(pool, &ev, t, &mut o, "cases x5");
    let res = st.args(r);
    o.check = fold_fps(&st, res);
    o.s2 = o.check;
    o.note = format!("selected {:?}", res.iter().map(|&s| st.args(s).len()).collect::<Vec<_>>());
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R14e: conditions that evaluate terms.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CondKind {
    Native,
    Eval,
}

pub fn r14e(pool: &ThreadPool, cfg: Config, n: usize, kind: CondKind) -> Out {
    let st = Heap::new();
    let (f, g, hi, lo, zz, c, list) =
        (st.sym("f"), st.sym("g"), st.sym("hi"), st.sym("lo"), st.sym("zz"), st.sym("c"), st.sym("list"));
    let (imod, iadd, imul, isp) = (st.sym("imod"), st.sym("iadd"), st.sym("imul"), st.sym("is_prime"));
    let mut ev = Eval::new(&st, Config { memo_bits: 22, ..cfg });
    ev.set_own(c, st.int(1));
    // g(x_ : integer) := imod(iadd(imul(x, 7919), c), 1000003)
    ev.define(
        papp(g, vec![xt(0, Ty::Integer)]),
        rhs(move |b, st| {
            let m = st.app(imul, &[b.one(0), st.int(7919)]);
            st.app(imod, &[st.app(iadd, &[m, c]), st.int(1_000_003)])
        }),
        None,
    );
    ev.define(papp(f, vec![x(0)]), rhs(move |b, st| st.app(lo, &[b.one(0)])), None);
    let p = match kind {
        // the same predicate, computed in the closure: one own-value read, no allocation, no dispatch
        CondKind::Native => cond(papp(f, vec![xt(0, Ty::Integer)]), move |b, cc| {
            let xv = cc.int(b.one(0)).unwrap();
            let cv = cc.value(c).and_then(|v| cc.int(v)).unwrap_or(0);
            Ok(crate::eval::is_prime((xv.wrapping_mul(7919) + cv).rem_euclid(1_000_003)))
        }),
        // is_prime(g(x)): built in the shared heap and evaluated, inside the match
        CondKind::Eval => cond(papp(f, vec![xt(0, Ty::Integer)]), move |b, cc| {
            let st = cc.heap();
            let t = st.app(isp, &[st.app(g, &[b.one(0)])]);
            let r = cc.eval(t)?;
            Ok(cc.is_true(r))
        }),
    };
    ev.define(p, rhs(move |b, st| st.app(hi, &[b.one(0)])), None);
    let calls: Vec<Term> = (0..n as i64).map(|i| st.app(f, &[st.int(i)])).collect();
    let t = st.app(list, &calls);
    let mut o = Out::default();
    let len0 = st.len();
    let r1 = run(pool, &ev, t, &mut o, "eval");
    let interned = st.len() - len0;
    let r2 = run(pool, &ev, t, &mut o, "re-eval, no change");
    ev.set_own(zz, st.int(1));
    let r3 = run(pool, &ev, t, &mut o, "re-eval, unrelated change");
    ev.set_own(c, st.int(2));
    let r4 = run(pool, &ev, t, &mut o, "re-eval, c changed");
    // g(x_ : integer) /; imod(x, 10) == 0 := 2 (goes before the general g rule: an instance of it, 6.3)
    let gc = cond(papp(g, vec![xt(0, Ty::Integer)]), |b, cc| Ok(cc.int(b.one(0)).unwrap() % 10 == 0));
    ev.define(gc, rhs(|_, st| st.int(2)), None);
    ev.safe_point();
    let r5 = run(pool, &ev, t, &mut o, "re-eval, g rule added");
    assert!(r1 == r2 && r2 == r3, "results changed without a relevant redefinition");
    let hits = |r: Term| st.args(r).iter().filter(|&&u| st.head(u) == Some(hi)).count();
    o.check = [r1, r4, r5].iter().fold(0, |h, &r| combine(h, fingerprint(&st, r)));
    o.note = format!(
        "hi: {} / {} / {} of {n}; {interned} terms interned by the first evaluation",
        hits(r1),
        hits(r4),
        hits(r5)
    );
    o
}

// ---------------------------------------------------------------------------------------------------------------
// MR: a parallel map of a rule-heavy function with mid-run redefinitions.

/// `layout`: `None` for the fixed layout, `Some(seed)` for a random one (the fuzz case).
pub fn mr(pool: &ThreadPool, cfg: Config, n: usize, layout: Option<u64>) -> Out {
    let st = Heap::new();
    let ff = st.sym("F");
    let (lo, lo2, hi, eleven, deep, one) =
        (st.sym("lo"), st.sym("lo2"), st.sym("hi"), st.sym("eleven"), st.sym("deep"), st.sym("one"));
    let (wrap, redef, newrule, show, c, list) =
        (st.sym("wrap"), st.sym("redef"), st.sym("newrule"), st.sym("show"), st.sym("c"), st.sym("list"));
    let kinds: Vec<Term> = (0..50).map(|j| st.sym(&format!("kind{j}"))).collect();
    let kindx = st.sym("kindx");
    let (imod, iadd, less, lp, define, defrule, print) = (
        st.sym("imod"),
        st.sym("iadd"),
        st.sym("less"),
        st.sym("loop"),
        st.sym("define"),
        st.sym("defrule"),
        st.sym("print"),
    );
    let mut ev = Eval::new(&st, cfg);
    ev.set_own(c, st.int(3));
    ev.define(papp(ff, vec![x(0)]), rhs(move |b, st| st.app(lo, &[b.one(0)])), None);
    // F(x_ : integer) /; less(imod(x, 7), c) := hi(iadd(loop(40, x), c)): the condition evaluates and reads c
    let p = cond(papp(ff, vec![xt(0, Ty::Integer)]), move |b, cc| {
        let st = cc.heap();
        let t = st.app(less, &[st.app(imod, &[b.one(0), st.int(7)]), c]);
        let r = cc.eval(t)?;
        Ok(cc.is_true(r))
    });
    ev.define(p, rhs(move |b, st| st.app(hi, &[st.app(iadd, &[st.app(lp, &[st.int(40), b.one(0)]), c])])), None);
    for k in (0..3000i64).step_by(3) {
        ev.define(papp(ff, vec![lit(st.int(k))]), rhs(move |_, st| st.int(3 * k + 1)), None);
    }
    for (j, &kj) in kinds.iter().enumerate() {
        let j = j as i64;
        ev.define(
            papp(ff, vec![papp(kj, vec![xt(0, Ty::Integer)])]),
            rhs(move |b, st| st.app(iadd, &[b.one(0), st.int(j)])),
            Some(kj),
        );
    }
    ev.define(papp(ff, vec![papp(redef, vec![x(0)])]), rhs(move |b, st| st.app(define, &[c, b.one(0)])), None);
    ev.define(papp(ff, vec![papp(newrule, vec![xt(0, Ty::Integer)])]), rhs(move |b, st| st.app(defrule, &[b.one(0)])), None);
    ev.define(papp(ff, vec![papp(show, vec![x(0)])]), rhs(move |b, st| st.app(print, &[st.app(ff, &[b.one(0)])])), None);
    // rules defined by `defrule(k)` during the map
    let eleven_c = cond(papp(ff, vec![xt(0, Ty::Integer)]), move |b, cc| {
        let st = cc.heap();
        let r = cc.eval(st.app(imod, &[b.one(0), st.int(11)]))?;
        Ok(cc.int(r) == Some(0))
    });
    ev.rule_table = vec![
        // 0: a literal rule (an instance of F(x_ : integer): ordered before it)
        RuleDef { lhs: papp(ff, vec![lit(st.int(1))]), rhs: rhs(move |_, _| one), attach: None },
        // 1: a conditional rule whose condition evaluates; defined twice in the fixed layout (the second replaces)
        RuleDef { lhs: eleven_c, rhs: rhs(move |b, st| st.app(eleven, &[b.one(0)])), attach: None },
        // 2: a new argument-head rule set
        RuleDef {
            lhs: papp(ff, vec![papp(kindx, vec![x(0)])]),
            rhs: rhs(move |b, st| st.app(iadd, &[b.one(0), st.int(1000)])),
            attach: Some(kindx),
        },
        // 3: a deeper path than any so far (the tree's depth grows while readers flatten subjects)
        RuleDef {
            lhs: papp(ff, vec![papp(wrap, vec![papp(wrap, vec![papp(wrap, vec![xt(0, Ty::Integer)])])])]),
            rhs: rhs(move |b, st| st.app(deep, &[b.one(0)])),
            attach: None,
        },
        // 4: the general rule again, same left side up to renaming: replaces it
        RuleDef { lhs: papp(ff, vec![x(3)]), rhs: rhs(move |b, st| st.app(lo2, &[b.one(3)])), attach: None },
    ];
    let mut rng = Rng::new(layout.unwrap_or(0) * 7 + 17);
    let mut xs: Vec<Term> = (0..n)
        .map(|_| {
            let r = rng.below(100);
            let i = st.int(rng.below(3000) as i64);
            if r < 70 {
                i
            } else if r < 80 {
                st.app(pick(&mut rng, &kinds), &[i])
            } else if r < 85 {
                st.app(kindx, &[i])
            } else {
                st.app(wrap, &[st.app(wrap, &[st.app(wrap, &[i])])])
            }
        })
        .collect();
    let effect = |k: u64, v: i64| match k {
        0 => st.app(redef, &[st.int(v)]),
        1 => st.app(newrule, &[st.int(v.rem_euclid(5))]),
        _ => st.app(show, &[st.int(v)]),
    };
    match layout {
        None => {
            let fixed: [(usize, u64, i64); 10] =
                [(10, 0, 7), (25, 1, 0), (40, 2, 5), (50, 0, 2), (60, 1, 1), (75, 1, 3), (85, 1, 2), (90, 0, 4), (95, 1, 4), (97, 1, 1)];
            for (pct, k, v) in fixed {
                xs[n * pct / 100] = effect(k, v);
            }
        }
        Some(_) => {
            for _ in 0..1 + rng.below(12) {
                let pos = rng.below(n as u64) as usize;
                let (k, v) = (rng.below(3), rng.below(9) as i64);
                xs[pos] = effect(k, v);
            }
        }
    }
    let map = st.sym("map");
    let t = st.app(map, &[ff, st.app(list, &xs)]);
    let mut o = Out::default();
    let r = run(pool, &ev, t, &mut o, "map");
    let outs = ev.output();
    let rs = ev.rules_of(ff).unwrap();
    let mut check = fingerprint(&st, r);
    for &u in &outs {
        check = combine(check, fingerprint(&st, u));
    }
    check = combine(check, ev.value_of(c).map_or(0, |v| fingerprint(&st, v)));
    for s in [ff, c, kindx, wrap] {
        check = combine(check, ev.version_of(s) as u64);
    }
    check = combine(combine(check, rs.len() as u64), rs.order_fingerprint());
    o.check = check;
    o.note = format!("{} outputs, F has {} rules, epoch {}", outs.len(), rs.len(), ev.epoch());
    o
}
