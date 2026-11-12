//! The explicit-stack evaluator (item 1a): recursion depth bounded by the frame limit, not the Rust stack,
//! through rules, blocks, `while` and `for`; the native-callback limit; the compiled-specialisation hook; the
//! native-interface additions (pattern compilation, the canonical order, the stop flag). Language-level cases run
//! on the assembled engine.
use mc_api::{Engine, StopFlag};
use mc_core::{Config, Heap, Store, Sym, View, T};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::pat::{call, lit, typed, var};
use mc_eval::{
    session, Attributes, Binds, Compiled, EvalConfig, Hold, Kernel, Native, NativeCx, Reply, Res, Rhs, Split, Status,
    Ty,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn rhs(f: impl for<'h> Fn(&Binds<'h>, &'h Heap) -> T<'h> + Send + Sync + 'static) -> Rhs {
    Arc::new(f)
}

/// Optimised builds go to the full sizes; unoptimised builds (larger frames, slower) to a tenth.
fn scaled(n: i64) -> i64 {
    if cfg!(debug_assertions) {
        n / 10
    } else {
        n
    }
}

/// `deep(0) = 0`, `deep(n_: integer) := iadd(1, deep(isub(n, 1)))`: one evaluation level per step.
fn define_deep(h: &Heap, k: &mut Kernel) {
    let (deep, iadd, isub) = (h.sym("deep"), h.sym("iadd"), h.sym("isub"));
    k.define(h, call(h, deep, vec![lit(h.int(0))]), rhs(|_, h| h.int(0))).expect("rule");
    let step = rhs(move |b, h| h.call(iadd, &[h.int(1), h.call(deep, &[h.call(isub, &[b.one(0), h.int(1)])])]));
    k.define(h, call(h, deep, vec![typed(0, Ty::Integer)]), step).expect("rule");
}

#[test]
fn a_million_deep_linear_recursion_needs_no_rust_stack() {
    let _s = serial();
    let n = scaled(1_000_000);
    for (threads, split) in [(1, Split::Lazy), (2, Split::Lazy), (2, Split::Always)] {
        let mut s = session(Config::default(), threads, EvalConfig { split, ..EvalConfig::default() });
        s.step(define_deep);
        let (v, status) = s.step(|h, k| {
            let e = k.eval(h, h.call(h.sym("deep"), &[h.int(n)]));
            (match h.view(e.value) { View::Int(x) => Some(x), _ => None }, e.status)
        });
        assert_eq!(v, Some(n), "{threads} threads {split:?}");
        assert_eq!(status, Status::Running);
    }
}

#[test]
fn the_frame_limit_gives_a_depth_failure_and_the_session_goes_on() {
    let _s = serial();
    let mut s = session(Config::default(), 2, EvalConfig { max_depth: 5_000, ..EvalConfig::default() });
    s.step(define_deep);
    let deep = |h: &Heap, k: &mut Kernel, n: i64| {
        let e = k.eval(h, h.call(h.sym("deep"), &[h.int(n)]));
        (format!("{:?}", h.view(e.value)), e.status)
    };
    let (_, status) = s.step(|h, k| deep(h, k, 10_000));
    assert_eq!(status, Status::Depth);
    let (v, status) = s.step(|h, k| deep(h, k, 4_000));
    assert_eq!((v.as_str(), status), ("Int(4000)", Status::Running));
}

/// `nest(x)`: evaluates its (held) argument through [`NativeCx::eval`], a native callback.
struct Nest;

impl Native for Nest {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = args else { return Ok(Reply::Decline) };
        Ok(Reply::Done(cx.eval(*x)?))
    }
}

fn nested<'h>(h: &'h Heap, n: usize) -> T<'h> {
    let (nest, iadd) = (h.sym("nest"), h.sym("iadd"));
    let mut t = h.call(iadd, &[h.int(1), h.int(1)]);
    for _ in 0..n {
        t = h.call(nest, &[t]);
    }
    t
}

#[test]
fn native_callbacks_nest_up_to_their_own_limit() {
    let _s = serial();
    // without the memo: every level is evaluated (an inner call met before would answer at once)
    let mut s = session(Config::default(), 1, EvalConfig { memo: false, ..EvalConfig::default() });
    s.step(|h, k| k.register(h.sym("nest"), Attributes { hold: Hold::All, ..Attributes::default() }, Box::new(Nest)));
    // the default limit fits the stacks of the session's pool
    let max = s.step(|_, k| k.config().max_nest as usize);
    let ok = s.step(|h, k| k.eval(h, nested(h, max - 10)).status);
    assert_eq!(ok, Status::Running);
    let too_deep = s.step(|h, k| {
        let e = k.eval(h, nested(h, max + 10));
        (format!("{:?}", h.view(e.value)), e.status)
    });
    assert_eq!(too_deep.1, Status::Depth, "{}", too_deep.0);
}

/// A toy compiled body for `sq(x)`: machine integers only, counting its calls.
struct Square(Arc<AtomicUsize>);

impl Compiled for Square {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<T<'h>> {
        let h = cx.heap();
        let [x] = args else { return None };
        let View::Int(v) = h.view(*x) else { return None };
        self.0.fetch_add(1, Relaxed);
        v.checked_mul(v).map(|r| h.int(r))
    }
}

/// `compile_sq()`: registers the compiled body of `sq` during an evaluation.
struct CompileSq(Arc<AtomicUsize>, Sym);

impl Native for CompileSq {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        cx.register_compiled(self.1, Arc::new(Square(self.0.clone())))?;
        Ok(Reply::Done(cx.null()))
    }
}

#[test]
fn compiled_specialisations_run_before_rules_and_may_decline() {
    let _s = serial();
    for threads in [1, 4] {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut s = session(Config::stress(), threads, EvalConfig::default());
        let c = calls.clone();
        s.step(move |h, k| {
            let (sq, imul) = (h.sym("sq"), h.sym("imul"));
            k.define(h, call(h, sq, vec![var(0)]), rhs(move |b, h| h.call(imul, &[b.one(0), b.one(0)]))).expect("rule");
            k.register(h.sym("compile_sq"), Attributes { effect: true, ..Attributes::default() }, Box::new(CompileSq(c, sq)));
        });
        let show = |s: &mut mc_core::Session<Kernel>, x: i64| {
            s.step(move |h, k| format!("{:?}", h.view(k.eval(h, h.call(h.sym("sq"), &[h.int(x)])).value)))
        };
        // the rules alone
        assert_eq!(show(&mut s, 7), "Int(49)");
        assert_eq!(calls.load(Relaxed), 0);
        // registered from a native, as an effect of the evaluation
        s.step(|h, k| k.eval(h, h.call(h.sym("compile_sq"), &[])).status);
        assert!(s.step(|h, k| k.compiled(h.sym("sq")).is_some()));
        assert_eq!(show(&mut s, 9), "Int(81)");
        assert_eq!(calls.load(Relaxed), 1);
        // an overflow declines: the rules (and imul's own decline) decide
        let big = show(&mut s, 1 << 40);
        assert!(big.contains("App"), "{big}");
        assert_eq!(calls.load(Relaxed), 2);
        // a non-integer is declined before counting; then the rules apply
        let sym = s.step(|h, k| {
            let v = k.eval(h, h.call(h.sym("sq"), &[h.sym_term(h.sym("y"))])).value;
            matches!(h.view(v), View::App { .. })
        });
        assert!(sym);
        assert_eq!(calls.load(Relaxed), 2);
        // removal and clear unlink it
        s.step(|h, k| k.remove_compiled(h.sym("sq")));
        assert_eq!(show(&mut s, 3), "Int(9)");
        assert_eq!(calls.load(Relaxed), 2);
        let c = calls.clone();
        s.step(move |h, k| {
            k.register_compiled(h.sym("sq"), Arc::new(Square(c)));
            k.clear(h.sym("sq"));
            assert!(k.compiled(h.sym("sq")).is_none());
        });
    }
}

/// `probe(pattern_subject, xs)`: uses the native-interface additions; returns `list(matches, sorted xs, stop)`.
struct Probe;

impl Native for Probe {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [subjects, xs] = args else { return Ok(Reply::Decline) };
        let f = h.sym("f");
        // f(x_: integer), compiled once, matched against every subject
        let p = cx.compile_pattern(call(h, f, vec![typed(0, Ty::Integer)])).expect("pattern");
        let mut bound = vec![];
        let mut b = Binds::default();
        for &t in cx.as_list(*subjects).unwrap_or(&[]) {
            if cx.match_binds(&p, t, &mut b)? {
                bound.push(b.one(0));
            }
        }
        let mut v = cx.as_list(*xs).unwrap_or(&[]).to_vec();
        cx.sort(&mut v);
        assert!(v.windows(2).all(|w| cx.cmp(w[0], w[1]).is_le()));
        let stop = cx.bool(cx.stopped());
        Ok(Reply::Done(cx.list(&[cx.list(&bound), cx.list(&v), stop])))
    }
}

#[test]
fn natives_compile_patterns_sort_and_see_the_stop_flag() {
    let _s = serial();
    // without the memo: the probe runs again after the flag is raised
    let mut s = session(Config::default(), 2, EvalConfig { memo: false, ..EvalConfig::default() });
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    s.step(move |h, k| {
        k.register(h.sym("probe"), Attributes::default(), Box::new(Probe));
        k.set_stop_flag(f2);
    });
    assert!(Arc::ptr_eq(&s.step(|_, k| k.stop_flag()), &flag));
    let run = |s: &mut mc_core::Session<Kernel>| {
        s.step(|h, k| {
            let (f, l) = (h.sym("f"), h.sym("list"));
            let subjects = h.call(l, &[h.call(f, &[h.int(3)]), h.call(f, &[h.sym_term(f)]), h.int(4), h.call(f, &[h.int(5)])]);
            let xs = h.call(l, &[h.int(3), h.int(-1), h.int(2)]);
            let v = k.eval(h, h.call(h.sym("probe"), &[subjects, xs])).value;
            let View::App { args, .. } = h.view(v) else { panic!("a list") };
            let ints = |t: T<'_>| match h.view(t) {
                View::App { args, .. } => args.iter().map(|&a| format!("{:?}", h.view(a))).collect::<Vec<_>>().join(" "),
                _ => String::new(),
            };
            (ints(args[0]), ints(args[1]), format!("{:?}", h.view(args[2])))
        })
    };
    let (m, sorted, stop) = run(&mut s);
    assert_eq!((m.as_str(), sorted.as_str()), ("Int(3) Int(5)", "Int(-1) Int(2) Int(3)"));
    assert!(stop.contains(&format!("{:?}", s.step(|h, _| h.sym("false")))), "{stop}");
    flag.store(true, Relaxed);
    let (_, _, stop) = run(&mut s);
    assert!(stop.contains(&format!("{:?}", s.step(|h, _| h.sym("true")))), "{stop}");
}

fn engine(threads: usize) -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() })
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).unwrap_or_else(|err| panic!("{src}: {err}")).printed
}

#[test]
fn recursion_through_blocks_and_loops_is_bounded_by_frames() {
    let _s = serial();
    let n = scaled(100_000);
    for threads in [1, 2] {
        let mut e = engine(threads);
        // a non-tail statement of a block, a while body with cells, a for body
        eval(&mut e, "rule b(0) = 0\nrule b(k_: Int) := { let v = b(k - 1); v + 1 }");
        eval(&mut e, "rule w(0) = 0\nrule w(k_: Int) := { var acc = 0; while acc < 1 { acc = 1 + w(k - 1) }; acc }");
        eval(&mut e, "rule f(0) = 0\nrule f(k_: Int) := { var acc = 0; for i in range(1) { acc = 1 + f(k - 1) }; acc }");
        for g in ["b", "w", "f"] {
            assert_eq!(eval(&mut e, &format!("{g}({n})")), n.to_string(), "{g} at {threads} threads");
        }
    }
}

#[test]
fn r2_at_its_largest_size() {
    let _s = serial();
    // bench/cases/R/R2.mc: setup, measure, property
    let m = scaled(100_000);
    let mut e = engine(2);
    eval(&mut e, "rule fib(0) = 0\nrule fib(1) = 1\nrule fib(k_: Int) := { let v = fib(k - 1) + fib(k - 2); fib(k) = v; v }");
    eval(&mut e, &format!("fib({m})"));
    e.bind_last("result").expect("a result");
    let property = format!("result^2 - fib({m} - 1)*fib({m} + 1) == (-1)^({m} - 1)");
    assert_eq!(eval(&mut e, &property), "true");
}
