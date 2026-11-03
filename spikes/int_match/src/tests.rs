//! Composition invariants:
//! - wide nodes intern to one id whatever the schedule (the spike 4 hash bug this piece found);
//! - a rule index read concurrently with its one writer shows each rule set before or after each definition: never a
//!   missing old rule, never a wrong order, also while the tree's depth grows and rules are replaced;
//! - the naive and the pruning AC matcher find the same first match on the concurrent store;
//! - parallel evaluation with mid-run definitions (ordered effects) equals sequential evaluation, in every split
//!   mode and memo kind, including conditions that evaluate and read globals;
//! - reads made inside conditions are what keeps memo stamps correct (untracked, they go stale);
//! - an effect reached inside a speculative condition is performed once, in sequential order;
//! - results agree with spike 2's sequential evaluator.
use crate::eval::{Config, Eval, Split};
use crate::heap::Heap;
use crate::index::{IndexMode, Rhs, Rule, RuleSet, Scratch};
use crate::matcher::{Binds, Matcher};
use crate::memo::MemoKind;
use crate::pat::{compile, cond, lit, papp, x, xt, Pat, Ty};
use crate::work::{self, CondKind};
use crate::{fingerprint, Rng, Term};
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Tests run one at a time (each uses up to 4 worker threads; the machine is shared).
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn pool(n: usize) -> ThreadPool {
    ThreadPoolBuilder::new().num_threads(n).stack_size(64 << 20).build().unwrap()
}

fn cfg(split: Split, memo: bool) -> Config {
    Config { memo, ..Config::new(split) }
}

#[test]
fn wide_nodes_intern_once_on_any_schedule() {
    let _s = serial();
    let st = Heap::new();
    let list = st.sym("list");
    let args: Vec<Term> = (0..200_000).map(|i| st.int(i * 7 + 3)).collect();
    let a = st.app(list, &args);
    for t in [1, 2, 4] {
        let p = pool(t);
        for _ in 0..3 {
            assert_eq!(p.install(|| st.app(list, &args)), a, "{t} threads");
        }
    }
    assert_eq!(st.app(list, &args), a);
}

fn rhs_const(st: &Heap, v: i64) -> Rhs {
    let t = st.int(v);
    Arc::new(move |_: &Binds, _: &Heap| t)
}

#[test]
fn index_readers_see_before_or_after_each_definition() {
    let _s = serial();
    let st = Heap::new();
    let (g, w) = (st.sym("g"), st.sym("w"));
    let rs = RuleSet::new(false, IndexMode::LockFree);
    let c = |p: Pat| compile(&st, p, &|_| false);
    rs.insert(&st, Rule::new(c(papp(g, vec![x(0)])), rhs_const(&st, -1)));
    const N: i64 = 3000;
    // subject k: g(w^d(k)) with d = k % 4; its literal rule (if published) must come first, the general one last
    let subject = |k: i64| {
        let mut t = st.int(k);
        for _ in 0..k % 4 {
            t = st.app(w, &[t]);
        }
        st.app(g, &[t])
    };
    let subjects: Vec<Term> = (0..N).map(subject).collect();
    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            for k in 0..N {
                rs.insert(&st, Rule::new(c(lit(subjects[k as usize])), rhs_const(&st, k)));
                if k % 10 == 0 {
                    // the same left side again: a replacement while readers may hold the old rule
                    rs.insert(&st, Rule::new(c(lit(subjects[k as usize])), rhs_const(&st, k)));
                }
            }
            done.store(true, Ordering::Release);
        });
        for r in 0..3u64 {
            let (rs, st, subjects, done) = (&rs, &st, &subjects, &done);
            s.spawn(move || {
                let mut rng = Rng::new(r + 1);
                let mut sc = Scratch::default();
                let mut seen = 0;
                while !done.load(Ordering::Acquire) || seen < 1000 {
                    let k = rng.below(N as u64) as usize;
                    rs.candidates(st, subjects[k], &mut sc);
                    let ids = sc.cands.clone();
                    assert_eq!(ids.last(), Some(&0), "the general rule is missing or out of order: {ids:?}");
                    assert!(ids.len() <= 2, "extra candidates {ids:?}");
                    if ids.len() == 2 {
                        let Pat::Lit(t) = rs.rule(ids[0]).lhs else { panic!("not a literal rule") };
                        assert_eq!(t, subjects[k]);
                    }
                    seen += 1;
                }
            });
        }
    });
    let mut sc = Scratch::default();
    for (k, &t) in subjects.iter().enumerate() {
        rs.candidates(&st, t, &mut sc);
        assert_eq!(sc.cands.len(), 2, "subject {k} after all definitions");
    }
    assert_eq!(rs.len() as i64, N + 1, "replacements must not add rules");
}

#[test]
fn naive_and_smart_agree_on_the_concurrent_store() {
    let _s = serial();
    let st = Heap::new();
    let (add, f, g) = (st.sym("add"), st.sym("f"), st.sym("g"));
    let atoms: Vec<Term> = ["a", "b", "c"].iter().map(|s| st.sym(s)).chain([st.int(1), st.int(2)]).collect();
    let mut ev = Eval::new(&st, Config::new(Split::Never));
    ev.set_ac(add);
    let mut rng = Rng::new(3);
    let term = |rng: &mut Rng| {
        let n = 1 + rng.below(6) as usize;
        let a: Vec<Term> = (0..n)
            .map(|_| {
                let t = atoms[rng.below(atoms.len() as u64) as usize];
                if rng.pct(30) {
                    st.app(f, &[t])
                } else {
                    t
                }
            })
            .collect();
        st.app_ac(add, &a)
    };
    let subjects: Vec<Term> = (0..200).map(|_| term(&mut rng)).collect();
    let mut pats = vec![];
    for _ in 0..200 {
        let n = 1 + rng.below(4) as usize;
        let mut a: Vec<Pat> = (0..n)
            .map(|_| match rng.below(5) {
                0 => lit(atoms[rng.below(atoms.len() as u64) as usize]),
                1 => papp(f, vec![x(rng.below(3) as u16)]),
                2 => xt(rng.below(3) as u16, Ty::Symbol),
                _ => x(rng.below(3) as u16),
            })
            .collect();
        if rng.pct(60) {
            a.push(if rng.pct(50) { crate::pat::seq0(3) } else { crate::pat::seq1(3) });
        }
        let p = ev.compile(papp(add, a));
        let n = p.n_vars();
        pats.push((p, n));
    }
    let _ = g;
    let (mut matched, mut b1, mut b2, mut s) = (0, Binds::default(), Binds::default(), Binds::default());
    ev.with_cx(|cx| {
        for (p, n) in &pats {
            for &t in &subjects {
                let m1 = Matcher::new(&ev, cx, false).first(p, *n, t, &mut s, &mut b1);
                let m2 = Matcher::new(&ev, cx, true).first(p, *n, t, &mut s, &mut b2);
                assert_eq!(m1, m2, "{} against {}", crate::pat::show(&st, p), st.show(t));
                if m1 {
                    matched += 1;
                    assert_eq!(b1.fingerprint(&st), b2.fingerprint(&st), "{} against {}", crate::pat::show(&st, p), st.show(t));
                }
            }
        }
    });
    assert!(matched > 100, "the test should exercise matches ({matched})");
}

#[test]
fn parallel_equals_sequential_with_midrun_redefinitions() {
    let _s = serial();
    let p1 = pool(1);
    let p4 = pool(4);
    for layout in [None, Some(1), Some(2), Some(3), Some(4), Some(5)] {
        let reference = work::mr(&p1, cfg(Split::Never, false), 1500, layout).check;
        let configs = [
            (Split::Never, true, MemoKind::Dense, 20, &p1),
            (Split::Always, true, MemoKind::Dense, 20, &p4),
            (Split::Lazy, true, MemoKind::Dense, 20, &p4),
            (Split::Always, true, MemoKind::Hashed, 3, &p4),
            (Split::Always, false, MemoKind::Dense, 20, &p4),
        ];
        for (split, memo, kind, bits, p) in configs {
            for _ in 0..3 {
                let c = Config { memo_kind: kind, memo_bits: bits, ..cfg(split, memo) };
                assert_eq!(work::mr(p, c, 1500, layout).check, reference, "layout {layout:?} {split:?} memo {memo} {kind:?}");
            }
        }
    }
}

#[test]
fn condition_reads_enter_the_read_set() {
    let _s = serial();
    let p1 = pool(1);
    let p4 = pool(4);
    for kind in [CondKind::Native, CondKind::Eval] {
        let reference = work::r14e(&p1, cfg(Split::Never, false), 3000, kind).check;
        for (split, p) in [(Split::Never, &p1), (Split::Always, &p4), (Split::Lazy, &p4)] {
            assert_eq!(work::r14e(p, cfg(split, true), 3000, kind).check, reference, "{kind:?} {split:?}");
        }
    }
    // without recording what conditions read, memoised normal forms survive the redefinition of `c`
    let untracked = Config { track_conds: false, ..cfg(Split::Lazy, true) };
    let reference = work::r14e(&p1, cfg(Split::Never, false), 3000, CondKind::Eval).check;
    assert_ne!(work::r14e(&p4, untracked, 3000, CondKind::Eval).check, reference, "untracked reads should go stale");
}

#[test]
fn effect_in_a_speculative_condition_happens_once_in_order() {
    let _s = serial();
    let run = |p: &ThreadPool, split: Split| {
        let st = Heap::new();
        let (f, hi, print, lp, list) = (st.sym("f"), st.sym("hi"), st.sym("print"), st.sym("loop"), st.sym("list"));
        let mut ev = Eval::new(&st, cfg(split, true));
        let p_ = cond(papp(f, vec![xt(0, Ty::Integer)]), move |b, cc| {
            let v = cc.int(b.one(0)).unwrap();
            if v % 8 == 0 {
                let st = cc.heap();
                cc.eval(st.app(print, &[b.one(0)]))?;
            }
            Ok(v % 3 == 0)
        });
        ev.define(p_, Arc::new(move |b: &Binds, st: &Heap| st.app(hi, &[st.app(lp, &[st.int(300), b.one(0)])])), None);
        let t = st.app(list, &(0..256).map(|i| st.app(f, &[st.int(i)])).collect::<Vec<_>>());
        let (r, _) = p.install(|| ev.run(t));
        let r = r.unwrap();
        let out: Vec<i64> = ev.output().iter().map(|&o| st.as_int(o).unwrap()).collect();
        (fingerprint(&st, r), out)
    };
    let (r0, o0) = run(&pool(1), Split::Never);
    assert_eq!(o0, (0..256).filter(|v| v % 8 == 0).collect::<Vec<_>>());
    let p4 = pool(4);
    for split in [Split::Always, Split::Lazy] {
        for _ in 0..5 {
            assert_eq!(run(&p4, split), (r0, o0.clone()), "{split:?}");
        }
    }
}

#[test]
fn safe_point_frees_retired_definitions() {
    let _s = serial();
    let st = Heap::new();
    let g = st.sym("g");
    let mut ev = Eval::new(&st, Config::new(Split::Never));
    for k in 0..200 {
        // grows the edge maps (retiring arrays) and replaces rules
        ev.define(papp(g, vec![lit(st.int(k % 100))]), rhs_const(&st, k), None);
    }
    let t = st.app(g, &[st.int(42)]);
    let before = ev.run(t).0.unwrap();
    assert!(ev.safe_point() > 0, "something must have been retired");
    assert_eq!(ev.run(t).0.unwrap(), before);
    assert_eq!(st.as_int(before), Some(142));
}

#[test]
fn agrees_with_spike_2() {
    let _s = serial();
    let p4 = pool(4);
    let opts = matcher::work::Opts { quick: true, limit: std::time::Duration::from_secs(60) };
    let s2 = |id: &str, variant: &str, n: usize| {
        let c = matcher::work::cases().into_iter().find(|c| c.id == id).unwrap();
        (c.run)(variant, n, &opts).check
    };
    assert_eq!(work::r4(&p4, cfg(Split::Lazy, true), 300, true).s2, s2("R4 AC rules", "indexed/smart", 300));
    assert_eq!(work::r5(&p4, cfg(Split::Always, false), 500, true).s2, s2("R5 literal rules", "indexed", 500));
    assert_eq!(work::r13(&p4, cfg(Split::Lazy, false), 50, true).s2, s2("R13 arg-head rules", "arg-head", 50));
    assert_eq!(work::r14(&p4, cfg(Split::Lazy, true), 10_000, true).s2, s2("R14 read sets", "memo/mask", 10_000));
    assert_eq!(work::s7(&p4, cfg(Split::Lazy, false), 10_000, true).s2, s2("S7 cases", "smart", 10_000));
}
