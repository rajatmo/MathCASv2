//! I2's workloads as `mc-core` sessions, for the tests and the benchmark binary. Each returns a check computed from
//! structure only (`mc_core::shapes::fingerprint`), so every configuration (threads, splitting, memo, collection
//! stress) must give the check of the sequential reference ([`Run::reference`]).
//!
//! - **R4**: AC matching: many rules on `add`/`mul` applied once at the top of each subject (`top` holds its argument
//!   and rewrites once); the subjects are the elements of one list, evaluated in parallel.
//! - **R5**: many rules with literal arguments; the calls are the elements of one list.
//! - **R13**: a generic `area` extended by many kinds through argument-head rules.
//! - **R14**: conditions that read global values; evaluations with top-level redefinitions between them.
//! - **S7**: `cases` (pattern selection without rule sets): five patterns over one held list.
//! - **R14e**: conditions that evaluate: `f(x_ : integer) /; is_prime(g(x)) := hi(x)`, where `g`'s rule reads the
//!   global `c`; redefinitions of an unrelated symbol, of `c`, and a new rule for `g` between evaluations.
//! - **MR**: `map(F, xs)` for a rule-heavy `F`, where some elements redefine a global the conditions read, add rules
//!   to `F`'s index or to argument-head rule sets, replace a rule, or print, in the middle of the parallel map.
//!   **MR fuzz** runs random layouts of those effects.
//!
//! Definitions are made in their own session steps, so collections (in stress mode, at every step) move the rule
//! tables' terms before the evaluations use them.
//!
//! The workload-specific built-ins (`loop`, `is_prime`, `top`, `cases`, `defrule`) are natives registered through
//! the same interface `mc-arith` will use.
use crate::eval::{session, EvalConfig, EvalStats, Kernel, Res};
use crate::index::Rhs;
use crate::matcher::{Binds, Val};
use crate::native::{Native, NativeCx, Pattern, Reply};
use crate::pat::{alt, blank, call, cond, lit, seq0, seq1, typed, var, Pat, Ty};
use crate::{Attributes, Hold};
use mc_core::shapes::{fingerprint, Rng};
use mc_core::{Config, Heap, Raw, Store, Sym, View, T};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How a workload runs: the heap configuration, the session's threads and the evaluator settings.
#[derive(Clone, Copy, Debug)]
pub struct Run {
    /// heap configuration (collection, stress)
    pub heap: Config,
    /// worker threads
    pub threads: usize,
    /// evaluator settings
    pub eval: EvalConfig,
}

impl Run {
    /// The sequential reference: one thread, no collection, no splitting, no memo.
    pub fn reference() -> Run {
        Run { heap: Config::no_collect(), threads: 1, eval: EvalConfig::reference() }
    }
}

/// What a workload observed.
#[derive(Clone, Debug, Default)]
pub struct Out {
    /// everything observable, from structure only
    pub check: u64,
    /// time spent in evaluations (definitions and checks excluded)
    pub time: Duration,
    /// counters, summed over the evaluations
    pub stats: EvalStats,
    /// a human-readable summary
    pub note: String,
}

/// Combine fingerprints in sequence.
pub fn combine(h: u64, v: u64) -> u64 {
    (h.rotate_left(7) ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

fn pct(rng: &mut Rng, p: u64) -> bool {
    rng.below(100) < p
}

fn pick<X: Copy>(rng: &mut Rng, v: &[X]) -> X {
    v[rng.below(v.len() as u64) as usize]
}

fn rhs(f: impl for<'h> Fn(&Binds<'h>, &'h Heap) -> T<'h> + Send + Sync + 'static) -> Rhs {
    Arc::new(f)
}

fn int(h: &Heap, t: T<'_>) -> Option<i64> {
    match h.view(t) {
        View::Int(v) => Some(v),
        _ => None,
    }
}

fn args<'h>(h: &'h Heap, t: T<'h>) -> &'h [T<'h>] {
    match h.view(t) {
        View::App { args, .. } => args,
        _ => &[],
    }
}

fn head<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    match h.view(t) {
        View::App { head, .. } => Some(head),
        _ => None,
    }
}

/// Evaluate `t` at the top, timing it into `o`. A failure is returned as the value (and enters the check).
fn run<'h>(k: &mut Kernel, h: &'h Heap, t: T<'h>, o: &mut Out) -> T<'h> {
    let t0 = Instant::now();
    let r = k.eval(h, t);
    o.time += t0.elapsed();
    o.stats.splits += r.stats.splits;
    o.stats.redos += r.stats.redos;
    o.stats.fuel += r.stats.fuel;
    o.stats.holes += r.stats.holes;
    r.value
}

// ---------------------------------------------------------------------------------------------------------------
// Workload natives

/// `loop(k, x)`: `k` steps of a linear congruential map (a cheap, fuel-costing computation).
struct Loop(Sym);

fn step_lcg(x: i64) -> i64 {
    (x.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x3fff_ffff
}

impl Native for Loop {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [k, x] = a else { return Ok(Reply::Decline) };
        let (Some(k), Some(x)) = (int(h, *k), int(h, *x)) else { return Ok(Reply::Decline) };
        if k <= 0 {
            return Ok(Reply::Done(h.int(x)));
        }
        Ok(Reply::Again(h.call(self.0, &[h.int(k - 1), h.int(step_lcg(x))])))
    }
}

/// Primality: trial division below 2^32, deterministic Miller-Rabin above.
pub fn is_prime(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    let n = n as u64;
    for p in [2u64, 3, 5, 7, 11, 13] {
        if n.is_multiple_of(p) {
            return n == p;
        }
    }
    if n < 1 << 32 {
        let mut d = 17u64;
        while d * d <= n {
            if n.is_multiple_of(d) || n.is_multiple_of(d + 2) {
                return false;
            }
            d += 6;
        }
        return true;
    }
    let mulm = |a: u64, b: u64| ((a as u128 * b as u128) % n as u128) as u64;
    let powm = |mut b: u64, mut e: u64| {
        let mut r = 1u64;
        while e > 0 {
            if e & 1 == 1 {
                r = mulm(r, b);
            }
            b = mulm(b, b);
            e >>= 1;
        }
        r
    };
    let (mut d, mut s) = (n - 1, 0);
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    'outer: for a in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        let mut x = powm(a, d);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mulm(x, x);
            if x == n - 1 {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

struct IsPrime;

impl Native for IsPrime {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = a else { return Ok(Reply::Decline) };
        let Some(x) = int(cx.heap(), *x) else { return Ok(Reply::Decline) };
        cx.tick(16)?;
        Ok(Reply::Done(cx.bool(is_prime(x))))
    }
}

/// `top(x)`, held: the first rule's replacement at the top of `x`, not evaluated, or `x`.
struct Top;

impl Native for Top {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = a else { return Ok(Reply::Decline) };
        Ok(Reply::Done(cx.rewrite_once(*x)?.unwrap_or(*x)))
    }
}

/// `cases(list, k)`, held: the elements that pattern `k` matches, selected as a parallel group.
struct Cases(Vec<Pattern>, Sym);

impl Native for Cases {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let [l, k] = a else { return Ok(Reply::Decline) };
        let Some(p) = int(cx.heap(), *k).and_then(|k| self.0.get(k as usize)) else { return Ok(Reply::Decline) };
        let Some(xs) = cx.as_list(*l) else { return Ok(Reply::Decline) };
        let none = cx.heap().sym_term(self.1);
        let sel = cx.group(xs.len(), 1, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
            cx.tick(1)?;
            Ok(if cx.matches(p, xs[i])? { xs[i] } else { none })
        })?;
        let kept: Vec<T> = sel.into_iter().filter(|&x| x != none).collect();
        Ok(Reply::Done(cx.list(&kept)))
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.0.iter_mut().for_each(|p| p.trace(visit));
    }
}

/// A rule that `defrule(k)` defines during an evaluation.
struct RuleDef {
    lhs: Pat,
    rhs: Rhs,
    attach: Option<Sym>,
}

/// `defrule(k)`: the frontier defines rule `k` of the table (an effect).
struct DefRule(Vec<RuleDef>);

impl Native for DefRule {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let [k] = a else { return Ok(Reply::Decline) };
        let Some(d) = int(cx.heap(), *k).and_then(|k| self.0.get(k as usize)) else { return Ok(Reply::Decline) };
        cx.define(d.lhs.clone(), d.rhs.clone(), d.attach)?.expect("table rules are well formed");
        Ok(Reply::Done(cx.null()))
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.0.iter_mut().for_each(|d| d.lhs.trace(visit));
    }
}

fn strict() -> Attributes {
    Attributes::default()
}

fn ac() -> Attributes {
    Attributes { ac: true, ..Attributes::default() }
}

fn held() -> Attributes {
    Attributes { hold: Hold::All, ..Attributes::default() }
}

// ---------------------------------------------------------------------------------------------------------------
// R4

struct AcWorld {
    add: Sym,
    mul: Sym,
    cs: Vec<Sym>,
    fs: Vec<Sym>,
    hit: Sym,
    list: Sym,
}

fn ac_world(h: &Heap) -> AcWorld {
    AcWorld {
        add: h.sym("add"),
        mul: h.sym("mul"),
        cs: (0..100).map(|i| h.sym(&format!("c{i:02}"))).collect(),
        fs: (0..10).map(|i| h.sym(&format!("f{i}"))).collect(),
        hit: h.sym("hit"),
        list: h.sym("list"),
    }
}

/// `hit(k, b0, b1, ...)`: the rule number and its bindings.
fn hit_rhs(hit: Sym, list: Sym, k: i64, nvars: usize) -> Rhs {
    rhs(move |b, h| {
        let mut items = vec![h.int(k)];
        for v in 0..nvars as u16 {
            items.push(match b.get(v) {
                Val::One(t) => t,
                Val::Seq(..) => h.call(list, b.seq(v)),
                Val::Unbound => h.int(-1),
            });
        }
        h.call(hit, &items)
    })
}

fn ac_subject<'h>(k: &Kernel, h: &'h Heap, w: &AcWorld, rng: &mut Rng) -> T<'h> {
    let (hd, g) = if pct(rng, 50) { (w.add, w.mul) } else { (w.mul, w.add) };
    let n = 4 + rng.below(13) as usize;
    let mut a = Vec::with_capacity(n);
    for _ in 0..n {
        let r = rng.below(100);
        a.push(if r < 50 {
            h.sym_term(pick(rng, &w.cs))
        } else if r < 60 {
            h.int(1 + rng.below(4) as i64)
        } else if r < 85 {
            let f = pick(rng, &w.fs);
            let c = pick(rng, &w.cs[..10]);
            h.call(f, &[h.sym_term(c)])
        } else {
            let c = h.sym_term(pick(rng, &w.cs));
            let d = if pct(rng, 50) { h.sym_term(pick(rng, &w.cs)) } else { h.int(1 + rng.below(4) as i64) };
            k.make(h, h.sym_term(g), &[c, d])
        });
    }
    k.make(h, h.sym_term(hd), &a)
}

fn ac_rule(h: &Heap, w: &AcWorld, rng: &mut Rng, k: i64) -> (Pat, Rhs) {
    let (hd, g) = if pct(rng, 50) { (w.add, w.mul) } else { (w.mul, w.add) };
    let c = |rng: &mut Rng| lit(h.sym_term(pick(rng, &w.cs)));
    let fx = |rng: &mut Rng, v: u16| call(h, pick(rng, &w.fs), vec![var(v)]);
    let lhs = if k % 1000 == 999 {
        let p = call(h, hd, vec![typed(0, Ty::Symbol), typed(1, Ty::Symbol), typed(2, Ty::Symbol), seq0(3)]);
        cond(p, &[0, 1, 2], |cx| {
            let h = cx.heap();
            let s: u64 = (0..3).map(|i| if let View::Sym(s) = h.view(cx.one(i)) { s.0 as u64 } else { 0 }).sum();
            Ok(s.is_multiple_of(997))
        })
    } else {
        match rng.below(8) {
            0 => call(h, hd, vec![c(rng), c(rng), seq0(0)]),
            1 => call(h, hd, vec![c(rng), fx(rng, 0), seq0(1)]),
            2 => call(h, hd, vec![fx(rng, 0), fx(rng, 0), c(rng), seq0(1)]),
            3 => call(h, hd, vec![typed(0, Ty::Integer), c(rng), c(rng), seq1(1)]),
            4 => call(h, hd, vec![call(h, g, vec![c(rng), var(0)]), c(rng), var(1), seq0(2)]),
            5 => call(h, hd, vec![fx(rng, 0), var(0), c(rng), seq0(1)]),
            6 => {
                let f2 = pick(rng, &w.fs);
                call(h, hd, vec![fx(rng, 0), typed(1, Ty::Head(f2)), c(rng), seq0(2)])
            }
            _ => call(h, hd, vec![c(rng), var(0), var(1), var(2)]),
        }
    };
    let nv = lhs.n_vars();
    (lhs, hit_rhs(w.hit, w.list, k, nv))
}

/// R4 with `n_rules` rules and `n_sub` subjects.
pub fn r4(run_cfg: Run, n_rules: usize, n_sub: usize) -> Out {
    let mut s = session(run_cfg.heap, run_cfg.threads, run_cfg.eval);
    let mut o = Out::default();
    s.step(|h, k| {
        let w = ac_world(h);
        k.set_attributes(w.add, ac());
        k.set_attributes(w.mul, ac());
        k.register(h.sym("top"), held(), Box::new(Top));
        let mut rng = Rng::new(4);
        for j in 0..n_rules {
            let (l, r) = ac_rule(h, &w, &mut rng, j as i64);
            k.define(h, l, r).expect("R4 rule");
        }
    });
    s.step(|h, k| {
        let w = ac_world(h);
        let mut rng = Rng::new(44);
        let subs: Vec<T> = (0..n_sub).map(|_| ac_subject(k, h, &w, &mut rng)).collect();
        let top = h.sym("top");
        let tops: Vec<T> = subs.iter().map(|&x| h.call(top, &[x])).collect();
        let r = run(k, h, h.call(w.list, &tops), &mut o);
        let hits = args(h, r).iter().zip(&subs).filter(|(r, x)| r != x).count();
        o.check = fingerprint(h, r);
        o.note = format!("{hits} of {n_sub} subjects rewritten");
    });
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R5

/// R5 with `n_rules` literal rules and `n_calls` calls.
pub fn r5(run_cfg: Run, n_rules: usize, n_calls: usize) -> Out {
    let mut s = session(run_cfg.heap, run_cfg.threads, run_cfg.eval);
    let mut o = Out::default();
    s.step(|h, k| {
        let (g, miss) = (h.sym("g"), h.sym("miss"));
        k.define(h, call(h, g, vec![var(0)]), rhs(move |b, h| h.call(miss, &[b.one(0)]))).expect("rule");
        for j in 0..n_rules as i64 {
            k.define(h, call(h, g, vec![lit(h.int(j))]), rhs(move |_, h| h.int(1_000_000 + 3 * j))).expect("rule");
        }
    });
    s.step(|h, k| {
        let (g, list) = (h.sym("g"), h.sym("list"));
        let mut rng = Rng::new(5);
        let calls: Vec<T> = (0..n_calls).map(|_| h.call(g, &[h.int(rng.below(2 * n_rules as u64) as i64)])).collect();
        let r = run(k, h, h.call(list, &calls), &mut o);
        let hits = args(h, r).iter().filter(|&&x| int(h, x).is_some()).count();
        o.check = fingerprint(h, r);
        o.note = format!("{hits} of {n_calls} calls hit a literal rule");
    });
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R13

/// R13 with `kinds` argument-head rule sets and `n_calls` calls.
pub fn r13(run_cfg: Run, kinds: usize, n_calls: usize) -> Out {
    let mut s = session(run_cfg.heap, run_cfg.threads, run_cfg.eval);
    let mut o = Out::default();
    s.step(|h, k| {
        let (area, unknown) = (h.sym("area"), h.sym("unknown_area"));
        k.define(h, call(h, area, vec![var(0)]), rhs(move |b, h| h.call(unknown, &[b.one(0)]))).expect("rule");
        for j in 0..kinds {
            let kj = h.sym(&format!("kind{j}"));
            let j = j as i64;
            let r = rhs(move |b, h| {
                let a = int(h, b.one(0)).expect("integer");
                h.int((a * a + j) % 1_000_000_007)
            });
            k.define_attached(h, call(h, area, vec![call(h, kj, vec![typed(0, Ty::Integer)])]), r, kj).expect("rule");
        }
    });
    s.step(|h, k| {
        let (area, unknown, blob, list) = (h.sym("area"), h.sym("unknown_area"), h.sym("blob"), h.sym("list"));
        let ks: Vec<Sym> = (0..kinds).map(|j| h.sym(&format!("kind{j}"))).collect();
        let mut rng = Rng::new(13);
        let calls: Vec<T> = (0..n_calls)
            .map(|i| {
                let kind = if pct(&mut rng, 10) { blob } else { pick(&mut rng, &ks) };
                h.call(area, &[h.call(kind, &[h.int(i as i64 % 30_000)])])
            })
            .collect();
        let r = run(k, h, h.call(list, &calls), &mut o);
        let generic = args(h, r).iter().filter(|&&x| head(h, x) == Some(h.sym_term(unknown))).count();
        o.check = fingerprint(h, r);
        o.note = format!("{generic} calls fell through to the generic rule");
    });
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R14

/// R14 with `n` calls, re-evaluated after redefinitions.
pub fn r14(run_cfg: Run, n: usize) -> Out {
    let mut s = session(run_cfg.heap, run_cfg.threads, run_cfg.eval);
    let mut o = Out::default();
    s.step(|h, k| {
        let (f, hi, lo, list) = (h.sym("f"), h.sym("hi"), h.sym("lo"), h.sym("list"));
        let gs: Vec<Sym> = (0..100).map(|j| h.sym(&format!("g{j}"))).collect();
        for (j, &g) in gs.iter().enumerate() {
            k.set_own(g, h.int(j as i64 % 7));
        }
        k.define(h, call(h, f, vec![var(0)]), rhs(move |b, h| h.call(lo, &[b.one(0)]))).expect("rule");
        let p = cond(call(h, f, vec![typed(0, Ty::Integer)]), &[0], move |cx| {
            let xv = cx.int(cx.one(0)).expect("integer");
            let g = cx.value(gs[(xv % 100) as usize]).and_then(|v| cx.int(v)).unwrap_or(0);
            Ok(xv % 7 > g)
        });
        k.define(h, p, rhs(move |b, h| h.call(hi, &[b.one(0)]))).expect("rule");
        let calls: Vec<T> = (0..n as i64).map(|i| h.call(f, &[h.int(i)])).collect();
        k.roots.push(h.call(list, &calls));
    });
    let mut fps = vec![];
    for phase in 0..4 {
        fps.push(s.step(|h, k| {
            match phase {
                2 => k.set_own(h.sym("zz"), h.int(1)),
                3 => k.set_own(h.sym("g3"), h.int(6)),
                _ => {}
            }
            let t = k.roots.get(h, 0);
            let r = run(k, h, t, &mut o);
            // kept like session history: the memo is weak in values, so unkept results would not survive the safe point
            k.roots.push(r);
            fingerprint(h, r)
        }));
    }
    assert!(fps[0] == fps[1] && fps[1] == fps[2], "R14: results changed without a relevant redefinition");
    o.check = combine(fps[0], fps[3]);
    o.note = format!("result {}", if fps[0] == fps[3] { "unchanged" } else { "changed" });
    o
}

// ---------------------------------------------------------------------------------------------------------------
// S7

/// S7 over `n` elements.
pub fn s7(run_cfg: Run, n: usize) -> Out {
    let mut s = session(run_cfg.heap, run_cfg.threads, run_cfg.eval);
    let mut o = Out::default();
    s.step(|h, k| {
        let (f, g, hh, add, cases) = (h.sym("f"), h.sym("g"), h.sym("h"), h.sym("add"), h.sym("cases"));
        k.set_attributes(add, ac());
        let c1 = h.sym_term(h.sym("c01"));
        let pats: Vec<Pat> = vec![
            cond(call(h, f, vec![typed(0, Ty::Integer), typed(1, Ty::Integer)]), &[0, 1], |cx| Ok(cx.int(cx.one(0)) > cx.int(cx.one(1)))),
            alt(vec![call(h, g, vec![blank(), blank()]), call(h, hh, vec![blank()])]),
            call(h, f, vec![var(0), var(0)]),
            call(h, add, vec![lit(c1), seq1(0)]),
            call(h, add, vec![typed(0, Ty::Symbol), typed(1, Ty::Integer), seq0(2)]),
        ];
        let pats = pats.into_iter().map(|p| k.compile_pattern(h, p).expect("pattern")).collect();
        k.register(cases, held(), Box::new(Cases(pats, h.sym("none"))));
    });
    s.step(|h, k| {
        let (f, g, hh, add, cases, list) = (h.sym("f"), h.sym("g"), h.sym("h"), h.sym("add"), h.sym("cases"), h.sym("list"));
        let cs: Vec<T> = (0..20).map(|i| h.sym_term(h.sym(&format!("c{i:02}")))).collect();
        let mut rng = Rng::new(7);
        let elems: Vec<T> = (0..n)
            .map(|_| {
                let r = rng.below(100);
                let i = |rng: &mut Rng| h.int(rng.below(50) as i64);
                if r < 30 {
                    let a = i(&mut rng);
                    let b = i(&mut rng);
                    h.call(f, &[a, b])
                } else if r < 35 {
                    let a = i(&mut rng);
                    h.call(f, &[a, a])
                } else if r < 50 {
                    let c = pick(&mut rng, &cs);
                    let a = i(&mut rng);
                    h.call(g, &[c, a])
                } else if r < 60 {
                    h.call(hh, &[pick(&mut rng, &cs)])
                } else if r < 70 {
                    i(&mut rng)
                } else if r < 80 {
                    pick(&mut rng, &cs)
                } else {
                    let m = 2 + rng.below(5) as usize;
                    let a: Vec<T> = (0..m).map(|_| if pct(&mut rng, 70) { pick(&mut rng, &cs) } else { h.int(rng.below(9) as i64) }).collect();
                    k.make(h, h.sym_term(add), &a)
                }
            })
            .collect();
        let l = h.call(list, &elems);
        let calls: Vec<T> = (0..5).map(|j| h.call(cases, &[l, h.int(j)])).collect();
        let r = run(k, h, h.call(list, &calls), &mut o);
        o.check = fingerprint(h, r);
        o.note = format!("selected {:?}", args(h, r).iter().map(|&x| args(h, x).len()).collect::<Vec<_>>());
    });
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R14e

/// How R14e's condition is computed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CondKind {
    /// the predicate computed in the condition's code, reading `c` directly
    Native,
    /// `is_prime(g(x))` built in the heap and evaluated inside the match
    Eval,
}

/// R14e with `n` calls.
pub fn r14e(run_cfg: Run, n: usize, kind: CondKind) -> Out {
    let mut s = session(run_cfg.heap, run_cfg.threads, run_cfg.eval);
    let mut o = Out::default();
    s.step(|h, k| {
        let (f, g, hi, lo, c, list) = (h.sym("f"), h.sym("g"), h.sym("hi"), h.sym("lo"), h.sym("c"), h.sym("list"));
        let (imod, iadd, imul, isp) = (h.sym("imod"), h.sym("iadd"), h.sym("imul"), h.sym("is_prime"));
        k.register(isp, strict(), Box::new(IsPrime));
        k.set_own(c, h.int(1));
        // g(x_ : integer) := imod(iadd(imul(x, 7919), c), 1000003)
        let gr = rhs(move |b, h| {
            let m = h.call(imul, &[b.one(0), h.int(7919)]);
            h.call(imod, &[h.call(iadd, &[m, h.sym_term(c)]), h.int(1_000_003)])
        });
        k.define(h, call(h, g, vec![typed(0, Ty::Integer)]), gr).expect("rule");
        k.define(h, call(h, f, vec![var(0)]), rhs(move |b, h| h.call(lo, &[b.one(0)]))).expect("rule");
        let fx = call(h, f, vec![typed(0, Ty::Integer)]);
        let p = match kind {
            CondKind::Native => cond(fx, &[0], move |cx| {
                let xv = cx.int(cx.one(0)).expect("integer");
                let cv = cx.value(c).and_then(|v| cx.int(v)).unwrap_or(0);
                Ok(is_prime((xv.wrapping_mul(7919) + cv).rem_euclid(1_000_003)))
            }),
            CondKind::Eval => cond(fx, &[0], move |cx| {
                let h = cx.heap();
                let t = h.call(isp, &[h.call(g, &[cx.one(0)])]);
                let r = cx.eval(t)?;
                Ok(cx.is_true(r))
            }),
        };
        k.define(h, p, rhs(move |b, h| h.call(hi, &[b.one(0)]))).expect("rule");
        let calls: Vec<T> = (0..n as i64).map(|i| h.call(f, &[h.int(i)])).collect();
        k.roots.push(h.call(list, &calls));
    });
    let mut fps = vec![];
    let mut hits = vec![];
    for phase in 0..5 {
        let (fp, n_hi) = s.step(|h, k| {
            match phase {
                2 => k.set_own(h.sym("zz"), h.int(1)),
                3 => k.set_own(h.sym("c"), h.int(2)),
                4 => {
                    // g(x_ : integer) /; imod(x, 10) == 0 := 2: an instance of the general g rule, ordered first (6.3)
                    let gc = cond(call(h, h.sym("g"), vec![typed(0, Ty::Integer)]), &[0], |cx| Ok(cx.int(cx.one(0)).expect("integer") % 10 == 0));
                    k.define(h, gc, rhs(|_, h| h.int(2))).expect("rule");
                }
                _ => {}
            }
            let t = k.roots.get(h, 0);
            let r = run(k, h, t, &mut o);
            // kept like session history: the memo is weak in values, so unkept results would not survive the safe point
            k.roots.push(r);
            let hi = h.sym_term(h.sym("hi"));
            (fingerprint(h, r), args(h, r).iter().filter(|&&u| head(h, u) == Some(hi)).count())
        });
        fps.push(fp);
        hits.push(n_hi);
    }
    assert!(fps[0] == fps[1] && fps[1] == fps[2], "R14e: results changed without a relevant redefinition");
    o.check = [fps[0], fps[3], fps[4]].iter().fold(0, |a, &f| combine(a, f));
    o.note = format!("hi: {} / {} / {} of {n}", hits[0], hits[3], hits[4]);
    o
}

// ---------------------------------------------------------------------------------------------------------------
// MR

/// MR with `n` elements; `layout` is `None` for the fixed layout of effects, `Some(seed)` for a random one.
pub fn mr(run_cfg: Run, n: usize, layout: Option<u64>) -> Out {
    let mut s = session(run_cfg.heap, run_cfg.threads, run_cfg.eval);
    let mut o = Out::default();
    s.step(|h, k| {
        let ff = h.sym("F");
        let (lo, lo2, hi, eleven, deep, one) = (h.sym("lo"), h.sym("lo2"), h.sym("hi"), h.sym("eleven"), h.sym("deep"), h.sym("one"));
        let (wrap, redef, newrule, show, c) = (h.sym("wrap"), h.sym("redef"), h.sym("newrule"), h.sym("show"), h.sym("c"));
        let kindx = h.sym("kindx");
        let (imod, iadd, less, lp, define, defrule, print) =
            (h.sym("imod"), h.sym("iadd"), h.sym("less"), h.sym("loop"), h.sym("define"), h.sym("defrule"), h.sym("print"));
        k.register(lp, strict(), Box::new(Loop(lp)));
        k.set_own(c, h.int(3));
        k.define(h, call(h, ff, vec![var(0)]), rhs(move |b, h| h.call(lo, &[b.one(0)]))).expect("rule");
        // F(x_ : integer) /; less(imod(x, 7), c) := hi(iadd(loop(40, x), c)): the condition evaluates and reads c
        let p = cond(call(h, ff, vec![typed(0, Ty::Integer)]), &[0], move |cx| {
            let h = cx.heap();
            let t = h.call(less, &[h.call(imod, &[cx.one(0), h.int(7)]), h.sym_term(c)]);
            let r = cx.eval(t)?;
            Ok(cx.is_true(r))
        });
        k.define(h, p, rhs(move |b, h| h.call(hi, &[h.call(iadd, &[h.call(lp, &[h.int(40), b.one(0)]), h.sym_term(c)])]))).expect("rule");
        for j in (0..3000i64).step_by(3) {
            k.define(h, call(h, ff, vec![lit(h.int(j))]), rhs(move |_, h| h.int(3 * j + 1))).expect("rule");
        }
        for j in 0..50 {
            let kj = h.sym(&format!("kind{j}"));
            let j = j as i64;
            let r = rhs(move |b, h| h.call(iadd, &[b.one(0), h.int(j)]));
            k.define_attached(h, call(h, ff, vec![call(h, kj, vec![typed(0, Ty::Integer)])]), r, kj).expect("rule");
        }
        let ffr = |p| call(h, ff, vec![p]);
        k.define(h, ffr(call(h, redef, vec![var(0)])), rhs(move |b, h| h.call(define, &[h.sym_term(c), b.one(0)]))).expect("rule");
        k.define(h, ffr(call(h, newrule, vec![typed(0, Ty::Integer)])), rhs(move |b, h| h.call(defrule, &[b.one(0)]))).expect("rule");
        k.define(h, ffr(call(h, show, vec![var(0)])), rhs(move |b, h| h.call(print, &[h.call(ff, &[b.one(0)])]))).expect("rule");
        // rules defined by `defrule(k)` during the map
        let eleven_c = cond(call(h, ff, vec![typed(0, Ty::Integer)]), &[0], move |cx| {
            let h = cx.heap();
            let r = cx.eval(h.call(imod, &[cx.one(0), h.int(11)]))?;
            Ok(cx.int(r) == Some(0))
        });
        let table = vec![
            // 0: a literal rule (an instance of F(x_ : integer): ordered before it)
            RuleDef { lhs: call(h, ff, vec![lit(h.int(1))]), rhs: rhs(move |_, h| h.sym_term(one)), attach: None },
            // 1: a conditional rule whose condition evaluates; defined twice in the fixed layout (the second replaces)
            RuleDef { lhs: eleven_c, rhs: rhs(move |b, h| h.call(eleven, &[b.one(0)])), attach: None },
            // 2: a new argument-head rule set
            RuleDef {
                lhs: call(h, ff, vec![call(h, kindx, vec![var(0)])]),
                rhs: rhs(move |b, h| h.call(iadd, &[b.one(0), h.int(1000)])),
                attach: Some(kindx),
            },
            // 3: a deeper path than any so far (the tree's depth grows while readers flatten subjects)
            RuleDef {
                lhs: call(h, ff, vec![call(h, wrap, vec![call(h, wrap, vec![call(h, wrap, vec![typed(0, Ty::Integer)])])])]),
                rhs: rhs(move |b, h| h.call(deep, &[b.one(0)])),
                attach: None,
            },
            // 4: the general rule again, same left side up to renaming: replaces it
            RuleDef { lhs: call(h, ff, vec![var(3)]), rhs: rhs(move |b, h| h.call(lo2, &[b.one(3)])), attach: None },
        ];
        k.register(defrule, Attributes { effect: true, ..strict() }, Box::new(DefRule(table)));
    });
    s.step(|h, k| {
        let (ff, wrap, redef, newrule, show, kindx, list, map) =
            (h.sym("F"), h.sym("wrap"), h.sym("redef"), h.sym("newrule"), h.sym("show"), h.sym("kindx"), h.sym("list"), h.sym("map"));
        let kinds: Vec<Sym> = (0..50).map(|j| h.sym(&format!("kind{j}"))).collect();
        let mut rng = Rng::new(layout.unwrap_or(0) * 7 + 17);
        let mut xs: Vec<T> = (0..n)
            .map(|_| {
                let r = rng.below(100);
                let i = h.int(rng.below(3000) as i64);
                if r < 70 {
                    i
                } else if r < 80 {
                    h.call(pick(&mut rng, &kinds), &[i])
                } else if r < 85 {
                    h.call(kindx, &[i])
                } else {
                    h.call(wrap, &[h.call(wrap, &[h.call(wrap, &[i])])])
                }
            })
            .collect();
        let effect = |e: u64, v: i64| match e {
            0 => h.call(redef, &[h.int(v)]),
            1 => h.call(newrule, &[h.int(v.rem_euclid(5))]),
            _ => h.call(show, &[h.int(v)]),
        };
        match layout {
            None => {
                let fixed: [(usize, u64, i64); 10] =
                    [(10, 0, 7), (25, 1, 0), (40, 2, 5), (50, 0, 2), (60, 1, 1), (75, 1, 3), (85, 1, 2), (90, 0, 4), (95, 1, 4), (97, 1, 1)];
                for (p, e, v) in fixed {
                    xs[n * p / 100] = effect(e, v);
                }
            }
            Some(_) => {
                for _ in 0..1 + rng.below(12) {
                    let pos = rng.below(n as u64) as usize;
                    let (e, v) = (rng.below(3), rng.below(9) as i64);
                    xs[pos] = effect(e, v);
                }
            }
        }
        let r = run(k, h, h.call(map, &[h.sym_term(ff), h.call(list, &xs)]), &mut o);
        let outs = k.output(h);
        let c = h.sym("c");
        let mut check = fingerprint(h, r);
        for &u in &outs {
            check = combine(check, fingerprint(h, u));
        }
        check = combine(check, k.value_of(h, c).map_or(0, |v| fingerprint(h, v)));
        for x in [ff, c, kindx, wrap] {
            check = combine(check, k.version(x) as u64);
        }
        check = combine(check, k.rule_count(ff) as u64);
        check = k.rule_order(ff).iter().fold(check, |a, &id| combine(a, id as u64));
        o.check = check;
        o.note = format!("{} outputs, F has {} rules, epoch {}", outs.len(), k.rule_count(ff), k.epoch());
    });
    o
}

/// MR fuzz: random layouts `1..=layouts` of `n` elements each, checks combined.
pub fn mr_fuzz(run_cfg: Run, n: usize, layouts: u64) -> Out {
    let mut o = Out::default();
    for l in 1..=layouts {
        let x = mr(run_cfg, n, Some(l));
        o.check = combine(o.check, x.check);
        o.time += x.time;
        o.stats.splits += x.stats.splits;
        o.stats.redos += x.stats.redos;
        o.stats.fuel += x.stats.fuel;
        o.stats.holes += x.stats.holes;
    }
    o.note = format!("{layouts} random layouts");
    o
}
