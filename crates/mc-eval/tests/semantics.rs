//! Semantics the evaluator must keep at any number of threads: pure conditions (6.1, A.6), condition scope (6.1),
//! limits decided by the frontier only (18.4, A.5), interrupts, the depth limit, effect order (18.2), failure
//! propagation (10), rule order (6.3), argument modes (5) and safe points between evaluations.
use mc_core::shapes::fingerprint;
use mc_core::{Config, Heap, Session, Store, Sym, View, T};
use mc_eval::pat::{call, cond, lit, typed, var};
use mc_eval::{session, Binds, DefError, EvalConfig, Kernel, Rhs, Split, Status, Ty};
use std::sync::Arc;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn rhs(f: impl for<'h> Fn(&Binds<'h>, &'h Heap) -> T<'h> + Send + Sync + 'static) -> Rhs {
    Arc::new(f)
}

/// The sequential reference and parallel configurations (always-split at 2 and 4 threads, lazy at 4), each with
/// collection at every safe point.
fn configs() -> Vec<(usize, EvalConfig, Config)> {
    let mut v = vec![(1, EvalConfig::reference(), Config::no_collect())];
    for (t, split) in [(1, Split::Lazy), (2, Split::Always), (4, Split::Always), (4, Split::Lazy)] {
        v.push((t, EvalConfig { split, ..EvalConfig::default() }, Config::stress()));
    }
    v
}

fn show(h: &Heap, t: T<'_>) -> String {
    match h.view(t) {
        View::Int(v) => v.to_string(),
        View::Sym(s) => h.sym_name(s),
        View::App { head, args } => format!("{}({})", show(h, head), args.iter().map(|&a| show(h, a)).collect::<Vec<_>>().join(", ")),
        v => format!("{v:?}"),
    }
}

/// A list `list(f(0), ..., f(n-1))`.
fn calls<'h>(h: &'h Heap, f: Sym, n: i64) -> T<'h> {
    let xs: Vec<T> = (0..n).map(|i| h.call(f, &[h.int(i)])).collect();
    h.call(h.sym("list"), &xs)
}

/// Run `setup` in one step and `body` in the next, in every configuration; all must give the same observation.
fn same_everywhere<R: PartialEq + std::fmt::Debug + Send>(
    setup: impl Fn(&Heap, &mut Kernel) + Sync,
    body: impl Fn(&Heap, &mut Kernel) -> R + Sync,
) -> R {
    let mut first: Option<R> = None;
    for (threads, cfg, heap) in configs() {
        let mut s: Session<Kernel> = session(heap, threads, cfg);
        s.step(|h, k| setup(h, k));
        let r = s.step(|h, k| body(h, k));
        match &first {
            None => first = Some(r),
            Some(f) => assert_eq!(&r, f, "{threads} threads, {cfg:?}"),
        }
    }
    first.expect("a configuration")
}

#[test]
fn an_effect_in_a_condition_is_a_failure() {
    let _s = serial();
    // f(x_ : integer) /; (x % 8 == 0 and print(x) or true) := hi(x)
    let setup = |h: &Heap, k: &mut Kernel| {
        let (f, hi, print) = (h.sym("f"), h.sym("hi"), h.sym("print"));
        let p = cond(call(h, f, vec![typed(0, Ty::Integer)]), &[0], move |cx| {
            let x = cx.one(0);
            if cx.int(x).expect("integer") % 8 == 7 {
                let h = cx.heap();
                cx.eval(h.call(print, &[x]))?;
            }
            Ok(true)
        });
        k.define(h, p, rhs(move |b, h| h.call(hi, &[b.one(0)]))).expect("rule");
    };
    let r = same_everywhere(setup, |h, k| {
        let f = h.sym("f");
        // one call: its value is the failure
        let one = show(h, k.eval(h, h.call(f, &[h.int(7)])).value);
        // a list of calls: the first failure argument is the list's value (semantics 10)
        let many = show(h, k.eval(h, calls(h, f, 64)).value);
        let fine = show(h, k.eval(h, h.call(f, &[h.int(3)])).value);
        (one, many, fine, k.output(h).len())
    });
    assert_eq!(r, ("failure(effect_in_condition)".into(), "failure(effect_in_condition)".into(), "hi(3)".into(), 0));
}

#[test]
fn a_definition_in_a_nested_condition_is_a_failure() {
    let _s = serial();
    // g(x_) /; same(define(c, x), null) := 1, reached from f's condition: f(x_) /; same(g(x), 1) := 2
    let setup = |h: &Heap, k: &mut Kernel| {
        let (f, g, c, define, same, null) = (h.sym("f"), h.sym("g"), h.sym("c"), h.sym("define"), h.sym("same"), h.sym("null"));
        k.set_own(c, h.int(0));
        let pg = cond(call(h, g, vec![var(0)]), &[0], move |cx| {
            let h = cx.heap();
            let t = h.call(same, &[h.call(define, &[h.sym_term(c), cx.one(0)]), h.sym_term(null)]);
            let r = cx.eval(t)?;
            Ok(cx.is_true(r))
        });
        k.define(h, pg, rhs(|_, h| h.int(1))).expect("rule");
        let pf = cond(call(h, f, vec![var(0)]), &[0], move |cx| {
            let h = cx.heap();
            let r = cx.eval(h.call(g, &[cx.one(0)]))?;
            Ok(cx.int(r) == Some(1))
        });
        k.define(h, pf, rhs(|_, h| h.int(2))).expect("rule");
    };
    let r = same_everywhere(setup, |h, k| {
        let (f, g, c) = (h.sym("f"), h.sym("g"), h.sym("c"));
        let direct = show(h, k.eval(h, h.call(g, &[h.int(5)])).value);
        let nested = show(h, k.eval(h, calls(h, f, 16)).value);
        (direct, nested, k.value_of(h, c).map(|v| show(h, v)))
    });
    assert_eq!(r, ("failure(effect_in_condition)".into(), "failure(effect_in_condition)".into(), Some("0".into())));
}

#[test]
fn conditions_may_only_read_their_own_variables() {
    let _s = serial();
    let mut s = session(Config::default(), 1, EvalConfig::default());
    s.step(|h, k| {
        let f = h.sym("f");
        // f(x_ /; c(y), y_): the inner condition reads y, bound outside its subpattern
        let inner = cond(var(0), &[1], |_| Ok(true));
        let e = k.define(h, call(h, f, vec![inner, var(1)]), rhs(|_, h| h.int(0)));
        assert_eq!(e, Err(DefError::ConditionScope(1)));
        // on the whole left side it may
        let whole = cond(call(h, f, vec![var(0), var(1)]), &[0, 1], |cx| Ok(cx.one(0) == cx.one(1)));
        k.define(h, whole, rhs(|_, h| h.int(1))).expect("rule");
        let r = k.eval(h, h.call(f, &[h.int(4), h.int(4)])).value;
        assert_eq!(r, h.int(1));
    });
}

#[test]
fn effects_happen_once_in_sequential_order() {
    let _s = serial();
    // F(x_) := iadd(print(x), loop-free work): every element prints; the output order is the list order
    let setup = |h: &Heap, k: &mut Kernel| {
        let (ff, print, imul) = (h.sym("F"), h.sym("print"), h.sym("imul"));
        k.define(h, call(h, ff, vec![var(0)]), rhs(move |b, h| h.call(imul, &[h.call(print, &[b.one(0)]), h.int(3)]))).expect("rule");
    };
    let n = 300;
    let r = same_everywhere(setup, |h, k| {
        let (ff, map, list) = (h.sym("F"), h.sym("map"), h.sym("list"));
        let xs: Vec<T> = (0..n).map(|i| h.int(i)).collect();
        let v = k.eval(h, h.call(map, &[h.sym_term(ff), h.call(list, &xs)])).value;
        let out: Vec<i64> = k.output(h).iter().map(|&o| if let View::Int(i) = h.view(o) { i } else { -1 }).collect();
        (fingerprint(h, v), out)
    });
    assert_eq!(r.1, (0..n).collect::<Vec<_>>());
}

#[test]
fn a_genuine_fuel_limit_fails_the_same_way_everywhere() {
    let _s = serial();
    let r = same_everywhere(
        |_, _| {},
        |h, k| {
            let (iadd, list) = (h.sym("iadd"), h.sym("list"));
            // a list of 5000 sums of nested sums: 2 fuel each, 10000 in all, against a budget of 5000
            let xs: Vec<T> = (0..5000).map(|i| h.call(iadd, &[h.call(iadd, &[h.int(i), h.int(1)]), h.int(2)])).collect();
            let e = k.eval_with(h, h.call(list, &xs), 5000);
            let ok = k.eval_with(h, h.call(list, &xs[..100]), 5000);
            (show(h, e.value), e.status, ok.status, fingerprint(h, ok.value))
        },
    );
    assert_eq!((r.0.as_str(), r.1, r.2), ("failure(limit, fuel)", Status::Fuel, Status::Running));
}

/// Speculative work that exhausts the budget on its own never turns a result into a limit failure (A.5): here the
/// speculative tasks start on stale definitions that make every element loop forever, while the frontier first
/// redefines the mode (slowly) and then finds every element cheap.
#[test]
fn speculative_exhaustion_alone_never_fails() {
    let _s = serial();
    let setup = |h: &Heap, k: &mut Kernel| {
        let (hh, mode, ifs, same, lp) = (h.sym("H"), h.sym("mode"), h.sym("if"), h.sym("same"), h.sym("loop"));
        k.register(lp, mc_eval::Attributes::default(), Box::new(Loop(lp)));
        k.set_own(mode, h.int(0));
        // H(x_) := if(same(mode, 0), loop(10^12, x), x)
        let body = rhs(move |b, h| {
            let test = h.call(same, &[h.sym_term(mode), h.int(0)]);
            h.call(ifs, &[test, h.call(lp, &[h.int(1_000_000_000_000), b.one(0)]), b.one(0)])
        });
        k.define(h, call(h, hh, vec![var(0)]), body).expect("rule");
    };
    let budget = 200_000;
    let mut holes = 0;
    let mut first = None;
    for (threads, cfg, heap) in configs() {
        let mut s = session(heap, threads, cfg);
        s.step(|h, k| setup(h, k));
        let (fp, status, stats) = s.step(|h, k| {
            let (hh, define, mode, lp, list) = (h.sym("H"), h.sym("define"), h.sym("mode"), h.sym("loop"), h.sym("list"));
            // element 0: define(mode, loop(50000, 1)) takes a while; elements 1..64: H(i)
            let mut xs = vec![h.call(define, &[h.sym_term(mode), h.call(lp, &[h.int(50_000), h.int(1)])])];
            xs.extend((1..64).map(|i| h.call(hh, &[h.int(i)])));
            let e = k.eval_with(h, h.call(list, &xs), budget);
            (fingerprint(h, e.value), e.status, e.stats)
        });
        assert_eq!(status, Status::Running, "{threads} threads {cfg:?}");
        assert!(stats.fuel <= budget as u64);
        holes += stats.holes;
        match first {
            None => first = Some(fp),
            Some(f) => assert_eq!(fp, f, "{threads} threads {cfg:?}"),
        }
    }
    assert!(holes > 0, "speculative tasks should have exhausted their shares");
}

/// `loop(k, x)`: k steps of a linear congruential map.
struct Loop(Sym);

impl mc_eval::Native for Loop {
    fn call<'h>(&self, cx: &mut mc_eval::NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> mc_eval::Res<mc_eval::Reply<'h>> {
        let h = cx.heap();
        let (View::Int(k), View::Int(x)) = (h.view(a[0]), h.view(a[1])) else { return Ok(mc_eval::Reply::Decline) };
        if k <= 0 {
            return Ok(mc_eval::Reply::Done(a[1]));
        }
        let next = (x.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x3fff_ffff;
        Ok(mc_eval::Reply::Again(h.call(self.0, &[h.int(k - 1), h.int(next)])))
    }
}

#[test]
fn an_interrupt_stops_every_task() {
    let _s = serial();
    for (threads, split) in [(1, Split::Never), (4, Split::Always)] {
        let mut s = session(Config::default(), threads, EvalConfig { split, ..EvalConfig::default() });
        s.step(|h, k| {
            let lp = h.sym("loop");
            k.register(lp, mc_eval::Attributes::default(), Box::new(Loop(lp)));
        });
        let ctl = s.state.control();
        let budget = i64::MAX / 2;
        let stopper = std::thread::spawn(move || {
            // wait until the evaluation has drawn fuel (it has begun), then interrupt
            loop {
                let f = ctl.fuel.load(std::sync::atomic::Ordering::Relaxed);
                if f > 0 && f < budget {
                    break;
                }
                std::thread::yield_now();
            }
            ctl.interrupt();
        });
        let (v, status) = s.step(|h, k| {
            let (lp, list) = (h.sym("loop"), h.sym("list"));
            let xs: Vec<T> = (0..8).map(|i| h.call(lp, &[h.int(1 << 40), h.int(i)])).collect();
            let e = k.eval_with(h, h.call(list, &xs), budget);
            (show(h, e.value), e.status)
        });
        stopper.join().expect("stopper");
        assert_eq!((v.as_str(), status), ("failure(interrupted)", Status::Interrupted), "{threads} threads");
    }
}

#[test]
fn nesting_beyond_the_depth_limit_fails() {
    let _s = serial();
    fn deep(h: &Heap, n: usize) -> T<'_> {
        let (g, iadd) = (h.sym("g"), h.sym("iadd"));
        let mut t = h.call(iadd, &[h.int(1), h.int(1)]);
        for _ in 0..n {
            t = h.call(g, &[t]);
        }
        t
    }
    let r = same_everywhere(
        |_, k| k.set_config(EvalConfig { max_depth: 100, ..*k.config() }),
        |h, k| {
            let bad = k.eval(h, deep(h, 150));
            let good = k.eval(h, deep(h, 50));
            (show(h, bad.value), bad.status, good.status)
        },
    );
    assert_eq!(r, ("failure(limit, depth)".into(), Status::Depth, Status::Running));
    // the default limit is reachable without exhausting the stack
    let mut s = session(Config::default(), 1, EvalConfig::default());
    s.step(|h, k| {
        let d = k.config().max_depth as usize - 10;
        let e = k.eval(h, deep(h, d));
        assert_eq!(e.status, Status::Running);
    });
}

#[test]
fn failures_propagate_through_strict_arguments() {
    let _s = serial();
    let mut s = session(Config::default(), 1, EvalConfig::default());
    s.step(|h, k| {
        let (f, failure, oops, iadd) = (h.sym("f"), h.sym("failure"), h.sym("oops"), h.sym("iadd"));
        let bad = h.call(failure, &[h.sym_term(oops)]);
        let t = h.call(f, &[h.int(1), h.call(iadd, &[bad, h.int(2)]), h.int(3)]);
        assert_eq!(k.eval(h, t).value, bad);
    });
}

#[test]
fn rules_are_ordered_most_specific_first() {
    let _s = serial();
    let mut s = session(Config::stress(), 2, EvalConfig::default());
    s.step(|h, k| {
        let f = h.sym("f");
        let tag = |n: i64| rhs(move |_, h| h.int(n));
        k.define(h, call(h, f, vec![var(0)]), tag(1)).expect("rule");
        k.define(h, call(h, f, vec![typed(0, Ty::Integer)]), tag(2)).expect("rule");
        k.define(h, call(h, f, vec![lit(h.int(5))]), tag(3)).expect("rule");
        // the same left side up to renaming replaces the general rule
        k.define(h, call(h, f, vec![var(7)]), tag(4)).expect("rule");
        assert_eq!(k.rule_count(f), 3);
    });
    s.step(|h, k| {
        let f = h.sym("f");
        for (x, want) in [(h.int(5), 3), (h.int(6), 2), (h.sym_term(h.sym("a")), 4)] {
            assert_eq!(k.eval(h, h.call(f, &[x])).value, h.int(want));
        }
    });
}

#[test]
fn builtins_and_argument_modes() {
    let _s = serial();
    let mut s = session(Config::default(), 1, EvalConfig::default());
    s.step(|h, k| {
        let n = |s: &str| h.sym(s);
        let int = |v| h.int(v);
        let ev = |k: &mut Kernel, t| k.eval(h, t).value;
        assert_eq!(ev(k, h.call(n("iadd"), &[int(2), int(3), int(4)])), int(9));
        assert_eq!(ev(k, h.call(n("imod"), &[int(-7), int(5)])), int(3));
        assert_eq!(ev(k, h.call(n("less"), &[int(2), int(3)])), k.bool(h, true));
        // iadd declines on overflow: the call stays as it is
        let big = h.call(n("iadd"), &[int(i64::MAX), int(1)]);
        assert_eq!(ev(k, big), big);
        // if holds its branches: the other branch's effect never happens
        let p = |v| h.call(n("print"), &[int(v)]);
        let t = h.call(n("if"), &[h.call(n("less"), &[int(1), int(2)]), p(10), p(20)]);
        assert_eq!(ev(k, t), int(10));
        assert_eq!(k.output(h), vec![int(10)]);
        // define holds the symbol, evaluates the value
        let x = n("x");
        assert_eq!(ev(k, h.call(n("define"), &[h.sym_term(x), h.call(n("imul"), &[int(6), int(7)])])), h.sym_term(n("null")));
        assert_eq!(ev(k, h.call(n("iadd"), &[h.sym_term(x), int(1)])), int(43));
    });
}

#[test]
fn definitions_survive_collections_and_retired_parts_are_freed() {
    let _s = serial();
    let mut s = session(Config::stress(), 2, EvalConfig::default());
    s.step(|h, k| {
        let (g, w) = (h.sym("g"), h.sym("w"));
        for j in 0..200 {
            // literal compound left sides (young terms held by the rule table), replacements, growing edge maps
            let l = h.call(w, &[h.int(j % 100), h.real(j as f64 / 7.0)]);
            k.define(h, call(h, g, vec![lit(l)]), rhs(move |_, h| h.int(j))).expect("rule");
        }
        k.set_own(h.sym("c"), h.call(w, &[h.string("kept")]));
    });
    for _ in 0..3 {
        s.step(|h, k| {
            let (g, w) = (h.sym("g"), h.sym("w"));
            let t = h.call(g, &[h.call(w, &[h.int(42), h.real(142.0 / 7.0)])]);
            assert_eq!(k.eval(h, t).value, h.int(142));
            assert_eq!(k.value_of(h, h.sym("c")), Some(h.call(w, &[h.string("kept")])));
        });
    }
    assert_eq!(s.state.reclaim(), 0, "collections already freed what definitions retired");
}
