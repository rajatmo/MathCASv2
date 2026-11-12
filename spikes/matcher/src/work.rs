//! Workloads (ids from 02-benchmarks.md). Each runs one variant at one size and returns its phases' times, a
//! fingerprint of its results (compared across variants by the driver) and a note.
use crate::eval::{Cfg, Evaluator, Track};
use crate::index::{Dispatch, OrderBy, Rhs};
use crate::matcher::{Binds, Matcher, PlainEnv, Val};
use crate::pat::{self, alt, blank, cond, lit, papp, seq0, seq1, x, xt, Pat, Ty};
use crate::store::{Store, Term};
use crate::{combine, fingerprint, Limit, Rng};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub struct Opts {
    pub quick: bool,
    pub limit: Duration,
}

#[derive(Default)]
pub struct Out {
    pub phases: Vec<(&'static str, Duration)>,
    pub check: u64,
    pub note: String,
    /// A limit stopped the run: no check.
    pub limited: bool,
    /// Expected to differ from the other variants (and the note says why).
    pub exclude: bool,
}

pub struct Case {
    pub id: &'static str,
    pub sizes: [usize; 3],
    pub quick: usize,
    pub variants: &'static [&'static str],
    pub run: fn(&str, usize, &Opts) -> Out,
}

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "R4 AC rules",
            sizes: [100, 1000, 10_000],
            quick: 100,
            variants: &["linear/naive", "linear/smart", "indexed/naive", "indexed/naive+rest", "indexed/smart"],
            run: r4,
        },
        Case {
            id: "R4h AC pigeonhole",
            sizes: [6, 8, 10],
            quick: 6,
            variants: &["naive+rest", "smart"],
            run: r4h,
        },
        Case { id: "R5 literal rules", sizes: [500, 5000, 50_000], quick: 500, variants: &["linear", "indexed"], run: r5 },
        Case {
            id: "R13 arg-head rules",
            sizes: [10, 100, 1000],
            quick: 10,
            variants: &["arg-head", "on-area/linear", "on-area/indexed"],
            run: r13,
        },
        Case {
            id: "R14 read sets",
            sizes: [10_000, 100_000, 1_000_000],
            quick: 10_000,
            variants: &["no-memo", "memo/untracked", "memo/mask", "memo/exact"],
            run: r14,
        },
        Case { id: "S7 cases", sizes: [10_000, 100_000, 1_000_000], quick: 10_000, variants: &["naive", "smart"], run: s7 },
        Case { id: "D5 define literal", sizes: [500, 5000, 50_000], quick: 500, variants: &["scan", "index"], run: d5 },
        Case { id: "D4 define AC", sizes: [100, 1000, 10_000], quick: 100, variants: &["scan", "index"], run: d4 },
    ]
}

fn timed<R>(o: &mut Out, name: &'static str, f: impl FnOnce() -> R) -> R {
    let t = Instant::now();
    let r = f();
    o.phases.push((name, t.elapsed()));
    r
}

fn limit(opts: &Opts) -> Limit {
    Limit::new(Some(opts.limit))
}

/// A note for a run stopped by the limit: how far it got and the cost per item so far.
fn limited(o: &mut Out, what: &str, done: usize, total: usize, t: Duration) {
    o.limited = true;
    o.note = format!(
        "limit: {done} of {total} {what} in {:.1} s ({:.2} us each)",
        t.as_secs_f64(),
        t.as_secs_f64() * 1e6 / done.max(1) as f64
    );
}

/// A right side that records which rule fired and what it bound: `hit(k, b0, b1, ...)`, sequences as lists.
fn hit_rhs(hit: Term, list: Term, k: i64, nvars: usize) -> Rhs {
    Rc::new(move |b: &Binds, st: &mut Store| {
        let mut items = vec![st.int(k)];
        for v in 0..nvars as u16 {
            let t = match b.get(v) {
                Val::One(t) => t,
                Val::Seq(..) => {
                    let s = b.seq(v).to_vec();
                    st.app(list, &s)
                }
                Val::Unbound => st.int(-1),
            };
            items.push(t);
        }
        st.app(hit, &items)
    })
}

// ---------------------------------------------------------------------------------------------------------------
// R4: AC matching, many rules on add/mul applied once at the top of 10^4 terms

struct AcWorld {
    add: Term,
    mul: Term,
    cs: Vec<Term>,
    fs: Vec<Term>,
    hit: Term,
    list: Term,
}

fn ac_world(ev: &mut Evaluator) -> AcWorld {
    let st = &mut ev.st;
    let add = st.sym("add");
    let mul = st.sym("mul");
    let cs = (0..100).map(|i| st.sym(&format!("c{i:02}"))).collect();
    let fs = (0..10).map(|i| st.sym(&format!("f{i}"))).collect();
    let hit = st.sym("hit");
    let list = st.sym("list");
    ev.set_ac(add);
    ev.set_ac(mul);
    AcWorld { add, mul, cs, fs, hit, list }
}

fn pick(rng: &mut Rng, v: &[Term]) -> Term {
    v[rng.below(v.len() as u64) as usize]
}

/// A random canonical sum or product of 4 to 16 arguments (sums in a computer algebra system are wide).
fn ac_subject(st: &mut Store, w: &AcWorld, rng: &mut Rng) -> Term {
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
            // f_i of a small pool, so that non-linear patterns like f(x_) + g(x_) find partners
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

/// The k-th random AC rule. Most rules require two things of the subject (two literals, or a literal and a head),
/// as rules in a real rule set are selective; one in a thousand requires nothing the index can see.
fn ac_rule(st: &mut Store, w: &AcWorld, rng: &mut Rng, k: i64) -> (Term, Pat, Rhs) {
    let (h, g) = if rng.pct(50) { (w.add, w.mul) } else { (w.mul, w.add) };
    let c = |rng: &mut Rng| lit(pick(rng, &w.cs));
    let f = |rng: &mut Rng| pick(rng, &w.fs);
    let fx = |rng: &mut Rng, v: u16| papp(pick(rng, &w.fs), vec![x(v)]);
    let lhs = if k % 1000 == 999 {
        // a rare rule without literal arguments: the index cannot exclude it
        let p = papp(h, vec![xt(0, Ty::Symbol), xt(1, Ty::Symbol), xt(2, Ty::Symbol), seq0(3)]);
        cond(p, |b, env| {
            let st = env.st();
            let s: u64 = (0..3).map(|v| st.sym_index(b.one(v)).unwrap() as u64).sum();
            s.is_multiple_of(997)
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
    let _ = st;
    (h, lhs, hit_rhs(w.hit, w.list, k, nv))
}

fn r4(variant: &str, n_rules: usize, opts: &Opts) -> Out {
    let (dispatch, smart, rest) = match variant {
        "linear/naive" => (Dispatch::Linear, false, false),
        "linear/smart" => (Dispatch::Linear, true, false),
        "indexed/naive" => (Dispatch::Indexed, false, false),
        "indexed/naive+rest" => (Dispatch::Indexed, false, true),
        _ => (Dispatch::Indexed, true, false),
    };
    let n_sub = if opts.quick { 1000 } else { 10_000 };
    let cfg = Cfg { dispatch, smart, rest, ..Cfg::default() };
    let mut ev = Evaluator::new(Store::new(), cfg, limit(opts));
    let w = ac_world(&mut ev);
    let mut o = Out::default();
    let mut rng = Rng::new(4);
    let rules: Vec<_> = (0..n_rules).map(|k| ac_rule(&mut ev.st, &w, &mut rng, k as i64)).collect();
    let mut rng = Rng::new(44);
    let subs: Vec<Term> = (0..n_sub).map(|_| ac_subject(&mut ev.st, &w, &mut rng)).collect();
    let ok = timed(&mut o, "define", || rules.into_iter().try_for_each(|(_, l, r)| ev.define(l, r, None)));
    if ok.is_err() {
        o.limited = true;
        o.note = "limit while defining".into();
        return o;
    }
    let t0 = Instant::now();
    let mut res = Vec::with_capacity(n_sub);
    let r = timed(&mut o, "apply", || {
        for &s in &subs {
            match ev.rewrite_top(s) {
                Ok(r) => res.push(r.unwrap_or(s)),
                Err(_) => return Err(()),
            }
        }
        Ok(())
    });
    if r.is_err() {
        limited(&mut o, "subjects", res.len(), n_sub, t0.elapsed());
        return o;
    }
    let hits = res.iter().zip(&subs).filter(|(r, s)| r != s).count();
    o.check = res.iter().fold(0, |h, &t| combine(h, fingerprint(&ev.st, t)));
    o.note = format!("{hits} of {n_sub} subjects rewritten");
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R4h: the case candidate sets and the Hall test exist for. `add(f(x1_), ..., f(xn_), rest___)` against sums with
// only n-1 arguments of head f among 12 others: backtracking tries every assignment of the n-1 f-terms to the first
// n-1 patterns before the last one fails, (n-1)! of them; the Hall test fails before the search. Half the subjects
// have n f-terms and match.

fn r4h(variant: &str, n: usize, opts: &Opts) -> Out {
    let smart = variant == "smart";
    let n_sub = if opts.quick { 100 } else { 1000 };
    let mut st = Store::new();
    let (add, f, list) = (st.sym("add"), st.sym("f"), st.sym("list"));
    let cs: Vec<Term> = (0..40).map(|i| st.sym(&format!("c{i:02}"))).collect();
    let is_ac = |t: Term| t == add;
    let mut args: Vec<Pat> = (0..n as u16).map(|v| papp(f, vec![x(v)])).collect();
    args.push(seq0(n as u16));
    let p = pat::compile(&mut st, papp(add, args), &is_ac);
    let mut rng = Rng::new(8);
    let subs: Vec<Term> = (0..n_sub)
        .map(|i| {
            let k = if i % 2 == 0 { n - 1 } else { n };
            let mut a: Vec<Term> = (0..k).map(|j| st.app(f, &[cs[j]])).collect();
            a.extend((0..12).map(|_| pick(&mut rng, &cs[10..])));
            st.app_ac(add, &a)
        })
        .collect();
    let lim = limit(opts);
    let mut o = Out::default();
    let mut res = vec![];
    let t0 = Instant::now();
    timed(&mut o, "match", || {
        let env = PlainEnv(&st);
        let mt = Matcher::new(&env, smart, &lim).with_rest(true);
        let (mut s, mut b) = (Binds::default(), Binds::default());
        for &t in &subs {
            let m = mt.first(&p, n + 1, t, &mut s, &mut b);
            if lim.hit() {
                return;
            }
            res.push(if m { b.fingerprint(&st) } else { 0 });
        }
    });
    if lim.hit() {
        limited(&mut o, "subjects", res.len(), n_sub, t0.elapsed());
        return o;
    }
    let _ = list;
    o.check = res.iter().fold(0, |h, &v| combine(h, v));
    o.note = format!("{} of {n_sub} match", res.iter().filter(|&&v| v != 0).count());
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R5: many rules with literal arguments, 10^6 calls

fn r5(variant: &str, n_rules: usize, opts: &Opts) -> Out {
    let dispatch = if variant == "linear" { Dispatch::Linear } else { Dispatch::Indexed };
    let n_calls = if opts.quick { 10_000 } else { 1_000_000 };
    let mut ev = Evaluator::new(Store::new(), Cfg { dispatch, ..Cfg::default() }, limit(opts));
    let g = ev.st.sym("g");
    let miss = ev.st.sym("miss");
    let mut o = Out::default();
    // the general rule first, so that every literal rule must be ordered before it (semantics 6.3)
    let ok = timed(&mut o, "define", || -> Result<(), crate::Stop> {
        let p = papp(g, vec![x(0)]);
        ev.define(p, Rc::new(move |b, st| st.app(miss, &[b.one(0)])), None)?;
        for k in 0..n_rules as i64 {
            let p = papp(g, vec![lit(ev.st.int(k))]);
            ev.define(p, Rc::new(move |_, st| st.int(1_000_000 + 3 * k)), None)?;
        }
        Ok(())
    });
    if ok.is_err() {
        o.limited = true;
        o.note = "limit while defining".into();
        return o;
    }
    let mut rng = Rng::new(5);
    let calls: Vec<Term> = (0..n_calls)
        .map(|_| {
            let k = ev.st.int(rng.below(2 * n_rules as u64) as i64);
            ev.st.app(g, &[k])
        })
        .collect();
    let t0 = Instant::now();
    let mut res = Vec::with_capacity(n_calls);
    let r = timed(&mut o, "calls", || calls.iter().try_for_each(|&c| ev.eval(c).map(|r| res.push(r))));
    if r.is_err() {
        limited(&mut o, "calls", res.len(), n_calls, t0.elapsed());
        return o;
    }
    let hits = res.iter().filter(|&&r| ev.st.as_int(r).is_some()).count();
    o.check = res.iter().fold(0, |h, &t| combine(h, fingerprint(&ev.st, t)));
    o.note = format!("{hits} of {n_calls} calls hit a literal rule");
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R13: a generic `area` extended by many kinds of objects

fn r13(variant: &str, kinds: usize, opts: &Opts) -> Out {
    let (dispatch, on_arg) = match variant {
        "arg-head" => (Dispatch::Indexed, true),
        "on-area/linear" => (Dispatch::Linear, false),
        _ => (Dispatch::Indexed, false),
    };
    let n_calls = if opts.quick { 10_000 } else { 1_000_000 };
    let mut ev = Evaluator::new(Store::new(), Cfg { dispatch, ..Cfg::default() }, limit(opts));
    let area = ev.st.sym("area");
    let unknown = ev.st.sym("unknown_area");
    let unk = ev.st.sym("blob");
    let ks: Vec<Term> = (0..kinds).map(|j| ev.st.sym(&format!("kind{j}"))).collect();
    let mut o = Out::default();
    let ok = timed(&mut o, "define", || -> Result<(), crate::Stop> {
        ev.define(papp(area, vec![x(0)]), Rc::new(move |b, st| st.app(unknown, &[b.one(0)])), None)?;
        for (j, &kj) in ks.iter().enumerate() {
            let p = papp(area, vec![papp(kj, vec![xt(0, Ty::Integer)])]);
            let j = j as i64;
            let rhs: Rhs = Rc::new(move |b, st| {
                let a = st.as_int(b.one(0)).unwrap();
                st.int((a * a + j) % 1_000_000_007)
            });
            ev.define(p, rhs, if on_arg { Some(kj) } else { None })?;
        }
        Ok(())
    });
    if ok.is_err() {
        o.limited = true;
        o.note = "limit while defining".into();
        return o;
    }
    let mut rng = Rng::new(13);
    let calls: Vec<Term> = (0..n_calls)
        .map(|i| {
            let k = if rng.pct(10) { unk } else { pick(&mut rng, &ks) };
            let a = ev.st.int(i as i64 % 30_000);
            let obj = ev.st.app(k, &[a]);
            ev.st.app(area, &[obj])
        })
        .collect();
    let t0 = Instant::now();
    let mut res = Vec::with_capacity(n_calls);
    let r = timed(&mut o, "calls", || calls.iter().try_for_each(|&c| ev.eval(c).map(|r| res.push(r))));
    if r.is_err() {
        limited(&mut o, "calls", res.len(), n_calls, t0.elapsed());
        return o;
    }
    let generic = res.iter().filter(|&&r| ev.st.head(r) == Some(unknown)).count();
    o.check = res.iter().fold(0, |h, &t| combine(h, fingerprint(&ev.st, t)));
    o.note = format!("{generic} calls fell through to the generic rule");
    o
}

// ---------------------------------------------------------------------------------------------------------------
// R14: conditions that read global values; read-set tracking and invalidation

struct R14World {
    f: Term,
    gs: Rc<Vec<Term>>,
    zz: Term,
    list: Term,
}

fn r14_setup(ev: &mut Evaluator) -> R14World {
    let f = ev.st.sym("f");
    let hi = ev.st.sym("hi");
    let lo = ev.st.sym("lo");
    let zz = ev.st.sym("zz");
    let list = ev.st.sym("list");
    let gs: Rc<Vec<Term>> = Rc::new((0..100).map(|k| ev.st.sym(&format!("g{k}"))).collect());
    for (k, &g) in gs.iter().enumerate() {
        let v = ev.st.int(k as i64 % 7);
        ev.set_own(g, v);
    }
    // defined general rule first: the typed, conditional rule is an instance of it and goes before it
    ev.define(papp(f, vec![x(0)]), Rc::new(move |b, st| st.app(lo, &[b.one(0)])), None).unwrap();
    let gs2 = gs.clone();
    let p = cond(papp(f, vec![xt(0, Ty::Integer)]), move |b, env| {
        let xv = env.st().as_int(b.one(0)).unwrap();
        let g = env.value(gs2[(xv % 100) as usize]).and_then(|v| env.st().as_int(v)).unwrap_or(0);
        xv % 7 > g
    });
    ev.define(p, Rc::new(move |b, st| st.app(hi, &[b.one(0)])), None).unwrap();
    R14World { f, gs, zz, list }
}

fn r14_term(ev: &mut Evaluator, w: &R14World, n: usize) -> Term {
    let calls: Vec<Term> = (0..n as i64)
        .map(|i| {
            let a = ev.st.int(i);
            ev.st.app(w.f, &[a])
        })
        .collect();
    ev.st.app(w.list, &calls)
}

fn r14(variant: &str, n: usize, opts: &Opts) -> Out {
    let (memo, track) = match variant {
        "no-memo" => (false, Track::Off),
        "memo/untracked" => (true, Track::Off),
        "memo/mask" => (true, Track::Mask),
        _ => (true, Track::Exact),
    };
    let mut ev = Evaluator::new(Store::new(), Cfg { memo, track, ..Cfg::default() }, limit(opts));
    let w = r14_setup(&mut ev);
    let t = r14_term(&mut ev, &w, n);
    let mut o = Out::default();
    let mut steps = vec![];
    let mut run = |ev: &mut Evaluator, o: &mut Out, name: &'static str| -> Option<Term> {
        let s0 = ev.steps;
        let r = timed(o, name, || ev.eval(t)).ok();
        steps.push(ev.steps - s0);
        r
    };
    let Some(r1) = run(&mut ev, &mut o, "eval") else { return stopped(o) };
    let Some(r2) = run(&mut ev, &mut o, "re-eval, no change") else { return stopped(o) };
    let one = ev.st.int(1);
    ev.set_own(w.zz, one);
    let Some(r3) = run(&mut ev, &mut o, "re-eval, unrelated change") else { return stopped(o) };
    let six = ev.st.int(6);
    ev.set_own(w.gs[3], six);
    let Some(r4) = run(&mut ev, &mut o, "re-eval, g3 changed") else { return stopped(o) };
    assert!(r1 == r2 && r2 == r3, "results changed without a relevant redefinition");
    // ground truth: a fresh evaluator without memo, with g3 = 6 from the start
    let mut fresh = Evaluator::new(Store::new(), Cfg::default(), Limit::none());
    let fw = r14_setup(&mut fresh);
    let six = fresh.st.int(6);
    fresh.set_own(fw.gs[3], six);
    let ft = r14_term(&mut fresh, &fw, n);
    let truth = fresh.eval(ft).unwrap();
    let (fp, tp) = (fingerprint(&ev.st, r4), fingerprint(&fresh.st, truth));
    o.check = fp;
    let changed = fingerprint(&ev.st, r1) != fp;
    o.note = format!(
        "rule steps per phase {steps:?}; {}",
        match (fp == tp, track, memo) {
            (true, _, _) => format!("correct after the g3 change (result {})", if changed { "changed" } else { "unchanged" }),
            (false, Track::Off, true) => {
                o.exclude = true;
                "STALE after the g3 change, as expected without read sets".to_string()
            }
            _ => "WRONG after the g3 change".to_string(),
        }
    );
    o
}

fn stopped(mut o: Out) -> Out {
    o.limited = true;
    o.note = "limit".into();
    o
}

// ---------------------------------------------------------------------------------------------------------------
// S7: `cases` (pattern selection without rule sets)

fn s7(variant: &str, n: usize, opts: &Opts) -> Out {
    let smart = variant == "smart";
    let mut st = Store::new();
    let (f, g, h, add, list) = (st.sym("f"), st.sym("g"), st.sym("h"), st.sym("add"), st.sym("list"));
    let cs: Vec<Term> = (0..20).map(|i| st.sym(&format!("c{i:02}"))).collect();
    let mut rng = Rng::new(7);
    let elems: Vec<Term> = (0..n)
        .map(|_| {
            let r = rng.below(100);
            let i = |rng: &mut Rng, st: &mut Store| st.int(rng.below(50) as i64);
            if r < 30 {
                let (a, b) = (i(&mut rng, &mut st), i(&mut rng, &mut st));
                st.app(f, &[a, b])
            } else if r < 35 {
                let a = i(&mut rng, &mut st);
                st.app(f, &[a, a])
            } else if r < 50 {
                let (c, a) = (pick(&mut rng, &cs), i(&mut rng, &mut st));
                st.app(g, &[c, a])
            } else if r < 60 {
                let c = pick(&mut rng, &cs);
                st.app(h, &[c])
            } else if r < 70 {
                i(&mut rng, &mut st)
            } else if r < 80 {
                pick(&mut rng, &cs)
            } else {
                let k = 2 + rng.below(5) as usize;
                let args: Vec<Term> = (0..k)
                    .map(|_| if rng.pct(70) { pick(&mut rng, &cs) } else { st.int(rng.below(9) as i64) })
                    .collect();
                st.app_ac(add, &args)
            }
        })
        .collect();
    let is_ac = |t: Term| t == add;
    let c1 = cs[1];
    let pats: Vec<Pat> = vec![
        cond(papp(f, vec![xt(0, Ty::Integer), xt(1, Ty::Integer)]), |b, env| {
            env.st().as_int(b.one(0)) > env.st().as_int(b.one(1))
        }),
        alt(vec![papp(g, vec![blank(), blank()]), papp(h, vec![blank()])]),
        papp(f, vec![x(0), x(0)]),
        papp(add, vec![lit(c1), seq1(0)]),
        papp(add, vec![xt(0, Ty::Symbol), xt(1, Ty::Integer), seq0(2)]),
    ]
    .into_iter()
    .map(|p| pat::compile(&mut st, p, &is_ac))
    .collect();
    let lim = limit(opts);
    let mut o = Out::default();
    let mut found: Vec<Vec<Term>> = vec![];
    let t0 = Instant::now();
    timed(&mut o, "cases x5", || {
        let env = PlainEnv(&st);
        let mt = Matcher::new(&env, smart, &lim);
        let mut scratch = Binds::default();
        for p in &pats {
            let nv = p.n_vars();
            let mut sel = vec![];
            for &e in &elems {
                if mt.matches(p, nv, e, &mut scratch) {
                    sel.push(e);
                }
                if lim.hit() {
                    return;
                }
            }
            found.push(sel);
        }
    });
    if lim.hit() {
        limited(&mut o, "patterns", found.len(), pats.len(), t0.elapsed());
        return o;
    }
    let results: Vec<Term> = found.iter().map(|sel| st.app(list, sel)).collect();
    o.check = results.iter().fold(0, |h, &t| combine(h, fingerprint(&st, t)));
    o.note = format!("selected {:?}", found.iter().map(Vec::len).collect::<Vec<_>>());
    o
}

// ---------------------------------------------------------------------------------------------------------------
// Definition-time cost of rule order (semantics 6.3)

fn d5(variant: &str, n: usize, opts: &Opts) -> Out {
    let order = if variant == "scan" { OrderBy::Scan } else { OrderBy::Index };
    let mut ev = Evaluator::new(Store::new(), Cfg { order, ..Cfg::default() }, limit(opts));
    let g = ev.st.sym("g");
    let miss = ev.st.sym("miss");
    let mut o = Out::default();
    let t0 = Instant::now();
    let mut done = 0;
    let ok = timed(&mut o, "define", || -> Result<(), crate::Stop> {
        ev.define(papp(g, vec![x(0)]), Rc::new(move |b, st| st.app(miss, &[b.one(0)])), None)?;
        for k in 0..n as i64 {
            // every tenth rule repeats an earlier left side: it must replace, not add
            let kk = if k % 10 == 9 { k / 2 } else { k };
            let p = papp(g, vec![lit(ev.st.int(kk))]);
            ev.define(p, Rc::new(move |_, st| st.int(k)), None)?;
            done += 1;
        }
        Ok(())
    });
    if ok.is_err() {
        limited(&mut o, "definitions", done, n, t0.elapsed());
        return o;
    }
    let rs = ev.rules(g).unwrap();
    // the general rule must be last
    assert_eq!(*rs.order.last().unwrap(), 0, "the general rule is not last");
    o.check = rs.order_fingerprint();
    o.note = format!("{} rules after replacements", rs.len());
    o
}

fn d4(variant: &str, n: usize, opts: &Opts) -> Out {
    let order = if variant == "scan" { OrderBy::Scan } else { OrderBy::Index };
    let mut ev = Evaluator::new(Store::new(), Cfg { order, ..Cfg::default() }, limit(opts));
    let w = ac_world(&mut ev);
    let mut rng = Rng::new(4);
    let rules: Vec<_> = (0..n).map(|k| ac_rule(&mut ev.st, &w, &mut rng, k as i64)).collect();
    let mut o = Out::default();
    let t0 = Instant::now();
    let mut done = 0;
    let ok = timed(&mut o, "define", || {
        for (_, l, r) in rules {
            ev.define(l, r, None)?;
            done += 1;
        }
        Ok::<(), crate::Stop>(())
    });
    if ok.is_err() {
        limited(&mut o, "definitions", done, n, t0.elapsed());
        return o;
    }
    let (a, m) = (ev.rules(w.add).unwrap(), ev.rules(w.mul).unwrap());
    // how many rules did ordering move (not appended at the end)?
    let moved = |rs: &crate::index::RuleSet| rs.order.windows(2).filter(|p| p[0] > p[1]).count();
    o.check = combine(a.order_fingerprint(), m.order_fingerprint());
    o.note = format!("{} + {} rules, {} out of definition order", a.len(), m.len(), moved(a) + moved(m));
    o
}
